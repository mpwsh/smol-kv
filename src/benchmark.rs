use crate::kv::{KVStore, RocksDB};
use rand::{distributions::Alphanumeric, Rng};

use actix_web::{
    web::{Data, Query},
    HttpRequest, HttpResponse,
};
use serde_json::json;

use serde::Deserialize;
use std::time::Instant;

#[derive(Deserialize)]
pub struct BenchmarkParams {
    count: usize, // Number of operations to perform
    size: usize,  // Size of each value in bytes
    #[serde(default = "default_batch_size")]
    batch_size: usize, // Optional: How many operations per batch
}

fn default_batch_size() -> usize {
    100
}

fn generate(id: usize) -> serde_json::Value {
    let status = ["active", "inactive", "pending"];
    let types = ["user", "admin", "guest"];
    let mut rng = rand::thread_rng();

    json!({
        "id": format!("user_{}", id),
        "type": types[rng.gen_range(0..3)],
        "status": status[rng.gen_range(0..3)],
        "data": {
            "name": format!("User {}", id),
            "email": format!("user{}@example.com", id),
            "age": rng.gen_range(18..80),
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

pub async fn start(
    db: Data<RocksDB>,
    token: Data<String>,
    params: Query<BenchmarkParams>,
    req: HttpRequest,
) -> HttpResponse {
    // Auth check
    let token_header = req
        .headers()
        .get("Authorization")
        .and_then(|hv| hv.to_str().ok());
    if token_header.is_none() || token_header != Some(token.get_ref().as_str()) {
        return HttpResponse::Unauthorized().finish();
    }
    // Generate test data with realistic JSON
    let random_data = (0..params.count)
        .map(|i| {
            let value = generate(i);
            (format!("bench_key_{}", i), value)
        })
        .collect::<Vec<_>>();

    let start_total = Instant::now();
    let mut metrics = json!({
        "params": {
            "count": params.count,
            "size": params.size,
            "batch_size": params.batch_size
        },
        "operations": {
            "writes": { "count": 0, "success": 0, "duration_ms": 0 },
            "reads": { "count": 0, "success": 0, "duration_ms": 0 },
            "deletes": { "count": 0, "success": 0, "duration_ms": 0 }
        },
        "throughput": {
            "writes_per_sec": 0.0,
            "reads_per_sec": 0.0,
            "mb_written_per_sec": 0.0
        },
        "total_duration_ms": 0,
        "total_data_written_mb": 0.0
    });

    // Print sample data size
    if let Ok(sample_json) = serde_json::to_vec(&random_data[0].1) {
        metrics["data_sample"] = json!({
            "example": random_data[0].1.clone(),
            "size_bytes": sample_json.len()
        });
    }
    // Generate test data once
    let random_data = (0..params.count)
        .map(|i| {
            let value: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(params.size)
                .map(char::from)
                .collect();

            (format!("bench_key_{}", i), value)
        })
        .collect::<Vec<_>>();

    let total_data_size = params.count * params.size;

    // Perform writes in batches
    let write_start = Instant::now();
    for chunk in random_data.chunks(params.batch_size) {
        let batch_items: Vec<_> = chunk.iter().map(|(k, v)| (k.as_str(), v)).collect();

        if db.batch_insert(&batch_items).is_ok() {
            metrics["operations"]["writes"]["success"] = json!(
                metrics["operations"]["writes"]["success"].as_u64().unwrap()
                    + batch_items.len() as u64
            );
        }
        metrics["operations"]["writes"]["count"] = json!(
            metrics["operations"]["writes"]["count"].as_u64().unwrap() + batch_items.len() as u64
        );
    }
    let write_duration = write_start.elapsed();
    metrics["operations"]["writes"]["duration_ms"] = json!(write_duration.as_millis());

    // Perform reads
    let read_start = Instant::now();
    for (key, _) in &random_data {
        if db.find(key).is_ok() {
            metrics["operations"]["reads"]["success"] =
                json!(metrics["operations"]["reads"]["success"].as_u64().unwrap() + 1);
        }
        metrics["operations"]["reads"]["count"] =
            json!(metrics["operations"]["reads"]["count"].as_u64().unwrap() + 1);
    }
    let read_duration = read_start.elapsed();
    metrics["operations"]["reads"]["duration_ms"] = json!(read_duration.as_millis());

    // Perform deletes
    let delete_start = Instant::now();
    for (key, _) in &random_data {
        if db.delete(key).is_ok() {
            metrics["operations"]["deletes"]["success"] = json!(
                metrics["operations"]["deletes"]["success"]
                    .as_u64()
                    .unwrap()
                    + 1
            );
        }
        metrics["operations"]["deletes"]["count"] =
            json!(metrics["operations"]["deletes"]["count"].as_u64().unwrap() + 1);
    }
    let delete_duration = delete_start.elapsed();
    metrics["operations"]["deletes"]["duration_ms"] = json!(delete_duration.as_millis());

    // Calculate throughput metrics
    let writes_per_sec = params.count as f64 / write_duration.as_secs_f64();
    let reads_per_sec = params.count as f64 / read_duration.as_secs_f64();
    let total_duration_secs = start_total.elapsed().as_secs_f64();
    let total_ops = params.count as f64 * 3.0; // total operations (writes + reads + deletes)
    let total_ops_per_sec = total_ops / total_duration_secs;
    let mb_written = total_data_size as f64 / (1024.0 * 1024.0);
    let mb_per_sec = mb_written / write_duration.as_secs_f64();

    // Create metrics without the duplicate fields
    let metrics = json!({
        "params": {
            "count": params.count,
            "size": params.size,
            "batch_size": params.batch_size
        },
        "operations": {
            "writes": {
                "count": metrics["operations"]["writes"]["count"],
                "success": metrics["operations"]["writes"]["success"],
                "duration_ms": write_duration.as_millis()
            },
            "reads": {
                "count": metrics["operations"]["reads"]["count"],
                "success": metrics["operations"]["reads"]["success"],
                "duration_ms": read_duration.as_millis()
            },
            "deletes": {
                "count": metrics["operations"]["deletes"]["count"],
                "success": metrics["operations"]["deletes"]["success"],
                "duration_ms": delete_duration.as_millis()
            }
        },
        "sample_size": metrics["data_sample"],
        "throughput": {
            "writes_per_sec": writes_per_sec,
            "reads_per_sec": reads_per_sec,
            "mb_written_per_sec": mb_per_sec,
            "total_ops_per_sec": total_ops_per_sec
        },
        "totals": {
            "duration_secs": total_duration_secs,
            "duration_ms": start_total.elapsed().as_millis(),
            "operations": total_ops,
            "data_written_mb": mb_written
        }
    });
    HttpResponse::Ok()
        .content_type("application/json")
        .body(metrics.to_string())
}
