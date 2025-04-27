use crate::services::youtube_service::handle_youtube_api;
use actix_web::{HttpResponse, Responder, get, web};
use serde::Deserialize;

#[derive(Deserialize)]
struct VideoStruct {
    query: String,
    limit: Option<u32>,
    sorting: Option<String>,
}

#[get("/api/v1/echo")]
async fn echo() -> impl Responder {
    HttpResponse::Ok().body("Hello, world!")
}

// Search only for video tutorials on Youtube and get a playlist of them
#[get("/api/v1/resources/video")]
async fn get_video(vquery: web::Query<VideoStruct>) -> impl Responder {
    let query = &vquery.query;

    // Call the YouTube API to get video results
    match handle_youtube_api(query).await {
        Ok(videos) => {
            // Return the video results as JSON
            HttpResponse::Ok().json(videos)
        }
        Err(e) => {
            // If there was an error, return a 500 internal server error response
            HttpResponse::InternalServerError().body(format!("Error: {}", e))
        }
    }
}
