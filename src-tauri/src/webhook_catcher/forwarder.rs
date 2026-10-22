use std::collections::HashMap;
use std::time::{Duration, Instant};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use super::storage::find_captured_request_by_id;
use super::types::{CapturedWebhookRequest, ReplayRequest, ReplayResult};

pub async fn execute_replay(req: ReplayRequest) -> Result<ReplayResult, String> {
    let captured = find_captured_request_by_id(&req.request_id)
        .ok_or_else(|| format!("Captured request {} not found", req.request_id))?;

    let target_url = req.target_url.trim();
    if !target_url.starts_with("http://") && !target_url.starts_with("https://") {
        return Err("Target URL must start with http:// or https://".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 {
                attempt.error("too many redirects (max 3)")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| format!("Failed to build HTTP client for replay: {}", e))?;

    let method = match captured.method.to_uppercase().as_str() {
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        _ => reqwest::Method::GET,
    };

    let mut request_builder = client.request(method, target_url);

    // Headers 优先级：override_headers > captured.headers (过滤 host、content-length)
    let mut header_map = HeaderMap::new();
    let mut merged_headers = captured.headers.clone();
    if let Some(ref overrides) = req.override_headers {
        for (k, v) in overrides {
            merged_headers.insert(k.clone(), v.clone());
        }
    }

    for (k, v) in &merged_headers {
        let lower = k.to_lowercase();
        if lower == "host" || lower == "content-length" {
            continue;
        }
        if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
            header_map.insert(name, val);
        }
    }
    request_builder = request_builder.headers(header_map);

    // Body 优先级：override_body > captured.body
    let body_payload = req
        .override_body
        .clone()
        .unwrap_or_else(|| captured.body.clone());
    if !body_payload.is_empty() {
        request_builder = request_builder.body(body_payload);
    }

    let start = Instant::now();
    let resp = request_builder
        .send()
        .await
        .map_err(|e| format!("Replay HTTP request failed: {}", e))?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let status_code = resp.status().as_u16();

    let mut resp_headers = HashMap::new();
    for (name, val) in resp.headers() {
        if let Ok(v_str) = val.to_str() {
            resp_headers.insert(name.as_str().to_string(), v_str.to_string());
        }
    }

    // 限制读取响应体最大 2MB
    let body_text = match resp.text().await {
        Ok(t) => {
            if t.len() > 2 * 1024 * 1024 {
                format!("{}... (truncated)", &t[..2 * 1024 * 1024])
            } else {
                t
            }
        }
        Err(e) => format!("Failed to read response body: {}", e),
    };

    let executed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    Ok(ReplayResult {
        status_code,
        duration_ms,
        headers: resp_headers,
        response_body: body_text,
        executed_at,
    })
}

pub async fn forward_live_relay(captured: &CapturedWebhookRequest, target_url: &str) {
    let target = target_url.trim();
    if !target.starts_with("http://") && !target.starts_with("https://") {
        return;
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };

    let method = match captured.method.to_uppercase().as_str() {
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        _ => reqwest::Method::GET,
    };

    let mut request_builder = client.request(method, target);

    let mut header_map = HeaderMap::new();
    for (k, v) in &captured.headers {
        let lower = k.to_lowercase();
        if lower == "host" || lower == "content-length" {
            continue;
        }
        if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
            header_map.insert(name, val);
        }
    }
    request_builder = request_builder.headers(header_map);

    if !captured.body.is_empty() {
        request_builder = request_builder.body(captured.body.clone());
    }

    let _ = request_builder.send().await;
}
