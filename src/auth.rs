use crate::error::ApiError;
use crate::kv::RocksDB;
use crate::SECRETS_CF;
use actix_web::dev::ServiceRequest;
use ring::digest;
use serde::{Deserialize, Serialize};

// ── Types set by middleware ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct InternalCollection(pub String);

#[derive(Debug, Clone)]
pub struct SecretKey(pub String);

pub const AUTH_HEADER_NAME: &str = "X-SECRET-KEY";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Secret {
    pub created_at: String,
    pub secret: String,
}

// ── Key resolution ───────────────────────────────────────────────────────────
// One function to extract the secret key from a request.
// Checks header first, then falls back to `?key=` query param.

pub fn extract_secret_key(req: &ServiceRequest) -> Option<String> {
    // Try header first
    req.headers()
        .get(AUTH_HEADER_NAME)
        .and_then(|h| h.to_str().ok())
        .map(String::from)
        .or_else(|| {
            // Fall back to query param
            req.query_string()
                .split('&')
                .find_map(|pair| pair.strip_prefix("key=").map(String::from))
        })
}

// ── Verification ─────────────────────────────────────────────────────────────

pub fn verify_admin_token(headers: &actix_web::http::header::HeaderMap, admin_token: &str) -> bool {
    headers
        .get("X-ADMIN-TOKEN")
        .and_then(|token| token.to_str().ok())
        .map(|token| token == admin_token)
        .unwrap_or(false)
}

/// Verify that the given secret key matches the stored secret for a collection.
/// Takes the key directly instead of re-reading from headers.
pub fn verify_collection_secret(
    secret_key: Option<&str>,
    db: &RocksDB,
    internal_collection: &str,
) -> Result<bool, ApiError> {
    let secret_key = match secret_key {
        Some(key) => key,
        None => return Ok(false),
    };

    let stored_secret = db
        .get_cf::<Secret>(SECRETS_CF, internal_collection)
        .unwrap_or_default();

    let input_hash = hash_secret_key(secret_key);
    Ok(stored_secret.secret == input_hash)
}

pub fn hash_secret_key(secret_key: &str) -> String {
    let hash = digest::digest(&digest::SHA256, secret_key.as_bytes());
    hex::encode(hash.as_ref())
}

pub fn create_secret(secret_key: &str, timestamp: String) -> Secret {
    Secret {
        created_at: timestamp,
        secret: hash_secret_key(secret_key),
    }
}
