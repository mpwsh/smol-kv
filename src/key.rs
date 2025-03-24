use crate::{
    error::ApiError,
    kv::{KVStore, KvStoreError, RocksDB},
    namespace::CollectionPath,
    sub::*,
};

use actix_multipart::Multipart;
use actix_web::{
    web::{Data, Query},
    HttpResponse,
};
use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt::{self, Display},
    sync::Arc,
    time::Duration,
};

#[derive(Serialize, Clone, Debug)]
pub enum Operation {
    Create,
    Update,
    Delete,
}

#[derive(Deserialize)]
pub struct ImportQuery {
    key: Option<String>,
}

#[derive(Serialize)]
struct ImportResponse {
    message: String,
    imported_count: usize,
    collection: String,
    errors: Option<Vec<String>>,
}
impl Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operation::Create => write!(f, "create"),
            Operation::Update => write!(f, "update"),
            Operation::Delete => write!(f, "delete"),
        }
    }
}
pub async fn exists(
    collection_path: CollectionPath,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let key = collection_path
        .path_key()
        .ok_or_else(ApiError::missing_key)?;

    match db.get_cf::<Value>(&collection_path, key) {
        Ok(_) => Ok(HttpResponse::Ok().finish()),
        Err(KvStoreError::KeyNotFound(_)) | Err(KvStoreError::InvalidColumnFamily(_)) => {
            Ok(HttpResponse::NotFound().finish())
        }
        Err(e) => Err(ApiError::internal("Storage error", e)),
    }
}

pub async fn get(
    collection_path: CollectionPath,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let key = collection_path
        .path_key()
        .ok_or_else(ApiError::missing_key)?;

    match db.get_cf::<Value>(&collection_path, key) {
        Ok(value) => Ok(HttpResponse::Ok().json(value)),
        Err(KvStoreError::KeyNotFound(_)) | Err(KvStoreError::InvalidColumnFamily(_)) => {
            Ok(HttpResponse::NotFound().finish())
        }
        Err(e) => Err(ApiError::internal("Failed to get item", e)),
    }
}
pub async fn import_values(
    collection_path: CollectionPath,
    db: Data<RocksDB>,
    sub_manager: Data<Arc<SubscriptionManager>>,
    query: Query<ImportQuery>,
    mut payload: Multipart,
) -> Result<HttpResponse, ApiError> {
    let internal_collection = collection_path.internal_collection().to_string();
    let user_collection = collection_path.user_collection().to_string();

    // Check if collection exists
    if !db.cf_exists(&internal_collection) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", user_collection))
        );
    }

    // Process the file upload
    let mut imported_count = 0;
    let mut errors = Vec::new();
    let mut all_notifications = Vec::new();

    // Handle file upload
    while let Ok(Some(mut field)) = payload.try_next().await {
        if field.name() == Some("file") {
            // Collect all file data
            let mut data = Vec::new();
            while let Some(chunk) = field.next().await {
                data.extend_from_slice(
                    &chunk.map_err(|e| ApiError::internal("Failed to read upload", e))?,
                );
            }

            // Parse JSON data
            let json_data = match serde_json::from_slice::<Value>(&data) {
                Ok(value) => value,
                Err(e) => {
                    return Ok(
                        HttpResponse::BadRequest().json(format!("Invalid JSON format: {}", e))
                    );
                }
            };

            // Check if the data is an array
            if !json_data.is_array() {
                return Ok(HttpResponse::BadRequest().json("Expected a JSON array of objects"));
            }

            let items = json_data.as_array().unwrap();
            let key_field = query.key.as_deref();

            log::info!(
                "Starting import of {} items to collection '{}'",
                items.len(),
                user_collection
            );

            // Determine batch size for database operations
            let batch_size = if items.len() < 5000 {
                items.len() // If less than 500 items, just do one batch
            } else {
                5000 // Otherwise use batches of 500
            };

            // Process items in batches for database operations (no delay here)
            for chunk in items.chunks(batch_size) {
                let mut batch_items = Vec::with_capacity(chunk.len());

                // Prepare the batch
                for (index, item) in chunk.iter().enumerate() {
                    let absolute_index = imported_count + index;

                    if !item.is_object() {
                        errors.push(format!(
                            "Item at position {} is not an object",
                            absolute_index
                        ));
                        continue;
                    }

                    // Generate or extract key
                    let key = match key_field {
                        Some(field) => {
                            if let Some(key_value) = get_nested_value(item, field) {
                                // Use the specified field as the key if it exists and is a string or number
                                match key_value {
                                    Value::String(s) => s.clone(),
                                    Value::Number(n) => n.to_string(),
                                    _ => {
                                        // If key field exists but is not a string or number, generate a key
                                        errors.push(format!(
                                            "Key field '{}' at position {} is not a string or number",
                                            field, absolute_index
                                        ));
                                        format!("item_{}", absolute_index + 1)
                                    }
                                }
                            } else {
                                // If key field doesn't exist, generate a key
                                errors.push(format!(
                                    "Key field '{}' not found in item at position {}",
                                    field, absolute_index
                                ));
                                format!("item_{}", absolute_index + 1)
                            }
                        }
                        None => {
                            // If no key field specified, generate a key
                            format!("item_{}", absolute_index + 1)
                        }
                    };

                    // Add to batch
                    batch_items.push((key.clone(), item));

                    // Store notification for later
                    let event = CollectionEvent {
                        operation: Operation::Create,
                        key,
                        value: item.clone(),
                    };
                    all_notifications.push(event);
                }

                // Execute the batch insert (no delay)
                if !batch_items.is_empty() {
                    // Convert to the format expected by batch_insert_cf
                    let insert_items: Vec<(&str, &Value)> = batch_items
                        .iter()
                        .map(|(key, value)| (key.as_str(), *value))
                        .collect();

                    match db.batch_insert_cf(&collection_path, &insert_items) {
                        Ok(_) => {
                            imported_count += batch_items.len();
                        }
                        Err(e) => {
                            errors.push(format!("Failed to insert batch: {}", e));
                        }
                    }
                }
            }
            break;
        }
    }

    if imported_count == 0 {
        return Ok(HttpResponse::BadRequest().json("No items were imported"));
    }

    // Now send notifications with appropriate delays
    log::info!(
        "Values import to collection {internal_collection} complete. Sending {} notifications to subscribers",
        all_notifications.len()
    );

    // Determine batch size for notifications
    let notification_batch_size = 200;
    let use_delay = all_notifications.len() >= 200;

    // Send notifications in batches with delays
    for (i, chunk) in all_notifications
        .chunks(notification_batch_size)
        .enumerate()
    {
        for event in chunk {
            sub_manager
                .publish(&collection_path.internal_collection, event.clone())
                .await;
        }

        // Add delay between notification batches (only for large imports)
        if use_delay && i < all_notifications.chunks(notification_batch_size).len() - 1 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    // Create response
    let response = ImportResponse {
        message: format!("Successfully imported {} items", imported_count),
        imported_count,
        collection: user_collection,
        errors: if errors.is_empty() {
            None
        } else {
            Some(errors)
        },
    };

    Ok(HttpResponse::Created().json(response))
}

// Function to get a value from a nested JSON path using dot notation
fn get_nested_value<'a>(obj: &'a Value, path: &str) -> Option<&'a Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = obj;

    for part in parts {
        match current.get(part) {
            Some(value) => current = value,
            None => return None,
        }
    }

    Some(current)
}

pub async fn create(
    collection_path: CollectionPath,
    db: Data<RocksDB>,
    sub_manager: Data<Arc<SubscriptionManager>>,
    body: Bytes,
) -> Result<HttpResponse, ApiError> {
    let key = collection_path
        .path_key()
        .ok_or_else(ApiError::missing_key)?;

    let obj = match serde_json::from_slice::<Value>(&body) {
        Ok(obj) => obj,
        Err(_) => {
            return Ok(
                HttpResponse::BadRequest().json("Parsing failed. value is not in JSON Format")
            )
        }
    };

    match db.insert_cf(&collection_path, key, &obj) {
        Ok(_) => {
            // Notify subscribers
            let event = CollectionEvent {
                operation: Operation::Create,
                key: key.to_string(),
                value: obj.clone(),
            };
            sub_manager
                .publish(&collection_path.internal_collection, event)
                .await;
            Ok(HttpResponse::Created().json(obj))
        }
        Err(KvStoreError::InvalidColumnFamily(_)) => Ok(HttpResponse::NotFound().finish()),
        Err(e) => Err(ApiError::internal("Failed to insert item", e)),
    }
}

pub async fn delete(
    collection_path: CollectionPath,
    sub_manager: Data<Arc<SubscriptionManager>>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let key = collection_path
        .path_key()
        .ok_or_else(ApiError::missing_key)?;

    match db.delete_cf(&collection_path, key) {
        Ok(_) => {
            let event = CollectionEvent {
                operation: Operation::Delete,
                key: key.to_string(),
                value: Value::Null,
            };
            sub_manager
                .publish(&collection_path.internal_collection, event)
                .await;
            Ok(HttpResponse::Ok().finish())
        }
        Err(KvStoreError::KeyNotFound(_)) | Err(KvStoreError::InvalidColumnFamily(_)) => {
            Ok(HttpResponse::NotFound().finish())
        }
        Err(e) => Err(ApiError::internal("Failed to delete item", e)),
    }
}
