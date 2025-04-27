use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;

#[derive(Debug, Deserialize, Serialize)]
pub struct Video {
    id: String,
    title: String,
    description: String,
    thumbnail_url: String,
}

pub async fn handle_youtube_api(query: &str) -> Result<Vec<Video>, reqwest::Error> {
    // Load API key from .env
    let api_key = env::var("YOUTUBE_API_KEY").expect("YOUTUBE_API_KEY not set");

    // Build YouTube API URL
    let api_url = format!(
        "https://www.googleapis.com/youtube/v3/search?part=snippet&q={}&type=video&key={}&maxResults=5",
        query, api_key
    );

    // Send HTTP GET request
    let response = reqwest::get(&api_url).await?;
    let json: Value = response.json().await?;

    let mut videos: Vec<Video> = Vec::new();

    if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
        for item in items {
            if let (Some(id), Some(snippet)) = (item.get("id"), item.get("snippet")) {
                if let Some(video_id) = id.get("videoId").and_then(|v| v.as_str()) {
                    let title = snippet.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let description = snippet
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let thumbnail_url = snippet
                        .get("thumbnails")
                        .and_then(|v| v.get("default"))
                        .and_then(|v| v.get("url"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    videos.push(Video {
                        id: video_id.to_string(),
                        title: title.to_string(),
                        description: description.to_string(),
                        thumbnail_url: thumbnail_url.to_string(),
                    });
                }
            }
        }
    }

    Ok(videos)
}
