use std::collections::HashSet;
use std::time::{Duration, Instant};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, LOCATION, SET_COOKIE, USER_AGENT};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlHopDetail {
    pub hop_index: u32,
    pub url: String,
    pub status_code: u16,
    pub status_text: String,
    pub latency_ms: u64,
    pub location: Option<String>,
    pub resolved_next_url: Option<String>,
    pub set_cookies: Vec<String>,
    pub headers: Vec<(String, String)>,
    pub is_final: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlTraceReport {
    pub initial_url: String,
    pub final_url: String,
    pub total_hops: usize,
    pub total_duration_ms: u64,
    pub has_loop: bool,
    pub hops: Vec<UrlHopDetail>,
    pub error: Option<String>,
}

const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

pub async fn trace_redirect_hops(
    initial_url: &str,
    user_agent: Option<&str>,
    custom_headers: Option<&[(String, String)]>,
    max_hops: usize,
) -> UrlTraceReport {
    let mut current_url = initial_url.trim().to_string();
    let effective_max_hops = max_hops.clamp(1, 20);

    // Validate initial URL format
    if Url::parse(&current_url).is_err() {
        if !current_url.contains("://") {
            current_url = format!("https://{}", current_url);
        }
        if Url::parse(&current_url).is_err() {
            return UrlTraceReport {
                initial_url: initial_url.to_string(),
                final_url: current_url,
                total_hops: 0,
                total_duration_ms: 0,
                has_loop: false,
                hops: Vec::new(),
                error: Some("非法的初始 URL 格式".to_string()),
            };
        }
    }

    // Build HTTP client with non-following redirect policy
    let client_builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10));

    let client = crate::proxy::apply_current_proxy_to_builder(client_builder)
        .and_then(|b| b.build().map_err(|e| e.to_string()))
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut hops = Vec::new();
    let mut visited_urls = HashSet::new();
    let mut total_duration_ms: u64 = 0;
    let mut has_loop = false;
    let mut hop_index = 1;
    let mut error_msg: Option<String> = None;

    let ua = user_agent.unwrap_or(DEFAULT_USER_AGENT);

    while hop_index <= effective_max_hops {
        if visited_urls.contains(&current_url) {
            has_loop = true;
            error_msg = Some(format!("检测到重定向环路: {}", current_url));
            break;
        }
        visited_urls.insert(current_url.clone());

        let parsed_base = match Url::parse(&current_url) {
            Ok(u) => u,
            Err(e) => {
                error_msg = Some(format!("解析 URL 出错: {}", e));
                break;
            }
        };

        // Prepare request headers
        let mut headers = HeaderMap::new();
        if let Ok(ua_val) = HeaderValue::from_str(ua) {
            headers.insert(USER_AGENT, ua_val);
        }

        if let Some(extra) = custom_headers {
            for (k, v) in extra {
                if let (Ok(name), Ok(val)) = (
                    HeaderName::from_bytes(k.as_bytes()),
                    HeaderValue::from_str(v),
                ) {
                    headers.insert(name, val);
                }
            }
        }

        let start_time = Instant::now();
        let resp_result = client.get(&current_url).headers(headers).send().await;
        let latency_ms = start_time.elapsed().as_millis() as u64;
        total_duration_ms += latency_ms;

        match resp_result {
            Ok(resp) => {
                let status_code = resp.status().as_u16();
                let status_text = resp
                    .status()
                    .canonical_reason()
                    .unwrap_or("Unknown")
                    .to_string();

                let mut raw_headers = Vec::new();
                let mut set_cookies = Vec::new();
                let mut raw_location = None;

                for (name, val) in resp.headers().iter() {
                    let val_str = val.to_str().unwrap_or("[Binary]").to_string();
                    if name == LOCATION {
                        raw_location = Some(val_str.clone());
                    } else if name == SET_COOKIE {
                        set_cookies.push(val_str.clone());
                    }
                    raw_headers.push((name.as_str().to_string(), val_str));
                }

                // Check if this hop is a redirect (3xx with Location header)
                let is_redirect = resp.status().is_redirection() && raw_location.is_some();
                let resolved_next = if let Some(ref loc) = raw_location {
                    match parsed_base.join(loc) {
                        Ok(next) => Some(next.to_string()),
                        Err(_) => Some(loc.clone()),
                    }
                } else {
                    None
                };

                let hop = UrlHopDetail {
                    hop_index: hop_index as u32,
                    url: current_url.clone(),
                    status_code,
                    status_text,
                    latency_ms,
                    location: raw_location,
                    resolved_next_url: resolved_next.clone(),
                    set_cookies,
                    headers: raw_headers,
                    is_final: !is_redirect,
                };

                hops.push(hop);

                if !is_redirect {
                    // Reached final destination
                    break;
                }

                if let Some(next_url) = resolved_next {
                    current_url = next_url;
                } else {
                    break;
                }

                hop_index += 1;
            }
            Err(err) => {
                error_msg = Some(err.to_string());
                break;
            }
        }
    }

    if hop_index > effective_max_hops && !hops.is_empty() && !hops.last().unwrap().is_final {
        error_msg = Some(format!("超过最大重定向深度限制 ({} 跳)", effective_max_hops));
    }

    let final_url = hops.last().map(|h| h.url.clone()).unwrap_or(current_url);

    UrlTraceReport {
        initial_url: initial_url.to_string(),
        final_url,
        total_hops: hops.len(),
        total_duration_ms,
        has_loop,
        hops,
        error: error_msg,
    }
}

pub mod commands {
    use super::{trace_redirect_hops, UrlTraceReport};

    #[tauri::command]
    pub async fn url_trace_redirects(
        initial_url: String,
        user_agent: Option<String>,
        custom_headers: Option<Vec<(String, String)>>,
        max_hops: Option<usize>,
    ) -> Result<UrlTraceReport, String> {
        let clean = initial_url.trim();
        if clean.is_empty() {
            return Err("初始 URL 不能为空".to_string());
        }

        let max = max_hops.unwrap_or(12);
        let report = trace_redirect_hops(
            clean,
            user_agent.as_deref(),
            custom_headers.as_deref(),
            max,
        )
        .await;

        Ok(report)
    }
}

#[cfg(test)]
mod tests;
