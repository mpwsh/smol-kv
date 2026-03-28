use crate::{
    auth::{self, extract_secret_key, InternalCollection, SecretKey},
    error::ApiError,
    kv::RocksDB,
};
use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    error::Error,
    middleware::Next,
    HttpMessage,
};
use std::ops::Deref;

#[derive(Clone, Debug)]
pub struct CollectionAuth {
    name: String,
}

impl Deref for CollectionAuth {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.name
    }
}

impl CollectionAuth {
    fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

pub async fn require_auth(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    // Skip non-API routes
    if req.path().starts_with("/benchmark") || req.path().starts_with("/backups/") {
        return next.call(req).await;
    }

    let path_segments: Vec<&str> = req.path().split('/').collect();
    if path_segments.len() < 3 {
        return Err(ApiError::unauthorized("Invalid path").into());
    }

    let collection_name = path_segments[2];

    // Public endpoints — no auth needed
    let is_public = match (req.method().as_str(), collection_name, path_segments.get(3)) {
        ("PUT", _, None) => true,                      // Collection creation
        (_, name, _) if name.starts_with('_') => true, // System endpoints
        _ => false,
    };

    if is_public {
        return next.call(req).await;
    }

    // ── Auth required ────────────────────────────────────────────────────

    let db = req
        .app_data::<actix_web::web::Data<RocksDB>>()
        .ok_or_else(|| ApiError::internal("Database not found", "missing database"))?;

    let admin_token = req
        .app_data::<actix_web::web::Data<String>>()
        .ok_or_else(|| ApiError::internal("Admin token not found", "missing token"))?;

    // Get the key — already extracted by namespace middleware into extensions,
    // or extract fresh from header/query param
    let secret_key = req
        .extensions()
        .get::<SecretKey>()
        .map(|k| k.0.clone())
        .or_else(|| extract_secret_key(&req));

    // Get the internal collection name — set by namespace middleware
    let internal_collection = req
        .extensions()
        .get::<InternalCollection>()
        .map(|ic| ic.0.clone())
        .unwrap_or_else(|| {
            // Fallback: derive it here (shouldn't happen if namespace middleware ran)
            if let Some(ref key) = secret_key {
                let namespace = crate::namespace::hash_collection_namespace(key);
                format!("{}-{}", namespace, collection_name)
            } else {
                collection_name.to_string()
            }
        });

    // Check auth: admin token OR matching collection secret
    let is_authenticated = auth::verify_admin_token(req.headers(), admin_token)
        || auth::verify_collection_secret(secret_key.as_deref(), db, &internal_collection)?;

    if is_authenticated {
        req.extensions_mut()
            .insert(CollectionAuth::new(collection_name));
        next.call(req).await
    } else {
        Err(ApiError::unauthorized("Unauthorized access").into())
    }
}
