use actix_web::{App, HttpServer};
mod handlers;
mod services;
use actix_web::middleware::Logger;
use dotenv::dotenv;
use handlers::api::{echo, get_video};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .service(get_video)
            .service(echo)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
