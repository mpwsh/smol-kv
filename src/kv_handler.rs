use crate::kv::{KVStore, RocksDB};
use actix_web::{
    web::{Data, Path},
    HttpResponse,
};
use bytes::Bytes;
use serde_json::{json, Value};

pub async fn get(key: Path<String>, db: Data<RocksDB>) -> HttpResponse {
    match &db.find(&key.into_inner()) {
        Some(v) => serde_json::from_str(&v)
            .map(|obj: Value| {
                HttpResponse::Ok()
                    .content_type("application/json")
                    .body(obj.to_string())
            })
            .unwrap_or(
                HttpResponse::InternalServerError()
                    .content_type("application/json")
                    .finish(),
            ),
        None => HttpResponse::NotFound()
            .content_type("application/json")
            .finish(),
    }
}

pub async fn post(key: Path<String>, db: Data<RocksDB>, body: Bytes) -> HttpResponse {
    serde_json::from_slice(&body.slice(..))
        .map(|obj: Value| {
            if db.save(&key.into_inner(), &obj.to_string()) {
                HttpResponse::Ok()
                    .content_type("application/json")
                    .body(obj.to_string())
            } else {
                HttpResponse::InternalServerError()
                    .content_type("application/json")
                    .finish()
            }
        })
        .unwrap_or(
            HttpResponse::BadRequest()
                .content_type("application/json")
                .body(
                    json!({ "status": 400, "msg": "Parsing failed. value is not in JSON Format"})
                        .to_string(),
                ),
        )
}

pub async fn delete(key: Path<String>, db: Data<RocksDB>) -> HttpResponse {
    match &db.delete(&key.into_inner()) {
        true => HttpResponse::Ok().content_type("application/json").finish(),
        false => HttpResponse::InternalServerError()
            .content_type("application/json")
            .finish(),
    }
}
