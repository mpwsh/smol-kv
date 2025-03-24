use crate::error::ApiError;
use crate::kv::{KVStore, RocksDB};
use crate::SECRETS_CF;
use ring::digest;
use serde::{Deserialize, Serialize};

// Information stored by middleware
#[derive(Debug, Clone)]
pub struct InternalCollection(pub String);

// Wrapper type for secret key
#[derive(Debug, Clone)]
pub struct SecretKey(pub String);

pub const AUTH_HEADER_NAME: &str = "X-SECRET-KEY";

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
    internal_collection: &str,
) -> Result<bool, ApiError> {
    // Extract the secret key from headers
    let secret_key = match headers.get(AUTH_HEADER_NAME) {
        Some(key) => key
            .to_str()
            .map_err(|_| ApiError::unauthorized("Invalid secret key"))?,
        None => return Ok(false),
    };
    // Use the internal collection name directly to fetch the stored secret
    let stored_secret = db
        .get_cf::<Secret>(SECRETS_CF, internal_collection)
        .unwrap_or_default();

    // Compare hashed input with stored secret
    let input_hash = hash_secret_key(secret_key);
    Ok(stored_secret.secret == input_hash)
}

pub fn hash_secret_key(secret_key: &str) -> String {
    let hash = digest::digest(&digest::SHA256, secret_key.as_bytes());
    hex::encode(hash.as_ref())
}

// Create a new secret object with hashed key
pub fn create_secret(secret_key: &str, timestamp: String) -> Secret {
    Secret {
        created_at: timestamp,
        secret: hash_secret_key(secret_key),
    }
}
