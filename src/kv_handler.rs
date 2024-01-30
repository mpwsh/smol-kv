use crate::kv::{KVStore, RocksDB};
use rand::{distributions::Alphanumeric, Rng};

use actix_web::{
    web::{Data, Path},
    HttpRequest, HttpResponse, Responder,
};
use bytes::Bytes;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};

pub async fn head(key: Path<String>, db: Data<RocksDB>) -> impl Responder {
    match &db.find(&key.into_inner()) {
        Some(_) => HttpResponse::Ok().finish(),
        None => HttpResponse::NotFound().finish(),
    }
}

pub async fn get(key: Path<String>, db: Data<RocksDB>) -> HttpResponse {
    match &db.find(&key.into_inner()) {
        Some(v) => serde_json::from_str(v)
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

pub async fn benchmark(
    db: Data<RocksDB>,
    token: Data<String>,
    body: Bytes,
    req: HttpRequest,
) -> HttpResponse {
    let token_header = req
        .headers()
        .get("Authorization")
        .and_then(|hv| hv.to_str().ok());

    if token_header.is_none() || token_header != Some(token.get_ref().as_str()) {
        return HttpResponse::Unauthorized().finish();
    }
    // Generate a random string
    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    // Combine body and random string for SHA1 key
    let mut hasher = Sha1::new();
    hasher.update(&body);
    hasher.update(random_string.as_bytes());
    let result = hasher.finalize();
    let key = format!("{:x}", result);

    // Save data
    match serde_json::from_slice::<Value>(&body) {
        Ok(obj) => {
            if !db.save(&key, &obj.to_string()) {
                return HttpResponse::InternalServerError().finish();
            }

            // Read data
            match db.find(&key) {
                Some(_) => {
                    // Delete data
                    if !db.delete(&key) {
                        return HttpResponse::InternalServerError().finish();
                    }

                    // Respond with the key and data
                    HttpResponse::Ok()
                        .content_type("application/json")
                        .body(json!({ "key": key, "data": obj }).to_string())
                }
                None => HttpResponse::NotFound().finish(),
            }
        }
        Err(_) => HttpResponse::BadRequest()
            .content_type("application/json")
            .body(json!({ "status": 400, "msg": "Invalid JSON format"}).to_string()),
    }
}

pub async fn new(db: Data<RocksDB>, body: Bytes) -> impl Responder {
    let mut hasher = Sha1::new();
    hasher.update(&body);

    let result = hasher.finalize();
    let key = format!("{:x}", result);

    serde_json::from_slice(&body.slice(..))
        .map(|obj: Value| {
            if db.save(&key, &obj.to_string()) {
                HttpResponse::Ok()
                    .content_type("application/json")
                    .body(json!({ "key": key, "data": obj }).to_string())
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
