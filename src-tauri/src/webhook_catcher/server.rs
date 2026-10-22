use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::watch;

use super::forwarder::forward_live_relay;
use super::signature::{detect_body_format, detect_signature};
use super::storage::{append_captured_request, get_captured_requests, load_endpoints};
use super::types::{CapturedWebhookRequest, WebhookServerStatus};

const DEFAULT_WEBHOOK_PORT: u16 = 17692;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024; // 2MB

static SERVER_MANAGER: OnceLock<Arc<WebhookServerManager>> = OnceLock::new();

pub struct WebhookServerManager {
    inner: Mutex<Option<ServerHandle>>,
}

struct ServerHandle {
    port: u16,
    stop_tx: watch::Sender<bool>,
}

impl WebhookServerManager {
    pub fn global() -> Arc<Self> {
        SERVER_MANAGER
            .get_or_init(|| {
                Arc::new(Self {
                    inner: Mutex::new(None),
                })
            })
            .clone()
    }

    pub fn get_status(&self) -> WebhookServerStatus {
        let guard = self.inner.lock().unwrap();
        let endpoints = load_endpoints().unwrap_or_default();
        let active_count = endpoints.iter().filter(|e| e.is_active).count();
        let requests_count = get_captured_requests(None, None).len();

        if let Some(ref handle) = *guard {
            WebhookServerStatus {
                is_running: true,
                port: handle.port,
                active_endpoints_count: active_count,
                captured_requests_count: requests_count,
            }
        } else {
            WebhookServerStatus {
                is_running: false,
                port: DEFAULT_WEBHOOK_PORT,
                active_endpoints_count: active_count,
                captured_requests_count: requests_count,
            }
        }
    }

    pub async fn start(&self, requested_port: Option<u16>) -> Result<u16, String> {
        {
            let guard = self.inner.lock().unwrap();
            if let Some(ref handle) = *guard {
                return Ok(handle.port);
            }
        }

        let port_candidate = requested_port.unwrap_or(DEFAULT_WEBHOOK_PORT);
        let addr = SocketAddr::from(([127, 0, 0, 1], port_candidate));

        // 尝试绑定
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(_) => {
                let dynamic_addr = SocketAddr::from(([127, 0, 0, 1], 0));
                TcpListener::bind(dynamic_addr)
                    .await
                    .map_err(|e| format!("Failed to bind local webhook port: {}", e))?
            }
        };

        let bound_port = listener
            .local_addr()
            .map_err(|e| format!("Failed to get listener local_addr: {}", e))?
            .port();

        let (stop_tx, mut stop_rx) = watch::channel(false);

        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() {
                            break;
                        }
                    }
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((stream, client_addr)) => {
                                let io = TokioIo::new(stream);
                                tauri::async_runtime::spawn(async move {
                                    let service = service_fn(move |req| {
                                        handle_incoming_webhook(req, client_addr)
                                    });
                                    if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                                        log::debug!("Error serving webhook connection: {:?}", err);
                                    }
                                });
                            }
                            Err(e) => {
                                log::debug!("Webhook listener accept error: {:?}", e);
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        }
                    }
                }
            }
        });

        let mut guard = self.inner.lock().unwrap();
        *guard = Some(ServerHandle {
            port: bound_port,
            stop_tx,
        });

        Ok(bound_port)
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap();
        if let Some(handle) = guard.take() {
            let _ = handle.stop_tx.send(true);
        }
        Ok(())
    }
}

async fn handle_incoming_webhook(
    req: Request<Incoming>,
    client_addr: SocketAddr,
) -> Result<Response<Full<Bytes>>, hyper::http::Error> {
    let method = req.method().as_str().to_string();
    let uri = req.uri();
    let path = uri.path().to_string();

    let query_params: HashMap<String, String> = uri
        .query()
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default();

    let mut headers = HashMap::new();
    let mut content_type = String::new();
    for (name, val) in req.headers() {
        if let Ok(v_str) = val.to_str() {
            headers.insert(name.as_str().to_string(), v_str.to_string());
            if name.as_str().eq_ignore_ascii_case("content-type") {
                content_type = v_str.to_string();
            }
        }
    }

    // 匹配端点
    let endpoints = load_endpoints().unwrap_or_default();
    let matched_endpoint = endpoints
        .into_iter()
        .filter(|e| e.is_active)
        .find(|e| {
            let normalized_ep_path = if e.path.starts_with('/') {
                e.path.clone()
            } else {
                format!("/{}", e.path)
            };
            path == normalized_ep_path || path.starts_with(&format!("{}/", normalized_ep_path))
        });

    let endpoint = match matched_endpoint {
        Some(ep) => ep,
        None => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("content-type", "application/json")
                .body(Full::new(Bytes::from(
                    "{\"error\":\"Webhook endpoint not found or inactive\"}",
                )));
        }
    };

    // 读取 Body（带上限截断保护）
    let (body_bytes, is_truncated) = match req.into_body().collect().await {
        Ok(collected) => {
            let b = collected.to_bytes();
            if b.len() > MAX_BODY_BYTES {
                (b.slice(..MAX_BODY_BYTES), true)
            } else {
                (b, false)
            }
        }
        Err(_) => (Bytes::new(), false),
    };

    let body_str = String::from_utf8_lossy(&body_bytes).to_string();
    let body_format = detect_body_format(&content_type, &body_str);
    let signature = detect_signature(&headers);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let captured = CapturedWebhookRequest {
        id: uuid::Uuid::new_v4().to_string(),
        endpoint_id: endpoint.id.clone(),
        endpoint_name: endpoint.name.clone(),
        timestamp: now,
        method,
        path,
        query_params,
        headers,
        client_ip: client_addr.ip().to_string(),
        body: body_str,
        body_format,
        is_truncated,
        detected_signature: signature,
    };

    // 存入内存队列
    append_captured_request(captured.clone());

    // 实时透传代理转发（若已启用）
    if let Some(ref forward) = endpoint.forward_config {
        if forward.enabled && !forward.target_url.trim().is_empty() {
            let target_url = forward.target_url.clone();
            tauri::async_runtime::spawn(async move {
                forward_live_relay(&captured, &target_url).await;
            });
        }
    }

    // 模拟自定义响应延迟
    let custom_resp = &endpoint.custom_response;
    if custom_resp.delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(custom_resp.delay_ms as u64)).await;
    }

    let status = StatusCode::from_u16(custom_resp.status_code).unwrap_or(StatusCode::OK);
    Response::builder()
        .status(status)
        .header("content-type", &custom_resp.content_type)
        .body(Full::new(Bytes::from(custom_resp.body.clone())))
}
