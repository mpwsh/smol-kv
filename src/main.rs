mod kv;
mod kv_handler;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use actix_web::{
        middleware::Logger,
        web::{delete, get, post, resource, scope, Data},
        App, HttpServer,
    };

    let db: kv::RocksDB = kv::KVStore::init("rocks.db");

    std::env::set_var("RUST_LOG", "debug,actix_web=debug,actix_server=debug");
    env_logger::init();

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
    .bind("0.0.0:3031")?
    .run()
    .await
}
