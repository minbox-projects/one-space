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

    let client = crate::proxy::apply_current_proxy_to_builder(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 3 {
                    attempt.error("too many redirects (max 3)")
                } else {
                    attempt.follow()
                }
            })),
    )
    .map_err(|e| format!("Failed to build HTTP client for replay: {}", e))?
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

    let client = match crate::proxy::apply_current_proxy_to_builder(
        reqwest::Client::builder().timeout(Duration::from_secs(10)),
    )
    .and_then(|builder| builder.build().map_err(|e| e.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::test_support::{SERIAL_GUARD, with_global_proxy, HttpProxyRecorder};

    #[tokio::test]
    async fn webhook_catcher_forwarder_proxy_switch_replay_and_mirror() {
        let _g = SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());

        let mirror_a = HttpProxyRecorder::new("ok").await;
        let mirror_b = HttpProxyRecorder::new("ok").await;
        let replay_upstream_a = HttpProxyRecorder::new("replayed").await;
        let replay_upstream_b = HttpProxyRecorder::new("replayed").await;

        // mirror
        let captured = super::super::types::CapturedWebhookRequest {
            id: "c1".to_string(),
            endpoint_id: "e".to_string(),
            endpoint_name: "e".to_string(),
            timestamp: 0,
            method: "POST".to_string(),
            path: "/hook".to_string(),
            query_params: Default::default(),
            headers: Default::default(),
            client_ip: "127.0.0.1".to_string(),
            body: "body".to_string(),
            body_format: "raw".to_string(),
            is_truncated: false,
            detected_signature: None,
        };

        let a_cfg = crate::config::ProxyConfig {
            proxy_enabled: true, proxy_type: "http".to_string(), proxy_host: "127.0.0.1".to_string(),
            proxy_port: mirror_a.url().split(':').last().unwrap().parse().unwrap(),
            proxy_username: None, proxy_password: None, check_interval: 15,
        };
        let _guard = with_global_proxy(Some(a_cfg));
        forward_live_relay(&captured, "http://mirror.example.test/hook").await;
        assert_eq!(mirror_a.targets(), vec!["http://mirror.example.test/hook".to_string()]);

        drop(_guard);
        let b_cfg = crate::config::ProxyConfig {
            proxy_enabled: true, proxy_type: "http".to_string(), proxy_host: "127.0.0.1".to_string(),
            proxy_port: mirror_b.url().split(':').last().unwrap().parse().unwrap(),
            proxy_username: None, proxy_password: None, check_interval: 15,
        };
        let _guard2 = with_global_proxy(Some(b_cfg));
        forward_live_relay(&captured, "http://mirror.example.test/hook").await;
        assert_eq!(mirror_a.targets().len(), 1);
        assert_eq!(mirror_b.targets(), vec!["http://mirror.example.test/hook".to_string()]);

        // replay
        super::super::storage::append_captured_request(captured.clone());
        let req = super::super::types::ReplayRequest {
            request_id: "c1".to_string(),
            target_url: "http://replay.example.test/hook".to_string(),
            override_headers: None,
            override_body: None,
        };
        // need proxy to replay upstream; but target is replay.example.test, proxy records target. upstream fixture returns body.
        // Re-publish A using replay_upstream_a port? target host differs; proxy records target string. Use replay_upstream_a as proxy.
        drop(_guard2);
        let a_cfg2 = crate::config::ProxyConfig {
            proxy_enabled: true, proxy_type: "http".to_string(), proxy_host: "127.0.0.1".to_string(),
            proxy_port: replay_upstream_a.url().split(':').last().unwrap().parse().unwrap(),
            proxy_username: None, proxy_password: None, check_interval: 15,
        };
        let _guard3 = with_global_proxy(Some(a_cfg2));
        let res = execute_replay(req.clone()).await.unwrap();
        assert_eq!(res.status_code, 200);
        assert_eq!(res.response_body, "replayed");
        assert_eq!(replay_upstream_a.targets(), vec!["http://replay.example.test/hook".to_string()]);

        // B
        drop(_guard3);
        let b_cfg2 = crate::config::ProxyConfig {
            proxy_enabled: true, proxy_type: "http".to_string(), proxy_host: "127.0.0.1".to_string(),
            proxy_port: replay_upstream_b.url().split(':').last().unwrap().parse().unwrap(),
            proxy_username: None, proxy_password: None, check_interval: 15,
        };
        let _guard4 = with_global_proxy(Some(b_cfg2));
        // clear captured? storage already has c1; find works
        let res2 = execute_replay(req).await.unwrap();
        assert_eq!(res2.status_code, 200);
        assert_eq!(replay_upstream_a.targets().len(), 1);
        assert_eq!(replay_upstream_b.targets(), vec!["http://replay.example.test/hook".to_string()]);
    }
}
