mod kv;
mod kv_handler;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use actix_web::{
        middleware::Logger,
        web::{delete, get, head, post, resource, scope, Data, JsonConfig, PayloadConfig},
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
    let db_path = std::env::var("DATABABASE_PATH").unwrap_or("./rocksdb".to_string());
    let log_level = std::env::var("LOG_LEVEL").unwrap_or("info".to_string());
    let db: kv::RocksDB = kv::KVStore::init(&db_path);
    std::env::set_var(
        "RUST_LOG",
        format!("{0},actix_web={0},actix_server={0}", log_level),
    );
    env_logger::init();
    log::info!("starting HTTP server at http://0.0.0.0:{port}");
    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(db.clone()))
            .app_data(Data::new(token.clone()))
            .app_data(JsonConfig::default().limit(1024 * 1024 * 50)) // 50 MB
            .app_data(PayloadConfig::new(1024 * 1024 * 50))
            .wrap(Logger::default())
            .service(
                scope("/api")
                    .service(
                        resource("/{key}")
                            .route(get().to(kv_handler::get))
                            .route(head().to(kv_handler::head))
                            .route(post().to(kv_handler::post))
                            .route(delete().to(kv_handler::delete)),
                    )
                    .service(resource("").route(post().to(kv_handler::new))),
            )
            .service(resource("/benchmark").route(post().to(kv_handler::benchmark)))
    })
    .bind(("0.0.0.0", port))?
    .workers(workers)
    .run()
    .await
}
