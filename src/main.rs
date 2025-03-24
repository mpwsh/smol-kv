mod auth;
mod benchmark;
mod collection;
mod error;
pub mod key;
mod middleware;
mod namespace;
mod sst;
mod sub;
use crate::kv::KVStore;
pub use rocksdb_client as kv;
use std::sync::Arc;
pub const SECRETS_CF: &str = "secrets";
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use actix_cors::Cors;
    use actix_files as fs;
    use actix_web::{
        middleware::{from_fn, Logger},
        web::{delete, get, head, post, put, resource, scope, Data, JsonConfig, PayloadConfig},
        App, HttpServer,
    };

    let port = std::env::var("PORT")
        .unwrap_or("5050".to_string())
        .parse::<u16>()
        .unwrap();
    let workers = std::env::var("WORKERS")
        .unwrap_or("4".to_string())
        .parse::<usize>()
        .unwrap();
    let token = std::env::var("ADMIN_TOKEN").unwrap_or("supersecret".to_string());
    let db_path = std::env::var("DATABASE_PATH").unwrap_or("./rocksdb".to_string());
    let log_level = std::env::var("LOG_LEVEL").unwrap_or("info".to_string());
    std::env::set_var(
        "RUST_LOG",
        format!("{0},actix_web={0},actix_server={0}", log_level),
    );
    env_logger::init();
    log::info!("Using database path {db_path}");
    let sub_manager = Arc::new(sub::SubscriptionManager::new());
    let opts = config_db();
    let db: kv::RocksDB =
        kv::KVStore::open_with_existing_cfs(&opts, &db_path).expect("Failed to open database");
    if !db.cf_exists(SECRETS_CF) {
        db.create_cf(SECRETS_CF)
            .expect("Failed to create required secrets collection - cannot start server");
        log::info!("Initialized secrets collection");
    } else {
        log::info!("CF Secrets exists")
    };

    // Initialize backup and restore
    sst::initialize_backup_restore(&db)
        .expect("Failed to initialize backup and restore facilities");
    log::info!("Initialized backup and restore facilities");

    log::info!("starting HTTP server at http://0.0.0.0:{port}");
    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .app_data(Data::new(db.clone()))
            .app_data(Data::new(token.clone()))
            .app_data(Data::new(sub_manager.clone()))
            .app_data(JsonConfig::default().limit(1024 * 1024 * 50)) // 50 MB
            .app_data(PayloadConfig::new(1024 * 1024 * 50))
            .wrap(cors)
            .wrap(Logger::default())
            .service(
                scope("/api")
                    .wrap(namespace::CollectionNamespace)
                    .wrap(from_fn(middleware::require_auth))
                    .service(
                        resource("/{collection}")
                            .route(head().to(collection::exists))
                            .route(delete().to(collection::drop))
                            .route(put().to(collection::create))
                            .route(get().to(collection::list))
                            .route(post().to(collection::query)),
                    )
                    .service(
                        resource("/{collection}/_batch").route(put().to(collection::create_batch)),
                    )
                    .service(
                        resource("/{collection}/_subscribe").route(get().to(collection::subscribe)),
                    )
                    // New backup and restore endpoints
                    .service(
                        resource("/{collection}/_backup")
                            .route(post().to(sst::start_backup))
                            .route(get().to(sst::list_backups)),
                    )
                    .service(
                        resource("/{collection}/_backup/upload")
                            .route(post().to(sst::upload_backup)),
                    )
                    .service(
                        resource("{collection}/_backup/status").route(get().to(sst::backup_status)),
                    )
                    .service(
                        resource("/{collection}/_restore")
                            .route(post().to(sst::start_restore))
                            .route(get().to(sst::list_restores)),
                    )
                    .service(
                        resource("/{collection}/_restore/status")
                            .route(get().to(sst::restore_status)),
                    )
                    .service(resource("/{collection}/_import").route(post().to(key::import_values)))
                    .service(
                        resource("/{collection}/{key}")
                            .route(get().to(key::get))
                            .route(head().to(key::exists))
                            .route(put().to(key::create))
                            .route(delete().to(key::delete)),
                    ),
            )
            .service(resource("/benchmark").route(get().to(benchmark::start)))
            // Serve backup files
            .service(fs::Files::new("/backups/", sst::BACKUP_DIR))
    })
    .bind(("0.0.0.0", port))?
    .workers(workers)
    .run()
    .await
}

fn config_db() -> kv::Options {
    let mut opts = kv::Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    opts.set_enable_pipelined_write(true); // CRUCIAL for write performance

    // CRANK these write settings
    opts.set_write_buffer_size(64 * 1024 * 1024); // smaller buffers, more frequent flushes
    opts.set_max_write_buffer_number(8); // more buffers in memory
    opts.set_min_write_buffer_number_to_merge(1); // don't wait to merge, flush immediately
    opts.set_unordered_write(false); // trade consistency for speed
    opts.set_allow_concurrent_memtable_write(true);

    // parallel everything
    let cpu_cores = num_cpus::get() as i32;
    opts.increase_parallelism(cpu_cores);
    opts.set_max_background_jobs(cpu_cores * 2);

    // minimize compaction impact
    opts.set_level_zero_file_num_compaction_trigger(8); // wait longer before compacting
    opts.set_level_zero_slowdown_writes_trigger(20); // allow more L0 files
    opts.set_level_zero_stop_writes_trigger(40); // really allow more L0 files

    // DISABLE stuff we don't need
    opts.set_use_direct_io_for_flush_and_compaction(false); // let the OS handle this
    opts.set_use_direct_reads(false);
    opts.set_allow_mmap_reads(false);
    opts.set_allow_mmap_writes(false);

    // reduce WAL overhead
    opts.set_manual_wal_flush(true); // manual WAL flush for batching
    opts.set_wal_bytes_per_sync(0); // disable WAL syncing}

    opts
}
