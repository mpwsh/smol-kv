use crate::{
    error::ApiError,
    kv::{KVStore, KvStoreError, RocksDB},
};
use actix_web::{
    web::{Data, Path},
    HttpResponse,
};
use bytes::Bytes;
use serde_json::Value;

pub async fn head(
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

pub async fn post(
    path: Path<(String, String)>,
    db: Data<RocksDB>,
    body: Bytes,
) -> Result<HttpResponse, ApiError> {
    let (collection, key) = path.into_inner();

    let obj = match serde_json::from_slice::<Value>(&body) {
        Ok(obj) => obj,
        Err(_) => return Ok(HttpResponse::BadRequest().finish()),
    };

    match db.insert_cf(&collection, &key, &obj) {
        Ok(_) => Ok(HttpResponse::Ok().json(obj)),
        Err(KvStoreError::InvalidColumnFamily(_)) => Ok(HttpResponse::NotFound().finish()),
        Err(e) => Err(ApiError::internal("Failed to insert item", e)),
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
