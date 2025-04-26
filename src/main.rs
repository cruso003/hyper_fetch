use actix_web::{App, HttpServer};
mod handlers;
use actix_web::middleware::Logger;
use handlers::api::{echo, get_video};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(
        || {
            App::new()
                .wrap(Logger::default()) // Add logger middleware
                .service(get_video)
                .service(echo)
        }, // Add the echo service
    )
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
