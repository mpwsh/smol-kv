use crate::{
    error::ApiError,
    kv::{Direction, KvStoreError, RocksDB},
    namespace::CollectionPath,
};

use std::{fs, path::Path};

use actix_multipart::Multipart;
use actix_web::{
    web::{self, Data, Query},
    HttpResponse,
};
use chrono::{DateTime, Utc};
use futures::{StreamExt, TryStreamExt};
use nanoid::nanoid;
use serde::{Deserialize, Serialize};

pub const BACKUPS_CF: &str = "backups";
pub const RESTORES_CF: &str = "restores";
pub const BACKUP_DIR: &str = "./backups";

#[derive(Debug, Serialize)]
pub struct OperationResponse {
    pub message: String,
    pub id: String,
    pub collection: String,
}

#[derive(Deserialize)]
pub struct RestoreParams {
    backup_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum OperationStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupRecord {
    pub id: String,
    pub collection: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: OperationStatus,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RestoreRecord {
    pub id: String,
    pub collection: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: OperationStatus,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OperationStatusRequest {
    pub id: String,
}

pub fn initialize_backup_restore(db: &RocksDB) -> Result<(), KvStoreError> {
    if !Path::new(BACKUP_DIR).exists() {
        fs::create_dir_all(BACKUP_DIR)?;
    }

    if !db.cf_exists(BACKUPS_CF) {
        db.create_cf(BACKUPS_CF)?;
        log::info!("Initialized backups collection");
    }

    if !db.cf_exists(RESTORES_CF) {
        db.create_cf(RESTORES_CF)?;
        log::info!("Initialized restores collection");
    }

    Ok(())
}

pub async fn start_backup(
    collection: CollectionPath,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let internal_collection = collection.internal_collection().to_string();
    let user_collection = collection.user_collection().to_string();

    if !db.cf_exists(&internal_collection) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", user_collection))
        );
    }

    let backup_id = nanoid!(21);

    let backup_record = BackupRecord {
        id: backup_id.clone(),
        collection: user_collection.clone(),
        started_at: Utc::now(),
        finished_at: None,
        status: OperationStatus::InProgress,
        url: None,
        error: None,
    };

    db.insert_cf(BACKUPS_CF, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to create backup record", e))?;

    db.insert_cf(
        &format!("{internal_collection}-backups"),
        &backup_id,
        &backup_record,
    )
    .map_err(|e| ApiError::internal("Failed to create backup record", e))?;

    let backup_path = format!("{}/{}-{}.sst", BACKUP_DIR, user_collection, backup_id);

    let db_clone = db.clone();
    let backup_id_clone = backup_id.clone();
    let user_collection_name_clone = user_collection.clone();
    let internal_collection_name_clone = internal_collection.clone();
    let backup_path_clone = backup_path.clone();

    actix_web::rt::spawn(async move {
        let db_for_backup = db_clone.clone();
        let path_for_backup = backup_path_clone.clone();

        let result = web::block(move || {
            db_for_backup.create_backup(&internal_collection_name_clone, &path_for_backup)
        })
        .await;

        let backup_record = match db_clone.get_cf::<BackupRecord>(BACKUPS_CF, &backup_id_clone) {
            Ok(record) => record,
            Err(e) => {
                log::error!("Failed to retrieve backup record: {}", e);

                let failure_record = BackupRecord {
                    id: backup_id_clone.clone(),
                    collection: user_collection_name_clone.clone(),
                    started_at: Utc::now(),
                    finished_at: Some(Utc::now()),
                    status: OperationStatus::Failed,
                    url: None,
                    error: Some(format!("Failed to retrieve backup record: {}", e)),
                };

                let _ = db_clone.insert_cf(BACKUPS_CF, &backup_id_clone, &failure_record);
                let _ = db_clone.insert_cf(
                    &format!("{internal_collection}-backups"),
                    &backup_id_clone,
                    &failure_record,
                );
                return;
            }
        };

        let mut updated_record = backup_record;

        match result {
            Ok(Ok(_)) => {
                updated_record.status = OperationStatus::Completed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.url = Some(format!(
                    "/backups/{}-{}.sst",
                    user_collection_name_clone, backup_id_clone
                ));
            }
            Ok(Err(e)) => {
                updated_record.status = OperationStatus::Failed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.error = Some(format!("Backup operation failed: {}", e));
                let _ = fs::remove_file(&backup_path_clone);
            }
            Err(e) => {
                updated_record.status = OperationStatus::Failed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.error = Some(format!("Task execution failed: {}", e));
                let _ = fs::remove_file(&backup_path_clone);
            }
        }

        if let Err(e) = db_clone.insert_cf(BACKUPS_CF, &backup_id_clone, &updated_record) {
            log::error!("Failed to update backup record: {}", e);
        }
        if let Err(e) = db_clone.insert_cf(
            &format!("{internal_collection}-backups"),
            &backup_id_clone,
            &updated_record,
        ) {
            log::error!("Failed to update backup record: {}", e);
        }
    });

    let response = OperationResponse {
        message: "Backup started".to_string(),
        id: backup_id,
        collection: user_collection,
    };

    Ok(HttpResponse::Ok().json(response))
}

pub async fn backup_status(
    query: Query<OperationStatusRequest>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let backup_id = &query.id;

    match db.get_cf::<BackupRecord>(BACKUPS_CF, backup_id) {
        Ok(record) => Ok(HttpResponse::Ok().json(record)),
        Err(KvStoreError::KeyNotFound(_)) => {
            Ok(HttpResponse::NotFound().json(format!("Backup with ID {} not found", backup_id)))
        }
        Err(e) => Err(ApiError::internal("Failed to retrieve backup status", e)),
    }
}

pub async fn upload_backup(
    collection: CollectionPath,
    mut payload: Multipart,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let internal_collection = collection.internal_collection().to_string();
    let user_collection = collection.user_collection().to_string();

    if !db.cf_exists(&internal_collection) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", user_collection))
        );
    }

    let backup_id = nanoid!(21);
    let now = Utc::now();

    let mut backup_record = BackupRecord {
        id: backup_id.clone(),
        collection: user_collection.clone(),
        started_at: now,
        finished_at: None,
        status: OperationStatus::InProgress,
        url: None,
        error: None,
    };

    db.insert_cf(BACKUPS_CF, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to create backup record", e))?;

    let backup_cf = format!("{}-backups", internal_collection);
    if !db.cf_exists(&backup_cf) {
        db.create_cf(&backup_cf)
            .map_err(|e| ApiError::internal("Failed to create backup collection", e))?;
    }

    db.insert_cf(&backup_cf, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to create backup record in collection", e))?;

    let backup_path = format!("{}/{}-{}.sst", BACKUP_DIR, user_collection, backup_id);

    let mut file_found = false;

    while let Ok(Some(mut field)) = payload.try_next().await {
        if field.name() == Some("file") {
            let mut data = Vec::new();
            while let Some(chunk) = field.next().await {
                data.extend_from_slice(
                    &chunk.map_err(|e| ApiError::internal("Failed to read upload", e))?,
                );
            }

            let path_to_write = backup_path.clone();
            let _ = web::block(move || std::fs::write(&path_to_write, &data))
                .await
                .map_err(|e| ApiError::internal("Failed to write file", e))?;

            file_found = true;
            break;
        }
    }

    if !file_found {
        backup_record.status = OperationStatus::Failed;
        backup_record.finished_at = Some(Utc::now());
        backup_record.error = Some("No file received".to_string());

        db.insert_cf(BACKUPS_CF, &backup_id, &backup_record)
            .map_err(|e| ApiError::internal("Failed to update backup record", e))?;
        db.insert_cf(&backup_cf, &backup_id, &backup_record)
            .map_err(|e| ApiError::internal("Failed to update backup record in collection", e))?;

        return Ok(HttpResponse::BadRequest().json("No file received"));
    }

    backup_record.status = OperationStatus::Completed;
    backup_record.finished_at = Some(Utc::now());
    backup_record.url = Some(format!("/backups/{}-{}.sst", user_collection, backup_id));

    db.insert_cf(BACKUPS_CF, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to update backup record", e))?;
    db.insert_cf(&backup_cf, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to update backup record in collection", e))?;

    let response = OperationResponse {
        message: "Backup file uploaded successfully".to_string(),
        id: backup_id,
        collection: user_collection,
    };

    Ok(HttpResponse::Created().json(response))
}

pub async fn start_restore(
    collection: CollectionPath,
    query: web::Query<RestoreParams>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let internal_collection = collection.internal_collection().to_string();
    let user_collection = collection.user_collection().to_string();

    if !db.cf_exists(&internal_collection) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", user_collection))
        );
    }

    let backup_id = match &query.backup_id {
        Some(id) => id,
        None => return Ok(HttpResponse::BadRequest().json("backup_id parameter is required")),
    };

    let restore_id = nanoid!(21);

    let restore_record = RestoreRecord {
        id: restore_id.clone(),
        collection: user_collection.clone(),
        started_at: Utc::now(),
        finished_at: None,
        status: OperationStatus::InProgress,
        error: None,
    };

    db.insert_cf(RESTORES_CF, &restore_id, &restore_record)
        .map_err(|e| ApiError::internal("Failed to create restore record", e))?;

    let backup = match db.get_cf::<BackupRecord>(BACKUPS_CF, backup_id) {
        Ok(record) => record,
        Err(e) => {
            let mut failed_record = restore_record;
            failed_record.status = OperationStatus::Failed;
            failed_record.finished_at = Some(Utc::now());
            failed_record.error = Some(format!("Backup {} not found: {}", backup_id, e));

            db.insert_cf(RESTORES_CF, &restore_id, &failed_record)
                .map_err(|e| ApiError::internal("Failed to update restore record", e))?;

            return Ok(HttpResponse::BadRequest().json(format!("Backup {} not found", backup_id)));
        }
    };

    if backup.status != OperationStatus::Completed {
        let mut failed_record = restore_record;
        failed_record.status = OperationStatus::Failed;
        failed_record.finished_at = Some(Utc::now());
        failed_record.error = Some(format!("Backup {} is not in a completed state", backup_id));

        db.insert_cf(RESTORES_CF, &restore_id, &failed_record)
            .map_err(|e| ApiError::internal("Failed to update restore record", e))?;

        return Ok(HttpResponse::BadRequest()
            .json(format!("Backup {} is not in a completed state", backup_id)));
    }

    let file_path = match backup.url {
        Some(path) => {
            let full_path = format!(".{}", path);
            if !std::path::Path::new(&full_path).exists() {
                let mut failed_record = restore_record;
                failed_record.status = OperationStatus::Failed;
                failed_record.finished_at = Some(Utc::now());
                failed_record.error =
                    Some(format!("Backup file not found for backup {}", backup_id));

                db.insert_cf(RESTORES_CF, &restore_id, &failed_record)
                    .map_err(|e| ApiError::internal("Failed to update restore record", e))?;

                return Ok(HttpResponse::BadRequest()
                    .json(format!("Backup file not found for backup {}", backup_id)));
            }
            full_path
        }
        None => {
            let mut failed_record = restore_record;
            failed_record.status = OperationStatus::Failed;
            failed_record.finished_at = Some(Utc::now());
            failed_record.error = Some(format!("No file path found for backup {}", backup_id));

            db.insert_cf(RESTORES_CF, &restore_id, &failed_record)
                .map_err(|e| ApiError::internal("Failed to update restore record", e))?;

            return Ok(HttpResponse::BadRequest()
                .json(format!("No file path found for backup {}", backup_id)));
        }
    };

    let db_clone = db.clone();
    let restore_id_clone = restore_id.clone();
    let file_path_clone = file_path.clone();
    let user_collection_name_clone = user_collection.clone();
    let internal_collection_name_clone = internal_collection.clone();

    actix_web::rt::spawn(async move {
        let db_for_restore = db_clone.clone();
        let path_for_restore = file_path_clone.clone();

        let result = web::block(move || {
            db_for_restore.restore_backup(&internal_collection_name_clone, &path_for_restore)
        })
        .await;

        let restore_record = match db_clone.get_cf::<RestoreRecord>(RESTORES_CF, &restore_id_clone)
        {
            Ok(record) => record,
            Err(e) => {
                log::error!("Failed to retrieve restore record: {}", e);

                let failure_record = RestoreRecord {
                    id: restore_id_clone.clone(),
                    collection: user_collection_name_clone.clone(),
                    started_at: Utc::now(),
                    finished_at: Some(Utc::now()),
                    status: OperationStatus::Failed,
                    error: Some(format!("Failed to retrieve restore record: {}", e)),
                };

                let _ = db_clone.insert_cf(RESTORES_CF, &restore_id_clone, &failure_record);
                return;
            }
        };

        let mut updated_record = restore_record;

        match result {
            Ok(Ok(_)) => {
                updated_record.status = OperationStatus::Completed;
                updated_record.finished_at = Some(Utc::now());
            }
            Ok(Err(e)) => {
                updated_record.status = OperationStatus::Failed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.error = Some(format!("Restore operation failed: {}", e));
            }
            Err(e) => {
                updated_record.status = OperationStatus::Failed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.error = Some(format!("Task execution failed: {}", e));
            }
        }

        if let Err(e) = db_clone.insert_cf(RESTORES_CF, &restore_id_clone, &updated_record) {
            log::error!("Failed to update restore record: {}", e);
        }
    });

    let response = OperationResponse {
        message: "Restore started".to_string(),
        id: restore_id,
        collection: user_collection,
    };

    Ok(HttpResponse::Ok().json(response))
}

pub async fn restore_status(
    query: Query<OperationStatusRequest>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let restore_id = &query.id;

    match db.get_cf::<RestoreRecord>(RESTORES_CF, restore_id) {
        Ok(record) => Ok(HttpResponse::Ok().json(record)),
        Err(KvStoreError::KeyNotFound(_)) => {
            Ok(HttpResponse::NotFound().json(format!("Restore with ID {} not found", restore_id)))
        }
        Err(e) => Err(ApiError::internal("Failed to retrieve restore status", e)),
    }
}

pub async fn list_backups(
    collection: CollectionPath,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let collection_name = collection.internal_collection().to_string();

    if !db.cf_exists(&collection_name) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", collection_name))
        );
    }

    // New API: get_range_cf returns Vec<serde_json::Value>, use get_range_cf_as for typed results
    let backups: Vec<BackupRecord> = db
        .get_range_cf_as(BACKUPS_CF, "", "\u{fff0}", usize::MAX, Direction::Forward)
        .map_err(|e| ApiError::internal("Failed to retrieve backups", e))?
        .into_iter()
        .filter(|backup: &BackupRecord| backup.collection == collection.user_collection)
        .collect();

    Ok(HttpResponse::Ok().json(backups))
}

pub async fn list_restores(
    collection: CollectionPath,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let collection_name = collection.internal_collection().to_string();

    if !db.cf_exists(&collection_name) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", collection_name))
        );
    }

    let restores: Vec<RestoreRecord> = db
        .get_range_cf_as(RESTORES_CF, "", "\u{fff0}", usize::MAX, Direction::Forward)
        .map_err(|e| ApiError::internal("Failed to retrieve restores", e))?
        .into_iter()
        .filter(|restore: &RestoreRecord| restore.collection == collection_name)
        .collect();

    Ok(HttpResponse::Ok().json(restores))
}
