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
    // let just print out the video query for now
    HttpResponse::Ok().body("Video search results for ".to_string() + query)
}
