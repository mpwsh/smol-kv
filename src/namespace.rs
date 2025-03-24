use crate::{
    auth::{self, InternalCollection, SecretKey, AUTH_HEADER_NAME},
    kv::{KVStore, RocksDB},
    SECRETS_CF,
};

use actix_web::{
    dev::{Payload, ServiceRequest},
    error::Error,
    http::Method,
    web::Data,
    FromRequest, HttpMessage, HttpRequest,
};
use futures::future::{ready, Ready};
use ring::digest;
use std::ops::Deref;

// Collection path extractor - works like Path<String> but resolves internal name
#[derive(Debug, Clone)]
pub struct CollectionPath {
    // User-visible collection name
    pub user_collection: String,
    // Item key from path (for /<collection>/<key> routes)
    pub path_key: Option<String>,
    // Internal namespaced name
    pub internal_collection: String,

    // Secret key if available
    pub secret_key: Option<String>,
}

impl CollectionPath {
    // Get the user-facing name
    pub fn user_collection(&self) -> &str {
        &self.user_collection
    }
    pub fn path_key(&self) -> Option<&str> {
        self.path_key.as_deref()
    }
    // Get the internal namespaced name for DB operations
    pub fn internal_collection(&self) -> &str {
        &self.internal_collection
    }

    // Get the secret key if available
    pub fn secret_key(&self) -> Option<&str> {
        self.secret_key.as_deref()
    }
}

// Allow using CollectionPath as a string reference
impl Deref for CollectionPath {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.internal_collection
    }
}

// Display the user-facing name
impl std::fmt::Display for CollectionPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.user_collection)
    }
}

// Implementing FromRequest lets it work as an extractor in handler signatures
impl FromRequest for CollectionPath {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        // Get the user-facing name from path parameter
        // Check both "name" and "collection" parameters to work with all routes
        let user_collection = match req.match_info().get("collection") {
            Some(name) => name.to_string(),
            None => {
                return ready(Err(actix_web::error::ErrorNotFound(
                    "Path parameter not found",
                )));
            }
        };
        // Look for a key parameter
        let path_key = req.match_info().get("key").map(ToString::to_string);

        // Get internal name from extensions
        let internal_collection = req
            .extensions()
            .get::<InternalCollection>()
            .map(|name| name.0.clone())
            .unwrap_or_else(|| user_collection.clone());

        // Get secret key if present
        let secret_key = req.extensions().get::<SecretKey>().map(|k| k.0.clone());

        ready(Ok(CollectionPath {
            user_collection,
            internal_collection,
            secret_key,
            path_key,
        }))
    }
}

// Middleware for collection name resolution
pub struct CollectionNamespace;

impl<S, B> actix_web::dev::Transform<S, ServiceRequest> for CollectionNamespace
where
    S: actix_web::dev::Service<
            ServiceRequest,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = Error,
        > + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = actix_web::dev::ServiceResponse<B>;
    type Error = Error;
    type Transform = CollectionNamespaceMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CollectionNamespaceMiddleware { service }))
    }
}

pub struct CollectionNamespaceMiddleware<S> {
    service: S,
}

impl<S, B> actix_web::dev::Service<ServiceRequest> for CollectionNamespaceMiddleware<S>
where
    S: actix_web::dev::Service<
            ServiceRequest,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = Error,
        > + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = actix_web::dev::ServiceResponse<B>;
    type Error = Error;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Only process paths that start with /api/
        if !req.path().starts_with("/api/") {
            return Box::pin(self.service.call(req));
        }

        // Extract the collection name from path
        let path_segments: Vec<&str> = req.path().split('/').collect();
        if path_segments.len() < 3 {
            return Box::pin(self.service.call(req));
        }

        let path_segments: Vec<&str> = req.path().split('/').collect();
        if path_segments.len() < 3 {
            return Box::pin(self.service.call(req));
        }

        let user_collection_name = path_segments[2].to_string();

        // Skip processing for paths that don't target a specific collection
        if user_collection_name.is_empty() {
            return Box::pin(self.service.call(req));
        }

        // Get DB reference
        let db = match req.app_data::<Data<RocksDB>>() {
            Some(db) => db.clone(),
            None => return Box::pin(self.service.call(req)),
        };

        // Get secret key from headers
        let secret_key = req
            .headers()
            .get(AUTH_HEADER_NAME)
            .and_then(|h| h.to_str().ok())
            .map(String::from);

        // Determine internal collection name
        let internal_collection = if req.method() == Method::PUT && path_segments.len() == 3 {
            // Collection creation - use the provided key or generate a new one
            let secret = secret_key.clone().unwrap_or_else(|| {
                let generated_key = nanoid::nanoid!(32);
                log::info!("Generated new secret key: {}", generated_key);
                generated_key
            });

            let namespace = hash_collection_namespace(&secret);
            let internal = format!("{}-{}", namespace, user_collection_name);

            // Always store the secret in extensions for the handler to use
            req.extensions_mut().insert(SecretKey(secret));

            internal
        } else if let Ok(secret) = db.get_cf::<auth::Secret>(SECRETS_CF, &user_collection_name) {
            // Existing collection - use prefix of stored hash for namespace
            let prefix = &secret.secret[..std::cmp::min(8, secret.secret.len())];
            let namespace = hash_collection_namespace(prefix);
            format!("{}-{}", namespace, user_collection_name)
        } else if let Some(key) = &secret_key {
            // Use provided key to try to access
            let namespace = hash_collection_namespace(key);
            format!("{}-{}", namespace, user_collection_name)
        } else {
            // Fallback to user-facing name (will likely 404 later)
            user_collection_name.clone()
        };

        // Store collection info in request extensions
        req.extensions_mut()
            .insert(InternalCollection(internal_collection));

        // If we have a secret key, store it too
        if let Some(key) = secret_key {
            req.extensions_mut().insert(SecretKey(key));
        }

        // Continue with the request
        let fut = self.service.call(req);

        Box::pin(fut)
    }
}

// Hash function for namespace generation
pub fn hash_collection_namespace(key: &str) -> String {
    let hash = digest::digest(&digest::SHA256, key.as_bytes());
    let hash_str = hex::encode(hash.as_ref());
    hash_str[..8].to_string()
}
