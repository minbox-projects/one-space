use std::time::{Duration, Instant};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::net::TcpStream;

use super::types::{
    HeartbeatRecord, HttpProbeOptions, MonitorStatus, MonitorTarget, ProbeTestResult, ProbeType,
    TcpProbeOptions,
};

pub async fn execute_probe(target: &MonitorTarget) -> HeartbeatRecord {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let test_res = match target.probe_type {
        ProbeType::Http => {
            let opts = target.http_options.clone().unwrap_or_default();
            probe_http(&target.target, &opts, target.timeout_ms).await
        }
        ProbeType::Tcp => {
            let opts = target.tcp_options.clone().unwrap_or_else(|| {
                // 尝试从 target 解析 host:port
                if let Some((h, p)) = target.target.split_once(':') {
                    TcpProbeOptions {
                        host: h.to_string(),
                        port: p.parse().unwrap_or(80),
                    }
                } else {
                    TcpProbeOptions {
                        host: target.target.clone(),
                        port: 80,
                    }
                }
            });
            probe_tcp(&opts.host, opts.port, target.timeout_ms).await
        }
        ProbeType::Ping => probe_ping(&target.target, target.timeout_ms).await,
        ProbeType::Dns => probe_dns(&target.target, target.timeout_ms).await,
    };

    HeartbeatRecord {
        id: None,
        monitor_id: target.id.clone(),
        timestamp: now,
        status: test_res.status,
        response_time_ms: test_res.response_time_ms,
        status_code: test_res.status_code,
        error_message: if test_res.success {
            None
        } else {
            Some(test_res.message)
        },
        ssl_days_remaining: test_res.ssl_days_remaining,
    }
}

pub fn format_reqwest_error(e: &reqwest::Error) -> String {
    use std::error::Error;
    let mut details = Vec::new();
    let mut curr: Option<&(dyn Error + 'static)> = e.source();
    while let Some(err) = curr {
        let msg = err.to_string();
        if !details.contains(&msg) {
            details.push(msg);
        }
        curr = err.source();
    }
    if details.is_empty() {
        format!("HTTP request error: {}", e)
    } else {
        format!("HTTP request error: {} (Details: {})", e, details.join(" -> "))
    }
}

pub async fn probe_http(
    url: &str,
    options: &HttpProbeOptions,
    timeout_ms: u32,
) -> ProbeTestResult {
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms as u64);

    let default_ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 OneSpace-Monitor/1.0";

    let mut client_builder = reqwest::Client::builder()
        .user_agent(default_ua)
        .timeout(timeout)
        .connect_timeout(timeout.min(Duration::from_millis(3000)))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 {
                attempt.error("too many redirects (max 3)")
            } else {
                attempt.follow()
            }
        }));

    if options.ignore_ssl_errors {
        client_builder = client_builder.danger_accept_invalid_certs(true);
    }

    let client = match client_builder.build() {
        Ok(c) => c,
        Err(e) => {
            return ProbeTestResult {
                success: false,
                status: MonitorStatus::Down,
                response_time_ms: start.elapsed().as_millis() as u32,
                status_code: None,
                message: format!("Failed to build HTTP client: {}", e),
                ssl_days_remaining: None,
            };
        }
    };

    let method = match options.method.to_uppercase().as_str() {
        "POST" => reqwest::Method::POST,
        "HEAD" => reqwest::Method::HEAD,
        "PUT" => reqwest::Method::PUT,
        _ => reqwest::Method::GET,
    };

    let mut req = client.request(method, url);

    // 填充 headers（默认注入 Accept: */*，并优先采用用户自定义的 headers 包括 User-Agent）
    let mut header_map = HeaderMap::new();
    header_map.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("*/*"),
    );
    for (k, v) in &options.headers {
        if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
            header_map.insert(name, val);
        }
    }
    req = req.headers(header_map);

    if let Some(ref body) = options.body {
        req = req.body(body.clone());
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return ProbeTestResult {
                success: false,
                status: MonitorStatus::Down,
                response_time_ms: start.elapsed().as_millis() as u32,
                status_code: None,
                message: format_reqwest_error(&e),
                ssl_days_remaining: None,
            };
        }
    };

    let elapsed_ms = start.elapsed().as_millis() as u32;
    let status_code = resp.status().as_u16();

    // 检查状态码
    let expected = if options.expected_status.is_empty() {
        vec![200, 201, 204]
    } else {
        options.expected_status.clone()
    };

    if !expected.contains(&status_code) {
        return ProbeTestResult {
            success: false,
            status: MonitorStatus::Down,
            response_time_ms: elapsed_ms,
            status_code: Some(status_code),
            message: format!("Status code {} not in expected list {:?}", status_code, expected),
            ssl_days_remaining: None,
        };
    }

    // 检查关键词
    if options.keyword.is_some() || options.keyword_absent.is_some() {
        let body_text = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                return ProbeTestResult {
                    success: false,
                    status: MonitorStatus::Down,
                    response_time_ms: elapsed_ms,
                    status_code: Some(status_code),
                    message: format!("Failed to read response body: {}", e),
                    ssl_days_remaining: None,
                };
            }
        };

        if let Some(ref kw) = options.keyword {
            if !body_text.contains(kw) {
                return ProbeTestResult {
                    success: false,
                    status: MonitorStatus::Down,
                    response_time_ms: elapsed_ms,
                    status_code: Some(status_code),
                    message: format!("Expected keyword '{}' not found in response body", kw),
                    ssl_days_remaining: None,
                };
            }
        }

        if let Some(ref kw_absent) = options.keyword_absent {
            if body_text.contains(kw_absent) {
                return ProbeTestResult {
                    success: false,
                    status: MonitorStatus::Down,
                    response_time_ms: elapsed_ms,
                    status_code: Some(status_code),
                    message: format!("Disallowed keyword '{}' was present in response body", kw_absent),
                    ssl_days_remaining: None,
                };
            }
        }
    }

    // 状态判定：如果超过 2000ms 视作 Degraded
    let final_status = if elapsed_ms > 2000 {
        MonitorStatus::Degraded
    } else {
        MonitorStatus::Up
    };

    ProbeTestResult {
        success: true,
        status: final_status,
        response_time_ms: elapsed_ms,
        status_code: Some(status_code),
        message: "OK".to_string(),
        ssl_days_remaining: None,
    }
}

pub async fn probe_tcp(host: &str, port: u16, timeout_ms: u32) -> ProbeTestResult {
    let start = Instant::now();
    let addr = format!("{}:{}", host, port);
    let timeout = Duration::from_millis(timeout_ms as u64);

    let connect_future = TcpStream::connect(&addr);
    match tokio::time::timeout(timeout, connect_future).await {
        Ok(Ok(_stream)) => {
            let elapsed_ms = start.elapsed().as_millis() as u32;
            let status = if elapsed_ms > 1500 {
                MonitorStatus::Degraded
            } else {
                MonitorStatus::Up
            };
            ProbeTestResult {
                success: true,
                status,
                response_time_ms: elapsed_ms,
                status_code: None,
                message: "Connected".to_string(),
                ssl_days_remaining: None,
            }
        }
        Ok(Err(e)) => ProbeTestResult {
            success: false,
            status: MonitorStatus::Down,
            response_time_ms: start.elapsed().as_millis() as u32,
            status_code: None,
            message: format!("TCP connect error: {}", e),
            ssl_days_remaining: None,
        },
        Err(_) => ProbeTestResult {
            success: false,
            status: MonitorStatus::Down,
            response_time_ms: timeout_ms,
            status_code: None,
            message: format!("TCP connect timeout after {}ms", timeout_ms),
            ssl_days_remaining: None,
        },
    }
}

pub async fn probe_ping(target: &str, timeout_ms: u32) -> ProbeTestResult {
    let start = Instant::now();
    let host = target
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or(target)
        .split(':')
        .next()
        .unwrap_or(target);

    let timeout_sec = ((timeout_ms + 999) / 1000).max(1);
    let output = tokio::process::Command::new("ping")
        .arg("-c")
        .arg("1")
        .arg("-W")
        .arg(format!("{}", timeout_sec * 1000))
        .arg(host)
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let elapsed = start.elapsed().as_millis() as u32;
            ProbeTestResult {
                success: true,
                status: MonitorStatus::Up,
                response_time_ms: elapsed,
                status_code: None,
                message: "Ping OK".to_string(),
                ssl_days_remaining: None,
            }
        }
        Ok(out) => {
            let err_msg = String::from_utf8_lossy(&out.stderr);
            ProbeTestResult {
                success: false,
                status: MonitorStatus::Down,
                response_time_ms: start.elapsed().as_millis() as u32,
                status_code: None,
                message: if err_msg.trim().is_empty() {
                    "Ping packet lost".to_string()
                } else {
                    err_msg.trim().to_string()
                },
                ssl_days_remaining: None,
            }
        }
        Err(e) => ProbeTestResult {
            success: false,
            status: MonitorStatus::Down,
            response_time_ms: start.elapsed().as_millis() as u32,
            status_code: None,
            message: format!("Failed to run ping command: {}", e),
            ssl_days_remaining: None,
        },
    }
}

pub async fn probe_dns(target: &str, timeout_ms: u32) -> ProbeTestResult {
    let start = Instant::now();
    let host = target
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or(target)
        .split(':')
        .next()
        .unwrap_or(target);

    let timeout = Duration::from_millis(timeout_ms as u64);
    let lookup_future = tokio::net::lookup_host(format!("{}:80", host));

    match tokio::time::timeout(timeout, lookup_future).await {
        Ok(Ok(addrs)) => {
            let addrs_vec: Vec<_> = addrs.collect();
            if addrs_vec.is_empty() {
                ProbeTestResult {
                    success: false,
                    status: MonitorStatus::Down,
                    response_time_ms: start.elapsed().as_millis() as u32,
                    status_code: None,
                    message: "No DNS records resolved".to_string(),
                    ssl_days_remaining: None,
                }
            } else {
                ProbeTestResult {
                    success: true,
                    status: MonitorStatus::Up,
                    response_time_ms: start.elapsed().as_millis() as u32,
                    status_code: None,
                    message: format!("Resolved: {:?}", addrs_vec),
                    ssl_days_remaining: None,
                }
            }
        }
        Ok(Err(e)) => ProbeTestResult {
            success: false,
            status: MonitorStatus::Down,
            response_time_ms: start.elapsed().as_millis() as u32,
            status_code: None,
            message: format!("DNS resolution error: {}", e),
            ssl_days_remaining: None,
        },
        Err(_) => ProbeTestResult {
            success: false,
            status: MonitorStatus::Down,
            response_time_ms: timeout_ms,
            status_code: None,
            message: format!("DNS resolution timed out after {}ms", timeout_ms),
            ssl_days_remaining: None,
        },
    }
}
