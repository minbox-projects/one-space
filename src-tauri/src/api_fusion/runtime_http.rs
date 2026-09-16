use super::forwarding::{forward_non_streaming, open_streaming_response};
use super::selection::{
    candidate_providers, classify_failure, register_failure, register_success, resolve_model,
    shuffled_candidates, FailureClass,
};
use super::storage::{local_base_url, read_config, write_config};
use super::{now_ts, FusionConfig, FusionKey, FusionStatus, FusionUpstreamProvider, UpstreamProtocol};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::sync::OnceLock;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex};

pub(in crate::api_fusion) struct RunningServer {
    pub(in crate::api_fusion) port: u16,
    pub(in crate::api_fusion) shutdown: Option<oneshot::Sender<()>>,
}

pub(in crate::api_fusion) static RUNNING_SERVER: OnceLock<Mutex<Option<RunningServer>>> =
    OnceLock::new();

pub(in crate::api_fusion) fn state_lock() -> &'static Mutex<Option<RunningServer>> {
    RUNNING_SERVER.get_or_init(|| Mutex::new(None))
}

pub(in crate::api_fusion) fn status_from_config(
    config: &FusionConfig,
    running: bool,
) -> FusionStatus {
    FusionStatus {
        running,
        enabled: config.enabled,
        port: config.port,
        local_base_url: local_base_url(config.port),
        provider_count: config.providers.len(),
        auto_disabled_count: config
            .providers
            .iter()
            .filter(|provider| provider.auto_disabled)
            .count(),
        key_count: config.keys.iter().filter(|key| key.enabled).count(),
        default_key_id: config.default_key_id.clone(),
    }
}

/// Bind the configured loopback port and spawn the listener.
///
/// On bind failure the port configuration is left untouched and the error is
/// actionable: it names the port and the underlying cause. It never falls back
/// to another port.
pub(in crate::api_fusion) async fn start_server() -> Result<FusionStatus, String> {
    let config = read_config()?;
    let mut guard = state_lock().lock().await;
    if let Some(running) = guard.as_ref() {
        if running.port == config.port {
            return Ok(status_from_config(&config, true));
        }
        if let Some(mut running) = guard.take() {
            if let Some(tx) = running.shutdown.take() {
                let _ = tx.send(());
            }
        }
    }
    let listener = TcpListener::bind(("127.0.0.1", config.port))
        .await
        .map_err(|e| {
            format!(
                "failed to bind API Fusion port {} on 127.0.0.1: {e}",
                config.port
            )
        })?;
    let (tx, rx) = oneshot::channel();
    let port = config.port;
    tauri::async_runtime::spawn(run_server(listener, rx));
    *guard = Some(RunningServer {
        port,
        shutdown: Some(tx),
    });
    Ok(status_from_config(&config, true))
}

pub(in crate::api_fusion) async fn stop_server() -> Result<FusionStatus, String> {
    let config = read_config()?;
    let mut guard = state_lock().lock().await;
    if let Some(mut running) = guard.take() {
        if let Some(tx) = running.shutdown.take() {
            let _ = tx.send(());
        }
    }
    Ok(status_from_config(&config, false))
}

pub(in crate::api_fusion) fn server_status() -> Result<FusionStatus, String> {
    let config = read_config()?;
    let running = state_lock()
        .try_lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false);
    Ok(status_from_config(&config, running))
}

pub(in crate::api_fusion) async fn autostart() -> Result<FusionStatus, String> {
    let config = read_config()?;
    if config.enabled {
        start_server().await
    } else {
        Ok(status_from_config(&config, false))
    }
}

pub(in crate::api_fusion) async fn run_server(
    listener: TcpListener,
    mut shutdown: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _)) => {
                        tauri::async_runtime::spawn(async move {
                            let _ = handle_connection(stream).await;
                        });
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        }
    }
}

#[derive(Debug)]
pub(in crate::api_fusion) struct HttpRequest {
    pub(in crate::api_fusion) method: String,
    pub(in crate::api_fusion) path: String,
    pub(in crate::api_fusion) headers: HashMap<String, String>,
    pub(in crate::api_fusion) body: Vec<u8>,
}

pub(in crate::api_fusion) struct HttpResponse {
    pub(in crate::api_fusion) status: u16,
    pub(in crate::api_fusion) content_type: &'static str,
    pub(in crate::api_fusion) body: Vec<u8>,
}

pub(in crate::api_fusion) async fn read_http_request(
    stream: &mut TcpStream,
) -> Result<HttpRequest, String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let mut header_end = None;
    loop {
        let read = stream.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..read]);
        if let Some(pos) = find_header_end(&buf) {
            header_end = Some(pos);
            break;
        }
        if buf.len() > 1024 * 1024 {
            return Err("request headers too large".to_string());
        }
    }
    let header_end = header_end.ok_or_else(|| "invalid http request".to_string())?;
    let headers_raw = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = headers_raw.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or("").to_string();
    let path = request_parts.next().unwrap_or("").to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[(header_end + 4)..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..read]);
    }
    body.truncate(content_length);
    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

pub(in crate::api_fusion) fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

fn clean_path(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

/// Normalize the incoming path to its canonical `/v1` form.
///
/// OpenAI-compatible clients disagree on whether the configured base URL
/// already ends in `/v1`, so `/chat/completions` and `/v1/chat/completions`
/// must both resolve; upstream always receives the versioned path.
fn canonical_api_path(path: &str) -> Option<&'static str> {
    match path {
        "/v1/models" | "/models" => Some("/v1/models"),
        "/v1/chat/completions" | "/chat/completions" => Some("/v1/chat/completions"),
        "/v1/responses" | "/responses" => Some("/v1/responses"),
        _ => None,
    }
}

pub(in crate::api_fusion) fn json_response(status: u16, body: Value) -> HttpResponse {
    let payload = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    HttpResponse {
        status,
        content_type: "application/json",
        body: payload,
    }
}

pub(in crate::api_fusion) fn reason_for_status(status: u16) -> &'static str {
    match status {
        200..=299 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        502 => "Bad Gateway",
        _ => "Internal Server Error",
    }
}

pub(in crate::api_fusion) fn http_response_bytes(response: HttpResponse) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        response.status,
        reason_for_status(response.status),
        response.content_type,
        response.body.len()
    );
    [header.into_bytes(), response.body].concat()
}

/// Accept credentials from either `Authorization: Bearer <key>` or `x-api-key`.
/// Any enabled local key matches; no enabled key means every request is rejected.
pub(in crate::api_fusion) fn is_authorized(request: &HttpRequest, config: &FusionConfig) -> bool {
    let enabled: Vec<&FusionKey> = config
        .keys
        .iter()
        .filter(|key| key.enabled && !key.value.trim().is_empty())
        .collect();
    if enabled.is_empty() {
        return false;
    }
    let bearer = request
        .headers
        .get("authorization")
        .and_then(|value| value.trim().strip_prefix("Bearer "))
        .map(str::trim);
    let x_api_key = request.headers.get("x-api-key").map(|value| value.trim());
    enabled
        .iter()
        .any(|key| {
            let value = key.value.trim();
            bearer == Some(value) || x_api_key == Some(value)
        })
}

/// Union of local model names across enabled, non-auto-disabled providers.
pub(in crate::api_fusion) fn local_model_names(config: &FusionConfig) -> Vec<String> {
    let mut names: Vec<String> = config
        .providers
        .iter()
        .filter(|provider| provider.enabled && !provider.auto_disabled)
        .flat_map(|provider| {
            provider
                .mappings
                .iter()
                .map(|mapping| mapping.local_model.trim().to_string())
        })
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

pub(in crate::api_fusion) fn models_payload(config: &FusionConfig) -> Value {
    let data: Vec<Value> = local_model_names(config)
        .into_iter()
        .map(|id| json!({ "id": id, "object": "model" }))
        .collect();
    json!({ "object": "list", "data": data })
}

fn all_unavailable_payload(message: impl Into<String>) -> Value {
    json!({
        "error": {
            "message": message.into(),
            "type": "server_error",
            "code": "all_providers_unavailable",
        }
    })
}

fn no_candidate_message(
    config: &FusionConfig,
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> String {
    let model = requested.unwrap_or("<none>");
    let endpoint = protocol.endpoint_path();
    let enabled: Vec<String> = config
        .providers
        .iter()
        .filter(|provider| provider.enabled && !provider.auto_disabled)
        .map(|provider| {
            if provider.protocol != protocol {
                format!(
                    "{} is configured for {}",
                    provider.name,
                    provider.protocol.endpoint_path()
                )
            } else {
                format!("{} cannot serve model '{}'", provider.name, model)
            }
        })
        .collect();
    if enabled.is_empty() {
        format!(
            "all providers unavailable: no enabled provider can serve model '{model}' via {endpoint}"
        )
    } else {
        format!(
            "all providers unavailable: no enabled provider can serve model '{model}' via {endpoint}; {}",
            enabled.join("; ")
        )
    }
}

fn failure_reason(status: u16, body_parsed: bool) -> String {
    if body_parsed {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status} with a non-JSON body")
    }
}

fn all_unavailable_message(failures: &[(String, String)]) -> String {
    if failures.is_empty() {
        return "all providers unavailable: every candidate failed".to_string();
    }
    format!(
        "all providers unavailable: {}",
        failures
            .iter()
            .map(|(name, reason)| format!("{name}: {reason}"))
            .collect::<Vec<_>>()
            .join("; ")
    )
}

fn apply_failure(
    config: &mut FusionConfig,
    provider: &FusionUpstreamProvider,
    class: FailureClass,
    reason: &str,
) {
    let at = now_ts();
    if let Some(stored) = config
        .providers
        .iter_mut()
        .find(|stored| stored.id == provider.id)
    {
        register_failure(stored, class, reason, at);
    }
    let _ = write_config(config);
}

fn apply_success(config: &mut FusionConfig, provider: &FusionUpstreamProvider) {
    if let Some(stored) = config
        .providers
        .iter_mut()
        .find(|stored| stored.id == provider.id)
    {
        register_success(stored);
    }
    let _ = write_config(config);
}

/// Try each candidate at most once in the given order for a non-streaming request.
pub(in crate::api_fusion) async fn attempt_non_streaming(
    ordered: &[FusionUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    config: &mut FusionConfig,
) -> HttpResponse {
    let mut failures: Vec<(String, String)> = Vec::new();
    for provider in ordered {
        let Some(model) = resolve_model(provider, requested) else {
            continue;
        };
        match forward_non_streaming(provider, path, body, &model).await {
            Ok(response) => {
                if response.status < 400 && response.parsed {
                    apply_success(config, provider);
                    return HttpResponse {
                        status: response.status,
                        content_type: "application/json",
                        body: response.body,
                    };
                }
                let class = classify_failure(response.status, false, response.parsed);
                if class == FailureClass::ReturnToClient {
                    return HttpResponse {
                        status: response.status,
                        content_type: "application/json",
                        body: response.body,
                    };
                }
                let reason = failure_reason(response.status, response.parsed);
                apply_failure(config, provider, class, &reason);
                failures.push((provider.name.clone(), reason));
            }
            Err(error) => {
                let reason = format!("network error: {error}");
                apply_failure(config, provider, FailureClass::Retryable, &reason);
                failures.push((provider.name.clone(), reason));
            }
        }
    }
    json_response(502, all_unavailable_payload(all_unavailable_message(&failures)))
}

/// A 2xx streaming response is only a success if it actually looks like a
/// Server-Sent Events stream. Checked before any byte reaches the caller so a
/// plain non-SSE body stays a retryable failure and can still switch.
fn is_sse_response_chunk(content_type: &str, first_chunk: &[u8]) -> bool {
    if !content_type.to_ascii_lowercase().contains("text/event-stream") {
        return false;
    }
    if first_chunk.is_empty() {
        return true;
    }
    let Ok(text) = std::str::from_utf8(first_chunk) else {
        return false;
    };
    let trimmed = text.trim_start_matches(['\r', '\n', ' ', '\t']);
    trimmed.starts_with("data:")
        || trimmed.starts_with("event:")
        || trimmed.starts_with("id:")
        || trimmed.starts_with("retry:")
        || trimmed.starts_with(':')
}

/// Try each candidate at most once for a streaming request.
///
/// Switching is permitted only until the first byte is written to `writer`; once
/// written, an upstream failure terminates the stream without retrying.
pub(in crate::api_fusion) async fn attempt_streaming<W: AsyncWrite + Unpin>(
    writer: &mut W,
    ordered: &[FusionUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    config: &mut FusionConfig,
) -> Result<(), String> {
    let mut failures: Vec<(String, String)> = Vec::new();
    for provider in ordered {
        let Some(model) = resolve_model(provider, requested) else {
            continue;
        };
        let response = match open_streaming_response(provider, path, body, &model).await {
            Ok(response) => response,
            Err(error) => {
                let reason = format!("network error: {error}");
                apply_failure(config, provider, FailureClass::Retryable, &reason);
                failures.push((provider.name.clone(), reason));
                continue;
            }
        };
        let status = response.status().as_u16();
        if status >= 400 {
            let bytes = match response.bytes().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    let reason = format!("network error: {error}");
                    apply_failure(config, provider, FailureClass::Retryable, &reason);
                    failures.push((provider.name.clone(), reason));
                    continue;
                }
            };
            let parsed = serde_json::from_slice::<Value>(&bytes).is_ok();
            let class = classify_failure(status, false, parsed);
            if class == FailureClass::ReturnToClient {
                write_response(
                    writer,
                    HttpResponse {
                        status,
                        content_type: "application/json",
                        body: bytes.to_vec(),
                    },
                )
                .await?;
                return Ok(());
            }
            let reason = failure_reason(status, parsed);
            apply_failure(config, provider, class, &reason);
            failures.push((provider.name.clone(), reason));
            continue;
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        let mut chunks = response.bytes_stream();
        match chunks.next().await {
            Some(Ok(first)) => {
                if !is_sse_response_chunk(&content_type, &first) {
                    let reason = format!(
                        "2xx response is not a valid SSE stream (content-type: {content_type})"
                    );
                    apply_failure(config, provider, FailureClass::Retryable, &reason);
                    failures.push((provider.name.clone(), reason));
                    continue;
                }
                write_stream_headers(writer, status).await?;
                writer.write_all(&first).await.map_err(|e| e.to_string())?;
                let _ = writer.flush().await;
                loop {
                    match chunks.next().await {
                        Some(Ok(chunk)) => {
                            if writer.write_all(&chunk).await.is_err() {
                                return Ok(());
                            }
                        }
                        // Bytes already sent: terminate the stream, never switch.
                        Some(Err(_)) => return Ok(()),
                        None => return Ok(()),
                    }
                }
            }
            Some(Err(error)) => {
                let reason = format!("stream failed before first byte: {error}");
                apply_failure(config, provider, FailureClass::Retryable, &reason);
                failures.push((provider.name.clone(), reason));
            }
            None => {
                let reason = "upstream returned an empty stream".to_string();
                apply_failure(config, provider, FailureClass::Retryable, &reason);
                failures.push((provider.name.clone(), reason));
            }
        }
    }

    let payload = all_unavailable_payload(all_unavailable_message(&failures));
    let sse = format!(
        "data: {}\n\ndata: [DONE]\n\n",
        serde_json::to_string(&payload).unwrap_or_default()
    );
    write_stream_headers(writer, 200).await?;
    writer
        .write_all(sse.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: HttpResponse,
) -> Result<(), String> {
    writer
        .write_all(&http_response_bytes(response))
        .await
        .map_err(|e| e.to_string())
}

async fn write_stream_headers<W: AsyncWrite + Unpin>(
    writer: &mut W,
    status: u16,
) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n",
        status,
        reason_for_status(status)
    );
    writer
        .write_all(header.as_bytes())
        .await
        .map_err(|e| e.to_string())
}

pub(in crate::api_fusion) async fn handle_connection(mut stream: TcpStream) -> Result<(), String> {
    let request = match read_http_request(&mut stream).await {
        Ok(request) => request,
        Err(error) => {
            let response = json_response(
                400,
                json!({ "error": { "message": error, "type": "invalid_request_error" } }),
            );
            let _ = stream.write_all(&http_response_bytes(response)).await;
            return Ok(());
        }
    };

    let mut config = match read_config() {
        Ok(config) => config,
        Err(error) => {
            let response = json_response(500, json!({ "error": { "message": error } }));
            let _ = stream.write_all(&http_response_bytes(response)).await;
            return Ok(());
        }
    };

    let raw_path = clean_path(&request.path);
    let Some(path) = canonical_api_path(raw_path) else {
        let response = json_response(
            404,
            json!({
                "error": {
                    "message": format!("unknown path: {raw_path}"),
                    "type": "invalid_request_error",
                    "code": "not_found",
                }
            }),
        );
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    };
    let is_models = path == "/v1/models";

    if !is_authorized(&request, &config) {
        let response = json_response(
            401,
            json!({
                "error": {
                    "message": "invalid or missing local API key",
                    "type": "invalid_request_error",
                    "code": "invalid_api_key",
                }
            }),
        );
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    if is_models {
        if request.method != "GET" {
            let response = json_response(
                404,
                json!({ "error": { "message": "not found", "type": "invalid_request_error" } }),
            );
            stream
                .write_all(&http_response_bytes(response))
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        let response = json_response(200, models_payload(&config));
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    if request.method != "POST" {
        let response = json_response(
            404,
            json!({ "error": { "message": "not found", "type": "invalid_request_error" } }),
        );
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    let body_value: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(error) => {
            let response = json_response(
                400,
                json!({ "error": { "message": error.to_string(), "type": "invalid_request_error" } }),
            );
            stream
                .write_all(&http_response_bytes(response))
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    };
    let requested = body_value
        .get("model")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let wants_stream = body_value
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let protocol = if path == "/v1/responses" {
        UpstreamProtocol::Responses
    } else {
        UpstreamProtocol::ChatCompletions
    };
    let candidates: Vec<FusionUpstreamProvider> =
        candidate_providers(&config.providers, requested.as_deref(), protocol)
            .into_iter()
            .cloned()
            .collect();
    if candidates.is_empty() {
        let message = no_candidate_message(&config, requested.as_deref(), protocol);
        if wants_stream {
            let payload = all_unavailable_payload(message);
            let sse = format!(
                "data: {}\n\ndata: [DONE]\n\n",
                serde_json::to_string(&payload).unwrap_or_default()
            );
            write_stream_headers(&mut stream, 200).await?;
            stream
                .write_all(sse.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
        } else {
            let response = json_response(502, all_unavailable_payload(message));
            stream
                .write_all(&http_response_bytes(response))
                .await
                .map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    let ordered = shuffled_candidates(&candidates);
    if wants_stream {
        attempt_streaming(
            &mut stream,
            &ordered,
            path,
            &request.body,
            requested.as_deref(),
            &mut config,
        )
        .await?;
    } else {
        let response = attempt_non_streaming(
            &ordered,
            path,
            &request.body,
            requested.as_deref(),
            &mut config,
        )
        .await;
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
