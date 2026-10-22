use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use super::types::{CapturedWebhookRequest, WebhookEndpoint};

static ENDPOINTS_LOCK: Mutex<()> = Mutex::new(());
static CAPTURED_REQUESTS: Mutex<Option<VecDeque<CapturedWebhookRequest>>> = Mutex::new(None);
const MAX_CAPTURED_REQUESTS: usize = 200;

fn get_storage_dir() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    Ok(dir)
}

fn get_endpoints_file_path() -> Result<PathBuf, String> {
    let dir = get_storage_dir()?;
    Ok(dir.join("webhook_endpoints.json"))
}

pub fn load_endpoints() -> Result<Vec<WebhookEndpoint>, String> {
    let _guard = ENDPOINTS_LOCK.lock().unwrap();
    let path = get_endpoints_file_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read endpoints json: {}", e))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse endpoints json: {}", e))
}

pub fn save_endpoints(endpoints: &[WebhookEndpoint]) -> Result<(), String> {
    let _guard = ENDPOINTS_LOCK.lock().unwrap();
    let path = get_endpoints_file_path()?;
    let json_bytes = serde_json::to_vec_pretty(endpoints)
        .map_err(|e| format!("Failed to serialize endpoints: {}", e))?;

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, json_bytes)
        .map_err(|e| format!("Failed to write tmp endpoints file: {}", e))?;
    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename endpoints file: {}", e))?;
    Ok(())
}

pub fn upsert_endpoint(endpoint: WebhookEndpoint) -> Result<WebhookEndpoint, String> {
    let mut endpoints = load_endpoints()?;
    let mut found = false;
    for item in endpoints.iter_mut() {
        if item.id == endpoint.id {
            *item = endpoint.clone();
            found = true;
            break;
        }
    }
    if !found {
        endpoints.push(endpoint.clone());
    }
    save_endpoints(&endpoints)?;
    Ok(endpoint)
}

pub fn delete_endpoint(id: &str) -> Result<(), String> {
    let mut endpoints = load_endpoints()?;
    let len_before = endpoints.len();
    endpoints.retain(|e| e.id != id);
    if endpoints.len() != len_before {
        save_endpoints(&endpoints)?;
    }
    clear_captured_requests(Some(id));
    Ok(())
}

pub fn append_captured_request(req: CapturedWebhookRequest) {
    let mut queue_guard = CAPTURED_REQUESTS.lock().unwrap();
    let queue = queue_guard.get_or_insert_with(|| VecDeque::with_capacity(MAX_CAPTURED_REQUESTS));
    if queue.len() >= MAX_CAPTURED_REQUESTS {
        queue.pop_back();
    }
    queue.push_front(req);
}

pub fn get_captured_requests(
    endpoint_id: Option<&str>,
    limit: Option<usize>,
) -> Vec<CapturedWebhookRequest> {
    let queue_guard = CAPTURED_REQUESTS.lock().unwrap();
    let queue = match queue_guard.as_ref() {
        Some(q) => q,
        None => return Vec::new(),
    };

    let max = limit.unwrap_or(MAX_CAPTURED_REQUESTS);
    queue
        .iter()
        .filter(|r| match endpoint_id {
            Some(id) if !id.trim().is_empty() => r.endpoint_id == id,
            _ => true,
        })
        .take(max)
        .cloned()
        .collect()
}

pub fn find_captured_request_by_id(id: &str) -> Option<CapturedWebhookRequest> {
    let queue_guard = CAPTURED_REQUESTS.lock().unwrap();
    let queue = queue_guard.as_ref()?;
    queue.iter().find(|r| r.id == id).cloned()
}

pub fn clear_captured_requests(endpoint_id: Option<&str>) {
    let mut queue_guard = CAPTURED_REQUESTS.lock().unwrap();
    if let Some(queue) = queue_guard.as_mut() {
        if let Some(id) = endpoint_id {
            if !id.trim().is_empty() {
                queue.retain(|r| r.endpoint_id != id);
                return;
            }
        }
        queue.clear();
    }
}
