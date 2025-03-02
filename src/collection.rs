use crate::{
    auth::*,
    error::ApiError,
    kv::{Direction, KVStore, KvStoreError, RocksDB},
    sub::*,
    SECRETS_CF,
};

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
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

#[derive(Deserialize, Serialize)]
struct BatchItem {
    key: String,
    value: Value,
}
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

impl From<SortOrder> for Direction {
    fn from(order: SortOrder) -> Self {
        match order {
            SortOrder::Ascending => Direction::Forward,
            SortOrder::Descending => Direction::Reverse,
        }
    }
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

        let secret = Secret {
            created_at: timestamp,
            secret: hash_secret_key(&secret_key),
        };

        db.insert_cf(SECRETS_CF, &name, &secret)
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

    let list_request: Option<ListRequest> = body.and_then(|b| serde_json::from_slice(&b).ok());
    let range_query = list_request
        .as_ref()
        .and_then(|req| req.range.as_ref())
        .or_else(|| query_params.as_ref().map(|q| &q.0))
        .cloned()
        .unwrap_or_default();

    let results = if let Some(query) = list_request.and_then(|req| req.query) {
        let query_str = match query {
            Value::String(s) => s,
            _ => query.to_string(),
        };
        db.query_cf::<Value>(&collection, &query_str, range_query.keys)?
    } else {
        db.get_range_cf::<Value>(
            &collection,
            range_query.from.as_deref().unwrap_or(""),
            range_query.to.as_deref().unwrap_or("\u{fff0}"),
            range_query.limit.unwrap_or(usize::MAX),
            range_query
                .order
                .map(Into::into)
                .unwrap_or(Direction::Forward),
            range_query.keys,
        )?
    };

    Ok(HttpResponse::Ok().json(results))
}

pub async fn create_batch(
    path: Path<String>,
    db: Data<RocksDB>,
    sub_manager: Data<Arc<SubscriptionManager>>,
    body: Bytes,
) -> Result<HttpResponse, ApiError> {
    let collection = path.into_inner();
    let items: Vec<BatchItem> = match serde_json::from_slice(&body) {
        Ok(items) => items,
        Err(_) => return Ok(HttpResponse::BadRequest().body("Invalid JSON batch format")),
    };

    let batch_items: Vec<(&str, &Value)> = items
        .iter()
        .map(|item| (item.key.as_str(), &item.value))
        .collect();

    match db.batch_insert_cf(&collection, &batch_items) {
        Ok(_) => {
            // Notify subscribers for each item in the batch
            for item in &items {
                let event = CollectionEvent {
                    operation: "create".to_string(),
                    key: item.key.clone(),
                    value: item.value.clone(),
                };
                sub_manager.publish(&collection, event).await;
            }
            Ok(HttpResponse::Created().json(items))
        }
        Err(KvStoreError::InvalidColumnFamily(_)) => Ok(HttpResponse::NotFound().finish()),
        Err(e) => Err(ApiError::internal("Failed to insert batch", e)),
    }
}
pub async fn subscribe(
    path: Path<String>,
    sub_manager: Data<Arc<SubscriptionManager>>,
) -> Result<HttpResponse, ApiError> {
    let collection = path.into_inner();
    let sender = sub_manager.get_or_create_channel(&collection).await;
    let mut receiver = sender.subscribe();

    let stream = async_stream::stream! {
        while let Ok(event) = receiver.recv().await {
            // Create new event with timestamp
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();

            // Convert event to Value, add timestamp, convert back
            let mut event_json = serde_json::to_value(&event)?;
            if let Some(value) = event_json.get_mut("value") {
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("serverTime".to_string(), serde_json::json!(timestamp));
                }
            }

            yield Ok::<_, actix_web::Error>(Bytes::from(
                serde_json::to_string(&event_json).unwrap_or_default()
            ));
        }
    };

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(stream))
}
