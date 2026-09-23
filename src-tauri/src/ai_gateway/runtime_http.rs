use super::forwarding::{forward_non_streaming, open_streaming_response};
use super::selection::{
    candidate_providers, classify_failure_with_message, clear_mapping_runtime_state,
    default_retry_delay, find_probe_candidate, is_quota_exceeded_message, is_retryable_with_message,
    mapping_matches_key, rearm_mapping_probe_cooldown, register_mapping_failure,
    register_mapping_success, resolve_model_for_protocol, resolve_session_id, retry_header_delay,
    session_affinity, try_acquire_probe_guard, weighted_candidates, FailureClass, MappingTarget,
    ModelResolution, ProbeCandidate, ProbeGuard, SessionOrder, MAX_RETRIES_PER_PROVIDER,
};
use super::storage::{local_base_url, read_config, write_config};
use super::usage_log::{
    compute_cost_at_time, extract_upstream_error_text, match_price_for_provider,
    normalize_retention_days, now_millis, parse_usage_from_response, sanitize_error_text,
    CanonicalUsage, SseUsageAccumulator, UsageAccounting, UsageLogEntry, UsageLogRecord,
    UsageLogStore, UsageResult, UsageTokens,
};
use super::{now_ts, GatewayConfig, GatewayKey, GatewayStatus, GatewayUpstreamProvider, UpstreamProtocol};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex};
use tokio::time::{sleep, Instant};

pub(in crate::ai_gateway) struct RunningServer {
    pub(in crate::ai_gateway) port: u16,
    pub(in crate::ai_gateway) shutdown: Option<oneshot::Sender<()>>,
}

pub(in crate::ai_gateway) static RUNNING_SERVER: OnceLock<Mutex<Option<RunningServer>>> =
    OnceLock::new();

pub(in crate::ai_gateway) fn state_lock() -> &'static Mutex<Option<RunningServer>> {
    RUNNING_SERVER.get_or_init(|| Mutex::new(None))
}

/// Application handle captured when the gateway server starts, so a settlement
/// can broadcast a runtime-state transition without any frontend action. `None`
/// means no handle was available and emission is skipped (REQ-005).
static APP_HANDLE: OnceLock<std::sync::Mutex<Option<tauri::AppHandle>>> = OnceLock::new();

fn app_handle_slot() -> &'static std::sync::Mutex<Option<tauri::AppHandle>> {
    APP_HANDLE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Replace the process-wide captured handle; `None` clears it. Called before
/// the early-return path of [`start_server`] so a restart refreshes the slot.
fn capture_app_handle(app: Option<tauri::AppHandle>) {
    let mut guard = app_handle_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *guard = app;
}

fn captured_app_handle() -> Option<tauri::AppHandle> {
    app_handle_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

/// Same-thread recorder for emission observability in tests. The relay settles
/// on a worker thread, so the process-wide handle slot is exercised separately;
/// this only exists to make the transition broadcast deterministic to assert.
#[cfg(test)]
thread_local! {
    pub(in crate::ai_gateway) static CONFIG_UPDATE_EVENTS: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Emit one `ai-gateway-config-update` event when a handle was captured; a
/// missing handle is a no-op that never affects settlement.
fn emit_config_updated() {
    #[cfg(test)]
    CONFIG_UPDATE_EVENTS.with(|events| {
        events
            .borrow_mut()
            .push(super::AI_GATEWAY_CONFIG_UPDATED_EVENT.to_string())
    });
    if let Some(handle) = captured_app_handle() {
        let _ = handle.emit(super::AI_GATEWAY_CONFIG_UPDATED_EVENT, ());
    }
}

pub(in crate::ai_gateway) fn status_from_config(
    config: &GatewayConfig,
    running: bool,
) -> GatewayStatus {
    GatewayStatus {
        running,
        enabled: config.enabled,
        port: config.port,
        local_base_url: local_base_url(config.port),
        provider_count: config.providers.len(),
        auto_disabled_count: config
            .providers
            .iter()
            .flat_map(|provider| provider.mappings.iter())
            .filter(|mapping| mapping.auto_disabled)
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
pub(in crate::ai_gateway) async fn start_server(
    app: Option<tauri::AppHandle>,
) -> Result<GatewayStatus, String> {
    // Store the handle before the early-return path so a restart always
    // refreshes (or clears) the process-wide slot (REQ-005).
    capture_app_handle(app);
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
                "failed to bind AI Gateway port {} on 127.0.0.1: {e}",
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

pub(in crate::ai_gateway) async fn stop_server() -> Result<GatewayStatus, String> {
    let config = read_config()?;
    let mut guard = state_lock().lock().await;
    if let Some(mut running) = guard.take() {
        if let Some(tx) = running.shutdown.take() {
            let _ = tx.send(());
        }
    }
    Ok(status_from_config(&config, false))
}

pub(in crate::ai_gateway) fn server_status() -> Result<GatewayStatus, String> {
    let config = read_config()?;
    let running = state_lock()
        .try_lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false);
    Ok(status_from_config(&config, running))
}

pub(in crate::ai_gateway) async fn autostart(
    app: Option<tauri::AppHandle>,
) -> Result<GatewayStatus, String> {
    let config = read_config()?;
    if config.enabled {
        start_server(app).await
    } else {
        Ok(status_from_config(&config, false))
    }
}

pub(in crate::ai_gateway) async fn run_server(
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
pub(in crate::ai_gateway) struct HttpRequest {
    pub(in crate::ai_gateway) method: String,
    pub(in crate::ai_gateway) path: String,
    pub(in crate::ai_gateway) headers: HashMap<String, String>,
    pub(in crate::ai_gateway) body: Vec<u8>,
}

pub(in crate::ai_gateway) struct HttpResponse {
    pub(in crate::ai_gateway) status: u16,
    pub(in crate::ai_gateway) content_type: &'static str,
    pub(in crate::ai_gateway) body: Vec<u8>,
    /// Usage/provider metadata captured while forwarding, consumed by the
    /// request logger. Never forwarded to the caller.
    pub(in crate::ai_gateway) capture: Option<ForwardCapture>,
}

/// Per-request forwarding metadata used to write exactly one usage log row.
#[derive(Debug, Clone, Default)]
pub(in crate::ai_gateway) struct ForwardCapture {
    pub(in crate::ai_gateway) status: u16,
    pub(in crate::ai_gateway) provider_id: String,
    pub(in crate::ai_gateway) provider_name: String,
    pub(in crate::ai_gateway) upstream_model: String,
    pub(in crate::ai_gateway) usage: Option<UsageTokens>,
    /// No candidate could serve the request (including every candidate failing).
    pub(in crate::ai_gateway) all_unavailable: bool,
    /// The upstream stream failed after bytes had already reached the caller.
    pub(in crate::ai_gateway) upstream_error: bool,
    /// The downstream client went away mid-forward, so the request is neither
    /// a success nor an error and its whole log buffer is discarded.
    pub(in crate::ai_gateway) downstream_cancelled: bool,
}

impl ForwardCapture {
    /// Final result classification: an HTTP 2xx is success only when the
    /// request was not cancelled, all-unavailable or an upstream error.
    pub(in crate::ai_gateway) fn result(&self) -> UsageResult {
        if self.downstream_cancelled {
            UsageResult::Cancelled
        } else if self.all_unavailable || self.upstream_error || self.status >= 400 || self.status == 0 {
            UsageResult::Failure
        } else {
            UsageResult::Success
        }
    }
}

/// One completed upstream attempt of an inbound request, buffered by the
/// connection handler until the request ends (REQ-001).
///
/// A `status` of 0 means the attempt never received an upstream HTTP status
/// (network or stream failure); `result` is `Success` or `Failure` only, because
/// a downstream-cancelled or undeliverable request discards its whole buffer
/// without writing any row. `error_message` is the sanitized upstream error text
/// and must be extracted where the raw bytes are still available.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::ai_gateway) struct AttemptLog {
    pub(in crate::ai_gateway) provider_id: String,
    pub(in crate::ai_gateway) provider_name: String,
    pub(in crate::ai_gateway) upstream_model: String,
    pub(in crate::ai_gateway) status: u16,
    pub(in crate::ai_gateway) result: UsageResult,
    pub(in crate::ai_gateway) error_message: Option<String>,
    pub(in crate::ai_gateway) usage: Option<UsageTokens>,
    /// Whether the captured usage object decomposed into valid canonical tiers
    /// (`false` for a missing or invalid/conflicting usage object).
    pub(in crate::ai_gateway) usage_valid: bool,
    pub(in crate::ai_gateway) duration_ms: u64,
}

impl Default for AttemptLog {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            provider_name: String::new(),
            upstream_model: String::new(),
            status: 0,
            result: UsageResult::Failure,
            error_message: None,
            usage: None,
            usage_valid: false,
            duration_ms: 0,
        }
    }
}

/// Buffer one completed upstream attempt with its own elapsed time. The parsed
/// canonical usage carries both the persisted tiers and their validity.
fn build_attempt_log(
    provider: &GatewayUpstreamProvider,
    upstream_model: &str,
    started: Instant,
    status: u16,
    result: UsageResult,
    error_message: Option<String>,
    usage: Option<CanonicalUsage>,
) -> AttemptLog {
    AttemptLog {
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        upstream_model: upstream_model.to_string(),
        status,
        result,
        error_message,
        usage: usage.map(|canonical| canonical.tokens),
        usage_valid: usage.is_some_and(|canonical| canonical.valid),
        duration_ms: started.elapsed().as_millis().max(1) as u64,
    }
}

pub(in crate::ai_gateway) async fn read_http_request(
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

pub(in crate::ai_gateway) fn find_header_end(buf: &[u8]) -> Option<usize> {
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

/// The protocol an inbound canonical path speaks.
fn protocol_for_path(path: &str) -> UpstreamProtocol {
    if path == "/v1/responses" {
        UpstreamProtocol::Responses
    } else {
        UpstreamProtocol::ChatCompletions
    }
}

pub(in crate::ai_gateway) fn json_response(status: u16, body: Value) -> HttpResponse {
    let payload = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    HttpResponse {
        status,
        content_type: "application/json",
        body: payload,
        capture: None,
    }
}

pub(in crate::ai_gateway) fn reason_for_status(status: u16) -> &'static str {
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

pub(in crate::ai_gateway) fn http_response_bytes(response: HttpResponse) -> Vec<u8> {
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
pub(in crate::ai_gateway) fn is_authorized(request: &HttpRequest, config: &GatewayConfig) -> bool {
    let enabled: Vec<&GatewayKey> = config
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

/// Union of local model names across enabled providers' user-enabled,
/// not-auto-disabled mapping rows.
pub(in crate::ai_gateway) fn local_model_names(config: &GatewayConfig) -> Vec<String> {
    let mut names: Vec<String> = config
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .flat_map(|provider| {
            provider
                .mappings
                .iter()
                .filter(|mapping| mapping.enabled && !mapping.auto_disabled)
                .map(|mapping| mapping.local_model.trim().to_string())
        })
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

pub(in crate::ai_gateway) fn models_payload(config: &GatewayConfig) -> Value {
    let data: Vec<Value> = local_model_names(config)
        .into_iter()
        .map(|id| json!({ "id": id, "object": "model" }))
        .collect();
    json!({ "object": "list", "data": data })
}

/// The one gateway-generated error envelope (REQ-006/AC-010): `error.message`,
/// `error.type` and `error.code` are always present and non-empty, and
/// `error.param` always exists as `null`.
fn error_envelope(message: impl Into<String>, error_type: &str, code: &str) -> Value {
    json!({
        "error": {
            "message": message.into(),
            "type": error_type,
            "code": code,
            "param": null,
        }
    })
}

/// Whether an upstream error body already uses the OpenAI standard shape: valid
/// JSON containing an `error` object. Such a body is passed through unchanged;
/// anything else is wrapped by the gateway (REQ-004/AC-007).
fn is_standard_error_body(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("error").map(Value::is_object))
        .unwrap_or(false)
}

/// The standard envelope for a non-standard upstream 4xx body (REQ-004/AC-006).
/// The message names the upstream status and carries only the readable body
/// text, never credentials or request headers.
fn upstream_error_payload(status: u16, body: &[u8]) -> Value {
    let readable = String::from_utf8_lossy(body);
    let trimmed = readable.trim();
    let message = if trimmed.is_empty() {
        format!("upstream returned HTTP {status} with an empty body")
    } else {
        format!("upstream returned HTTP {status}: {trimmed}")
    };
    error_envelope(message, "upstream_error", "upstream_error")
}

fn all_unavailable_payload(message: impl Into<String>) -> Value {
    error_envelope(message, "server_error", "all_providers_unavailable")
}

fn no_candidate_message(
    config: &GatewayConfig,
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> String {
    let model = requested.unwrap_or("<none>");
    let endpoint = protocol.endpoint_path();
    let enabled: Vec<String> = config
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .map(|provider| match resolve_model_for_protocol(provider, requested, protocol) {
            ModelResolution::ProtocolMismatch(configured) => format!(
                "{} serves model '{}' via {}",
                provider.name,
                model,
                configured.endpoint_path()
            ),
            _ if provider.protocol != protocol => format!(
                "{} is configured for {}",
                provider.name,
                provider.protocol.endpoint_path()
            ),
            _ => format!("{} cannot serve model '{}'", provider.name, model),
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

pub(in crate::ai_gateway) fn format_network_error_reason(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    let detail = if lower.contains("connection refused")
        || lower.contains("failed to connect")
        || lower.contains("unable to connect")
        || lower.contains("connection closed")
    {
        "connection refused / unreachable (请检查上游 Base URL 是否正确或网络代理设置)"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "connection timed out (连接超时，请检查网络稳定性或代理延迟)"
    } else if lower.contains("dns")
        || lower.contains("resolve")
        || lower.contains("name resolution")
        || lower.contains("nodename nor servname provided")
    {
        "DNS resolution failed (无法解析上游域名，请检查 Base URL 拼写或 DNS 设置)"
    } else if lower.contains("certificate") || lower.contains("ssl") || lower.contains("tls") {
        "SSL/TLS certificate error (上游 SSL 证书验证失败)"
    } else {
        "connection failed (网络连接失败)"
    };
    format!("network error: {detail} ({error})")
}

fn failure_reason(status: u16, body_parsed: bool, error_message: Option<&str>) -> String {
    let base = if body_parsed {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status} with a non-JSON body")
    };
    let Some(msg) = error_message.map(str::trim).filter(|m| !m.is_empty()) else {
        return base;
    };
    if status == 429 {
        if is_quota_exceeded_message(error_message) {
            return format!("{base} (额度已用尽 / Quota Exceeded: {msg})");
        } else {
            return format!("{base} (请求频次超限 / Rate Limited: {msg})");
        }
    }
    format!("{base} ({msg})")
}

fn all_unavailable_message(failures: &[(String, String)]) -> String {
    if failures.is_empty() {
        return "all providers unavailable: every candidate failed".to_string();
    }
    let summary = failures
        .iter()
        .map(|(name, reason)| format!("{name}: {reason}"))
        .collect::<Vec<_>>()
        .join("; ");

    let hint = if failures
        .iter()
        .all(|(_, r)| r.contains("Quota Exceeded") || (r.contains("429") && r.contains("额度已用尽")))
    {
        " [提示: 所有服务商额度均已耗尽，请更换服务商或检查账户额度]"
    } else if failures.iter().all(|(_, r)| r.contains("network error")) {
        " [提示: 无法连接到上游服务，请检查服务商 Base URL 与网络/代理设置]"
    } else {
        ""
    };
    format!("all providers unavailable: {summary}{hint}")
}

/// Persist one immediate-disable outcome (401/403) against the latest on-disk
/// configuration.
///
/// The request-start snapshot is never written back: it may predate providers,
/// keys, prices or toggles saved while this request was in flight (a streaming
/// response or retry backoff can span tens of seconds), and a whole-file
/// rewrite from it would silently discard those concurrent edits. Only the
/// matching mapping rows are touched; a provider deleted mid-request stays
/// deleted.
fn apply_failure(target: &MappingTarget, class: FailureClass, reason: &str) {
    let at = now_ts();
    let Ok(mut latest) = read_config() else {
        return;
    };
    let mut flipped = false;
    if let Some(stored) = latest
        .providers
        .iter_mut()
        .find(|stored| stored.id == target.provider_id)
    {
        let before = auto_disabled_snapshot(stored, target);
        register_mapping_failure(stored, target, class, reason, at);
        flipped = auto_disabled_flipped(stored, target, &before);
    }
    if write_config(&latest).is_ok() && flipped {
        emit_config_updated();
    }
}

/// Snapshot the `auto_disabled` flags of the mapping rows matching `target`'s
/// trimmed key, so a settlement can detect a transition in either direction.
fn auto_disabled_snapshot(
    provider: &GatewayUpstreamProvider,
    target: &MappingTarget,
) -> Vec<bool> {
    provider
        .mappings
        .iter()
        .filter(|mapping| {
            mapping_matches_key(mapping, &target.local_model, &target.upstream_model)
        })
        .map(|mapping| mapping.auto_disabled)
        .collect()
}

/// Whether any row matching `target` changed `auto_disabled` since `before`.
fn auto_disabled_flipped(
    provider: &GatewayUpstreamProvider,
    target: &MappingTarget,
    before: &[bool],
) -> bool {
    provider
        .mappings
        .iter()
        .filter(|mapping| {
            mapping_matches_key(mapping, &target.local_model, &target.upstream_model)
        })
        .map(|mapping| mapping.auto_disabled)
        .zip(before.iter().copied())
        .any(|(after, was)| after != was)
}

/// One provider still eligible for a bounded retry inside the current request.
/// This small scheduling state keeps the request's retry queue ordered by the
/// earliest monotonic deadline; equal deadlines keep the initial candidate order.
struct RetryCandidate {
    provider: GatewayUpstreamProvider,
    model: String,
    /// Upstream attempts already made for this provider in this request.
    attempts: u32,
    // None represents a valid header delay beyond the clock's range.
    ready_at: Option<Instant>,
}

impl RetryCandidate {
    async fn next_ready(candidates: &[RetryCandidate], remaining_wait: &mut Duration) -> Option<usize> {
        let index = Self::earliest(candidates)?;
        let ready_at = candidates[index].ready_at?;
        let now = Instant::now();
        let wait = ready_at.saturating_duration_since(now);
        if wait > *remaining_wait {
            return None;
        }
        if !wait.is_zero() {
            sleep(wait).await;
            *remaining_wait = remaining_wait.saturating_sub(now.elapsed());
        }
        Some(index)
    }

    fn earliest(candidates: &[RetryCandidate]) -> Option<usize> {
        candidates
            .iter()
            .enumerate()
            .min_by_key(|(index, candidate)| (candidate.ready_at.is_none(), candidate.ready_at, *index))
            .map(|(index, _)| index)
    }
}

/// Result of a single upstream attempt, classified for the retry loop.
enum AttemptResult {
    /// A usable response, returned to the caller unchanged.
    Success(HttpResponse),
    /// A client error the caller must see unchanged (400/413/422/...).
    ReturnToClient(HttpResponse),
    Failure {
        class: FailureClass,
        retryable: bool,
        /// True for a network/transport failure (send, body read, stream open
        /// or mid-stream read); false for an HTTP-status failure.
        transport: bool,
        reason: String,
        retry_delay: Option<Duration>,
    },
}

/// Per-request mapping-row health accumulation.
///
/// Health is counted in inbound-request units, not upstream attempts: however
/// many times a row is tried, its outcome is applied once when the request ends
/// normally. A final success clears the counter, a 404 / rate-limit 429 alone
/// never counts, a quota-exhausted 429 counts once, and a row that also had a
/// network/5xx failure counts once. Only mapping rows
/// carry health: an attempt served through the provider's `default_model` has no
/// target and records nothing.
#[derive(Default)]
struct RequestHealth {
    order: Vec<MappingTarget>,
    outcomes: HashMap<MappingTarget, ProviderOutcome>,
    /// Set once per inbound request from the system-resume grace: while true a
    /// transport `Retryable` failure is not counted toward mapping health.
    suppress_transport_failures: bool,
    /// The one half-open probe this request attempted, if any. Settled in
    /// [`RequestHealth::apply`] against the latest on-disk configuration.
    probe: Option<ProbeSettlement>,
}

#[derive(Default)]
struct ProviderOutcome {
    health_failure: bool,
    disable_immediately: bool,
    succeeded: bool,
    reason: String,
}

/// Outcome of one attempted half-open probe, buffered until the request ends.
struct ProbeSettlement {
    target: MappingTarget,
    /// Attempt time used to re-arm the cooldown on failure.
    at: u64,
    result: ProbeResult,
}

enum ProbeResult {
    Succeeded,
    Failed { transport: bool, reason: String },
}

impl RequestHealth {
    fn entry(&mut self, target: &MappingTarget) -> &mut ProviderOutcome {
        if !self.outcomes.contains_key(target) {
            self.order.push(target.clone());
            self.outcomes
                .insert(target.clone(), ProviderOutcome::default());
        }
        self.outcomes
            .get_mut(target)
            .expect("health entry inserted above")
    }

    fn record_failure(
        &mut self,
        target: &MappingTarget,
        class: FailureClass,
        reason: &str,
        transport: bool,
    ) {
        // A transport failure whose class is Retryable, settled inside the
        // post-resume grace, neither counts nor stamps `last_error_at`
        // (REQ-004/AC-006). HTTP-status failures, immediate auth disables and
        // every other class keep today's behavior.
        if transport && self.suppress_transport_failures && class == FailureClass::Retryable {
            return;
        }
        let entry = self.entry(target);
        match class {
            FailureClass::DisableImmediately => {
                if !entry.disable_immediately {
                    apply_failure(target, class, reason);
                }
                entry.disable_immediately = true;
                entry.reason = reason.to_string();
            }
            FailureClass::Retryable => {
                entry.health_failure = true;
                if entry.reason.is_empty() {
                    entry.reason = reason.to_string();
                }
            }
            // 404 / rate-limit 429 alone never count toward health; other 4xx
            // are returned to the caller and also do not count.
            // Quota-exhausted 429s arrive as Retryable and count above.
            FailureClass::Transient | FailureClass::ReturnToClient => {}
        }
    }

    fn record_success(&mut self, target: &MappingTarget) {
        self.entry(target).succeeded = true;
    }

    /// Mark the request's half-open probe as served: the settlement clears the
    /// probed row's runtime state.
    fn record_probe_success(&mut self, target: &MappingTarget) {
        self.probe = Some(ProbeSettlement {
            target: target.clone(),
            at: now_ts(),
            result: ProbeResult::Succeeded,
        });
    }

    /// Mark the request's half-open probe as failed at `at`: the settlement
    /// re-arms the probed row's cooldown. A suppressed transport failure keeps
    /// the counter, `last_error_at` and `disabled_reason` untouched.
    fn record_probe_failure(
        &mut self,
        target: &MappingTarget,
        at: u64,
        transport: bool,
        reason: &str,
    ) {
        self.probe = Some(ProbeSettlement {
            target: target.clone(),
            at,
            result: ProbeResult::Failed {
                transport,
                reason: reason.to_string(),
            },
        });
    }

    /// Merge this request's probe and non-probe outcomes into the latest on-disk
    /// configuration and persist it. Like [`apply_failure`], this never writes
    /// back a request-start snapshot, so concurrent provider/key/price/toggle
    /// edits survive the settlement of an older in-flight request.
    fn apply(&self) {
        let at = now_ts();
        let Ok(mut latest) = read_config() else {
            return;
        };
        let mut changed = false;
        // Whether any settled row flipped `auto_disabled` during this write:
        // exactly one transition event is emitted after a successful write even
        // when several rows flipped (REQ-005/AC-008).
        let mut flipped = false;
        // The half-open probe settles first: a success clears the row, a failure
        // re-arms its cooldown without ever touching the counter. Details are
        // refreshed unless the failure was a transport failure suppressed by the
        // resume grace (REQ-004/AC-006).
        if let Some(probe) = &self.probe {
            if let Some(stored) = latest
                .providers
                .iter_mut()
                .find(|stored| stored.id == probe.target.provider_id)
            {
                match &probe.result {
                    ProbeResult::Succeeded => {
                        for mapping in stored.mappings.iter_mut().filter(|mapping| {
                            mapping_matches_key(
                                mapping,
                                &probe.target.local_model,
                                &probe.target.upstream_model,
                            )
                        }) {
                            if mapping.auto_disabled {
                                flipped = true;
                            }
                            clear_mapping_runtime_state(mapping);
                            changed = true;
                        }
                    }
                    ProbeResult::Failed { transport, reason } => {
                        let update_details = !(*transport && self.suppress_transport_failures);
                        let matched = stored.mappings.iter().any(|mapping| {
                            mapping_matches_key(
                                mapping,
                                &probe.target.local_model,
                                &probe.target.upstream_model,
                            )
                        });
                        if matched {
                            let before = auto_disabled_snapshot(stored, &probe.target);
                            rearm_mapping_probe_cooldown(
                                stored,
                                &probe.target,
                                reason,
                                probe.at,
                                update_details,
                            );
                            if auto_disabled_flipped(stored, &probe.target, &before) {
                                flipped = true;
                            }
                            changed = true;
                        }
                    }
                }
            }
        }
        for target in &self.order {
            let Some(outcome) = self.outcomes.get(target) else {
                continue;
            };
            if outcome.disable_immediately {
                continue;
            }
            let Some(stored) = latest
                .providers
                .iter_mut()
                .find(|stored| stored.id == target.provider_id)
            else {
                continue;
            };
            if outcome.succeeded {
                register_mapping_success(stored, target);
                changed = true;
            } else if outcome.health_failure {
                let before = auto_disabled_snapshot(stored, target);
                register_mapping_failure(
                    stored,
                    target,
                    FailureClass::Retryable,
                    &outcome.reason,
                    at,
                );
                if auto_disabled_flipped(stored, target, &before) {
                    flipped = true;
                }
                changed = true;
            }
        }
        if changed && write_config(&latest).is_ok() && flipped {
            emit_config_updated();
        }
    }
}

/// Settle one finished attempt on the mapping row it belongs to, if any.
///
/// A `default_model` attempt (or any attempt resolving to no matching row)
/// produces no target and therefore no health outcome.
fn settle_failure(
    health: &mut RequestHealth,
    provider: &GatewayUpstreamProvider,
    requested: Option<&str>,
    upstream_model: &str,
    class: FailureClass,
    reason: &str,
    transport: bool,
) {
    if let Some(target) = MappingTarget::for_request(provider, requested, upstream_model) {
        health.record_failure(&target, class, reason, transport);
    }
}

/// Settle a served attempt on the mapping row it belongs to, if any.
fn settle_success(
    health: &mut RequestHealth,
    provider: &GatewayUpstreamProvider,
    requested: Option<&str>,
    upstream_model: &str,
) {
    if let Some(target) = MappingTarget::for_request(provider, requested, upstream_model) {
        health.record_success(&target);
    }
}

async fn attempt_candidate(
    provider: &GatewayUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    client_headers: &HashMap<String, String>,
) -> (AttemptResult, AttemptLog) {
    let started = Instant::now();
    match forward_non_streaming(provider, path, body, model, client_headers).await {
        Ok(response) => {
            // Usage is only meaningful for a successful 2xx upstream response;
            // an error body that happens to carry `usage` must never be billed.
            let usage = if (200..300).contains(&response.status) {
                parse_usage_from_response(&response.body)
            } else {
                None
            };
            // A served 2xx is a success; every other body is a failed attempt
            // whose error text must be extracted while the raw bytes are still
            // available, because the retryable path below keeps only a status
            // reason (REQ-003).
            let served = response.status < 400 && response.parsed;
            let error_message = if served {
                None
            } else {
                sanitize_error_text(
                    &extract_upstream_error_text(&response.body).unwrap_or_default(),
                    &provider.api_key,
                )
            };
            let log = build_attempt_log(
                provider,
                model,
                started,
                response.status,
                if served {
                    UsageResult::Success
                } else {
                    UsageResult::Failure
                },
                error_message.clone(),
                usage,
            );
            let capture = ForwardCapture {
                status: response.status,
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                upstream_model: model.to_string(),
                usage: usage.map(|canonical| canonical.tokens),
                ..Default::default()
            };
            if served {
                return (
                    AttemptResult::Success(HttpResponse {
                        status: response.status,
                        content_type: "application/json",
                        body: response.body,
                        capture: Some(capture),
                    }),
                    log,
                );
            }
            // Quota-exhausted 429s count toward mapping health (Retryable) but
            // never requeue the same provider; plain rate-limit 429s stay
            // Transient and are still retried.
            let class = classify_failure_with_message(
                response.status,
                false,
                response.parsed,
                error_message.as_deref(),
            );
            if class == FailureClass::ReturnToClient {
                // A standard upstream error body stays byte-for-byte; a
                // non-standard one keeps the status but is wrapped so clients
                // can read `error.message` (REQ-004/AC-006/AC-007).
                let standard = is_standard_error_body(&response.body);
                let body = if standard {
                    response.body
                } else {
                    serde_json::to_vec(&upstream_error_payload(response.status, &response.body))
                        .unwrap_or_else(|_| b"{}".to_vec())
                };
                return (
                    AttemptResult::ReturnToClient(HttpResponse {
                        status: response.status,
                        content_type: "application/json",
                        body,
                        capture: Some(capture),
                    }),
                    log,
                );
            }
            (
                AttemptResult::Failure {
                    class,
                    retryable: is_retryable_with_message(
                        class,
                        response.status,
                        error_message.as_deref(),
                    ),
                    transport: false,
                    reason: failure_reason(response.status, response.parsed, error_message.as_deref()),
                    retry_delay: retry_header_delay(&response.headers),
                },
                log,
            )
        }
        Err(error) => {
            let reason = format_network_error_reason(&error);
            let log = build_attempt_log(
                provider,
                model,
                started,
                0,
                UsageResult::Failure,
                sanitize_error_text(&reason, &provider.api_key),
                None,
            );
            (
                AttemptResult::Failure {
                    class: FailureClass::Retryable,
                    retryable: true,
                    transport: true,
                    reason,
                    retry_delay: None,
                },
                log,
            )
        }
    }
}

/// Keep one failure entry per provider in the all-unavailable message, holding
/// the most recent reason, so bounded retries do not spam the same provider.
fn record_provider_failure(failures: &mut Vec<(String, String)>, name: &str, reason: String) {
    if let Some(entry) = failures.iter_mut().find(|(provider, _)| provider == name) {
        entry.1 = reason;
    } else {
        failures.push((name.to_string(), reason));
    }
}

/// Try the candidates for a non-streaming request: one immediate fallback-first
/// pass in order, then bounded retries per provider in earliest-deadline order.
///
/// Every completed attempt is appended to `attempts` in completion order
/// (REQ-001); the caller owns the buffer so an attempt still in flight when the
/// request ends is dropped without losing the completed entries.
pub(in crate::ai_gateway) async fn attempt_non_streaming(
    ordered: &[GatewayUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    client_headers: &HashMap<String, String>,
    suppress_transport_failures: bool,
    probe: Option<&ProbeCandidate>,
    attempts: &mut Vec<AttemptLog>,
) -> HttpResponse {
    let protocol = protocol_for_path(path);
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut health = RequestHealth {
        suppress_transport_failures,
        ..Default::default()
    };
    let mut retries: Vec<RetryCandidate> = Vec::new();
    // Last attempted provider/model, reported when every candidate is unavailable.
    let mut last_capture: Option<ForwardCapture> = None;

    // Initial pass (REQ-001): every candidate is tried once, in order, without
    // waiting for any backoff, so a healthy later candidate answers before a
    // retry delay is paid.
    for provider in ordered {
        let model = match resolve_model_for_protocol(provider, requested, protocol) {
            ModelResolution::Serve(model) => model,
            ModelResolution::ProtocolMismatch(_) | ModelResolution::NoMatch => continue,
        };
        let (outcome, log) =
            attempt_candidate(provider, path, body, &model, client_headers).await;
        attempts.push(log);
        match outcome {
            AttemptResult::Success(response) => {
                settle_success(&mut health, provider, requested, &model);
                health.apply();
                return response;
            }
            AttemptResult::ReturnToClient(response) => {
                health.apply();
                return response;
            }
            AttemptResult::Failure {
                class,
                retryable,
                transport,
                reason,
                retry_delay,
            } => {
                settle_failure(
                    &mut health,
                    provider,
                    requested,
                    &model,
                    class,
                    &reason,
                    transport,
                );
                record_provider_failure(&mut failures, &provider.name, reason);
                last_capture = Some(ForwardCapture {
                    provider_id: provider.id.clone(),
                    provider_name: provider.name.clone(),
                    upstream_model: model.clone(),
                    ..Default::default()
                });
                if retryable && ordered.len() > 1 {
                    retries.push(RetryCandidate {
                        provider: provider.clone(),
                        model,
                        attempts: 1,
                        ready_at: Instant::now().checked_add(
                            retry_delay.unwrap_or_else(|| default_retry_delay(1)),
                        ),
                    });
                }
            }
        }
    }

    // Bounded retries (REQ-002): serial, earliest deadline first, ties keep the
    // initial candidate order. A 404 never enters this queue and 401/403 stop the
    // provider; only retryable classes are rescheduled.
    let mut remaining_wait = Duration::from_secs(120);
    loop {
        let Some(index) = RetryCandidate::next_ready(&retries, &mut remaining_wait).await else {
            break;
        };
        let mut candidate = retries.remove(index);
        let (outcome, log) = attempt_candidate(
            &candidate.provider,
            path,
            body,
            &candidate.model,
            client_headers,
        )
        .await;
        attempts.push(log);
        match outcome {
            AttemptResult::Success(response) => {
                settle_success(
                    &mut health,
                    &candidate.provider,
                    requested,
                    &candidate.model,
                );
                health.apply();
                return response;
            }
            AttemptResult::ReturnToClient(response) => {
                health.apply();
                return response;
            }
            AttemptResult::Failure {
                class,
                retryable,
                transport,
                reason,
                retry_delay,
            } => {
                settle_failure(
                    &mut health,
                    &candidate.provider,
                    requested,
                    &candidate.model,
                    class,
                    &reason,
                    transport,
                );
                record_provider_failure(&mut failures, &candidate.provider.name, reason);
                last_capture = Some(ForwardCapture {
                    provider_id: candidate.provider.id.clone(),
                    provider_name: candidate.provider.name.clone(),
                    upstream_model: candidate.model.clone(),
                    ..Default::default()
                });
                candidate.attempts += 1;
                if retryable && candidate.attempts <= MAX_RETRIES_PER_PROVIDER {
                    candidate.ready_at = Instant::now().checked_add(
                        retry_delay.unwrap_or_else(|| default_retry_delay(candidate.attempts)),
                    );
                    retries.insert(index, candidate);
                }
            }
        }
    }

    // Half-open probe (REQ-001): only after every healthy candidate and its
    // bounded retries have failed, try the single eligible auto-disabled row
    // once. The single-flight guard makes concurrent requests skip a probe that
    // is already in flight, and a probe is never queued for retry or backoff.
    // The guard is held until after the failed probe's settlement below so a
    // concurrent request cannot acquire it during the cooldown re-arm window,
    // matching the streaming path.
    let mut _probe_guard: Option<ProbeGuard> = None;
    if let Some(candidate) = probe {
        if let Some(guard) = try_acquire_probe_guard(&candidate.target) {
            _probe_guard = Some(guard);
            let provider = &candidate.provider;
            let model = candidate.upstream_model.as_str();
            let (outcome, log) =
                attempt_candidate(provider, path, body, model, client_headers).await;
            attempts.push(log);
            match outcome {
                AttemptResult::Success(response) => {
                    health.record_probe_success(&candidate.target);
                    health.apply();
                    return response;
                }
                AttemptResult::ReturnToClient(response) => {
                    let status = response.status;
                    health.record_probe_failure(
                        &candidate.target,
                        now_ts(),
                        false,
                        &format!("HTTP {status} returned to client"),
                    );
                    health.apply();
                    return response;
                }
                AttemptResult::Failure {
                    transport, reason, ..
                } => {
                    health.record_probe_failure(&candidate.target, now_ts(), transport, &reason);
                    record_provider_failure(&mut failures, &provider.name, reason);
                    last_capture = Some(ForwardCapture {
                        provider_id: provider.id.clone(),
                        provider_name: provider.name.clone(),
                        upstream_model: candidate.upstream_model.clone(),
                        ..Default::default()
                    });
                    // The probe is never requeued; fall through to the existing
                    // exhausted path so the 502 envelope names the provider.
                }
            }
        }
    }

    health.apply();
    let mut response = json_response(502, all_unavailable_payload(all_unavailable_message(&failures)));
    let mut capture = last_capture.unwrap_or_default();
    capture.status = 502;
    capture.all_unavailable = true;
    response.capture = Some(capture);
    response
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

/// Try immediate fallback candidates, then bounded retries before downstream output.
///
/// Switching is permitted only until the first byte is written to `writer`; once
/// written, an upstream failure terminates the stream without retrying.
///
/// Every completed attempt is appended to `attempts` in completion order
/// (REQ-001). An attempt whose bytes are still being forwarded when the request
/// ends is in flight and appends nothing.
pub(in crate::ai_gateway) async fn attempt_streaming<W: AsyncWrite + Unpin>(
    writer: &mut W,
    ordered: &[GatewayUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    client_headers: &HashMap<String, String>,
    suppress_transport_failures: bool,
    probe: Option<&ProbeCandidate>,
    attempts: &mut Vec<AttemptLog>,
) -> Result<ForwardCapture, String> {
    let protocol = protocol_for_path(path);
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut health = RequestHealth {
        suppress_transport_failures,
        ..Default::default()
    };
    let mut retries = Vec::new();
    let mut remaining_wait = Duration::from_secs(120);
    let mut initial = ordered.iter();
    // Read-only usage parser fed from the relay's own passthrough loop; it never
    // influences the bytes written to the caller.
    let mut usage = SseUsageAccumulator::default();
    let mut capture = ForwardCapture::default();
    // Real failure status to persist for a terminal all-unavailable outcome: the
    // last upstream HTTP status (0 for a network error). The caller-facing
    // pre-stream failure is a 502 JSON transport, which must never replace that
    // real status in the request log.
    let mut last_failure_status: Option<u16> = None;
    // The optional half-open probe is taken at most once, after every healthy
    // candidate and bounded retry has failed. `_probe_guard` holds the
    // single-flight guard for the whole probe and releases it on return.
    let mut pending_probe = probe;
    let mut _probe_guard: Option<ProbeGuard> = None;
    loop {
        let (mut candidate, retry_index, probe_target) = if let Some(provider) = initial.next() {
            let model = match resolve_model_for_protocol(provider, requested, protocol) {
                ModelResolution::Serve(model) => model,
                ModelResolution::ProtocolMismatch(_) | ModelResolution::NoMatch => continue,
            };
            (
                RetryCandidate {
                    provider: provider.clone(),
                    model,
                    attempts: 0,
                    ready_at: None,
                },
                retries.len(),
                None,
            )
        } else if let Some(index) = RetryCandidate::next_ready(&retries, &mut remaining_wait).await {
            (retries.remove(index), index, None)
        } else if let Some(candidate) = pending_probe.take() {
            match try_acquire_probe_guard(&candidate.target) {
                Some(guard) => {
                    _probe_guard = Some(guard);
                    (
                        RetryCandidate {
                            provider: candidate.provider.clone(),
                            model: candidate.upstream_model.clone(),
                            attempts: 0,
                            ready_at: None,
                        },
                        retries.len(),
                        Some(candidate.target.clone()),
                    )
                }
                // Another request already probes this mapping key: continue to
                // the exhausted path without an attempt.
                None => break,
            }
        } else {
            break;
        };
        capture.provider_id = candidate.provider.id.clone();
        capture.provider_name = candidate.provider.name.clone();
        capture.upstream_model = candidate.model.clone();
        let provider = &candidate.provider;
        let started = Instant::now();
        let (class, retryable, reason, retry_delay, transport) = 'attempt: {
            let streamed = open_streaming_response(
                provider, path, body, &candidate.model, client_headers,
            ).await;
            let response = match streamed {
                Ok(response) => response,
                Err(error) => {
                    last_failure_status = Some(0);
                    let reason = format_network_error_reason(&error);
                    attempts.push(build_attempt_log(
                        provider,
                        &candidate.model,
                        started,
                        0,
                        UsageResult::Failure,
                        sanitize_error_text(&reason, &provider.api_key),
                        None,
                    ));
                    break 'attempt (FailureClass::Retryable, true, reason, None, true);
                }
            };
            let status = response.status().as_u16();
            let retry_delay = retry_header_delay(response.headers());
            if status >= 400 {
                // An unreadable error body must not override the known HTTP
                // status (especially immediate disabling for 401/403).
                let bytes = response.bytes().await.unwrap_or_default();
                let parsed = serde_json::from_slice::<Value>(&bytes).is_ok();
                let error_message = sanitize_error_text(
                    &extract_upstream_error_text(&bytes).unwrap_or_default(),
                    &provider.api_key,
                );
                let class =
                    classify_failure_with_message(status, false, parsed, error_message.as_deref());
                if class == FailureClass::ReturnToClient {
                    if let Some(target) = &probe_target {
                        // A probe that returns the upstream 4xx unchanged still
                        // re-arms its cooldown (REQ-001).
                        health.record_probe_failure(
                            target,
                            now_ts(),
                            false,
                            &failure_reason(status, parsed, error_message.as_deref()),
                        );
                    }
                    health.apply();
                    capture.status = status;
                    attempts.push(build_attempt_log(
                        provider,
                        &candidate.model,
                        started,
                        status,
                        UsageResult::Failure,
                        error_message,
                        None,
                    ));
                    // Byte-for-byte only when the upstream body is already a
                    // standard error; otherwise keep the status and wrap it
                    // (REQ-004/AC-006/AC-007).
                    let standard = is_standard_error_body(&bytes);
                    let body = if standard {
                        bytes.to_vec()
                    } else {
                        serde_json::to_vec(&upstream_error_payload(status, &bytes))
                            .unwrap_or_else(|_| b"{}".to_vec())
                    };
                    write_response(
                        writer,
                        HttpResponse {
                            status,
                            content_type: "application/json",
                            body,
                            capture: None,
                        },
                    )
                    .await?;
                    return Ok(capture);
                }
                last_failure_status = Some(status);
                let reason = failure_reason(status, parsed, error_message.as_deref());
                // Quota-exhausted 429s count toward health but never requeue
                // the same provider; compute before `error_message` is moved
                // into the attempt log.
                let retryable =
                    is_retryable_with_message(class, status, error_message.as_deref());
                attempts.push(build_attempt_log(
                    provider,
                    &candidate.model,
                    started,
                    status,
                    UsageResult::Failure,
                    error_message,
                    None,
                ));
                break 'attempt (class, retryable, reason, retry_delay, false);
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
                        last_failure_status = Some(502);
                        let reason = format!(
                            "2xx response is not a valid SSE stream (content-type: {content_type})"
                        );
                        // The rejected 2xx body is the only readable failure
                        // information this attempt produces (REQ-003).
                        attempts.push(build_attempt_log(
                            provider,
                            &candidate.model,
                            started,
                            502,
                            UsageResult::Failure,
                            sanitize_error_text(
                                &extract_upstream_error_text(&first).unwrap_or_default(),
                                &provider.api_key,
                            ),
                            None,
                        ));
                        break 'attempt (FailureClass::Retryable, true, reason, retry_delay, false);
                    }
                    if let Err(error) = write_stream_headers(writer, status).await {
                        health.apply();
                        return Err(error);
                    }
                    // Capture usage from a read-only copy before the bytes are
                    // written; the forwarded payload is unchanged.
                    usage.feed(&first);
                    if let Err(error) = writer.write_all(&first).await {
                        health.apply();
                        return Err(error.to_string());
                    }
                    // Last byte forwarded to the caller, used to complete the
                    // SSE event boundary before a mid-stream error fragment
                    // (REQ-005/AC-008).
                    let mut last_forwarded = first.last().copied();
                    if writer.flush().await.is_err() {
                        health.apply();
                        capture.downstream_cancelled = true;
                        return Ok(capture);
                    }
                    loop {
                        match chunks.next().await {
                            Some(Ok(chunk)) => {
                                usage.feed(&chunk);
                                if writer.write_all(&chunk).await.is_err() {
                                    health.apply();
                                    capture.downstream_cancelled = true;
                                    return Ok(capture);
                                }
                                last_forwarded = chunk.last().copied();
                            }
                            // Bytes already sent: terminate the stream, never switch.
                            Some(Err(error)) => {
                                let reason = format!("stream failed after first byte: {error}");
                                if let Some(target) = &probe_target {
                                    // A mid-stream probe failure re-arms the
                                    // cooldown; it is never retried and no
                                    // second probe is attempted (REQ-001).
                                    health.record_probe_failure(target, now_ts(), true, &reason);
                                } else {
                                    settle_failure(
                                        &mut health,
                                        provider,
                                        requested,
                                        &candidate.model,
                                        FailureClass::Retryable,
                                        &reason,
                                        true,
                                    );
                                }
                                health.apply();
                                // The attempt is complete: its stream ended with
                                // an error and keeps the usage accumulated so far
                                // (REQ-003/REQ-005).
                                attempts.push(build_attempt_log(
                                    provider,
                                    &candidate.model,
                                    started,
                                    502,
                                    UsageResult::Failure,
                                    sanitize_error_text(&reason, &provider.api_key),
                                    usage.canonical_usage(),
                                ));
                                // Complete the SSE event boundary so the error
                                // fragment parses standalone even when the last
                                // forwarded byte is not a newline, then append one
                                // `data:` error event. No `[DONE]`, no retry, no
                                // candidate switch; downstream write failures stay
                                // best-effort (REQ-005/AC-008).
                                if last_forwarded != Some(b'\n') {
                                    let _ = writer.write_all(b"\n").await;
                                }
                                let _ = writer.write_all(b"\n").await;
                                let envelope = error_envelope(
                                    reason,
                                    "server_error",
                                    "upstream_stream_error",
                                );
                                let fragment = format!("data: {envelope}\n\n");
                                let _ = writer.write_all(fragment.as_bytes()).await;
                                let _ = writer.flush().await;
                                capture.status = 502;
                                capture.usage = usage.usage();
                                capture.upstream_error = true;
                                return Ok(capture);
                            }
                            None => {
                                if let Some(target) = &probe_target {
                                    // A served probe clears its row's runtime
                                    // state (REQ-001/AC-001).
                                    health.record_probe_success(target);
                                } else {
                                    settle_success(
                                        &mut health,
                                        provider,
                                        requested,
                                        &candidate.model,
                                    );
                                }
                                health.apply();
                                capture.status = status;
                                capture.usage = usage.usage();
                                // A normally ended stream is the served success
                                // of this attempt (REQ-001/REQ-003).
                                attempts.push(build_attempt_log(
                                    provider,
                                    &candidate.model,
                                    started,
                                    status,
                                    UsageResult::Success,
                                    None,
                                    usage.canonical_usage(),
                                ));
                                return Ok(capture);
                            }
                        }
                    }
                }
                Some(Err(error)) => {
                    last_failure_status = Some(0);
                    let reason = format!("stream failed before first byte: {error}");
                    attempts.push(build_attempt_log(
                        provider,
                        &candidate.model,
                        started,
                        0,
                        UsageResult::Failure,
                        sanitize_error_text(&reason, &provider.api_key),
                        None,
                    ));
                    break 'attempt (FailureClass::Retryable, true, reason, retry_delay, true);
                }
                None => {
                    last_failure_status = Some(502);
                    let reason = "upstream returned an empty stream".to_string();
                    // No readable body ever arrived, so this attempt stores no
                    // error message (REQ-003).
                    attempts.push(build_attempt_log(
                        provider,
                        &candidate.model,
                        started,
                        502,
                        UsageResult::Failure,
                        None,
                        None,
                    ));
                    break 'attempt (FailureClass::Retryable, true, reason, retry_delay, false);
                }
            }
        };
        if let Some(target) = &probe_target {
            // A probe that failed before its first byte re-arms its cooldown,
            // names the provider in the all-unavailable message and is never
            // queued for retry or backoff (REQ-001/AC-002).
            health.record_probe_failure(target, now_ts(), transport, &reason);
            record_provider_failure(&mut failures, &provider.name, reason);
        } else {
            settle_failure(
                &mut health,
                provider,
                requested,
                &candidate.model,
                class,
                &reason,
                transport,
            );
            record_provider_failure(&mut failures, &provider.name, reason);
            candidate.attempts += 1;
            if retryable && ordered.len() > 1 && candidate.attempts <= MAX_RETRIES_PER_PROVIDER {
                candidate.ready_at = Instant::now().checked_add(
                    retry_delay.unwrap_or_else(|| default_retry_delay(candidate.attempts)),
                );
                retries.insert(retry_index, candidate);
            }
        }
    }

    health.apply();
    // Nothing was written downstream yet, so the failure is a plain HTTP 502
    // JSON response rather than an SSE error event over HTTP 200 (REQ-003).
    let response = json_response(502, all_unavailable_payload(all_unavailable_message(&failures)));
    write_response(writer, response).await?;
    // The transport is 502, but the log records the real upstream failure status
    // (0 when no upstream HTTP status was determinable, for example a network
    // error) rather than the transport's status.
    capture.status = last_failure_status.unwrap_or(502);
    capture.usage = None;
    capture.all_unavailable = true;
    Ok(capture)
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

pub(in crate::ai_gateway) async fn handle_connection(mut stream: TcpStream) -> Result<(), String> {
    let request = match read_http_request(&mut stream).await {
        Ok(request) => request,
        Err(error) => {
            let response = json_response(
                400,
                error_envelope(error, "invalid_request_error", "invalid_request"),
            );
            let _ = stream.write_all(&http_response_bytes(response)).await;
            return Ok(());
        }
    };

    // Measured across the whole handled request so the recorded duration is
    // always within the gateway's processing time.
    let started = Instant::now();

    // Evaluate the post-resume grace exactly once, when the request starts:
    // every transport failure settled by this request is suppressed while the
    // window is open (REQ-004/AC-006).
    let suppress_transport_failures =
        crate::app_runtime::system_resume_grace_active(SystemTime::now());

    let config = match read_config() {
        Ok(config) => config,
        Err(error) => {
            let response = json_response(
                500,
                error_envelope(error, "server_error", "config_error"),
            );
            let _ = stream.write_all(&http_response_bytes(response)).await;
            return Ok(());
        }
    };

    let raw_path = clean_path(&request.path);
    let Some(path) = canonical_api_path(raw_path) else {
        let response = json_response(
            404,
            error_envelope(
                format!("unknown path: {raw_path}"),
                "invalid_request_error",
                "not_found",
            ),
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
            error_envelope(
                "invalid or missing local API key",
                "invalid_request_error",
                "invalid_api_key",
            ),
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
                error_envelope("not found", "invalid_request_error", "not_found"),
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
            error_envelope("not found", "invalid_request_error", "not_found"),
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
                error_envelope(error.to_string(), "invalid_request_error", "invalid_request"),
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
    let reasoning_effort = extract_reasoning_effort(&body_value);
    let wants_stream = body_value
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let protocol = protocol_for_path(path);
    let candidates: Vec<GatewayUpstreamProvider> =
        candidate_providers(&config.providers, requested.as_deref(), protocol)
            .into_iter()
            .cloned()
            .collect();
    // The half-open probe is computed against the healthy candidate provider
    // ids so an already-serving provider is never also probed (REQ-002). It is
    // the only attempt of a zero-candidate request that has an eligible row.
    let healthy_provider_ids: Vec<String> = candidates
        .iter()
        .map(|provider| provider.id.clone())
        .collect();
    let probe = find_probe_candidate(
        &config.providers,
        requested.as_deref(),
        protocol,
        &healthy_provider_ids,
        now_ts(),
    );
    let probe_only = candidates.is_empty();
    if probe_only && probe.is_none() {
        let message = no_candidate_message(&config, requested.as_deref(), protocol);
        // Streaming and non-streaming alike answer HTTP 502 + JSON before any
        // byte is written; the log always records the gateway failure status.
        let status = 502;
        let response = json_response(502, all_unavailable_payload(message));
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        // A request that entered the normalized flow but had no serving
        // upstream is a failure (REQ-007/REQ-008), never a silent no-log; it
        // keeps the existing single synthetic terminal row and writes it
        // through the single-row store call.
        record_usage_log(
            &config,
            &synthetic_terminal_row(
                &config,
                requested.as_deref().unwrap_or_default(),
                UsageResult::Failure,
                status,
                started.elapsed().as_millis().max(1) as u64,
                reasoning_effort,
            ),
        );
        return Ok(());
    }

    // A probe-only request neither reads nor writes a session-affinity binding
    // and uses no session id (REQ-002).
    let session_id = if probe_only {
        None
    } else {
        resolve_session_id(&request.headers)
    };
    let (ordered, bound_was_eligible) = if probe_only {
        (Vec::new(), false)
    } else {
        // The lookup, the shuffle and a first-request binding write share one
        // lock guard, which is released before any forwarding begins. A poisoned
        // binding table must not take the gateway down, so recover the guard
        // instead.
        let SessionOrder {
            ordered,
            bound_provider_id,
        } = session_affinity()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .resolve_order(session_id.as_deref(), requested.as_deref(), || {
                weighted_candidates(&candidates)
            });
        // The reorder is a no-op when the bound provider is not one of this
        // request's eligible candidates, which forces the binding to be replaced.
        let bound_was_eligible = bound_provider_id
            .as_deref()
            .is_some_and(|id| candidates.iter().any(|provider| provider.id == id));
        (ordered, bound_was_eligible)
    };
    let (mut reader, mut writer) = stream.into_split();
    let disconnected = async {
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    };
    // One entry per completed upstream attempt, owned by the connection handler
    // and borrowed by the forwarding future: dropping an in-flight attempt keeps
    // every entry that already completed (REQ-001).
    let mut attempts: Vec<AttemptLog> = Vec::new();
    let forward = async {
        if wants_stream {
            attempt_streaming(
                &mut writer,
                &ordered,
                path,
                &request.body,
                requested.as_deref(),
                &request.headers,
                suppress_transport_failures,
                probe.as_ref(),
                &mut attempts,
            )
            .await
        } else {
            let mut response = attempt_non_streaming(
                &ordered,
                path,
                &request.body,
                requested.as_deref(),
                &request.headers,
                suppress_transport_failures,
                probe.as_ref(),
                &mut attempts,
            )
            .await;
            let status = response.status;
            let capture = response.capture.take().unwrap_or_default();
            write_response(&mut writer, response).await?;
            Ok(ForwardCapture {
                status,
                ..capture
            })
        }
    };
    let outcome: Option<Result<ForwardCapture, String>> = tokio::select! {
        // Check disconnect first so a ready retry cannot start after EOF.
        // Dropping forwarding cancels upstream I/O and backoff without recording
        // the client cancellation as an upstream health failure.
        biased;
        _ = disconnected => None,
        result = forward => Some(result),
    };
    // A completed business outcome persists its buffered attempts with exactly
    // one terminal row (the successful, `ReturnToClient`, mid-stream-failure or
    // chronologically last exhausted attempt). A request without a completed
    // attempt writes the gateway's own synthetic failure terminal row instead.
    // A downstream cancellation or undeliverable response persists nothing: the
    // whole buffer, including completed attempts, is discarded. Logging is
    // best-effort and never changes the caller-visible response.
    match outcome {
        Some(Ok(capture)) if capture.result() != UsageResult::Cancelled => {
            // Settle the binding once per request at this terminal outcome,
            // before the best-effort usage rows are persisted so the binding
            // is already settled when the caller observes the response. A
            // request that reached no upstream carries an empty provider id
            // and settles nothing; the lock is held for this call only. A
            // probe-only request never read a binding, so it settles none.
            if !probe_only {
                let served = capture.provider_id.trim();
                let served = if served.is_empty() { None } else { Some(served) };
                session_affinity()
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .settle(
                        session_id.as_deref().unwrap_or_default(),
                        requested.as_deref().unwrap_or_default(),
                        bound_was_eligible,
                        served,
                    );
            }
            if attempts.is_empty() {
                // No upstream attempt completed, so the request's only row is
                // the gateway's own terminal row, like a no-candidate request
                // (REQ-001).
                let record = synthetic_terminal_row(
                    &config,
                    requested.as_deref().unwrap_or_default(),
                    capture.result(),
                    capture.status,
                    started.elapsed().as_millis().max(1) as u64,
                    reasoning_effort.clone(),
                );
                record_usage_log(&config, &record);
            } else {
                let terminal = attempts.len() - 1;
                record_request_usage_logs(
                    &config,
                    requested.as_deref(),
                    reasoning_effort.clone(),
                    &attempts,
                    terminal,
                );
            }
        }
        // A downstream cancellation or an undeliverable response is an internal
        // transport lifecycle event, not a business outcome: discard the entire
        // buffered log set so this inbound request writes no row at all.
        _ => {}
    }
    Ok(())
}

/// Extract the requested reasoning effort level from request JSON body, if any.
/// Compatible with OpenAI `reasoning_effort`, OpenCode `reasoningEffort`, and nested `reasoning.effort`.
fn extract_reasoning_effort(body: &Value) -> Option<String> {
    if let Some(effort) = body.get("reasoning_effort").and_then(|v| v.as_str()) {
        let trimmed = effort.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(effort) = body.get("reasoningEffort").and_then(|v| v.as_str()) {
        let trimmed = effort.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(effort) = body
        .get("reasoning")
        .and_then(|v| v.get("effort"))
        .and_then(|v| v.as_str())
    {
        let trimmed = effort.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Persist the request's buffered attempt rows with exactly one terminal row.
///
/// `terminal_index` names the buffered attempt that is the request's terminal
/// row: the successful, `ReturnToClient`, mid-stream-failure or chronologically
/// last exhausted attempt. The caller only reaches this helper for a completed
/// business outcome with at least one buffered attempt; a downstream-cancelled
/// or undeliverable request discards its whole buffer before logging, and a
/// request without a completed attempt writes the gateway's own synthetic
/// terminal row instead. All rows of one request go through one store and one
/// batch, and any storage failure is logged and swallowed — the response has
/// already been produced and must not be affected.
fn record_request_usage_logs(
    config: &GatewayConfig,
    local_model: Option<&str>,
    reasoning_effort: Option<String>,
    attempts: &[AttemptLog],
    terminal_index: usize,
) {
    let local_model = local_model.unwrap_or_default();
    let entries: Vec<UsageLogEntry> = attempts
        .iter()
        .enumerate()
        .map(|(index, attempt)| UsageLogEntry {
            record: build_usage_log_row(
                config,
                local_model,
                &attempt.provider_id,
                &attempt.provider_name,
                &attempt.upstream_model,
                attempt.result,
                attempt.status,
                attempt.usage,
                attempt.duration_ms,
                attempt.error_message.clone(),
                index == terminal_index,
                reasoning_effort.clone(),
            ),
            accounting: UsageAccounting {
                present: attempt.usage.is_some(),
                valid: attempt.usage_valid,
            },
        })
        .collect();
    write_usage_log_entries(config, entries);
}

/// The gateway's own terminal row, attributed to no provider: an empty upstream
/// model, no usage and no error message.
fn synthetic_terminal_row(
    config: &GatewayConfig,
    local_model: &str,
    result: UsageResult,
    status: u16,
    duration_ms: u64,
    reasoning_effort: Option<String>,
) -> UsageLogRecord {
    build_usage_log_row(
        config,
        local_model,
        "",
        "",
        "",
        result,
        status,
        None,
        duration_ms,
        None,
        true,
        reasoning_effort,
    )
}

/// Build one request-log row. The amount is fixed at record time from the price
/// table, so later price edits never rewrite history.
#[allow(clippy::too_many_arguments)]
fn build_usage_log_row(
    config: &GatewayConfig,
    local_model: &str,
    provider_id: &str,
    provider_name: &str,
    upstream_model: &str,
    result: UsageResult,
    status: u16,
    usage: Option<UsageTokens>,
    duration_ms: u64,
    error_message: Option<String>,
    terminal: bool,
    reasoning_effort: Option<String>,
) -> UsageLogRecord {
    let timestamp_ms = now_millis();
    let tokens = usage.unwrap_or_default();
    let amount = match_price_for_provider(provider_id, upstream_model, &config.model_prices)
        .map(|price| compute_cost_at_time(price, &tokens, timestamp_ms));
    UsageLogRecord {
        timestamp_ms,
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        provider_id: provider_id.to_string(),
        provider_name: provider_name.to_string(),
        result,
        status,
        input_tokens: tokens.input_tokens,
        cache_read_tokens: tokens.cache_read_tokens,
        cache_write_tokens: tokens.cache_write_tokens,
        output_tokens: tokens.output_tokens,
        total_tokens: tokens.total(),
        amount,
        duration_ms,
        error_message,
        terminal,
        reasoning_effort,
    }
}

/// Write one request row through a single store call; a storage failure is
/// reported only as a swallowed log-write warning (REQ-005). A synthetic
/// gateway row carries no upstream usage object.
fn record_usage_log(config: &GatewayConfig, record: &UsageLogRecord) {
    let retention = normalize_retention_days(config.usage_retention_days);
    let accounting = UsageAccounting {
        present: false,
        valid: false,
    };
    let write = UsageLogStore::default_store()
        .and_then(|store| store.append_with_accounting(record, accounting, retention));
    if let Err(error) = write {
        log::warn!("AI gateway usage log write failed: {error}");
    }
}

/// Write every entry of one request through a single store and batch; a storage
/// failure is reported only as a swallowed log-write warning (REQ-005).
fn write_usage_log_entries(config: &GatewayConfig, entries: Vec<UsageLogEntry>) {
    let retention = normalize_retention_days(config.usage_retention_days);
    let write = UsageLogStore::default_store()
        .and_then(|store| store.append_batch_with_accounting(&entries, retention));
    if let Err(error) = write {
        log::warn!("AI gateway usage log write failed: {error}");
    }
}
