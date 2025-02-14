use crate::{
    auth::*,
    error::ApiError,
    kv::{Direction, KVStore, KvStoreError, RocksDB},
};
use actix_web::{
    web::{Data, Path, Query},
    HttpRequest, HttpResponse,
};
use chrono::Utc;
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use bytes::Bytes;

#[derive(Debug, Deserialize, Clone)]
pub struct RangeQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    pub order: Option<SortOrder>,
    #[serde(default = "def_true")]
    pub keys: bool,
}

impl Default for RangeQuery {
    fn default() -> Self {
        Self {
            from: None,
            to: None,
            limit: None,
            order: None,
            keys: def_true(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListRequest {
    #[serde(flatten)]
    range: Option<RangeQuery>,
    query: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum SortOrder {
    #[serde(rename = "asc")]
    Ascending,
    #[serde(rename = "desc")]
    Descending,
}

fn def_true() -> bool {
    true
}
pub async fn exists(name: Path<String>, db: Data<RocksDB>) -> HttpResponse {
    if db.cf_exists(&name) {
        HttpResponse::Ok().finish()
    } else {
        HttpResponse::NotFound().finish()
    }
}

pub async fn create(
    name: Path<String>,
    req: HttpRequest,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    if db.cf_exists(&name) {
        Ok(HttpResponse::Conflict().body(format!("Collection {name} already exists")))
    } else {
        let timestamp = Utc::now().to_rfc3339();

        // Get secret from header or generate new one
        let secret_key = req
            .headers()
            .get("X-SECRET-KEY")
            .and_then(|h| h.to_str().ok())
            .map(String::from)
            .unwrap_or_else(|| nanoid!(32));

        db.create_cf(&name)
            .map_err(|e| ApiError::internal("Failed to create collection", e))?;

        db.insert_cf(
            "secrets",
            &name,
            &serde_json::json!({
                "created_at": timestamp,
                "secret": hash_secret_key(&secret_key)
            }),
        )
        .map_err(|e| match e {
            KvStoreError::InvalidColumnFamily(_) => Ok(HttpResponse::NotFound().finish()),
            _ => Err(ApiError::internal("Failed to insert item", e)),
        })
        .unwrap();

        log::info!("Created collection {name}");

        Ok(HttpResponse::Created().json(serde_json::json!({
            "message": format!("Collection {name} created")
        })))
    }
}

pub async fn drop(name: Path<String>, db: Data<RocksDB>) -> Result<HttpResponse, ApiError> {
    let name = name.into_inner();
    if !db.cf_exists(&name) {
        Ok(HttpResponse::NotFound().body(format!("Collection {name} does not exist")))
    } else {
        db.drop_cf(&name)
            .map_err(|e| ApiError::internal("Failed to drop collection", e))?;

        Ok(HttpResponse::Created().body(format!("Collection {name} deleted")))
    }
}
pub async fn list(
    collection: Path<String>,
    query: Query<RangeQuery>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    if !db.cf_exists(&collection) {
        Ok(HttpResponse::NotFound().finish())
    } else {
        let from = query.from.as_deref().unwrap_or("");
        let to = query.to.as_deref().unwrap_or("\u{fff0}");
        let direction = match query.order.clone().unwrap_or(SortOrder::Ascending) {
            SortOrder::Ascending => Direction::Forward,
            SortOrder::Descending => Direction::Reverse,
        };
        let items = db
            .get_range_cf::<Value>(
                &collection,
                from,
                to,
                query.limit.unwrap_or(usize::MAX),
                direction,
                query.keys,
            )
            .map_err(|e| ApiError::internal("Failed to fetch items", e))?;

        Ok(HttpResponse::Ok().json(items))
    }
}
pub async fn query(
    collection: Path<String>,
    query_params: Option<Query<RangeQuery>>,
    body: Option<Bytes>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    if !db.cf_exists(&collection) {
        return Ok(HttpResponse::NotFound().finish());
    }
    // If we have a body, try to parse it as a ListRequest
    let list_request = if let Some(body) = body {
        match serde_json::from_slice::<ListRequest>(&body) {
            Ok(req) => Some(req),
            Err(_) => return Ok(HttpResponse::BadRequest().body("Invalid request body format")),
        }
    } else {
        None
    };

    let range_query = list_request
        .as_ref()
        .and_then(|req| req.range.as_ref())
        .or_else(|| query_params.as_ref().map(|q| &q.0))
        .cloned()
        .unwrap_or_default();

    let from = range_query.from.as_deref().unwrap_or("");
    let to = range_query.to.as_deref().unwrap_or("\u{fff0}");
    let direction = match range_query.order.clone().unwrap_or(SortOrder::Ascending) {
        SortOrder::Ascending => Direction::Forward,
        SortOrder::Descending => Direction::Reverse,
    };
    let limit = range_query.limit.unwrap_or(usize::MAX);

    // Get base results - either filtered or all
    let mut results = if let Some(query) = list_request.and_then(|req| req.query) {
        let filtered = db
            .as_ref()
            .query_cf::<Value>(&collection, &query.to_string())
            .map_err(|e| ApiError::internal("Failed to query items", e))?;

        if range_query.keys {
            let all_with_keys = db
                .get_range_cf::<Value>(
                    &collection,
                    "",
                    "\u{fff0}",
                    usize::MAX,
                    Direction::Forward,
                    true,
                )
                .map_err(|e| ApiError::internal("Failed to fetch items", e))?;

            all_with_keys
                .into_iter()
                .filter(|keyed_item| {
                    filtered
                        .iter()
                        .any(|f| f == keyed_item.get("value").unwrap())
                })
                .collect()
        } else {
            filtered
        }
    } else {
        // No query - get all items with requested range params
        db.get_range_cf::<Value>(&collection, from, to, limit, direction, range_query.keys)
            .map_err(|e| ApiError::internal("Failed to fetch items", e))?
    };

    // Apply numeric range if specified
    if let (Ok(from_idx), Ok(to_idx)) = (
        range_query.from.as_deref().unwrap_or("0").parse::<usize>(),
        range_query
            .to
            .as_deref()
            .unwrap_or(&results.len().to_string())
            .parse::<usize>(),
    ) {
        let start = from_idx.min(results.len());
        let end = (to_idx + 1).min(results.len());
        results = results[start..end].to_vec();
    }

    // Apply direction and limit
    match direction {
        Direction::Reverse => results.reverse(),
        Direction::Forward => {}
    }
    results.truncate(limit);

    Ok(HttpResponse::Ok().json(results))
}
