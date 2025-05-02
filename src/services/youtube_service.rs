use reqwest;
use serde::{Deserialize, Serialize};
use serde_json;
use std::error::Error;
use urlencoding::encode;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct Video {
    pub title: String,
    pub url: String,
    #[serde(rename = "videoId")]
    pub video_id: String,
    pub r#type: String,
    pub free: bool,
    pub image: String,
    pub source: String,
    pub difficulty: String,
    pub description: String,
}

pub async fn handle_youtube_scraper(query: &str, limit: u32) -> Result<Vec<Video>, Box<dyn Error>> {
    log::info!("Fetching YouTube data for: {}", query);
    let videos = fetch_youtube_videos(query, limit).await.unwrap_or_else(|_| {
        log::warn!("Failed to fetch videos for query: {}. Returning fallback videos.", query);
        get_fallback_videos(query)
    });
    Ok(videos)
}

async fn fetch_youtube_videos(query: &str, limit: u32) -> Result<Vec<Video>, Box<dyn Error>> {
    let search_url = format!(
        "https://www.youtube.com/results?search_query={}+tutorial",
        encode(query)
    );
    log::info!("Fetching YouTube URL: {}", search_url);

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response = client
        .get(&search_url)
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.5")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("YouTube request failed with status: {}", response.status()).into());
    }

    let body = response.text().await?;
    log::debug!("YouTube response length: {} bytes", body.len());

    let videos = extract_videos_from_html(&body, query, limit)?;
    if videos.is_empty() {
        return Err("No videos found".into());
    }

    log::info!("Fetched {} videos for query: {}", videos.len(), query);
    Ok(videos)
}

fn extract_videos_from_html(html: &str, _query: &str, limit: u32) -> Result<Vec<Video>, Box<dyn Error>> {
    let json_start = html.find("var ytInitialData = ").ok_or("Could not find ytInitialData")?;
    let json_end = html[json_start..].find(";</script>").ok_or("Could not find end of JSON")?;
    let json_str = &html[json_start + 19..json_start + json_end];

    let json_data: serde_json::Value = serde_json::from_str(json_str)?;
    let contents = json_data
        .get("contents")
        .and_then(|c| c.get("twoColumnSearchResultsRenderer"))
        .and_then(|r| r.get("primaryContents"))
        .and_then(|p| p.get("sectionListRenderer"))
        .and_then(|s| s.get("contents"))
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("itemSectionRenderer"))
        .and_then(|i| i.get("contents"))
        .ok_or("Could not find video contents in JSON")?;

    let mut videos = Vec::new();
    if let serde_json::Value::Array(items) = contents {
        for item in items {
            if let Some(video_renderer) = item.get("videoRenderer") {
                let video_id = video_renderer
                    .get("videoId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let title = video_renderer
                    .get("title")
                    .and_then(|t| t.get("runs"))
                    .and_then(|r| r.get(0))
                    .and_then(|r| r.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                let image = video_renderer
                    .get("thumbnail")
                    .and_then(|t| t.get("thumbnails"))
                    .and_then(|t| t.get(0))
                    .and_then(|t| t.get("url"))
                    .and_then(|u| u.as_str())
                    .map(|url| url.split('?').next().unwrap_or(url))
                    .unwrap_or("");

                if video_id.is_empty() || title.is_empty() {
                    continue;
                }

                let url = format!("https://www.youtube.com/watch?v={}", video_id);
                let difficulty = determine_difficulty(title);

                videos.push(Video {
                    title: title.to_string(),
                    url,
                    video_id: video_id.to_string(),
                    r#type: "video".to_string(),
                    free: true,
                    image: image.to_string(),
                    source: "YouTube".to_string(),
                    difficulty,
                    description: "".to_string(),
                });

                if videos.len() >= limit as usize {
                    break;
                }
            }
        }
    }

    Ok(videos)
}

fn get_fallback_videos(query: &str) -> Vec<Video> {
    let fallback_data = vec![
        ("docker", "Docker Tutorial for Beginners", "3c-iBn73dDE"),
        ("rust programming", "Rust Programming Course for Beginners", "MsocPEZBd-M"),
        ("python", "Python Tutorial for Beginners", "rfscVS0vtbw"),
        ("javascript", "JavaScript Tutorial for Beginners", "W6NZfCO5SIk"),
    ];

    let fallback = fallback_data
        .iter()
        .find(|(q, _, _)| query.to_lowercase().contains(q))
        .map(|(_, title, video_id)| {
            let url = format!("https://www.youtube.com/watch?v={}", video_id);
            let image = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id);
            let difficulty = determine_difficulty(title);
            vec![Video {
                title: title.to_string(),
                url,
                video_id: video_id.to_string(),
                r#type: "video".to_string(),
                free: true,
                image,
                source: "YouTube".to_string(),
                difficulty,
                description: "".to_string(),
            }]
        })
        .unwrap_or_else(|| {
            vec![Video {
                title: "Learn the Basics".to_string(),
                url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
                video_id: "dQw4w9WgXcQ".to_string(),
                r#type: "video".to_string(),
                free: true,
                image: "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg".to_string(),
                source: "YouTube".to_string(),
                difficulty: "beginner".to_string(),
                description: "".to_string(),
            }]
        });

    fallback
}

fn determine_difficulty(title: &str) -> String {
    let lower_title = title.to_lowercase();
    if lower_title.contains("beginner") || lower_title.contains("basics") || lower_title.contains("introduction") || lower_title.contains("101") {
        "beginner".to_string()
    } else if lower_title.contains("advanced") || lower_title.contains("expert") || lower_title.contains("master") {
        "advanced".to_string()
    } else {
        "intermediate".to_string()
    }
}
