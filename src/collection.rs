use crate::{
    error::ApiError,
    kv::{Direction, KVStore, RocksDB},
};
use actix_web::{
    web::{Data, Path, Query},
    HttpResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    from: Option<String>,
    to: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    order: Option<SortOrder>,
    #[serde(default)]
    include_keys: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum SortOrder {
    #[serde(rename = "asc")]
    Ascending,
    #[serde(rename = "desc")]
    Descending,
}

pub async fn exists(name: Path<String>, db: Data<RocksDB>) -> HttpResponse {
    if db.cf_exists(&name) {
        HttpResponse::Ok().finish()
    } else {
        HttpResponse::NotFound().finish()
    }
}

pub async fn create(name: Path<String>, db: Data<RocksDB>) -> Result<HttpResponse, ApiError> {
    if db.cf_exists(&name) {
        Ok(HttpResponse::Conflict().finish())
    } else {
        db.create_cf(&name)
            .map_err(|e| ApiError::internal("Failed to create collection", e))?;

        log::info!("Created collection {name}");
        Ok(HttpResponse::Created().finish())
    }
}

pub async fn drop(name: Path<String>, db: Data<RocksDB>) -> Result<HttpResponse, ApiError> {
    let name = name.into_inner();
    if !db.cf_exists(&name) {
        Ok(HttpResponse::NotFound().finish())
    } else {
        db.drop_cf(&name)
            .map_err(|e| ApiError::internal("Failed to drop collection", e))?;

        Ok(HttpResponse::Ok().finish())
    }
}

pub async fn list(
    collection: Path<String>,
    query: Query<RangeQuery>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    if !db.cf_exists(&collection) {
        Ok(HttpResponse::NotFound().finish())
    } else {
        let from = query.from.as_deref().unwrap_or("");
        let to = query.to.as_deref().unwrap_or("\u{fff0}");
        let direction = match query.order.clone().unwrap_or(SortOrder::Ascending) {
            SortOrder::Ascending => Direction::Forward,
            SortOrder::Descending => Direction::Reverse,
        };
        let items = db
            .get_range_cf::<Value>(
                &collection,
                from,
                to,
                query.limit.unwrap_or(usize::MAX),
                direction,
                query.include_keys,
            )
            .map_err(|e| ApiError::internal("Failed to fetch items", e))?;

        Ok(HttpResponse::Ok().json(items))
    }
}
