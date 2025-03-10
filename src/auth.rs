use crate::error::ApiError;
use crate::kv::{KVStore, RocksDB};
use crate::SECRETS_CF;
use ring::digest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Secret {
    pub created_at: String,
    pub secret: String,
}
pub fn verify_admin_token(headers: &actix_web::http::header::HeaderMap, admin_token: &str) -> bool {
    headers
        .get("X-ADMIN-TOKEN")
        .and_then(|token| token.to_str().ok())
        .map(|token| token == admin_token)
        .unwrap_or(false)
}

pub fn verify_collection_secret(
    headers: &actix_web::http::header::HeaderMap,
    db: &RocksDB,
    collection_name: &str,
) -> Result<bool, ApiError> {
    let secret_key = match headers.get("X-SECRET-KEY") {
        Some(key) => key
            .to_str()
            .map_err(|_| ApiError::unauthorized("Invalid secret key"))?,
        None => return Ok(false),
    };

    let stored_secret = db
        .get_cf::<Secret>(SECRETS_CF, collection_name)
        .unwrap_or_default();

    let input_hash = hash_secret_key(secret_key);
    Ok(stored_secret.secret == input_hash)
}

pub fn hash_secret_key(secret_key: &str) -> String {
    let hash = digest::digest(&digest::SHA256, secret_key.as_bytes());
    hex::encode(hash.as_ref())
}
