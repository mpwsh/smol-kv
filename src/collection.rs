use crate::{
    auth::*,
    error::ApiError,
    key::Operation,
    kv::{Direction, KvStoreError, RocksDB},
    namespace::{hash_collection_namespace, CollectionPath},
    sub::*,
    SECRETS_CF,
};

use std::{sync::Arc, time::SystemTime, time::UNIX_EPOCH};

use actix_web::{
    web::{Data, Json, Query},
    HttpMessage, HttpRequest, HttpResponse,
};
use chrono::Utc;
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
    pub query: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct CollectionCreatedResponse {
    message: String,
    secret_key: String,
}

impl Default for RangeQuery {
    fn default() -> Self {
        Self {
            from: None,
            to: None,
            limit: None,
            order: None,
            keys: def_true(),
            query: None,
        }
    }
}
#[derive(Debug, Serialize)]
struct CollectionInfo {
    name: String,
    internal_name: String,
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

/// Query params for collection creation — supports optional TTL.
#[derive(Debug, Deserialize)]
pub struct CreateCollectionQuery {
    /// TTL in seconds for keys in this collection.
    /// If omitted, keys never expire.
    pub ttl: Option<u64>,
}

pub async fn exists(name: CollectionPath, db: Data<RocksDB>) -> HttpResponse {
    if db.cf_exists(&name) {
        HttpResponse::Ok().finish()
    } else {
        HttpResponse::NotFound().finish()
    }
}

pub async fn create(
    name: CollectionPath,
    req: HttpRequest,
    query: Query<CreateCollectionQuery>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let collection_name = name.internal_collection();
    if db.cf_exists(collection_name) {
        Ok(HttpResponse::Conflict().json(format!("Collection {name} already exists")))
    } else {
        let timestamp = Utc::now().to_rfc3339();

        let secret_key = match req.extensions().get::<SecretKey>() {
            Some(secret) => secret.0.clone(),
            None => {
                return Err(ApiError::internal(
                    "Secret key not found in request extensions",
                    "Authentication middleware may not be configured correctly",
                ));
            }
        };

        // Create CF with or without TTL
        if let Some(ttl_secs) = query.ttl {
            db.create_cf_with_ttl(collection_name, ttl_secs)
                .map_err(|e| ApiError::internal("Failed to create collection with TTL", e))?;
            log::info!(
                "Created collection '{}' with TTL of {}s",
                name.user_collection(),
                ttl_secs
            );
        } else {
            db.create_cf(collection_name)
                .map_err(|e| ApiError::internal("Failed to create collection", e))?;
        }

        if let Err(e) = db.create_cf(&format!("{collection_name}-backups")) {
            log::error!("Failed to create backups collection: {}", e);
        }

        let secret = create_secret(&secret_key, timestamp);

        db.insert_cf(SECRETS_CF, collection_name, &secret)
            .map_err(|e| match e {
                KvStoreError::InvalidColumnFamily(_) => Ok(HttpResponse::NotFound().finish()),
                _ => Err(ApiError::internal("Failed to insert item", e)),
            })
            .unwrap();

        db.insert_cf(SECRETS_CF, &format!("{collection_name}-backups"), &secret)
            .map_err(|e| match e {
                KvStoreError::InvalidColumnFamily(_) => Ok(HttpResponse::NotFound().finish()),
                _ => Err(ApiError::internal("Failed to insert item", e)),
            })
            .unwrap();

        Ok(HttpResponse::Created().json(CollectionCreatedResponse {
            message: format!("Collection {} created", name.user_collection()),
            secret_key,
        }))
    }
}

pub async fn drop(collection: CollectionPath, db: Data<RocksDB>) -> Result<HttpResponse, ApiError> {
    let user_collection = collection.user_collection();
    let internal_collection = collection.internal_collection();

    if !db.cf_exists(internal_collection) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {user_collection} does not exist"))
        );
    };

    if user_collection.contains("-backups") {
        let base_collection = &internal_collection.replace("-backups", "");
        if db.cf_exists(base_collection) {
            Ok(HttpResponse::Ok().json(format!(
                "Can't delete backups collection without deleting {user_collection} collection first"
            )))
        } else {
            db.drop_cf(internal_collection)
                .map_err(|e| ApiError::internal("Failed to drop collection", e))?;

            Ok(HttpResponse::Ok().json(format!("Collection {user_collection} deleted")))
        }
    } else {
        let response = if db.cf_exists(&format!("{}-backups", collection.internal_collection())) {
            format!("Collection {user_collection} deleted. Your backups are still available at collection {user_collection}-backups. Make a DELETE request to that collection to remove them")
        } else {
            format!("Collection {user_collection} deleted")
        };
        db.drop_cf(collection.internal_collection())
            .map_err(|e| ApiError::internal("Failed to drop collection", e))?;
        Ok(HttpResponse::Ok().json(response))
    }
}

/// Compact a collection to trigger TTL cleanup.
pub async fn compact(
    collection: CollectionPath,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let internal_collection = collection.internal_collection();
    if !db.cf_exists(internal_collection) {
        return Ok(HttpResponse::NotFound().json(format!(
            "Collection {} does not exist",
            collection.user_collection()
        )));
    }

    db.compact_cf(internal_collection)
        .map_err(|e| ApiError::internal("Failed to compact collection", e))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": format!("Collection {} compacted", collection.user_collection())
    })))
}

/// Get the size of a collection.
pub async fn size(collection: CollectionPath, db: Data<RocksDB>) -> Result<HttpResponse, ApiError> {
    let internal_collection = collection.internal_collection();
    if !db.cf_exists(internal_collection) {
        return Ok(HttpResponse::NotFound().json(format!(
            "Collection {} does not exist",
            collection.user_collection()
        )));
    }

    let cf_size = db
        .get_cf_size(internal_collection)
        .map_err(|e| ApiError::internal("Failed to get collection size", e))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "collection": collection.user_collection(),
        "total_mb": cf_size.total_mb(),
        "sst_bytes": cf_size.sst_bytes,
        "mem_table_bytes": cf_size.mem_table_bytes,
        "blob_bytes": cf_size.blob_bytes,
    })))
}

/// List all collections belonging to the caller's namespace.
///
/// `GET /api/_collections` with `X-SECRET-KEY` header.
///
/// Scans the secrets CF for entries whose internal name starts with the
/// caller's namespace hash prefix, then returns the user-facing names.
pub async fn list_collections(
    req: HttpRequest,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let secret_key = match req
        .headers()
        .get(AUTH_HEADER_NAME)
        .and_then(|h| h.to_str().ok())
    {
        Some(key) => key.to_string(),
        None => return Ok(HttpResponse::Ok().json(Vec::<CollectionInfo>::new())),
    };

    // Same hash as namespace middleware uses during creation
    let namespace = hash_collection_namespace(&secret_key);
    let prefix = format!("{}-", namespace);

    // Scan all entries in the secrets CF
    let all_entries: Vec<serde_json::Value> = db
        .get_range_cf(
            SECRETS_CF,
            "",
            "\u{fff0}",
            usize::MAX,
            Direction::Forward,
            true,
        )
        .map_err(|e| ApiError::internal("Failed to scan collections", e))?;

    let mut collections = Vec::new();

    for entry in &all_entries {
        if let Some(key) = entry.get("key").and_then(|k| k.as_str()) {
            // Match entries whose key starts with our namespace prefix
            if let Some(user_name) = key.strip_prefix(&prefix) {
                // Skip backup CFs and metadata entries
                if user_name.ends_with("-backups") || key.starts_with("_cf_meta:") {
                    continue;
                }

                // Verify this collection actually exists as a CF
                if db.cf_exists(key) {
                    collections.push(CollectionInfo {
                        name: user_name.to_string(),
                        internal_name: key.to_string(),
                    });
                }
            }
        }
    }

    Ok(HttpResponse::Ok().json(collections))
}

pub async fn list(
    collection: CollectionPath,
    query: Query<RangeQuery>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    if !db.cf_exists(collection.internal_collection()) {
        return Ok(HttpResponse::NotFound().finish());
    }

    let from = query.from.as_deref().unwrap_or("");
    let to = query.to.as_deref().unwrap_or("\u{fff0}");
    let limit = query.limit.unwrap_or(usize::MAX);
    let direction = match query.order.clone().unwrap_or(SortOrder::Ascending) {
        SortOrder::Ascending => Direction::Forward,
        SortOrder::Descending => Direction::Reverse,
    };

    // New API: get_range_cf takes include_keys as last param and returns Vec<Value>
    let result = db
        .get_range_cf(&collection, from, to, limit, direction, query.keys)
        .map_err(|e| ApiError::internal("Failed to fetch items", e))?;

    Ok(HttpResponse::Ok().json(result))
}

pub async fn query(
    collection: CollectionPath,
    query: Json<RangeQuery>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    if !db.cf_exists(&collection) {
        return Ok(HttpResponse::NotFound().finish());
    }

    let result = if let Some(query_str) = &query.query {
        // New API: query_cf takes include_keys as last param
        db.query_cf(&collection, query_str, query.keys)
            .map_err(|e| ApiError::internal("Failed to execute query", e))?
    } else {
        let from = query.from.as_deref().unwrap_or("");
        let to = query.to.as_deref().unwrap_or("\u{fff0}");
        let limit = query.limit.unwrap_or(usize::MAX);
        let direction = query
            .order
            .clone()
            .map(Into::into)
            .unwrap_or(Direction::Forward);

        db.get_range_cf(&collection, from, to, limit, direction, query.keys)
            .map_err(|e| ApiError::internal("Failed to execute range query", e))?
    };

    Ok(HttpResponse::Ok().json(result))
}

pub async fn create_batch(
    path: CollectionPath,
    db: Data<RocksDB>,
    sub_manager: Data<Arc<SubscriptionManager>>,
    body: Bytes,
) -> Result<HttpResponse, ApiError> {
    let collection = path.internal_collection();
    let items: Vec<BatchItem> = match serde_json::from_slice(&body) {
        Ok(items) => items,
        Err(_) => return Ok(HttpResponse::BadRequest().json("Invalid JSON batch format")),
    };

    let batch_items: Vec<(&str, &Value)> = items
        .iter()
        .map(|item| (item.key.as_str(), &item.value))
        .collect();

    match db.batch_insert_cf(collection, &batch_items) {
        Ok(_) => {
            for item in &items {
                let event = CollectionEvent {
                    operation: Operation::Create,
                    key: item.key.clone(),
                    value: item.value.clone(),
                };
                sub_manager.publish(collection, event).await;
            }
            Ok(HttpResponse::Created().json(items))
        }
        Err(KvStoreError::InvalidColumnFamily(_)) => Ok(HttpResponse::NotFound().finish()),
        Err(e) => Err(ApiError::internal("Failed to insert batch", e)),
    }
}

pub async fn subscribe(
    path: CollectionPath,
    sub_manager: Data<Arc<SubscriptionManager>>,
) -> Result<HttpResponse, ApiError> {
    let internal_collection = path.internal_collection().to_string();
    let user_collection = path.user_collection().to_string();
    let sender = sub_manager
        .get_or_create_channel(&internal_collection)
        .await;
    let mut receiver = sender.subscribe();

    log::info!(
        "New subscriber connected to collection '{}'",
        internal_collection
    );

    let stream = async_stream::stream! {
        let init_message = serde_json::json!({"type": "connected", "collection": user_collection});
        let sse_msg = format!("data: {}\n\n", serde_json::to_string(&init_message).unwrap_or_default());
        yield Ok::<_, actix_web::Error>(Bytes::from(sse_msg));

        loop {
            match receiver.recv().await {
                Ok(event) => {
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();

                    let mut event_json = serde_json::to_value(&event)?;
                    if let Some(value) = event_json.get_mut("value") {
                        if let Some(obj) = value.as_object_mut() {
                            obj.insert("serverTime".to_string(), serde_json::json!(timestamp));
                        }
                    }

                    let msg = format!("data: {}\n\n", serde_json::to_string(&event_json).unwrap_or_default());
                    log::debug!("Sending SSE message: {}", msg);
                    yield Ok::<_, actix_web::Error>(Bytes::from(msg));
                },
                Err(e) => {
                    log::error!("Error receiving from broadcast channel: {:?}", e);
                    match e {
                        tokio::sync::broadcast::error::RecvError::Lagged(n) => {
                            log::warn!("Receiver lagged and missed {} messages", n);
                            continue;
                        },
                        tokio::sync::broadcast::error::RecvError::Closed => {
                            log::error!("Broadcast channel was closed");
                            break;
                        }
                    }
                }
            }
        }

        log::info!("SSE stream closed for collection '{}'", internal_collection);
    };

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream))
}
