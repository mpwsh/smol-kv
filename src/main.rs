mod kv;
mod kv_handler;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use actix_web::{
        middleware::Logger,
        web::{delete, get, post, resource, scope, Data},
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
            .wrap(Logger::default())
            .service(
                scope("/api").service(
                    resource("/{key}")
                        .route(get().to(kv_handler::get))
                        .route(post().to(kv_handler::post))
                        .route(delete().to(kv_handler::delete)),
                ),
            )
    })
    .bind(("0.0.0.0", port))?
    .workers(workers)
    .run()
    .await
}
