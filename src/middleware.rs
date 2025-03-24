use crate::{
    auth::{self, InternalCollection, AUTH_HEADER_NAME},
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
    // Skip auth for benchmark endpoint
    if req.path().starts_with("/benchmark") || req.path().starts_with("/backups/") {
        return next.call(req).await;
    }

    // Parse the path properly
    let path_segments: Vec<&str> = req.path().split('/').collect();
    if path_segments.len() < 3 {
        return Err(ApiError::unauthorized("Invalid path").into());
    }

    // Is this a public endpoint? (collection creation or backup download)
    let is_public_endpoint = match (
        req.method().as_str(),
        path_segments.get(2),
        path_segments.get(3),
    ) {
        // PUT /api/{collection} - create collection
        ("PUT", Some(_), None) => true,
        // Other public endpoints...
        _ => false,
    };

    if is_public_endpoint {
        return next.call(req).await;
    }

    // For auth-required endpoints:
    let db = req
        .app_data::<actix_web::web::Data<RocksDB>>()
        .ok_or_else(|| ApiError::internal("Database not found", "missing database"))?;

    let admin_token = req
        .app_data::<actix_web::web::Data<String>>()
        .ok_or_else(|| ApiError::internal("Admin token not found", "missing token"))?;

    // Extract collection name from path
    let user_collection_name = path_segments[2];

    // Try to get the internal collection name from extensions
    let internal_collection = if let Some(ic) = req.extensions().get::<InternalCollection>() {
        ic.0.clone()
    } else {
        // If middleware didn't set it, calculate it here as fallback
        // Get secret key from headers
        let secret_key = req
            .headers()
            .get(AUTH_HEADER_NAME)
            .and_then(|h| h.to_str().ok())
            .map(String::from);

        if let Some(key) = &secret_key {
            let namespace = crate::namespace::hash_collection_namespace(key);
            let internal = format!("{}-{}", namespace, user_collection_name);
            internal
        } else {
            user_collection_name.to_string()
        }
    };

    let is_authenticated = auth::verify_admin_token(req.headers(), admin_token)
        || auth::verify_collection_secret(req.headers(), db, &internal_collection)?;

    if is_authenticated {
        req.extensions_mut()
            .insert(CollectionAuth::new(user_collection_name));
        next.call(req).await
    } else {
        Err(ApiError::unauthorized("Unauthorized access").into())
    }
}
