use super::forwarding::{forward_non_streaming, open_streaming_response};
use super::selection::{
    candidate_providers, classify_failure, default_retry_delay, is_retryable_failure,
    register_failure, register_success, resolve_model_for_protocol, retry_header_delay, shuffled_candidates,
    FailureClass, ModelResolution, MAX_RETRIES_PER_PROVIDER,
};
use super::storage::{local_base_url, read_config, write_config};
use super::usage_log::{
    compute_cost, match_price, normalize_retention_days, now_millis, parse_usage_from_response,
    SseUsageAccumulator, UsageLogRecord, UsageLogStore, UsageResult, UsageTokens,
};
use super::{now_ts, FusionConfig, FusionKey, FusionStatus, FusionUpstreamProvider, UpstreamProtocol};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex};
use tokio::time::{sleep, Instant};

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
                "failed to bind API Gateway port {} on 127.0.0.1: {e}",
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
    /// Usage/provider metadata captured while forwarding, consumed by the
    /// request logger. Never forwarded to the caller.
    pub(in crate::api_fusion) capture: Option<ForwardCapture>,
}

/// Per-request forwarding metadata used to write exactly one usage log row.
#[derive(Debug, Clone, Default)]
pub(in crate::api_fusion) struct ForwardCapture {
    pub(in crate::api_fusion) status: u16,
    pub(in crate::api_fusion) provider_id: String,
    pub(in crate::api_fusion) provider_name: String,
    pub(in crate::api_fusion) upstream_model: String,
    pub(in crate::api_fusion) usage: Option<UsageTokens>,
    /// No candidate could serve the request (including every candidate failing).
    pub(in crate::api_fusion) all_unavailable: bool,
    /// The upstream stream failed after bytes had already reached the caller.
    pub(in crate::api_fusion) upstream_error: bool,
    /// The downstream client went away mid-forward, so the request is neither
    /// a success nor an error.
    pub(in crate::api_fusion) downstream_cancelled: bool,
}

impl ForwardCapture {
    /// Final result classification: an HTTP 2xx is success only when the
    /// request was not cancelled, all-unavailable or an upstream error.
    pub(in crate::api_fusion) fn result(&self) -> UsageResult {
        if self.downstream_cancelled {
            UsageResult::Cancelled
        } else if self.all_unavailable || self.upstream_error || self.status >= 400 || self.status == 0 {
            UsageResult::Failure
        } else {
            UsageResult::Success
        }
    }
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

/// The protocol an inbound canonical path speaks.
fn protocol_for_path(path: &str) -> UpstreamProtocol {
    if path == "/v1/responses" {
        UpstreamProtocol::Responses
    } else {
        UpstreamProtocol::ChatCompletions
    }
}

pub(in crate::api_fusion) fn json_response(status: u16, body: Value) -> HttpResponse {
    let payload = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    HttpResponse {
        status,
        content_type: "application/json",
        body: payload,
        capture: None,
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

/// One provider still eligible for a bounded retry inside the current request.
/// This small scheduling state keeps the request's retry queue ordered by the
/// earliest monotonic deadline; equal deadlines keep the initial candidate order.
struct RetryCandidate {
    provider: FusionUpstreamProvider,
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
        reason: String,
        retry_delay: Option<Duration>,
    },
}

/// Per-request provider health accumulation.
///
/// Health is counted in inbound-request units, not upstream attempts: however
/// many times a provider is tried, its outcome is applied once when the request
/// ends normally. A final success clears the counter, a 404/429 alone never
/// counts, and a provider that also had a network/5xx failure counts once.
#[derive(Default)]
struct RequestHealth {
    order: Vec<String>,
    outcomes: HashMap<String, ProviderOutcome>,
}

#[derive(Default)]
struct ProviderOutcome {
    health_failure: bool,
    disable_immediately: bool,
    succeeded: bool,
    reason: String,
}

impl RequestHealth {
    fn entry(&mut self, provider_id: &str) -> &mut ProviderOutcome {
        if !self.outcomes.contains_key(provider_id) {
            self.order.push(provider_id.to_string());
            self.outcomes
                .insert(provider_id.to_string(), ProviderOutcome::default());
        }
        self.outcomes
            .get_mut(provider_id)
            .expect("health entry inserted above")
    }

    fn record_failure(
        &mut self,
        config: &mut FusionConfig,
        provider: &FusionUpstreamProvider,
        class: FailureClass,
        reason: &str,
    ) {
        let entry = self.entry(&provider.id);
        match class {
            FailureClass::DisableImmediately => {
                if !entry.disable_immediately {
                    apply_failure(config, provider, class, reason);
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
            // 404/429 alone never count toward health; other 4xx are returned to
            // the caller and also do not count.
            FailureClass::Transient | FailureClass::ReturnToClient => {}
        }
    }

    fn record_success(&mut self, provider_id: &str) {
        self.entry(provider_id).succeeded = true;
    }

    fn apply(&self, config: &mut FusionConfig) {
        let at = now_ts();
        let mut changed = false;
        for provider_id in &self.order {
            let Some(outcome) = self.outcomes.get(provider_id) else {
                continue;
            };
            if outcome.disable_immediately {
                continue;
            }
            let Some(stored) = config
                .providers
                .iter_mut()
                .find(|stored| stored.id == *provider_id)
            else {
                continue;
            };
            if outcome.succeeded {
                register_success(stored);
                changed = true;
            } else if outcome.health_failure {
                register_failure(stored, FailureClass::Retryable, &outcome.reason, at);
                changed = true;
            }
        }
        if changed {
            let _ = write_config(config);
        }
    }
}

async fn attempt_candidate(
    provider: &FusionUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    client_headers: &HashMap<String, String>,
) -> AttemptResult {
    match forward_non_streaming(provider, path, body, model, client_headers).await {
        Ok(response) => {
            // Usage is only meaningful for a successful 2xx upstream response;
            // an error body that happens to carry `usage` must never be billed.
            let usage = if (200..300).contains(&response.status) {
                parse_usage_from_response(&response.body)
            } else {
                None
            };
            let capture = ForwardCapture {
                status: response.status,
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                upstream_model: model.to_string(),
                usage,
                ..Default::default()
            };
            if response.status < 400 && response.parsed {
                return AttemptResult::Success(HttpResponse {
                    status: response.status,
                    content_type: "application/json",
                    body: response.body,
                    capture: Some(capture),
                });
            }
            let class = classify_failure(response.status, false, response.parsed);
            if class == FailureClass::ReturnToClient {
                return AttemptResult::ReturnToClient(HttpResponse {
                    status: response.status,
                    content_type: "application/json",
                    body: response.body,
                    capture: Some(capture),
                });
            }
            AttemptResult::Failure {
                class,
                retryable: is_retryable_failure(class, response.status),
                reason: failure_reason(response.status, response.parsed),
                retry_delay: retry_header_delay(&response.headers),
            }
        }
        Err(error) => AttemptResult::Failure {
            class: FailureClass::Retryable,
            retryable: true,
            reason: format!("network error: {error}"),
            retry_delay: None,
        },
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
pub(in crate::api_fusion) async fn attempt_non_streaming(
    ordered: &[FusionUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    config: &mut FusionConfig,
    client_headers: &HashMap<String, String>,
) -> HttpResponse {
    let protocol = protocol_for_path(path);
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut health = RequestHealth::default();
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
        match attempt_candidate(provider, path, body, &model, client_headers).await {
            AttemptResult::Success(response) => {
                health.record_success(&provider.id);
                health.apply(config);
                return response;
            }
            AttemptResult::ReturnToClient(response) => {
                health.apply(config);
                return response;
            }
            AttemptResult::Failure {
                class,
                retryable,
                reason,
                retry_delay,
            } => {
                health.record_failure(config, provider, class, &reason);
                record_provider_failure(&mut failures, &provider.name, reason);
                last_capture = Some(ForwardCapture {
                    provider_id: provider.id.clone(),
                    provider_name: provider.name.clone(),
                    upstream_model: model.clone(),
                    ..Default::default()
                });
                if retryable {
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
        match attempt_candidate(
            &candidate.provider,
            path,
            body,
            &candidate.model,
            client_headers,
        )
        .await
        {
            AttemptResult::Success(response) => {
                health.record_success(&candidate.provider.id);
                health.apply(config);
                return response;
            }
            AttemptResult::ReturnToClient(response) => {
                health.apply(config);
                return response;
            }
            AttemptResult::Failure {
                class,
                retryable,
                reason,
                retry_delay,
            } => {
                health.record_failure(config, &candidate.provider, class, &reason);
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

    health.apply(config);
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
pub(in crate::api_fusion) async fn attempt_streaming<W: AsyncWrite + Unpin>(
    writer: &mut W,
    ordered: &[FusionUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    config: &mut FusionConfig,
    client_headers: &HashMap<String, String>,
) -> Result<ForwardCapture, String> {
    let protocol = protocol_for_path(path);
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut health = RequestHealth::default();
    let mut retries = Vec::new();
    let mut remaining_wait = Duration::from_secs(120);
    let mut initial = ordered.iter();
    // Read-only usage parser fed from the relay's own passthrough loop; it never
    // influences the bytes written to the caller.
    let mut usage = SseUsageAccumulator::default();
    let mut capture = ForwardCapture::default();
    // Real failure status to persist for a terminal all-unavailable outcome: the
    // last upstream HTTP status (0 for a network error). The caller-facing
    // streaming failure is an SSE event over HTTP 200, which must never be the
    // status recorded in the request log.
    let mut last_failure_status: Option<u16> = None;
    loop {
        let (mut candidate, retry_index) = if let Some(provider) = initial.next() {
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
            )
        } else if let Some(index) = RetryCandidate::next_ready(&retries, &mut remaining_wait).await {
            (retries.remove(index), index)
        } else {
            break;
        };
        capture.provider_id = candidate.provider.id.clone();
        capture.provider_name = candidate.provider.name.clone();
        capture.upstream_model = candidate.model.clone();
        let provider = &candidate.provider;
        let (class, retryable, reason, retry_delay) = 'attempt: {
            let streamed = open_streaming_response(
                provider, path, body, &candidate.model, client_headers,
            ).await;
            let response = match streamed {
                Ok(response) => response,
                Err(error) => {
                    last_failure_status = Some(0);
                    let reason = format!("network error: {error}");
                    break 'attempt (FailureClass::Retryable, true, reason, None);
                }
            };
            let status = response.status().as_u16();
            let retry_delay = retry_header_delay(response.headers());
            if status >= 400 {
                // An unreadable error body must not override the known HTTP
                // status (especially immediate disabling for 401/403).
                let bytes = response.bytes().await.unwrap_or_default();
                let parsed = serde_json::from_slice::<Value>(&bytes).is_ok();
                let class = classify_failure(status, false, parsed);
                if class == FailureClass::ReturnToClient {
                    health.apply(config);
                    capture.status = status;
                    write_response(
                        writer,
                        HttpResponse {
                            status,
                            content_type: "application/json",
                            body: bytes.to_vec(),
                            capture: None,
                        },
                    )
                    .await?;
                    return Ok(capture);
                }
                last_failure_status = Some(status);
                let reason = failure_reason(status, parsed);
                break 'attempt (class, is_retryable_failure(class, status), reason, retry_delay);
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
                        break 'attempt (FailureClass::Retryable, true, reason, retry_delay);
                    }
                    if let Err(error) = write_stream_headers(writer, status).await {
                        health.apply(config);
                        return Err(error);
                    }
                    // Capture usage from a read-only copy before the bytes are
                    // written; the forwarded payload is unchanged.
                    usage.feed(&first);
                    if let Err(error) = writer.write_all(&first).await {
                        health.apply(config);
                        return Err(error.to_string());
                    }
                    if writer.flush().await.is_err() {
                        health.apply(config);
                        capture.downstream_cancelled = true;
                        return Ok(capture);
                    }
                    loop {
                        match chunks.next().await {
                            Some(Ok(chunk)) => {
                                usage.feed(&chunk);
                                if writer.write_all(&chunk).await.is_err() {
                                    health.apply(config);
                                    capture.downstream_cancelled = true;
                                    return Ok(capture);
                                }
                            }
                            // Bytes already sent: terminate the stream, never switch.
                            Some(Err(error)) => {
                                health.record_failure(
                                    config, provider, FailureClass::Retryable,
                                    &format!("stream failed after first byte: {error}"),
                                );
                                health.apply(config);
                                capture.status = 502;
                                capture.usage = usage.usage();
                                capture.upstream_error = true;
                                return Ok(capture);
                            }
                            None => {
                                health.record_success(&provider.id);
                                health.apply(config);
                                capture.status = status;
                                capture.usage = usage.usage();
                                return Ok(capture);
                            }
                        }
                    }
                }
                Some(Err(error)) => {
                    last_failure_status = Some(0);
                    let reason = format!("stream failed before first byte: {error}");
                    break 'attempt (FailureClass::Retryable, true, reason, retry_delay);
                }
                None => {
                    last_failure_status = Some(502);
                    let reason = "upstream returned an empty stream".to_string();
                    break 'attempt (FailureClass::Retryable, true, reason, retry_delay);
                }
            }
        };
        health.record_failure(config, provider, class, &reason);
        record_provider_failure(&mut failures, &provider.name, reason);
        candidate.attempts += 1;
        if retryable && candidate.attempts <= MAX_RETRIES_PER_PROVIDER {
            candidate.ready_at = Instant::now().checked_add(
                retry_delay.unwrap_or_else(|| default_retry_delay(candidate.attempts)),
            );
            retries.insert(retry_index, candidate);
        }
    }

    health.apply(config);
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
    // HTTP 200 with an SSE error event is still a failure: never a success, and
    // the log records the real upstream failure status (502 when no upstream
    // HTTP status was determinable) rather than the SSE transport's 200.
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

    // Measured across the whole handled request so the recorded duration is
    // always within the gateway's processing time.
    let started = Instant::now();

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

    let protocol = protocol_for_path(path);
    let candidates: Vec<FusionUpstreamProvider> =
        candidate_providers(&config.providers, requested.as_deref(), protocol)
            .into_iter()
            .cloned()
            .collect();
    if candidates.is_empty() {
        let message = no_candidate_message(&config, requested.as_deref(), protocol);
        // Streaming answers HTTP 200 + an SSE error event, but the log always
        // records the gateway failure status, never the transport's 200.
        let status = 502;
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
        // A request that entered the normalized flow but had no serving
        // upstream is a failure (REQ-007/REQ-008), never a silent no-log.
        record_usage_log(
            &config,
            started,
            requested.as_deref(),
            UsageResult::Failure,
            status,
            ForwardCapture {
                status,
                all_unavailable: true,
                ..Default::default()
            },
        );
        return Ok(());
    }

    let ordered = shuffled_candidates(&candidates);
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
    let forward = async {
        if wants_stream {
            attempt_streaming(
                &mut writer,
                &ordered,
                path,
                &request.body,
                requested.as_deref(),
                &mut config,
                &request.headers,
            )
            .await
        } else {
            let mut response = attempt_non_streaming(
                &ordered,
                path,
                &request.body,
                requested.as_deref(),
                &mut config,
                &request.headers,
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
    // Exactly one log row per forwarded request. Logging is best-effort and
    // never changes the caller-visible response.
    match outcome {
        Some(Ok(capture)) => record_usage_log(
            &config,
            started,
            requested.as_deref(),
            capture.result(),
            capture.status,
            capture,
        ),
        // A downstream disconnect (or a failed write to it) is a cancellation.
        _ => record_usage_log(
            &config,
            started,
            requested.as_deref(),
            UsageResult::Cancelled,
            0,
            ForwardCapture {
                downstream_cancelled: true,
                ..Default::default()
            },
        ),
    }
    Ok(())
}

/// Persist one usage-log row for a forwarded request.
///
/// The amount is fixed at record time from the price table, so later price
/// edits never rewrite history. Any storage failure is logged and swallowed:
/// the response has already been produced and must not be affected.
fn record_usage_log(
    config: &FusionConfig,
    started: Instant,
    local_model: Option<&str>,
    result: UsageResult,
    status: u16,
    capture: ForwardCapture,
) {
    let tokens = capture.usage.unwrap_or_default();
    let amount = match_price(&capture.upstream_model, &config.model_prices)
        .map(|price| compute_cost(price, &tokens));
    let duration_ms = started.elapsed().as_millis().max(1) as u64;
    let record = UsageLogRecord {
        timestamp_ms: now_millis(),
        local_model: local_model.unwrap_or_default().to_string(),
        upstream_model: capture.upstream_model,
        provider_id: capture.provider_id,
        provider_name: capture.provider_name,
        result,
        status,
        input_tokens: tokens.input_tokens,
        cache_read_tokens: tokens.cache_read_tokens,
        cache_write_tokens: tokens.cache_write_tokens,
        output_tokens: tokens.output_tokens,
        total_tokens: tokens.total(),
        amount,
        duration_ms,
    };
    let retention = normalize_retention_days(config.usage_retention_days);
    let write = UsageLogStore::default_store().and_then(|store| store.append(&record, retention));
    if let Err(error) = write {
        log::warn!("API gateway usage log write failed: {error}");
    }
}
