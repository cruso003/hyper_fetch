use chrono::{DateTime, Utc};
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
    job_type: Option<&str>,
) -> Result<Vec<Job>, Box<dyn Error>> {
    let remote_flag = remote_only.unwrap_or(false);
    let is_trending = query.to_lowercase().starts_with("trending:") || query.to_lowercase().starts_with("trending ");
    let clean_query = if is_trending {
        if query.to_lowercase().starts_with("trending:") {
            query.trim_start_matches("trending:").trim()
        } else {
            query.trim_start_matches("trending ").trim()
        }
    } else {
        query
    };

    let cache_key = format!(
        "jobs_{}_{}_{}_{}_{}",
        clean_query.to_lowercase().replace(" ", "_"),
        limit,
        location.to_lowercase().replace(" ", "_"),
        remote_flag,
        job_type.unwrap_or("").to_lowercase().replace(" ", "_")
    );

    if let Some(jobs) = cache::get_cache::<Vec<Job>>(&cache_key) {
        info!(
            "Using cached job data for: {} (limit: {}, location: {}, remote_only: {}, job_type: {:?})",
            query, limit, location, remote_flag, job_type
        );
        return Ok(jobs);
    }

    info!(
        "Fetching fresh job data for: {} (limit: {}, location: {}, remote_only: {}, job_type: {:?})",
        query, limit, location, remote_flag, job_type
    );

    let mut jobs = Vec::new();

    if !location.is_empty() {
        match fetch_remoteok_jobs_with_location(clean_query, limit, location, job_type).await {
            Ok(location_jobs) => {
                info!("Found {} jobs for location: {}", location_jobs.len(), location);
                jobs.extend(location_jobs);
            }
            Err(e) => warn!("Location search failed: {}", e),
        }
    }

    if remote_flag || jobs.len() < limit as usize || is_trending {
        let remaining = limit as usize - jobs.len();
        match fetch_remoteok_jobs(clean_query, remaining as u32, job_type, is_trending).await {
            Ok(remote_jobs) => {
                let modified_remote_jobs = if !location.is_empty() && jobs.is_empty() {
                    remote_jobs
                        .into_iter()
                        .map(|mut job| {
                            job.location = format!("Remote (Worldwide, including {})", location);
                            job
                        })
                        .collect()
                } else {
                    remote_jobs
                };

                // For trending searches, sort by recency
                let sorted_jobs = if is_trending {
                    let mut jobs_with_date: Vec<(Job, Option<DateTime<Utc>>)> = modified_remote_jobs
                        .into_iter()
                        .map(|job| {
                            let date = job.date_posted.as_ref()
                                .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
                                .map(|d| d.with_timezone(&Utc));
                            (job, date)
                        })
                        .collect();
                    jobs_with_date.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.title.cmp(&b.0.title))); // Secondary sort by title for stability
                    jobs_with_date.into_iter().map(|(job, _)| job).collect()
                } else {
                    modified_remote_jobs
                };

                info!("Found {} additional remote jobs", sorted_jobs.len());
                jobs.extend(sorted_jobs);
            }
            Err(e) => warn!("Remote job search failed: {}", e),
        }
    }

    jobs.truncate(limit as usize);

    if !jobs.is_empty() {
        cache::set_cache(&cache_key, &jobs);
        info!("Cached {} jobs with key: {}", jobs.len(), cache_key);
    } else {
        warn!(
            "No jobs found for query: {} (location: {}, remote_only: {}, job_type: {:?})",
            query, location, remote_flag, job_type
        );
    }

    Ok(jobs)
}

async fn fetch_remoteok_jobs(
    query: &str,
    limit: u32,
    job_type: Option<&str>,
    is_trending: bool,
) -> Result<Vec<Job>, Box<dyn Error>> {
    let api_url = "https://remoteok.io/api";

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client.get(api_url).send().await?;

    if !response.status().is_success() {
        return Err(format!("RemoteOK API request failed with status: {}", response.status()).into());
    }

    let jobs_data: Vec<serde_json::Value> = response.json().await?;
    let query_lower = query.to_lowercase();
    let query_parts: Vec<&str> = query_lower.split_whitespace().collect();

    // Define common filler words to exclude from matching
    let filler_words = vec!["jobs", "trending", "remote", "work", "career", "opportunity"];
    let meaningful_parts: Vec<&str> = query_parts
        .iter()
        .copied()
        .filter(|&part| !filler_words.contains(&part) && part.len() > 2) // Exclude short words
        .collect();

    let mut jobs = Vec::new();
    for job in jobs_data.iter().skip(1) {
        let position = job
            .get("position")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_lowercase();

        let description = job
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_lowercase();

        // For trending searches, match at least one meaningful part
        let position_matches = if is_trending {
            if meaningful_parts.is_empty() {
                // Fallback: If no meaningful parts, use query_parts but exclude fillers
                let fallback_parts: Vec<&str> = query_parts
                    .iter()
                    .copied()
                    .filter(|&part| !filler_words.contains(&part))
                    .collect();
                !fallback_parts.is_empty()
                    && fallback_parts.iter().any(|part| position.contains(part) || description.contains(part))
            } else {
                meaningful_parts.iter().any(|part| position.contains(part) || description.contains(part))
            }
        } else {
            !query_parts.is_empty()
                && query_parts.iter().all(|part| position.contains(part) || description.contains(part))
        };
        if !position_matches {
            continue;
        }

        // Apply job_type filter
        let determined_job_type = determine_job_type(job);
        if let Some(jt) = job_type {
            let jt_lower = jt.to_lowercase();
            let type_matches = determined_job_type.as_ref().map_or(false, |t| {
                t.to_lowercase().contains(&jt_lower)
                    || (jt_lower == "full-time" && t.to_lowercase().contains("full"))
                    || (jt_lower == "part-time" && t.to_lowercase().contains("part"))
                    || (jt_lower == "contract"
                        && (t.to_lowercase().contains("contract") || t.to_lowercase().contains("freelance")))
            });
            if !type_matches {
                continue;
            }
        }

        let id = job
            .get("id")
            .and_then(|i| i.as_str())
            .unwrap_or(&format!("remoteok_{}", jobs.len()))
            .to_string();

        let title = job
            .get("position")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();

        let company = job
            .get("company")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let description_raw = job
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        let apply_url = job
            .get("url")
            .and_then(|u| u.as_str())
            .map(|u| {
                if u.starts_with("http") {
                    u.to_string()
                } else {
                    format!("https://remoteok.com{}", u)
                }
            })
            .unwrap_or_default();

        let date_posted = job
            .get("date")
            .and_then(|d| d.as_str())
            .map(|d| d.to_string());

        // Try to get salary from the "salary" field first
        let mut salary_text = job
            .get("salary")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();

        // If salary field is empty, extract from description
        if salary_text.is_empty() {
            let salary_regex = Regex::new(r"\$(\d+(?:,\d+)*(?:\.\d+)?)\s*(?:-|\s*to\s*)\s*\$?(\d+(?:,\d+)*(?:\.\d+)?)\s*(?:a year)?").unwrap();
            if let Some(caps) = salary_regex.captures(&description) {
                salary_text = format!("${} - ${}", caps.get(1).unwrap().as_str(), caps.get(2).unwrap().as_str());
            }
        }

        let (salary_min, salary_max) = parse_salary(&salary_text);

        let logo = job
            .get("logo")
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
            description: description_raw,
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

async fn fetch_remoteok_jobs_with_location(
    query: &str,
    limit: u32,
    location: &str,
    job_type: Option<&str>,
) -> Result<Vec<Job>, Box<dyn Error>> {
    let api_url = "https://remoteok.io/api";

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client.get(api_url).send().await?;

    if !response.status().is_success() {
        return Err(format!("RemoteOK API request failed with status: {}", response.status()).into());
    }

    let jobs_data: Vec<serde_json::Value> = response.json().await?;
    let query_lower = query.to_lowercase();
    let query_parts: Vec<&str> = query_lower.split_whitespace().collect();
    let location_lower = location.to_lowercase();
    let location_parts: Vec<&str> = location_lower.split(',').map(|s| s.trim()).collect();
    let city = location_parts.first().copied().unwrap_or(&location_lower);

    let mut jobs = Vec::new();
    for job in jobs_data.iter().skip(1) {
        let position = job
            .get("position")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_lowercase();

        // Stricter matching: Check if the position contains all parts of the query
        let position_matches = query_parts.iter().all(|part| position.contains(part));
        if !position_matches {
            continue;
        }

        // Check if the job mentions the location
        let description = job
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_lowercase();

        let location_mentioned = description.contains(city) ||
                               description.contains(&location_lower) ||
                               position.contains(city) ||
                               position.contains(&location_lower);

        if !location_mentioned {
            continue;
        }

        // Apply job_type filter
        let determined_job_type = determine_job_type(job);
        if let Some(jt) = job_type {
            let jt_lower = jt.to_lowercase();
            let type_matches = determined_job_type.as_ref().map_or(false, |t| {
                t.to_lowercase().contains(&jt_lower) ||
                (jt_lower == "full-time" && t.to_lowercase().contains("full")) ||
                (jt_lower == "part-time" && t.to_lowercase().contains("part")) ||
                (jt_lower == "contract" && (t.to_lowercase().contains("contract") || t.to_lowercase().contains("freelance")))
            });
            if !type_matches {
                continue;
            }
        }

        let id = job
            .get("id")
            .and_then(|i| i.as_str())
            .unwrap_or(&format!("remoteok_loc_{}", jobs.len()))
            .to_string();

        let title = job
            .get("position")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();

        let company = job
            .get("company")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let job_description = job
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        let apply_url = job
            .get("url")
            .and_then(|u| u.as_str())
            .map(|u| {
                if u.starts_with("http") {
                    u.to_string()
                } else {
                    format!("https://remoteok.com{}", u)
                }
            })
            .unwrap_or_default();

        let date_posted = job
            .get("date")
            .and_then(|d| d.as_str())
            .map(|d| d.to_string());

        let (salary_min, salary_max) = job
            .get("salary")
            .and_then(|s| s.as_str())
            .map(|s| parse_salary(s))
            .unwrap_or((None, None));

        let logo = job
            .get("logo")
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
            location: format!("{} (Remote)", location),
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
            info!("Job type determined from tags: Full-time for job {:?}", job.get("position"));
            return Some("Full-time".to_string());
        } else if tags.iter().any(|&t| t.to_lowercase() == "contract" || t.to_lowercase() == "contractor") {
            info!("Job type determined from tags: Contract for job {:?}", job.get("position"));
            return Some("Contract".to_string());
        } else if tags.iter().any(|&t| t.to_lowercase() == "part_time" || t.to_lowercase() == "part-time") {
            info!("Job type determined from tags: Part-time for job {:?}", job.get("position"));
            return Some("Part-time".to_string());
        } else if tags.iter().any(|&t| t.to_lowercase() == "internship" || t.to_lowercase() == "intern") {
            info!("Job type determined from tags: Internship for job {:?}", job.get("position"));
            return Some("Internship".to_string());
        }
    }
    
    // Try to extract from description
    let description = job.get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("");
        
    let job_type = extract_job_type(description);
    if let Some(ref jt) = job_type {
        info!("Job type determined from description: {} for job {:?}", jt, job.get("position"));
    } else if description.to_lowercase().contains("years'? experience")
        || description.to_lowercase().contains("senior")
        || description.to_lowercase().contains("professional")
        || description.to_lowercase().contains("collaborative team") {
        info!("Job type inferred as Full-time due to professional indicators for job {:?}", job.get("position"));
        return Some("Full-time".to_string());
    } else {
        info!("No job type determined for job {:?}", job.get("position"));
    }
    job_type
}

// Helper function to extract job type from text
fn extract_job_type(text: &str) -> Option<String> {
    let text = text.to_lowercase();
    
    let re_full_time = Regex::new(r"\bfull(?:-|\s)?time\b|fully remote position|competitive salary|benefits package|[\d+]\s*years'? experience").unwrap();
    let re_part_time = Regex::new(r"\bpart(?:-|\s)?time\b").unwrap();
    let re_contract = Regex::new(r"\bcontract(?:or)?\b").unwrap();
    let re_intern = Regex::new(r"\bintern(?:ship)?\b").unwrap();
    let re_temp = Regex::new(r"\b(?:temporary|temp)\b").unwrap();
    let re_freelance = Regex::new(r"\bfreelance\b").unwrap();

    // Check for Full-time first to give it precedence
    if re_full_time.is_match(&text) {
        return Some("Full-time".to_string());
    }

    if re_part_time.is_match(&text) {
        return Some("Part-time".to_string());
    }

    if re_contract.is_match(&text) {
        return Some("Contract".to_string());
    }

    // Check for Internship, but exclude cases where it's negated
    let has_intern = re_intern.find_iter(&text).any(|mat| {
        let start = mat.start();
        let prefix_start = if start >= 20 { start - 20 } else { 0 };
        let prefix = &text[prefix_start..start];
        !prefix.contains("not hiring associate/")
    });
    if has_intern {
        return Some("Internship".to_string());
    }

    if re_temp.is_match(&text) {
        return Some("Temporary".to_string());
    }

    if re_freelance.is_match(&text) {
        return Some("Freelance".to_string());
    }

    None
}

// Enhanced salary parser
fn parse_salary(salary_text: &str) -> (Option<f64>, Option<f64>) {
    if salary_text.is_empty() {
        return (None, None);
    }
    
    let salary_text = salary_text.to_lowercase().replace(" a year", ""); // Remove " a year" suffix
    
    // Check for range format: $X - $Y or $X to $Y
    let range_regex = Regex::new(r"\$(\d+(?:,\d+)*(?:\.\d+)?)\s*(?:-|\s*to\s*)\s*\$?(\d+(?:,\d+)*(?:\.\d+)?)").unwrap();
    
    if let Some(caps) = range_regex.captures(&salary_text) {
        let min_str = caps.get(1).unwrap().as_str().replace(",", "");
        let max_str = caps.get(2).unwrap().as_str().replace(",", "");
        
        let min = min_str.parse::<f64>().ok();
        let max = max_str.parse::<f64>().ok();
        
        return (min, max);
    }
    
    // Check for single value: $X
    let single_regex = Regex::new(r"\$(\d+(?:,\d+)*(?:\.\d+)?)").unwrap();
    if let Some(caps) = single_regex.captures(&salary_text) {
        let val_str = caps.get(1).unwrap().as_str().replace(",", "");
        let val = val_str.parse::<f64>().ok();
        return (val, val);
    }
    
    (None, None)
}
