use crate::services::youtube_service::{handle_youtube_scraper, Video};
use crate::services::job_service::{handle_job_scraper, Job};
use crate::services::cache::{clear_cache, remove_cache};
use actix_web::{HttpResponse, Responder, get, web};
use serde::Deserialize;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Deserialize, ToSchema)]
struct VideoStruct {
    query: String,
    limit: Option<u32>,
    sorting: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct JobStruct {
    query: String,
    limit: Option<u32>,
    location: Option<String>,
    remote_only: Option<bool>,
    job_type: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct CacheRefreshStruct {
    cache_key: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/echo",
    responses(
        (status = 200, description = "Echo endpoint to verify server is running", body = String)
    )
)]
#[get("/api/v1/echo")]
async fn echo() -> impl Responder {
    HttpResponse::Ok().body("Hello, world!")
}

#[utoipa::path(
    get,
    path = "/api/v1/health",
    responses(
        (status = 200, description = "Health check endpoint", body = String)
    )
)]
#[get("/api/v1/health")]
async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("Healthy")
}

#[utoipa::path(
    get,
    path = "/api/v1/resources/video",
    params(
        ("query" = String, Query, description = "Search query for YouTube videos"),
        ("limit" = Option<u32>, Query, description = "Maximum number of videos to return (default: 5)"),
        ("sorting" = Option<String>, Query, description = "Sorting method (default: relevance)")
    ),
    responses(
        (status = 200, description = "List of YouTube videos", body = [Video]),
        (status = 500, description = "Internal server error", body = String)
    )
)]
#[get("/api/v1/resources/video")]
async fn get_video(vquery: web::Query<VideoStruct>) -> impl Responder {
    let query = &vquery.query;
    let limit = vquery.limit.unwrap_or(5);
    let sorting = vquery.sorting.as_deref().unwrap_or("relevance");
    log::info!("Fetching YouTube videos for query: {}, limit: {}, sorting: {}", query, limit, sorting);

    match handle_youtube_scraper(query, limit).await {
        Ok(videos) => {
            log::info!("Returning {} YouTube videos", videos.len());
            HttpResponse::Ok().json(videos)
        }
        Err(e) => {
            log::error!("YouTube scraper error: {}", e);
            HttpResponse::InternalServerError().body(format!("Failed to fetch videos: {}", e))
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/jobs",
    params(
        ("query" = String, Query, description = "Search query for jobs"),
        ("limit" = Option<u32>, Query, description = "Maximum number of jobs to return (default: 10)"),
        ("location" = Option<String>, Query, description = "Location filter for jobs"),
        ("remote_only" = Option<bool>, Query, description = "Filter for remote-only jobs"),
        ("job_type" = Option<String>, Query, description = "Filter for job type (e.g., Full-time, Contract)")
    ),
    responses(
        (status = 200, description = "List of jobs", body = [Job]),
        (status = 500, description = "Internal server error", body = String)
    )
)]
#[get("/api/v1/jobs")]
async fn get_jobs(jquery: web::Query<JobStruct>) -> impl Responder {
    let query = &jquery.query;
    let limit = jquery.limit.unwrap_or(10);
    let location = jquery.location.as_deref().unwrap_or("");
    let remote_only = jquery.remote_only;
    let job_type = jquery.job_type.as_deref();
    
    log::info!("Fetching jobs for query: {}, limit: {}, location: {}, remote_only: {:?}, job_type: {:?}", 
              query, limit, location, remote_only, job_type);

    match handle_job_scraper(query, limit, location, remote_only, job_type).await {
        Ok(jobs) => {
            log::info!("Returning {} jobs", jobs.len());
            HttpResponse::Ok().json(jobs)
        }
        Err(e) => {
            log::error!("Job scraper error: {}", e);
            HttpResponse::InternalServerError().body(format!("Failed to fetch jobs: {}", e))
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/cache/clear",
    responses(
        (status = 200, description = "Clear all cache entries", body = String)
    )
)]
#[get("/api/v1/cache/clear")]
async fn clear_all_cache() -> impl Responder {
    log::info!("Clearing all cache");
    clear_cache();
    HttpResponse::Ok().body("Cache cleared")
}

#[utoipa::path(
    get,
    path = "/api/v1/cache/refresh",
    params(
        ("cache_key" = String, Query, description = "Cache key to refresh")
    ),
    responses(
        (status = 200, description = "Refresh specific cache entry", body = String)
    )
)]
#[get("/api/v1/cache/refresh")]
async fn refresh_cache(query: web::Query<CacheRefreshStruct>) -> impl Responder {
    let cache_key = &query.cache_key;
    log::info!("Refreshing cache for key: {}", cache_key);
    remove_cache(cache_key);
    HttpResponse::Ok().body(format!("Cache refreshed for key: {}", cache_key))
}

#[derive(OpenApi)]
#[openapi(
    paths(echo, health_check, get_video, get_jobs, clear_all_cache, refresh_cache),
    components(schemas(Video, Job, VideoStruct, JobStruct, CacheRefreshStruct))
)]
struct ApiDoc;

pub fn configure_swagger(cfg: &mut web::ServiceConfig) {
    cfg.service(
        SwaggerUi::new("/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );
}
