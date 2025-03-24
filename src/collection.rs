use crate::{
    auth::*,
    error::ApiError,
    key::Operation,
    kv::{Direction, KVStore, KvStoreError, RocksDB},
    namespace::CollectionPath,
    sub::*,
    SECRETS_CF,
};

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use actix_web::{
    web::{Data, Json, Query},
    HttpMessage, HttpRequest, HttpResponse,
};
use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let collection_name = name.internal_collection();
    if db.cf_exists(collection_name) {
        Ok(HttpResponse::Conflict().json(format!("Collection {name} already exists")))
    } else {
        let timestamp = Utc::now().to_rfc3339();

        // Get the secret key that was set by the middleware
        let secret_key = match req.extensions().get::<SecretKey>() {
            Some(secret) => secret.0.clone(),
            None => {
                // This should rarely happen if middleware is working correctly
                return Err(ApiError::internal(
                    "Secret key not found in request extensions",
                    "Authentication middleware may not be configured correctly",
                ));
            }
        };
        db.create_cf(collection_name)
            .map_err(|e| ApiError::internal("Failed to create collection", e))?;

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

fn execute_range_query<T: DeserializeOwned + Serialize>(
    db: &RocksDB,
    collection: &str,
    range_query: &RangeQuery,
) -> Result<Value, KvStoreError> {
    let from = range_query.from.as_deref().unwrap_or("");
    let to = range_query.to.as_deref().unwrap_or("\u{fff0}");
    let limit = range_query.limit.unwrap_or(usize::MAX);
    let direction = range_query
        .order
        .clone()
        .map(Into::into)
        .unwrap_or(Direction::Forward);

    // Use the appropriate method based on the keys flag
    let result = if range_query.keys {
        let items = db.get_range_cf_with_keys::<T>(collection, from, to, limit, direction)?;
        serde_json::to_value(items).map_err(|e| KvStoreError::SerializationError(e.to_string()))?
    } else {
        let items = db.get_range_cf::<T>(collection, from, to, limit, direction)?;
        serde_json::to_value(items).map_err(|e| KvStoreError::SerializationError(e.to_string()))?
    };

    Ok(result)
}

// Same for JSONPath queries
fn execute_query<T: DeserializeOwned + Serialize>(
    db: &RocksDB,
    collection: &str,
    query_str: &str,
    include_keys: bool,
) -> Result<Value, KvStoreError> {
    let result = if include_keys {
        let items = db.query_cf_with_keys::<T>(collection, query_str)?;
        serde_json::to_value(items).map_err(|e| KvStoreError::SerializationError(e.to_string()))?
    } else {
        let items = db.query_cf::<T>(collection, query_str)?;
        serde_json::to_value(items).map_err(|e| KvStoreError::SerializationError(e.to_string()))?
    };

    Ok(result)
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

    // Convert the results to serde_json::Value to handle the type difference
    let result = if query.keys {
        // With keys (key-value pairs)
        let items = db
            .get_range_cf_with_keys::<Value>(&collection, from, to, limit, direction)
            .map_err(|e| ApiError::internal("Failed to fetch items with keys", e))?;

        serde_json::to_value(items)
            .map_err(|e| ApiError::internal("Failed to serialize items", e))?
    } else {
        // Without keys (values only)
        let items = db
            .get_range_cf::<Value>(&collection, from, to, limit, direction)
            .map_err(|e| ApiError::internal("Failed to fetch items", e))?;

        serde_json::to_value(items)
            .map_err(|e| ApiError::internal("Failed to serialize items", e))?
    };

    Ok(HttpResponse::Ok().json(result))
}

// With helpers, the endpoints become much cleaner:
pub async fn query(
    collection: CollectionPath,
    query: Json<RangeQuery>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    if !db.cf_exists(&collection) {
        return Ok(HttpResponse::NotFound().finish());
    }

    if !db.cf_exists(&collection) {
        return Ok(HttpResponse::NotFound().finish());
    }

    let result = if let Some(query_str) = &query.query {
        // If a JSONPath query is provided, use it
        execute_query::<Value>(&db, &collection, query_str, query.keys)
            .map_err(|e| ApiError::internal("Failed to execute query", e))?
    } else {
        // Otherwise, perform a range query
        execute_range_query::<Value>(&db, &collection, &query)
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
            // Notify subscribers for each item in the batch
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
    let internal_collection = path.internal_collection().to_string(); // Clone to own the string
    let user_collection = path.user_collection().to_string(); // Clone to own the string
    let sender = sub_manager
        .get_or_create_channel(&internal_collection)
        .await;
    let mut receiver = sender.subscribe();

    // Log that a new subscriber connected
    log::info!(
        "New subscriber connected to collection '{}'",
        internal_collection
    );

    let stream = async_stream::stream! {
        // Send initial connection message
        let init_message = serde_json::json!({"type": "connected", "collection": user_collection});
        let sse_msg = format!("data: {}\n\n", serde_json::to_string(&init_message).unwrap_or_default());
        yield Ok::<_, actix_web::Error>(Bytes::from(sse_msg));

        loop {
            match receiver.recv().await {
                Ok(event) => {
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

                    // Format as proper SSE message with data: prefix and double newline
                    let msg = format!("data: {}\n\n", serde_json::to_string(&event_json).unwrap_or_default());
                    log::debug!("Sending SSE message: {}", msg);
                    yield Ok::<_, actix_web::Error>(Bytes::from(msg));
                },
                Err(e) => {
                    log::error!("Error receiving from broadcast channel: {:?}", e);
                    // For lagged errors, we can continue
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

        // This message won't be sent because we're breaking out of the loop,
        // but it's here to show that the stream ending is expected
        log::info!("SSE stream closed for collection '{}'", internal_collection);
    };

    // Set proper headers for SSE
    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("X-Accel-Buffering", "no")) // Disable proxy buffering
        .streaming(stream))
}
