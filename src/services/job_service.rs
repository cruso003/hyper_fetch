use reqwest;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use std::error::Error;
use log::{info, warn};
use tokio::time::Duration;
use regex::Regex;
use crate::services::cache;

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub employer_name: String,
    pub location: String,
    pub description: String,
    pub apply_url: String,
    pub salary_min: Option<f64>,
    pub salary_max: Option<f64>,
    pub date_posted: Option<String>,
    pub remote: bool,
    pub job_type: Option<String>,
    pub employer_logo: Option<String>,
}

pub async fn handle_job_scraper(
    query: &str, 
    limit: u32, 
    location: &str,
    remote_only: Option<bool>,
    job_type: Option<&str>
) -> Result<Vec<Job>, Box<dyn Error>> {
    let remote_flag = remote_only.unwrap_or(false);
    
    // Create a cache key that includes all parameters
    let cache_key = format!(
        "jobs_{}_{}_{}_{}_{}", 
        query.to_lowercase().replace(" ", "_"), 
        limit, 
        location.to_lowercase().replace(" ", "_"),
        remote_flag,
        job_type.unwrap_or("").to_lowercase().replace(" ", "_")
    );
    
    // Check cache
    if let Some(jobs) = cache::get_cache::<Vec<Job>>(&cache_key) {
        info!("Using cached job data for: {} (limit: {}, location: {}, remote_only: {}, job_type: {:?})", 
              query, limit, location, remote_flag, job_type);
        return Ok(jobs);
    }
    
    info!("Fetching fresh job data for: {} (limit: {}, location: {}, remote_only: {}, job_type: {:?})", 
          query, limit, location, remote_flag, job_type);
    
    let mut jobs = Vec::new();
    
    // If a location is specified, try to find jobs with that location first
    if !location.is_empty() {
        match fetch_remoteok_jobs_with_location(query, limit, location, job_type).await {
            Ok(location_jobs) => {
                info!("Found {} jobs for location: {}", location_jobs.len(), location);
                jobs.extend(location_jobs);
            }
            Err(e) => warn!("Location search failed: {}", e),
        }
    }
    
    // If remote_only flag is set or we didn't find enough location-specific jobs,
    // fetch remote jobs
    if remote_flag || jobs.len() < limit as usize {
        let remaining = limit as usize - jobs.len();
        match fetch_remoteok_jobs(query, remaining as u32, job_type).await {
            Ok(remote_jobs) => {
                // For location-based searches with no results, change the location display
                // to indicate worldwide remote availability
                let modified_remote_jobs = if !location.is_empty() && jobs.is_empty() {
                    remote_jobs.into_iter()
                        .map(|mut job| {
                            job.location = format!("Remote (Worldwide, including {})", location);
                            job
                        })
                        .collect::<Vec<Job>>()
                } else {
                    remote_jobs
                };
                
                info!("Found {} additional remote jobs", modified_remote_jobs.len());
                jobs.extend(modified_remote_jobs);
            }
            Err(e) => warn!("Remote job search failed: {}", e),
        }
    }
    
    // Filter to requested limit
    jobs.truncate(limit as usize);
    
    // Cache results if we found any
    if !jobs.is_empty() {
        cache::set_cache(&cache_key, &jobs);
        info!("Cached {} jobs with key: {}", jobs.len(), cache_key);
    } else {
        warn!("No jobs found for query: {} (location: {}, remote_only: {}, job_type: {:?})", 
             query, location, remote_flag, job_type);
    }
    
    Ok(jobs)
}

// Main RemoteOK job fetch function 
async fn fetch_remoteok_jobs(
    query: &str, 
    limit: u32,
    job_type: Option<&str>
) -> Result<Vec<Job>, Box<dyn Error>> {
    let api_url = "https://remoteok.io/api";
    
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .timeout(Duration::from_secs(10))
        .build()?;
    
    let response = client
        .get(api_url)
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(format!("RemoteOK API request failed with status: {}", response.status()).into());
    }
    
    let jobs_data: Vec<serde_json::Value> = response.json().await?;
    let query_lower = query.to_lowercase();
    
    let mut jobs = Vec::new();
    for job in jobs_data.iter().skip(1) { // Skip first item (it's metadata)
        // Only include jobs that match our search query
        let position = job.get("position")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_lowercase();
            
        let tags = job.get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|tag| tag.as_str())
                    .collect::<Vec<&str>>()
                    .join(" ")
                    .to_lowercase()
            })
            .unwrap_or_default();
            
        // Check if job matches our query
        if !position.contains(&query_lower) && !tags.contains(&query_lower) {
            continue;
        }
        
        // Determine job type from tags and description
        let determined_job_type = determine_job_type(job);
        
        // Skip if job type filter specified and doesn't match
        if let Some(jt) = job_type {
            let jt_lower = jt.to_lowercase();
            
            // Check job type - be more flexible with matching
            let type_matches = determined_job_type.as_ref().map_or(false, |t| {
                t.to_lowercase().contains(&jt_lower) ||
                (jt_lower == "full-time" && t.to_lowercase().contains("full")) ||
                (jt_lower == "part-time" && t.to_lowercase().contains("part")) ||
                (jt_lower == "contract" && (t.to_lowercase().contains("contract") || 
                                           t.to_lowercase().contains("freelance")))
            });
            
            if !type_matches {
                continue;
            }
        }
        
        let id = job.get("id")
            .and_then(|i| i.as_str())
            .unwrap_or(&format!("remoteok_{}", jobs.len()))
            .to_string();
            
        let title = job.get("position")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
            
        let company = job.get("company")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
            
        let description = job.get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();
            
        // Fix the URL issue
        let apply_url = job.get("url")
            .and_then(|u| u.as_str())
            .map(|u| {
                if u.starts_with("http") {
                    u.to_string()
                } else {
                    format!("https://remoteok.com{}", u)
                }
            })
            .unwrap_or_default();
            
        let date_posted = job.get("date")
            .and_then(|d| d.as_str())
            .map(|d| d.to_string());
            
        // Parse salary if available
        let (salary_min, salary_max) = job.get("salary")
            .and_then(|s| s.as_str())
            .map(|s| parse_salary(s))
            .unwrap_or((None, None));
        
        // Get company logo if available
        let logo = job.get("logo")
            .and_then(|l| l.as_str())
            .map(|l| {
                if l.starts_with("http") {
                    l.to_string()
                } else {
                    format!("https://remoteok.com/assets/img/jobs/{}", l)
                }
            });
            
        jobs.push(Job {
            id,
            title,
            employer_name: company,
            location: "Remote".to_string(),
            description,
            apply_url,
            salary_min,
            salary_max,
            date_posted,
            remote: true,
            job_type: determined_job_type,
            employer_logo: logo,
        });
        
        if jobs.len() >= limit as usize {
            break;
        }
    }
    
    Ok(jobs)
}

// Find RemoteOK jobs that might be relevant to a specific location
async fn fetch_remoteok_jobs_with_location(
    query: &str, 
    limit: u32,
    location: &str,
    job_type: Option<&str>
) -> Result<Vec<Job>, Box<dyn Error>> {
    let api_url = "https://remoteok.io/api";
    
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .timeout(Duration::from_secs(10))
        .build()?;
    
    let response = client
        .get(api_url)
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(format!("RemoteOK API request failed with status: {}", response.status()).into());
    }
    
    let jobs_data: Vec<serde_json::Value> = response.json().await?;
    let query_lower = query.to_lowercase();
    let location_lower = location.to_lowercase();
    
    // Get city and region/state components for better matching
    let location_parts: Vec<&str> = location_lower.split(',').map(|s| s.trim()).collect();
    let city = location_parts.first().copied().unwrap_or(&location_lower);
    
    let mut jobs = Vec::new();
    for job in jobs_data.iter().skip(1) { // Skip first item (it's metadata)
        // Check if job matches both query and location
        let position = job.get("position")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_lowercase();
            
        let description = job.get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_lowercase();
            
        let tags = job.get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|tag| tag.as_str())
                    .collect::<Vec<&str>>()
                    .join(" ")
                    .to_lowercase()
            })
            .unwrap_or_default();
            
        // First check if it matches our query
        if !position.contains(&query_lower) && !tags.contains(&query_lower) {
            continue;
        }
        
        // Then check if it mentions our location
        let location_mentioned = description.contains(city) || 
                               description.contains(&location_lower) ||
                               position.contains(city) ||
                               position.contains(&location_lower);
                               
        if !location_mentioned {
            continue;
        }
        
        // Check job type if specified
        let determined_job_type = determine_job_type(job);
        
        if let Some(jt) = job_type {
            let jt_lower = jt.to_lowercase();
            
            // Check job type - be more flexible with matching
            let type_matches = determined_job_type.as_ref().map_or(false, |t| {
                t.to_lowercase().contains(&jt_lower) ||
                (jt_lower == "full-time" && t.to_lowercase().contains("full")) ||
                (jt_lower == "part-time" && t.to_lowercase().contains("part")) ||
                (jt_lower == "contract" && (t.to_lowercase().contains("contract") || 
                                           t.to_lowercase().contains("freelance")))
            });
            
            if !type_matches {
                continue;
            }
        }
        
        // If we got here, the job matches our criteria
        let id = job.get("id")
            .and_then(|i| i.as_str())
            .unwrap_or(&format!("remoteok_loc_{}", jobs.len()))
            .to_string();
            
        let title = job.get("position")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
            
        let company = job.get("company")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
            
        let job_description = job.get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();
            
        // Fix the URL issue
        let apply_url = job.get("url")
            .and_then(|u| u.as_str())
            .map(|u| {
                if u.starts_with("http") {
                    u.to_string()
                } else {
                    format!("https://remoteok.com{}", u)
                }
            })
            .unwrap_or_default();
            
        let date_posted = job.get("date")
            .and_then(|d| d.as_str())
            .map(|d| d.to_string());
            
        // Parse salary if available
        let (salary_min, salary_max) = job.get("salary")
            .and_then(|s| s.as_str())
            .map(|s| parse_salary(s))
            .unwrap_or((None, None));
        
        // Get company logo if available
        let logo = job.get("logo")
            .and_then(|l| l.as_str())
            .map(|l| {
                if l.starts_with("http") {
                    l.to_string()
                } else {
                    format!("https://remoteok.com/assets/img/jobs/{}", l)
                }
            });
            
        jobs.push(Job {
            id,
            title,
            employer_name: company,
            location: format!("{} (Remote)", location), // Indicate both location and remote status
            description: job_description,
            apply_url,
            salary_min,
            salary_max,
            date_posted,
            remote: true,
            job_type: determined_job_type,
            employer_logo: logo,
        });
        
        if jobs.len() >= limit as usize {
            break;
        }
    }
    
    Ok(jobs)
}

// Helper function to determine job type from RemoteOK data
fn determine_job_type(job: &serde_json::Value) -> Option<String> {
    // Try to extract from tags first
    if let Some(tag_arr) = job.get("tags").and_then(|t| t.as_array()) {
        let tags: Vec<&str> = tag_arr.iter()
            .filter_map(|t| t.as_str())
            .collect();
            
        if tags.iter().any(|&t| t.to_lowercase() == "full_time" || t.to_lowercase() == "full-time") {
            return Some("Full-time".to_string());
        } else if tags.iter().any(|&t| t.to_lowercase() == "contract" || t.to_lowercase() == "contractor") {
            return Some("Contract".to_string());
        } else if tags.iter().any(|&t| t.to_lowercase() == "part_time" || t.to_lowercase() == "part-time") {
            return Some("Part-time".to_string());
        }
    }
    
    // Try to extract from description
    let description = job.get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("");
        
    extract_job_type(description)
}

// Helper function to extract job type from text
fn extract_job_type(text: &str) -> Option<String> {
    let text = text.to_lowercase();
    
    if text.contains("full-time") || text.contains("fulltime") || text.contains("full time") {
        Some("Full-time".to_string())
    } else if text.contains("part-time") || text.contains("parttime") || text.contains("part time") {
        Some("Part-time".to_string())
    } else if text.contains("contract") || text.contains("contractor") {
        Some("Contract".to_string())
    } else if text.contains("internship") || text.contains("intern") {
        Some("Internship".to_string())
    } else if text.contains("temporary") || text.contains("temp") {
        Some("Temporary".to_string())
    } else if text.contains("freelance") {
        Some("Freelance".to_string())
    } else {
        None
    }
}

// Enhanced salary parser
fn parse_salary(salary_text: &str) -> (Option<f64>, Option<f64>) {
    if salary_text.is_empty() {
        return (None, None);
    }
    
    let salary_text = salary_text.to_lowercase();
    
    // Check for range format: $X - $Y
    let range_regex = Regex::new(r"(\d+[,\d]*(?:\.\d+)?)\s*k?(?:\s*-\s*|\s*to\s*)(\d+[,\d]*(?:\.\d+)?)\s*k?").unwrap();
    
    if let Some(caps) = range_regex.captures(&salary_text) {
        let min_str = caps.get(1).unwrap().as_str().replace(",", "");
        let max_str = caps.get(2).unwrap().as_str().replace(",", "");
        
        let min = min_str.parse::<f64>().ok();
        let max = max_str.parse::<f64>().ok();
        
        // Convert to yearly if K is in the string or amount is small (likely hourly)
        let (min, max) = match (min, max) {
            (Some(min), Some(max)) => {
                if salary_text.contains('k') {
                    (Some(min * 1000.0), Some(max * 1000.0))
                } else if min < 1000.0 && max < 1000.0 {
                    // Likely hourly rates, convert to yearly (40 hrs * 52 weeks)
                    (Some(min * 2080.0), Some(max * 2080.0))
                } else {
                    (Some(min), Some(max))
                }
            }
            _ => (None, None),
        };
        
        return (min, max);
    }
    
    // Check for single value: $X
    let single_regex = Regex::new(r"(\d+[,\d]*(?:\.\d+)?)\s*k?").unwrap();
    if let Some(caps) = single_regex.captures(&salary_text) {
        let val_str = caps.get(1).unwrap().as_str().replace(",", "");
        let val = val_str.parse::<f64>().ok();
        
        let val = val.map(|v| {
            if salary_text.contains('k') {
                v * 1000.0
            } else if v < 100.0 {
                // Likely hourly rate, convert to yearly
                v * 2080.0
            } else {
                v
            }
        });
        
        return (val, val);
    }
    
    (None, None)
}
