use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use lazy_static::lazy_static;

lazy_static! {
    static ref CACHE: Mutex<HashMap<String, CacheEntry>> = Mutex::new(HashMap::new());
}

const CACHE_DURATION: u64 = 4 * 60 * 60; // 4 hours in seconds

struct CacheEntry {
    data: serde_json::Value,
    timestamp: u64,
}

pub fn get_cache<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    let cache = CACHE.lock().unwrap();
    if let Some(entry) = cache.get(key) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now - entry.timestamp < CACHE_DURATION {
            return serde_json::from_value(entry.data.clone()).ok();
        }
    }
    None
}

pub fn set_cache<T: serde::Serialize>(key: &str, data: &T) {
    let mut cache = CACHE.lock().unwrap();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let data = serde_json::to_value(data).unwrap();
    cache.insert(
        key.to_string(),
        CacheEntry {
            data,
            timestamp,
        },
    );
}

pub fn clear_cache() {
    let mut cache = CACHE.lock().unwrap();
    cache.clear();
}

pub fn remove_cache(key: &str) {
    let mut cache = CACHE.lock().unwrap();
    cache.remove(key);
}
