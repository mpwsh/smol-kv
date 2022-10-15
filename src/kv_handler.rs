use actix_web::{
    web::{Data, Path},
    HttpResponse,
};
use bytes::Bytes;
use crate::kv::{KVStore, RocksDB};
use serde_json::Value;

// curl -i -X GET -H "Content-Type: application/json" http://localhost:8080/api/foo
pub async fn get(key: Path<String>, db: Data<RocksDB>) -> HttpResponse {
    match &db.find(&key.into_inner()) {
        Some(v) => {
            serde_json::from_str(&v)
                .map(|obj: Value|{
                    HttpResponse::Ok()
                        .content_type("application/json")
                        .body(obj.to_string())
                })
                .unwrap_or(
                    HttpResponse::InternalServerError()
                        .content_type("application/json")
                        .finish(),
                )
        }
        None => HttpResponse::NotFound()
            .content_type("application/json")
            .finish(),
    }
}

// curl -i -X POST -H "Content-Type: application/json" -d '{"bar":"baz"}' http://localhost:8080/api/foo
pub async fn post(key: Path<String>, db: Data<RocksDB>, body: Bytes) -> HttpResponse {
    match String::from_utf8(body.to_vec()) {
        Ok(body) => match &db.save(&key.into_inner(), &body) {
            true =>
                serde_json::from_str(&body)
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
            false => HttpResponse::InternalServerError()
                .content_type("application/json")
                .finish(),
        },
        Err(_) => HttpResponse::InternalServerError()
            .content_type("application/json")
            .finish(),
    }
}

// curl -i -X DELETE -H "Content-Type: application/json" http://localhost:8080/api/foo
pub async fn delete(key: Path<String>, db: Data<RocksDB>) -> HttpResponse {
    match &db.delete(&key.into_inner()) {
        true => HttpResponse::NoContent()
            .content_type("application/json")
            .finish(),
        false => HttpResponse::InternalServerError()
            .content_type("application/json")
            .finish(),
    }
}
