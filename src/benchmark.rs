use crate::kv::{Direction, RocksDB};
use rand::Rng;

use actix_web::{
    web::{Data, Query},
    HttpRequest, HttpResponse,
};
use serde_json::{json, Value};

use serde::Deserialize;
use std::time::Instant;

#[derive(Deserialize)]
pub struct BenchmarkParams {
    #[serde(default = "default_count")]
    count: usize,
    #[serde(default = "default_size")]
    size: usize,
    #[serde(default = "default_batch_size")]
    batch_size: usize,
    #[serde(default = "default_query_count")]
    query_count: usize,
    #[serde(default)]
    include_storage: bool,
}

fn default_count() -> usize {
    1000
}
fn default_size() -> usize {
    512
}
fn default_batch_size() -> usize {
    100
}
fn default_query_count() -> usize {
    10
}

fn generate_user(id: usize) -> Value {
    let status = ["active", "inactive", "pending"];
    let types = ["user", "admin", "guest"];
    let tags = ["alpha", "beta", "production", "testing", "development"];
    let mut rng = rand::thread_rng();

    json!({
        "id": id,
        "username": format!("user_{}", id),
        "type": types[rng.gen_range(0..3)],
        "status": status[rng.gen_range(0..3)],
        "age": rng.gen_range(18..80),
        "score": rng.gen_range(0..1000),
        "premium": rng.gen_bool(0.3),
        "tags": (0..rng.gen_range(0..5))
            .map(|_| tags[rng.gen_range(0..tags.len())])
            .collect::<Vec<_>>(),
        "data": {
            "name": format!("User {}", id),
            "email": format!("user{}@example.com", id),
            "verified": rng.gen_bool(0.7)
        },
        "metadata": {
            "last_login": format!("2024-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                rng.gen_range(1..13),
                rng.gen_range(1..29),
                rng.gen_range(0..24),
                rng.gen_range(0..60),
                rng.gen_range(0..60)
            ),
            "created_at": "2024-10-26T20:00:00Z",
            "login_count": rng.gen_range(0..1000)
        }
    })
}

async fn get_storage_metrics(db: &RocksDB, cf_name: &str) -> Value {
    match db.get_cf_size(cf_name) {
        Ok(size) => json!({
            "total_mb": size.total_mb(),
            "sst_mb": size.sst_bytes as f64 / (1024.0 * 1024.0),
            "mem_table_mb": size.mem_table_bytes as f64 / (1024.0 * 1024.0),
            "blob_mb": size.blob_bytes as f64 / (1024.0 * 1024.0)
        }),
        Err(_) => json!({
            "error": "Could not retrieve storage metrics"
        }),
    }
}

fn generate_queries() -> Vec<(String, String)> {
    vec![
        ("All users".into(), "$[*]".into()),
        ("Single user by index".into(), "$[0]".into()),
        (
            "Users with premium accounts".into(),
            "$[?@.premium==true]".into(),
        ),
        ("Active users".into(), "$[?@.status=='active']".into()),
        ("Admin users".into(), "$[?@.type=='admin']".into()),
        ("Users with high scores".into(), "$[?@.score>800]".into()),
        (
            "Young users with premium".into(),
            "$[?@.age<30&&@.premium==true]".into(),
        ),
        (
            "Users with specific tag".into(),
            "$[?@.tags[*]=='alpha']".into(),
        ),
        (
            "Recently logged in users".into(),
            "$[?@.metadata.login_count>500]".into(),
        ),
        (
            "Complex filter".into(),
            "$[?@.age>50&&@.status!='inactive'&&@.score<500]".into(),
        ),
    ]
}

pub async fn start(
    db: Data<RocksDB>,
    token: Data<String>,
    params: Query<BenchmarkParams>,
    req: HttpRequest,
) -> HttpResponse {
    let token_header = req
        .headers()
        .get("X-ADMIN-TOKEN")
        .and_then(|hv| hv.to_str().ok());

    if token_header.is_none() || token_header != Some(token.get_ref().as_str()) {
        return HttpResponse::Unauthorized().finish();
    }

    let benchmark_start = Instant::now();
    let cf_name = "benchmark_cf";

    if !db.cf_exists(cf_name) {
        if let Err(e) = db.create_cf(cf_name) {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("Failed to create column family: {}", e) }));
        }
    }

    let records: Vec<Value> = (0..params.count).map(generate_user).collect();
    let sample_size = serde_json::to_string(&records[0]).unwrap_or_default().len();

    let mut results = json!({
        "params": {
            "count": params.count,
            "size": params.size,
            "batch_size": params.batch_size,
            "query_count": params.query_count
        },
        "sample_record": records[0],
        "sample_size_bytes": sample_size,
        "operations": {
            "inserts": { "count": 0, "success": 0, "duration_ms": 0 },
            "queries": {
                "values_only": { "count": 0, "success": 0, "duration_ms": 0, "avg_results": 0 },
                "with_keys": { "count": 0, "success": 0, "duration_ms": 0, "avg_results": 0 }
            },
            "range_queries": {
                "values_only": { "count": 0, "success": 0, "duration_ms": 0, "avg_results": 0 },
                "with_keys": { "count": 0, "success": 0, "duration_ms": 0, "avg_results": 0 }
            },
            "deletes": { "count": 0, "success": 0, "duration_ms": 0 }
        },
        "throughput": {},
        "storage": {}
    });

    // 1. Inserts
    let insert_start = Instant::now();
    let mut success_count = 0;
    let mut batch_id = 0;

    for chunk in records.chunks(params.batch_size) {
        let keys: Vec<String> = chunk
            .iter()
            .map(|_| {
                let key = format!("bench_key_{}", batch_id);
                batch_id += 1;
                key
            })
            .collect();

        let batch_items: Vec<_> = keys
            .iter()
            .zip(chunk.iter())
            .map(|(key, value)| (key.as_str(), value))
            .collect();

        if db.batch_insert_cf(cf_name, &batch_items).is_ok() {
            success_count += batch_items.len();
        }
    }

    let insert_duration = insert_start.elapsed();
    results["operations"]["inserts"]["count"] = json!(params.count);
    results["operations"]["inserts"]["success"] = json!(success_count);
    results["operations"]["inserts"]["duration_ms"] = json!(insert_duration.as_millis());

    // 2. JSONPath Queries (values only — include_keys=false)
    let queries = generate_queries();
    let query_start = Instant::now();
    let mut query_success = 0;
    let mut total_results = 0;

    for (_, query) in &queries[0..std::cmp::min(queries.len(), params.query_count)] {
        if let Ok(results_vec) = db.query_cf(cf_name, query, false) {
            query_success += 1;
            total_results += results_vec.len();
        }
    }

    let query_duration = query_start.elapsed();
    let avg_results = if query_success > 0 {
        total_results / query_success
    } else {
        0
    };

    results["operations"]["queries"]["values_only"]["count"] = json!(params.query_count);
    results["operations"]["queries"]["values_only"]["success"] = json!(query_success);
    results["operations"]["queries"]["values_only"]["duration_ms"] =
        json!(query_duration.as_millis());
    results["operations"]["queries"]["values_only"]["avg_results"] = json!(avg_results);

    // 3. JSONPath Queries (with keys — include_keys=true)
    let query_keys_start = Instant::now();
    let mut query_keys_success = 0;
    let mut total_keys_results = 0;

    for (_, query) in &queries[0..std::cmp::min(queries.len(), params.query_count)] {
        if let Ok(results_vec) = db.query_cf(cf_name, query, true) {
            query_keys_success += 1;
            total_keys_results += results_vec.len();
        }
    }

    let query_keys_duration = query_keys_start.elapsed();
    let avg_keys_results = if query_keys_success > 0 {
        total_keys_results / query_keys_success
    } else {
        0
    };

    results["operations"]["queries"]["with_keys"]["count"] = json!(params.query_count);
    results["operations"]["queries"]["with_keys"]["success"] = json!(query_keys_success);
    results["operations"]["queries"]["with_keys"]["duration_ms"] =
        json!(query_keys_duration.as_millis());
    results["operations"]["queries"]["with_keys"]["avg_results"] = json!(avg_keys_results);

    // 4. Range Queries (values only — include_keys=false)
    let range_start = Instant::now();
    let mut range_success = 0;
    let mut total_range_results = 0;

    let range_sizes = [10, 50, 100, 500];
    for limit in &range_sizes[0..std::cmp::min(range_sizes.len(), params.query_count)] {
        if let Ok(results_vec) = db.get_range_cf(
            cf_name,
            "0",
            &params.count.to_string(),
            *limit,
            Direction::Forward,
            false,
        ) {
            range_success += 1;
            total_range_results += results_vec.len();
        }
    }

    let range_duration = range_start.elapsed();
    let avg_range_results = if range_success > 0 {
        total_range_results / range_success
    } else {
        0
    };

    results["operations"]["range_queries"]["values_only"]["count"] = json!(range_sizes.len());
    results["operations"]["range_queries"]["values_only"]["success"] = json!(range_success);
    results["operations"]["range_queries"]["values_only"]["duration_ms"] =
        json!(range_duration.as_millis());
    results["operations"]["range_queries"]["values_only"]["avg_results"] = json!(avg_range_results);

    // 5. Range Queries (with keys — include_keys=true)
    let range_keys_start = Instant::now();
    let mut range_keys_success = 0;
    let mut total_range_keys_results = 0;

    for limit in &range_sizes[0..std::cmp::min(range_sizes.len(), params.query_count)] {
        if let Ok(results_vec) = db.get_range_cf(
            cf_name,
            "0",
            &params.count.to_string(),
            *limit,
            Direction::Forward,
            true,
        ) {
            range_keys_success += 1;
            total_range_keys_results += results_vec.len();
        }
    }

    let range_keys_duration = range_keys_start.elapsed();
    let avg_range_keys_results = if range_keys_success > 0 {
        total_range_keys_results / range_keys_success
    } else {
        0
    };

    results["operations"]["range_queries"]["with_keys"]["count"] = json!(range_sizes.len());
    results["operations"]["range_queries"]["with_keys"]["success"] = json!(range_keys_success);
    results["operations"]["range_queries"]["with_keys"]["duration_ms"] =
        json!(range_keys_duration.as_millis());
    results["operations"]["range_queries"]["with_keys"]["avg_results"] =
        json!(avg_range_keys_results);

    // 6. Delete (drop CF)
    let delete_start = Instant::now();
    let delete_success = db.drop_cf(cf_name).is_ok();
    let delete_duration = delete_start.elapsed();
    results["operations"]["deletes"]["count"] = json!(1);
    results["operations"]["deletes"]["success"] = json!(delete_success);
    results["operations"]["deletes"]["duration_ms"] = json!(delete_duration.as_millis());

    // Throughput
    let total_duration_secs = benchmark_start.elapsed().as_secs_f64();
    let insert_throughput = params.count as f64 / insert_duration.as_secs_f64();
    let query_throughput = params.query_count as f64 / query_duration.as_secs_f64();
    let query_keys_throughput = params.query_count as f64 / query_keys_duration.as_secs_f64();

    let total_data_mb = (params.count * sample_size) as f64 / (1024.0 * 1024.0);
    let mb_per_sec = total_data_mb / insert_duration.as_secs_f64();

    results["throughput"] = json!({
        "inserts_per_sec": insert_throughput,
        "queries_per_sec": {
            "values_only": query_throughput,
            "with_keys": query_keys_throughput
        },
        "mb_written_per_sec": mb_per_sec,
        "total_duration_sec": total_duration_secs
    });

    if params.include_storage {
        let cf_storage = "storage_benchmark_cf";
        if !db.cf_exists(cf_storage) || db.create_cf(cf_storage).is_err() {
            results["storage"] = json!({ "error": "Failed to create storage test column family" });
            return HttpResponse::Ok().json(results);
        }

        let storage_sample_size = std::cmp::min(params.count, 1000);
        for i in 0..storage_sample_size {
            let key = format!("storage_key_{}", i);
            let value = generate_user(i);
            let _ = db.insert_cf(cf_storage, &key, &value);
        }

        results["storage"] = get_storage_metrics(&db, cf_storage).await;

        if let Some(total_mb) = results["storage"].get("total_mb") {
            let inserted_mb = (storage_sample_size * sample_size) as f64 / (1024.0 * 1024.0);
            if let Some(total_mb) = total_mb.as_f64() {
                results["storage"]["efficiency"] = json!({
                    "raw_data_mb": inserted_mb,
                    "storage_ratio": total_mb / inserted_mb
                });
            }
        }

        let _ = db.drop_cf(cf_storage);
    }

    if query_duration.as_secs_f64() > 0.0 && query_keys_duration.as_secs_f64() > 0.0 {
        let query_comparison = query_keys_duration.as_secs_f64() / query_duration.as_secs_f64();
        results["comparisons"] = json!({
            "keys_vs_values_ratio": {
                "jsonpath_query": query_comparison,
                "range_query": range_keys_duration.as_secs_f64() / range_duration.as_secs_f64()
            },
            "summary": "Performance impact of including keys in results"
        });
    }

    HttpResponse::Ok().json(results)
}
