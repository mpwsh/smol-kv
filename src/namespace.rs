use crate::{
    auth::{self, extract_secret_key, InternalCollection, SecretKey},
    kv::RocksDB,
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

// ── CollectionPath extractor ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CollectionPath {
    pub user_collection: String,
    pub path_key: Option<String>,
    pub internal_collection: String,
    pub secret_key: Option<String>,
}

impl CollectionPath {
    pub fn user_collection(&self) -> &str {
        &self.user_collection
    }
    pub fn path_key(&self) -> Option<&str> {
        self.path_key.as_deref()
    }
    pub fn internal_collection(&self) -> &str {
        &self.internal_collection
    }
    pub fn secret_key(&self) -> Option<&str> {
        self.secret_key.as_deref()
    }
}

impl Deref for CollectionPath {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.internal_collection
    }
}

impl std::fmt::Display for CollectionPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.user_collection)
    }
}

impl FromRequest for CollectionPath {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let user_collection = match req.match_info().get("collection") {
            Some(name) => name.to_string(),
            None => {
                return ready(Err(actix_web::error::ErrorNotFound(
                    "Path parameter not found",
                )));
            }
        };
        let path_key = req.match_info().get("key").map(ToString::to_string);

        let internal_collection = req
            .extensions()
            .get::<InternalCollection>()
            .map(|name| name.0.clone())
            .unwrap_or_else(|| user_collection.clone());

        let secret_key = req.extensions().get::<SecretKey>().map(|k| k.0.clone());

        ready(Ok(CollectionPath {
            user_collection,
            internal_collection,
            secret_key,
            path_key,
        }))
    }
}

// ── Namespace middleware ──────────────────────────────────────────────────────
//
// Resolves user collection name → internal (namespaced) collection name.
//
// On PUT (creation): hash the secret key → namespace prefix
// On other methods:  look up stored secret, or hash provided key
//
// The resolved key + internal name are stored in request extensions
// so downstream middleware and handlers don't re-derive them.

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
        // Only process /api/ routes
        if !req.path().starts_with("/api/") {
            return Box::pin(self.service.call(req));
        }

        let path_segments: Vec<&str> = req.path().split('/').collect();
        if path_segments.len() < 3 {
            return Box::pin(self.service.call(req));
        }

        let user_collection_name = path_segments[2].to_string();

        // Skip empty and system endpoints (e.g. /_collections)
        if user_collection_name.is_empty() || user_collection_name.starts_with('_') {
            return Box::pin(self.service.call(req));
        }

        let db = match req.app_data::<Data<RocksDB>>() {
            Some(db) => db.clone(),
            None => return Box::pin(self.service.call(req)),
        };

        // Extract key once — header or query param
        let secret_key = extract_secret_key(&req);

        // Resolve internal collection name
        let internal_collection = if req.method() == Method::PUT && path_segments.len() == 3 {
            // ── Collection creation ──
            // Use provided key or generate one
            let secret = secret_key.clone().unwrap_or_else(|| {
                let generated = nanoid::nanoid!(32);
                log::info!("Generated new secret key: {}", generated);
                generated
            });

            let namespace = hash_collection_namespace(&secret);
            let internal = format!("{}-{}", namespace, user_collection_name);

            req.extensions_mut().insert(SecretKey(secret));
            internal
        } else if let Ok(stored) = db.get_cf::<auth::Secret>(SECRETS_CF, &user_collection_name) {
            // ── Stored secret found by user name ──
            // (legacy path — secrets stored under user name)
            let prefix = &stored.secret[..std::cmp::min(8, stored.secret.len())];
            let namespace = hash_collection_namespace(prefix);
            format!("{}-{}", namespace, user_collection_name)
        } else if let Some(ref key) = secret_key {
            // ── Derive from provided key ──
            let namespace = hash_collection_namespace(key);
            format!("{}-{}", namespace, user_collection_name)
        } else {
            // ── No key, no stored secret — use raw name ──
            user_collection_name.clone()
        };

        req.extensions_mut()
            .insert(InternalCollection(internal_collection));

        if let Some(key) = secret_key {
            req.extensions_mut().insert(SecretKey(key));
        }

        Box::pin(self.service.call(req))
    }
}

pub fn hash_collection_namespace(key: &str) -> String {
    let hash = digest::digest(&digest::SHA256, key.as_bytes());
    let hash_str = hex::encode(hash.as_ref());
    hash_str[..8].to_string()
}
