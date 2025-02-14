mod auth;
mod benchmark;
mod collection;
mod error;
pub mod key;
mod middleware;
use crate::kv::KVStore;
pub use rocksdb_client as kv;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use actix_cors::Cors;
    use actix_web::{
        middleware::{from_fn, Logger},
        web::{delete, get, head, post, put, resource, scope, Data, JsonConfig, PayloadConfig},
        App, HttpServer,
    };
    const SECRETS_CF: &str = "secrets";

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

    let opts = config_db();
    let db: kv::RocksDB =
        kv::KVStore::open_with_existing_cfs(&opts, &db_path).expect("Failed to open database");
    db.create_cf("secrets")
        .expect("Unable to create secrets collection");

    if !db.cf_exists(SECRETS_CF) {
        db.create_cf(SECRETS_CF)
            .expect("Failed to create required secrets collection - cannot start server");
    }

    std::env::set_var(
        "RUST_LOG",
        format!("{0},actix_web={0},actix_server={0}", log_level),
    );
    env_logger::init();
    log::info!("Using database path {db_path}");
    log::info!("starting HTTP server at http://0.0.0.0:{port}");
    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .app_data(Data::new(db.clone()))
            .app_data(Data::new(token.clone()))
            .app_data(JsonConfig::default().limit(1024 * 1024 * 50)) // 50 MB
            .app_data(PayloadConfig::new(1024 * 1024 * 50))
            .wrap(cors)
            .wrap(Logger::default())
            .service(
                scope("/api")
                    .wrap(from_fn(middleware::require_auth))
                    .service(
                        resource("/{name}")
                            .route(head().to(collection::exists))
                            .route(delete().to(collection::drop))
                            .route(put().to(collection::create))
                            .route(get().to(collection::list))
                            .route(post().to(collection::query)),
                    )
                    .service(
                        resource("/{collection}/{key}")
                            .route(get().to(key::get))
                            .route(head().to(key::exists))
                            .route(put().to(key::create))
                            .route(delete().to(key::delete)),
                    ),
            )
            .service(resource("/benchmark").route(get().to(benchmark::start)))
    })
    .bind(("0.0.0.0", port))?
    .workers(workers)
    .run()
    .await
}

fn config_db() -> kv::Options {
    let mut opts = kv::Options::default();

    // Basic options
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);

    // Write performance
    opts.set_write_buffer_size(256 * 1024 * 1024); // 256MB write buffer
    opts.set_max_write_buffer_number(6); // Allow more write buffers
    opts.set_min_write_buffer_number_to_merge(2);

    // Read performance
    opts.set_max_open_files(-1); // Keep all files open, good for production
    opts.set_use_direct_io_for_flush_and_compaction(true);
    opts.set_use_direct_reads(false);

    // Parallelism
    let cpu_cores = num_cpus::get() as i32;
    opts.increase_parallelism(cpu_cores);
    opts.set_max_background_jobs(cpu_cores * 2);

    // Compaction settings
    opts.set_level_zero_file_num_compaction_trigger(4);
    opts.set_target_file_size_base(128 * 1024 * 1024); // 128MB
    opts.set_max_bytes_for_level_base(512 * 1024 * 1024); // 512MB

    // Memory settings
    opts.set_allow_mmap_reads(true);
    opts.set_allow_mmap_writes(false); // mmap writes can be dangerous
    opts.set_max_total_wal_size(256 * 1024 * 1024); // 256MB max WAL size
                                                    //
    opts.set_level_compaction_dynamic_level_bytes(true); // Better for varying workloads
    opts.set_optimize_filters_for_hits(true); // Good for read-heavy workloads
    opts.set_report_bg_io_stats(true); // Helpful for monitoring

    // Cache settings
    let cache_size = 1024 * 1024 * 1024; // 1GB cache
    opts.optimize_for_point_lookup(cache_size);

    opts
}
