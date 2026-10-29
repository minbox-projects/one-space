use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant};
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};

const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024; // 5 MB 上限保护

#[derive(Debug, Clone, Deserialize)]
pub struct HttpRequestPayload {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpResponsePayload {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub elapsed_ms: u64,
    pub is_truncated: bool,
}

#[tauri::command]
pub async fn send_http_request(
    request: HttpRequestPayload,
) -> Result<HttpResponsePayload, String> {
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(30000));
    let client = crate::proxy::apply_current_proxy_to_builder(Client::builder().timeout(timeout))
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let method = Method::from_str(&request.method.to_uppercase())
        .map_err(|e| format!("Invalid HTTP method '{}': {e}", request.method))?;

    let mut req_builder = client.request(method, &request.url);

    // 设置请求头
    for (name, val) in request.headers {
        if let (Ok(header_name), Ok(header_val)) = (
            HeaderName::from_str(&name),
            HeaderValue::from_str(&val),
        ) {
            req_builder = req_builder.header(header_name, header_val);
        }
    }

    // 设置请求体
    if let Some(body_content) = request.body {
        req_builder = req_builder.body(body_content);
    }

    let start_time = Instant::now();
    let response = req_builder
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let elapsed_ms = start_time.elapsed().as_millis() as u64;
    let status = response.status().as_u16();
    let status_text = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();

    let mut response_headers = HashMap::new();
    for (k, v) in response.headers() {
        if let Ok(v_str) = v.to_str() {
            response_headers.insert(k.as_str().to_string(), v_str.to_string());
        }
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    let is_truncated = bytes.len() > MAX_RESPONSE_BYTES;
    let slice = if is_truncated {
        &bytes[..MAX_RESPONSE_BYTES]
    } else {
        &bytes[..]
    };

    let body = String::from_utf8_lossy(slice).to_string();

    Ok(HttpResponsePayload {
        status,
        status_text,
        headers: response_headers,
        body,
        elapsed_ms,
        is_truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_serialization() {
        let payload = HttpResponsePayload {
            status: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            body: "test".to_string(),
            elapsed_ms: 120,
            is_truncated: false,
        };
        let json = serde_json::to_string(&payload).expect("serialize");
        assert!(json.contains("\"status\":200"));
        assert!(json.contains("\"elapsed_ms\":120"));
    }
}
