use crate::kv::{KVStore, KvStoreError, RocksDB};

use actix_web::{
    web::{Data, Path},
    HttpResponse, Responder,
};
use bytes::Bytes;
use serde_json::{json, Value};

pub async fn head(path: Path<(String, String)>, db: Data<RocksDB>) -> impl Responder {
    let (collection, key) = path.into_inner();
    match db.get_cf::<Value>(&collection, &key) {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(KvStoreError::KeyNotFound(_)) => HttpResponse::NotFound().finish(),
        Err(KvStoreError::InvalidColumnFamily(_)) => HttpResponse::NotFound().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

pub async fn get(path: Path<(String, String)>, db: Data<RocksDB>) -> HttpResponse {
    let (collection, key) = path.into_inner();
    match db.get_cf::<Value>(&collection, &key) {
        Ok(timestamped_value) => HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&timestamped_value).unwrap()),
        Err(KvStoreError::KeyNotFound(_)) => HttpResponse::NotFound()
            .content_type("application/json")
            .body(json!({ "error": "Item not found" }).to_string()),
        Err(KvStoreError::InvalidColumnFamily(_)) => HttpResponse::NotFound()
            .content_type("application/json")
            .body(json!({ "error": "Collection not found" }).to_string()),
        Err(_) => HttpResponse::InternalServerError()
            .content_type("application/json")
            .body(json!({ "error": "Internal server error" }).to_string()),
    }
}

pub async fn post(path: Path<(String, String)>, db: Data<RocksDB>, body: Bytes) -> HttpResponse {
    let (collection, key) = path.into_inner();
    match serde_json::from_slice::<Value>(&body) {
        Ok(obj) => match db.insert_cf(&collection, &key, &obj) {
            Ok(_) => HttpResponse::Ok()
                .content_type("application/json")
                .body(obj.to_string()),
            Err(KvStoreError::InvalidColumnFamily(_)) => HttpResponse::NotFound()
                .content_type("application/json")
                .body(json!({ "error": "Collection not found" }).to_string()),
            Err(_) => HttpResponse::InternalServerError()
                .content_type("application/json")
                .finish(),
        },
        Err(_) => HttpResponse::BadRequest()
            .content_type("application/json")
            .body(
                json!({ "status": 400, "msg": "Parsing failed. Value is not in JSON Format"})
                    .to_string(),
            ),
    }
}

pub async fn delete(path: Path<(String, String)>, db: Data<RocksDB>) -> HttpResponse {
    let (collection, key) = path.into_inner();
    match db.delete_cf(&collection, &key) {
        Ok(_) => HttpResponse::Ok()
            .content_type("application/json")
            .body(json!({ "message": "Item deleted successfully" }).to_string()),
        Err(KvStoreError::KeyNotFound(_)) => HttpResponse::NotFound()
            .content_type("application/json")
            .body(json!({ "error": "Item not found" }).to_string()),
        Err(KvStoreError::InvalidColumnFamily(_)) => HttpResponse::NotFound()
            .content_type("application/json")
            .body(json!({ "error": "Collection not found" }).to_string()),
        Err(_) => HttpResponse::InternalServerError()
            .content_type("application/json")
            .finish(),
    }
}
