use actix_web::{App, HttpServer};
mod handlers;
mod services;
use actix_web::middleware::Logger;
use actix_governor::Governor;
use dotenv::dotenv;
use env_logger;
use handlers::api::{clear_all_cache, configure_swagger, echo, get_jobs, get_video, health_check, refresh_cache};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting server on http://127.0.0.1:8081");
    
    HttpServer::new(|| {
        let governor_conf = Governor::new(&actix_governor::GovernorConfigBuilder::default()
            .per_second(60)  // 60 requests per second
            .burst_size(100)
            .finish()
            .unwrap());

        App::new()
            .wrap(governor_conf)
            .wrap(Logger::default())
            .configure(configure_swagger)
            .service(get_video)
            .service(get_jobs)
            .service(clear_all_cache)
            .service(refresh_cache)
            .service(echo)
            .service(health_check)
    })
    .bind(("127.0.0.1", 8081))?
    .run()
    .await
}
