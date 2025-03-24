use crate::{
    error::ApiError,
    kv::{Direction, KVStore, KvStoreError, RocksDB},
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

// Constants
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
// Status enum for backup and restore operations
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum OperationStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
}

// Backup record structure
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

// Restore record structure
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

// Initialize backup and restore facilities
pub fn initialize_backup_restore(db: &RocksDB) -> Result<(), KvStoreError> {
    // Create backup directory if it doesn't exist
    if !Path::new(BACKUP_DIR).exists() {
        fs::create_dir_all(BACKUP_DIR)?;
    }

    // Create column families for backup and restore records if they don't exist
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

// Start a backup operation
pub async fn start_backup(
    collection: CollectionPath,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let internal_collection = collection.internal_collection().to_string();
    let user_collection = collection.user_collection().to_string();

    // Check if collection exists
    if !db.cf_exists(&internal_collection) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", user_collection))
        );
    }

    // Generate unique ID for the backup
    let backup_id = nanoid!(21);

    // Create backup record
    let backup_record = BackupRecord {
        id: backup_id.clone(),
        collection: user_collection.clone(),
        started_at: Utc::now(),
        finished_at: None,
        status: OperationStatus::InProgress,
        url: None,
        error: None,
    };

    // Store initial backup record
    db.insert_cf(BACKUPS_CF, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to create backup record", e))?;

    db.insert_cf(
        &format!("{internal_collection}-backups"),
        &backup_id,
        &backup_record,
    )
    .map_err(|e| ApiError::internal("Failed to create backup record", e))?;

    // Create backup path
    let backup_path = format!("{}/{}-{}.sst", BACKUP_DIR, user_collection, backup_id);

    // Prepare for async backup process
    let db_clone = db.clone();
    let backup_id_clone = backup_id.clone();
    let user_collection_name_clone = user_collection.clone();
    let internal_collection_name_clone = internal_collection.clone();
    let backup_path_clone = backup_path.clone();

    // Use actix's runtime spawner for the async task
    actix_web::rt::spawn(async move {
        let db_for_backup = db_clone.clone();
        let path_for_backup = backup_path_clone.clone();

        // Execute CPU-bound operation in a thread pool
        let result = web::block(move || {
            // Perform the backup
            db_for_backup.create_backup(&internal_collection_name_clone, &path_for_backup)
        })
        .await;

        // Get updated record - handle errors properly
        let backup_record = match db_clone.get_cf::<BackupRecord>(BACKUPS_CF, &backup_id_clone) {
            Ok(record) => record,
            Err(e) => {
                log::error!("Failed to retrieve backup record: {}", e);

                // Try to create a failure record anyway
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
                // Backup completed successfully
                updated_record.status = OperationStatus::Completed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.url = Some(format!(
                    "/backups/{}-{}.sst",
                    user_collection_name_clone, backup_id_clone
                ));
            }
            Ok(Err(e)) => {
                // Backup failed with known error
                updated_record.status = OperationStatus::Failed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.error = Some(format!("Backup operation failed: {}", e));

                // Clean up any partial backup file
                let _ = fs::remove_file(&backup_path_clone);
            }
            Err(e) => {
                // Task execution failed
                updated_record.status = OperationStatus::Failed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.error = Some(format!("Task execution failed: {}", e));

                // Clean up any partial backup file
                let _ = fs::remove_file(&backup_path_clone);
            }
        }

        // Update the backup record
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

    // Return immediate response with backup ID
    let response = OperationResponse {
        message: "Backup started".to_string(),
        id: backup_id,
        collection: user_collection,
    };

    Ok(HttpResponse::Ok().json(response))
}

// Get backup status
pub async fn backup_status(
    query: Query<OperationStatusRequest>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let backup_id = &query.id;

    // Get backup record
    match db.get_cf::<BackupRecord>(BACKUPS_CF, backup_id) {
        Ok(record) => Ok(HttpResponse::Ok().json(record)),
        Err(KvStoreError::KeyNotFound(_)) => {
            Ok(HttpResponse::NotFound().json(format!("Backup with ID {} not found", backup_id)))
        }
        Err(e) => Err(ApiError::internal("Failed to retrieve backup status", e)),
    }
}

// Upload a backup file and create a backup record
pub async fn upload_backup(
    collection: CollectionPath,
    mut payload: Multipart,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let internal_collection = collection.internal_collection().to_string();
    let user_collection = collection.user_collection().to_string();

    // Check if collection exists
    if !db.cf_exists(&internal_collection) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", user_collection))
        );
    }

    // Generate unique ID for the backup
    let backup_id = nanoid!(21);
    let now = Utc::now();

    // Create backup record (pre-upload)
    let mut backup_record = BackupRecord {
        id: backup_id.clone(),
        collection: user_collection.clone(),
        started_at: now,
        finished_at: None,
        status: OperationStatus::InProgress,
        url: None,
        error: None,
    };

    // Store initial backup record
    db.insert_cf(BACKUPS_CF, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to create backup record", e))?;

    // Ensure the backup collection exists
    let backup_cf = format!("{}-backups", internal_collection);
    if !db.cf_exists(&backup_cf) {
        db.create_cf(&backup_cf)
            .map_err(|e| ApiError::internal("Failed to create backup collection", e))?;
    }

    // Also store in the collection-specific backups CF
    db.insert_cf(&backup_cf, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to create backup record in collection", e))?;

    // Create backup path
    let backup_path = format!("{}/{}-{}.sst", BACKUP_DIR, user_collection, backup_id);

    // Process the file upload
    let mut file_found = false;

    // Handle file upload
    while let Ok(Some(mut field)) = payload.try_next().await {
        if field.name() == Some("file") {
            // Collect all data
            let mut data = Vec::new();
            while let Some(chunk) = field.next().await {
                data.extend_from_slice(
                    &chunk.map_err(|e| ApiError::internal("Failed to read upload", e))?,
                );
            }

            // Write file in a blocking operation
            let path_to_write = backup_path.clone();
            let _ = web::block(move || std::fs::write(&path_to_write, &data))
                .await
                .map_err(|e| ApiError::internal("Failed to write file", e))?;

            file_found = true;
            break;
        }
    }

    // Check if we received a file
    if !file_found {
        // Update record to failed state
        backup_record.status = OperationStatus::Failed;
        backup_record.finished_at = Some(Utc::now());
        backup_record.error = Some("No file received".to_string());

        // Update both records
        db.insert_cf(BACKUPS_CF, &backup_id, &backup_record)
            .map_err(|e| ApiError::internal("Failed to update backup record", e))?;
        db.insert_cf(&backup_cf, &backup_id, &backup_record)
            .map_err(|e| ApiError::internal("Failed to update backup record in collection", e))?;

        return Ok(HttpResponse::BadRequest().json("No file received"));
    }

    // Update record to completed state
    backup_record.status = OperationStatus::Completed;
    backup_record.finished_at = Some(Utc::now());
    backup_record.url = Some(format!("/backups/{}-{}.sst", user_collection, backup_id));

    // Update both records
    db.insert_cf(BACKUPS_CF, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to update backup record", e))?;
    db.insert_cf(&backup_cf, &backup_id, &backup_record)
        .map_err(|e| ApiError::internal("Failed to update backup record in collection", e))?;

    // Return response with backup ID
    let response = OperationResponse {
        message: "Backup file uploaded successfully".to_string(),
        id: backup_id,
        collection: user_collection,
    };

    Ok(HttpResponse::Created().json(response))
}

// Now start_restore can be simplified to only use backup_id
pub async fn start_restore(
    collection: CollectionPath,
    query: web::Query<RestoreParams>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let internal_collection = collection.internal_collection().to_string();
    let user_collection = collection.user_collection().to_string();

    // Check if collection exists
    if !db.cf_exists(&internal_collection) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", user_collection))
        );
    }

    // Check if backup_id is provided
    let backup_id = match &query.backup_id {
        Some(id) => id,
        None => return Ok(HttpResponse::BadRequest().json("backup_id parameter is required")),
    };

    // Generate unique ID for the restore operation
    let restore_id = nanoid!(21);

    // Create restore record
    let restore_record = RestoreRecord {
        id: restore_id.clone(),
        collection: user_collection.clone(),
        started_at: Utc::now(),
        finished_at: None,
        status: OperationStatus::InProgress,
        error: None,
    };

    // Store initial restore record
    db.insert_cf(RESTORES_CF, &restore_id, &restore_record)
        .map_err(|e| ApiError::internal("Failed to create restore record", e))?;

    // Get backup record to find the file path
    let backup = match db.get_cf::<BackupRecord>(BACKUPS_CF, backup_id) {
        Ok(record) => record,
        Err(e) => {
            // Update record to failed state
            let mut failed_record = restore_record;
            failed_record.status = OperationStatus::Failed;
            failed_record.finished_at = Some(Utc::now());
            failed_record.error = Some(format!("Backup {} not found: {}", backup_id, e));

            db.insert_cf(RESTORES_CF, &restore_id, &failed_record)
                .map_err(|e| ApiError::internal("Failed to update restore record", e))?;

            return Ok(HttpResponse::BadRequest().json(format!("Backup {} not found", backup_id)));
        }
    };

    // Check backup status
    if backup.status != OperationStatus::Completed {
        // Update record to failed state
        let mut failed_record = restore_record;
        failed_record.status = OperationStatus::Failed;
        failed_record.finished_at = Some(Utc::now());
        failed_record.error = Some(format!("Backup {} is not in a completed state", backup_id));

        db.insert_cf(RESTORES_CF, &restore_id, &failed_record)
            .map_err(|e| ApiError::internal("Failed to update restore record", e))?;

        return Ok(HttpResponse::BadRequest()
            .json(format!("Backup {} is not in a completed state", backup_id)));
    }

    // Make sure backup file exists
    let file_path = match backup.url {
        Some(path) => {
            let full_path = format!(".{}", path);
            if !std::path::Path::new(&full_path).exists() {
                // Update record to failed state
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
            // Update record to failed state
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

    // Prepare for async restore process
    let db_clone = db.clone();
    let restore_id_clone = restore_id.clone();
    let file_path_clone = file_path.clone();
    let user_collection_name_clone = user_collection.clone();
    let internal_collection_name_clone = internal_collection.clone();

    // Use actix's runtime spawner for the async task
    actix_web::rt::spawn(async move {
        // Make additional clones for the inner block
        let db_for_restore = db_clone.clone();
        let path_for_restore = file_path_clone.clone();

        // Execute CPU-bound operation in a thread pool
        let result = web::block(move || {
            // Perform the restore
            db_for_restore.restore_backup(&internal_collection_name_clone, &path_for_restore)
        })
        .await;

        // Get updated record - handle errors properly
        let restore_record = match db_clone.get_cf::<RestoreRecord>(RESTORES_CF, &restore_id_clone)
        {
            Ok(record) => record,
            Err(e) => {
                log::error!("Failed to retrieve restore record: {}", e);

                // Try to create a failure record anyway
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
                // Restore completed successfully
                updated_record.status = OperationStatus::Completed;
                updated_record.finished_at = Some(Utc::now());
            }
            Ok(Err(e)) => {
                // Restore failed
                updated_record.status = OperationStatus::Failed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.error = Some(format!("Restore operation failed: {}", e));
            }
            Err(e) => {
                // Task execution failed
                updated_record.status = OperationStatus::Failed;
                updated_record.finished_at = Some(Utc::now());
                updated_record.error = Some(format!("Task execution failed: {}", e));
            }
        }

        // Update the restore record
        if let Err(e) = db_clone.insert_cf(RESTORES_CF, &restore_id_clone, &updated_record) {
            log::error!("Failed to update restore record: {}", e);
        }
    });

    // Return immediate response with restore ID
    let response = OperationResponse {
        message: "Restore started".to_string(),
        id: restore_id,
        collection: user_collection,
    };

    Ok(HttpResponse::Ok().json(response))
}
// Get restore status
pub async fn restore_status(
    query: Query<OperationStatusRequest>,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let restore_id = &query.id;

    // Get restore record
    match db.get_cf::<RestoreRecord>(RESTORES_CF, restore_id) {
        Ok(record) => Ok(HttpResponse::Ok().json(record)),
        Err(KvStoreError::KeyNotFound(_)) => {
            Ok(HttpResponse::NotFound().json(format!("Restore with ID {} not found", restore_id)))
        }
        Err(e) => Err(ApiError::internal("Failed to retrieve restore status", e)),
    }
}

// List all backups for a collection
pub async fn list_backups(
    collection: CollectionPath,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let collection_name = collection.internal_collection().to_string();

    // Check if collection exists
    if !db.cf_exists(&collection_name) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", collection_name))
        );
    }

    // Get all backups for this collection
    let backups: Vec<BackupRecord> = db
        .get_range_cf(BACKUPS_CF, "", "\u{fff0}", usize::MAX, Direction::Forward)
        .map_err(|e| ApiError::internal("Failed to retrieve backups", e))?
        .into_iter()
        .filter(|backup: &BackupRecord| backup.collection == collection.user_collection)
        .collect();

    Ok(HttpResponse::Ok().json(backups))
}

// List all restores for a collection
pub async fn list_restores(
    collection: CollectionPath,
    db: Data<RocksDB>,
) -> Result<HttpResponse, ApiError> {
    let collection_name = collection.internal_collection().to_string();

    // Check if collection exists
    if !db.cf_exists(&collection_name) {
        return Ok(
            HttpResponse::NotFound().json(format!("Collection {} does not exist", collection_name))
        );
    }

    // Get all restores for this collection
    let restores: Vec<RestoreRecord> = db
        .get_range_cf(RESTORES_CF, "", "\u{fff0}", usize::MAX, Direction::Forward)
        .map_err(|e| ApiError::internal("Failed to retrieve restores", e))?
        .into_iter()
        .filter(|restore: &RestoreRecord| restore.collection == collection_name)
        .collect();

    Ok(HttpResponse::Ok().json(restores))
}
