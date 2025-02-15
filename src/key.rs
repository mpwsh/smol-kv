use crate::{
    error::ApiError,
    kv::{KVStore, KvStoreError, RocksDB},
    sub::*,
};

use actix_web::{
    web::{Data, Path},
    HttpResponse,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Deserialize, Serialize)]
struct BatchItem {
    key: String,
    value: Value,
}

pub async fn exists(
    path: Path<(String, String)>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let (collection, key) = path.into_inner();
    match db.get_cf::<Value>(&collection, &key) {
        Ok(_) => Ok(HttpResponse::Ok().finish()),
        Err(KvStoreError::KeyNotFound(_)) | Err(KvStoreError::InvalidColumnFamily(_)) => {
            Ok(HttpResponse::NotFound().finish())
        }
        Err(e) => Err(ApiError::internal("Storage error", e)),
    }
}

pub async fn get(
    path: Path<(String, String)>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let (collection, key) = path.into_inner();
    match db.get_cf::<Value>(&collection, &key) {
        Ok(value) => Ok(HttpResponse::Ok().json(value)),
        Err(KvStoreError::KeyNotFound(_)) | Err(KvStoreError::InvalidColumnFamily(_)) => {
            Ok(HttpResponse::NotFound().finish())
        }
        Err(e) => Err(ApiError::internal("Failed to get item", e)),
    }
}

pub async fn create(
    path: Path<(String, String)>,
    db: Data<RocksDB>,
    sub_manager: Data<Arc<SubscriptionManager>>,
    body: Bytes,
) -> Result<HttpResponse, ApiError> {
    let (collection, key) = path.into_inner();
    let obj = match serde_json::from_slice::<Value>(&body) {
        Ok(obj) => obj,
        Err(_) => {
            return Ok(
                HttpResponse::BadRequest().body("Parsing failed. value is not in JSON Format")
            )
        }
    };

    match db.insert_cf(&collection, &key, &obj) {
        Ok(_) => {
            // Notify subscribers
            let event = CollectionEvent {
                operation: "create".to_string(),
                key: key.clone(),
                value: obj.clone(),
            };
            sub_manager.publish(&collection, event).await;
            Ok(HttpResponse::Created().json(obj))
        }
        Err(KvStoreError::InvalidColumnFamily(_)) => Ok(HttpResponse::NotFound().finish()),
        Err(e) => Err(ApiError::internal("Failed to insert item", e)),
    }
}

// Modified create_batch to notify subscribers
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

pub async fn delete(
    path: Path<(String, String)>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let (collection, key) = path.into_inner();
    match db.delete_cf(&collection, &key) {
        Ok(_) => Ok(HttpResponse::Ok().finish()),
        Err(KvStoreError::KeyNotFound(_)) | Err(KvStoreError::InvalidColumnFamily(_)) => {
            Ok(HttpResponse::NotFound().finish())
        }
        Err(e) => Err(ApiError::internal("Failed to delete item", e)),
    }
}
