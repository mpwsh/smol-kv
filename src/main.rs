mod auth;
mod benchmark;
mod collection;
mod error;
pub mod key;
mod middleware;
mod namespace;
mod sst;
mod sub;

pub use rocksdb_client as kv;
use std::sync::Arc;

pub const SECRETS_CF: &str = "secrets";

/// System column families that should never expire.
const SYSTEM_CFS: &[&str] = &["secrets", "backups", "restores"];

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

    let opts = kv::RocksDB::write_optimized_opts();

    // Open DB with per-CF TTL support.
    //
    // This does a two-phase open:
    //   1. Opens all existing CFs with TTL disabled to read metadata from secrets CF
    //   2. Closes and reopens with correct per-CF TTLs from saved metadata
    //
    // System CFs (secrets, backups, restores) always get TTL disabled (no expiry).
    // User CFs get the TTL that was specified at collection creation time.
    let db = kv::RocksDB::open_with_ttl_metadata(&opts, &db_path, SECRETS_CF, SYSTEM_CFS)
        .expect("Failed to open database");

    log::info!("Database opened with per-CF TTL support");

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
            .app_data(JsonConfig::default().limit(1024 * 1024 * 50))
            .app_data(PayloadConfig::new(1024 * 1024 * 50))
            .wrap(cors)
            .wrap(Logger::default())
            // API routes FIRST (before static files!)
            .service(
                scope("/api")
                    .wrap(namespace::CollectionNamespace)
                    .wrap(from_fn(middleware::require_auth))
                    .service(
                        resource("/_collections").route(get().to(collection::list_collections)),
                    )
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
                    .service(
                        resource("/{collection}/_compact").route(post().to(collection::compact)),
                    )
                    .service(resource("/{collection}/_size").route(get().to(collection::size)))
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
            .service(fs::Files::new("/backups/", sst::BACKUP_DIR))
            // Static files LAST so they don't swallow API routes
            .service(fs::Files::new("/", "./web").index_file("index.html"))
    })
    .bind(("0.0.0.0", port))?
    .workers(workers)
    .run()
    .await
}
