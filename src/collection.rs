use crate::kv::{KVStore, KvStoreError, RocksDB};
use actix_web::{
    web::{Data, Path, Query},
    HttpResponse, Responder,
};
use log::{error, info};
use serde::Deserialize;

use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct RangeQuery {
    from: Option<String>,
    to: Option<String>,
}
pub async fn exists(name: Path<String>, db: Data<RocksDB>) -> impl Responder {
    if db.cf_exists(&name) {
        HttpResponse::Ok().finish()
    } else {
        HttpResponse::NotFound().finish()
    }
}

pub async fn create(name: Path<String>, db: Data<RocksDB>) -> impl Responder {
    info!("Received request to create collection: {}", name);
    match db.create_cf(&name) {
        Ok(_) => {
            info!("Collection '{}' created successfully", name);
            HttpResponse::Created().json(json!({
                "message": "Collection created successfully",
                "collection": name.to_string()
            }))
        }
        Err(KvStoreError::InvalidColumnFamily(_)) => {
            info!("Attempted to create existing collection: {}", name);
            HttpResponse::Conflict().json(json!({
                "error": "Collection already exists",
                "collection": name.to_string()
            }))
        }
        Err(e) => {
            error!("Failed to create collection '{}': {:?}", name, e);
            HttpResponse::InternalServerError().json(json!({
                "error": "Failed to create collection",
                "details": format!("{:?}", e),
                "collection": name.to_string()
            }))
        }
    }
}

pub async fn drop(name: Path<String>, db: Data<RocksDB>) -> HttpResponse {
    match db.drop_cf(&name) {
        Ok(_) => HttpResponse::Ok()
            .content_type("application/json")
            .body(json!({ "message": "Collection dropped successfully" }).to_string()),
        Err(KvStoreError::InvalidColumnFamily(_)) => HttpResponse::NotFound()
            .content_type("application/json")
            .body(json!({ "error": "Collection not found" }).to_string()),
        Err(_) => HttpResponse::InternalServerError()
            .content_type("application/json")
            .body(json!({ "error": "Failed to drop collection" }).to_string()),
    }
}

pub async fn list(
    collection: Path<String>,
    query: Query<RangeQuery>,
    db: Data<RocksDB>,
) -> HttpResponse {
    let collection = collection.into_inner();

    let from = query.from.as_deref().unwrap_or("");
    let to = query.to.as_deref().unwrap_or("\u{fff0}");
    match db.get_range_cf::<Value>(&collection, from, to) {
        Ok(items) => HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&items).unwrap()),
        Err(KvStoreError::InvalidColumnFamily(_)) => HttpResponse::NotFound()
            .content_type("application/json")
            .body(json!({ "error": "Collection not found" }).to_string()),
        Err(_) => HttpResponse::InternalServerError()
            .content_type("application/json")
            .body(json!({ "error": "Internal server error" }).to_string()),
    }
}
