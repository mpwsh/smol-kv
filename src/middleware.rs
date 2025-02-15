use crate::{auth, error::ApiError, kv::RocksDB};
use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    error::Error,
    http::Method,
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
    if req.path().starts_with("/benchmark") {
        return next.call(req).await;
    }

    let db = req
        .app_data::<actix_web::web::Data<RocksDB>>()
        .ok_or_else(|| ApiError::internal("Database not found", "missing database"))?;
    let admin_token = req
        .app_data::<actix_web::web::Data<String>>()
        .ok_or_else(|| ApiError::internal("Admin token not found", "missing token"))?;

    // Extract first path segment after /api/
    let path = req.path();
    let collection_name = path
        .split('/')
        .nth(2)
        .ok_or_else(|| ApiError::unauthorized("Collection name required"))?;

    // Skip auth for collection creation (paths with exactly 3 segments: /api/collection)
    if req.method() == Method::PUT && path.split('/').count() == 3 {
        return next.call(req).await;
    }

    if req.method() == Method::GET && path.split('/').count() == 4 {
        return next.call(req).await;
    }

    let is_authenticated = auth::verify_admin_token(req.headers(), admin_token)
        || auth::verify_collection_secret(req.headers(), db, collection_name)?;

    if is_authenticated {
        req.extensions_mut()
            .insert(CollectionAuth::new(collection_name));
        next.call(req).await
    } else {
        Err(ApiError::unauthorized("Unauthorized access").into())
    }
}
