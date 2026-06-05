use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

const CONFIG_FILE: &str = "protocol_router.json";
const LEGACY_CONFIG_FILE: &str = "protocol_proxy.json";
const STATS_FILE: &str = "protocol_router_calls.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireApi {
    OpenAiChat,
    OpenAiResponses,
}

impl Default for WireApi {
    fn default() -> Self {
        Self::OpenAiChat
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CatalogModel {
    pub id: String,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub created: Option<u64>,
    #[serde(default)]
    pub owned_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMapping {
    pub claude_model: String,
    pub upstream_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolRoute {
    pub id: String,
    pub name: String,
    pub claude_provider_id: String,
    pub claude_provider_name: String,
    pub upstream_provider_id: String,
    pub upstream_provider_name: String,
    pub base_url: String,
    #[serde(default)]
    pub auth_header: Option<String>,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub wire_api: WireApi,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub mappings: Vec<ModelMapping>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolRouterConnectionTestInput {
    #[serde(default)]
    pub route_id: String,
    #[serde(default)]
    pub claude_provider_id: String,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolRouterConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_retention_days")]
    pub retention_days: u64,
    #[serde(default)]
    pub routes: Vec<ProtocolRoute>,
}

impl Default for ProtocolRouterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_port(),
            token: generate_token(),
            retention_days: default_retention_days(),
            routes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolRouterStatus {
    pub running: bool,
    pub enabled: bool,
    pub port: u16,
    pub route_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolRouterCallRecord {
    pub ts: u64,
    pub route_id: String,
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    pub wire_api: WireApi,
    pub status: u16,
    pub latency_ms: u128,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub error_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProtocolRouterStats {
    pub calls: Vec<ProtocolRouterCallRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProtocolRouterStatsSummary {
    pub total_calls: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub by_route: Vec<AggregateRow>,
    pub by_provider: Vec<AggregateRow>,
    pub by_model: Vec<AggregateRow>,
    pub calls: Vec<ProtocolRouterCallRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregateRow {
    pub key: String,
    pub calls: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    #[serde(default)]
    pub days: Option<u64>,
}

struct RunningServer {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
}

static RUNNING_SERVER: OnceLock<Mutex<Option<RunningServer>>> = OnceLock::new();

fn default_port() -> u16 {
    17687
}

fn default_retention_days() -> u64 {
    30
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn clamp_retention_days(days: u64) -> u64 {
    days.clamp(1, 365)
}

fn generate_token() -> String {
    format!("osp_{}", uuid::Uuid::new_v4().simple())
}

fn state_lock() -> &'static Mutex<Option<RunningServer>> {
    RUNNING_SERVER.get_or_init(|| Mutex::new(None))
}

fn config_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(CONFIG_FILE))
}

fn legacy_config_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(LEGACY_CONFIG_FILE))
}

fn stats_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(STATS_FILE))
}

fn read_config() -> Result<ProtocolRouterConfig, String> {
    let path = config_path()?;
    if !path.exists() {
        let mut config = read_legacy_runtime_config()?.unwrap_or_default();
        normalize_config(&mut config);
        config.routes = derived_routes().unwrap_or_default();
        return Ok(config);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        let mut config = ProtocolRouterConfig::default();
        config.routes = derived_routes().unwrap_or_default();
        return Ok(config);
    }
    let mut config: ProtocolRouterConfig =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;
    normalize_config(&mut config);
    config.routes = derived_routes().unwrap_or_default();
    Ok(config)
}

fn write_config(config: &ProtocolRouterConfig) -> Result<(), String> {
    let path = config_path()?;
    let mut next = config.clone();
    normalize_config(&mut next);
    next.routes.clear();
    let content = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

fn normalize_config(config: &mut ProtocolRouterConfig) {
    if config.port == 0 {
        config.port = default_port();
    }
    if config.token.trim().is_empty() {
        config.token = generate_token();
    }
    config.retention_days = clamp_retention_days(config.retention_days);
}

fn safe_id(raw: &str) -> String {
    let normalized = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if normalized.is_empty() {
        format!("id-{}", uuid::Uuid::new_v4().simple())
    } else {
        normalized
    }
}

fn validate_config(config: &ProtocolRouterConfig) -> Result<(), String> {
    if config.port == 0 {
        return Err("router port must be greater than 0".to_string());
    }
    if config.retention_days < 1 || config.retention_days > 365 {
        return Err("retention days must be between 1 and 365".to_string());
    }
    Ok(())
}

fn validate_http_url(value: &str, label: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value).map_err(|e| format!("invalid {label}: {e}"))?;
    match url.scheme() {
        "http" | "https" => Ok(()),
        _ => Err(format!("{label} must use http or https")),
    }
}

fn read_legacy_runtime_config() -> Result<Option<ProtocolRouterConfig>, String> {
    let path = legacy_config_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let mut config = ProtocolRouterConfig::default();
    if let Some(enabled) = value.get("enabled").and_then(|v| v.as_bool()) {
        config.enabled = enabled;
    }
    if let Some(port) = value.get("port").and_then(|v| v.as_u64()) {
        config.port = port as u16;
    }
    if let Some(token) = value.get("token").and_then(|v| v.as_str()) {
        config.token = token.to_string();
    }
    if let Some(retention_days) = value.get("retention_days").and_then(|v| v.as_u64()) {
        config.retention_days = retention_days;
    }
    let report_path = crate::config::get_app_dir()?.join("protocol_router_migration_report.json");
    let report = json!({
        "migrated_at": now_ts(),
        "source": LEGACY_CONFIG_FILE,
        "note": "Runtime settings were migrated. Manual catalog and route records are no longer managed here; Claude routes are derived from service provider bindings."
    });
    let _ = fs::write(report_path, serde_json::to_string_pretty(&report).unwrap_or_default());
    Ok(Some(config))
}

fn derived_routes() -> Result<Vec<ProtocolRoute>, String> {
    let state = crate::app_store::load_service_providers_state()?;
    let mut routes = Vec::new();
    for claude in state.providers.iter().filter(|provider| provider.tool == "claude") {
        let legacy_router = claude.claude_api_format == "open_ai_chat"
            || claude.claude_api_format == "open_ai_responses"
            || claude.tool_config.get("model_source").and_then(|v| v.as_str()) == Some("protocol_proxy")
            || claude.tool_config.get("protocol_proxy_route_id").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
        if claude.claude_connection_mode != "protocol_router" && !legacy_router {
            continue;
        }
        match route_from_claude_provider(claude) {
            Ok(route) => routes.push(route),
            Err(err) => routes.push(unresolved_route(claude, &err)),
        }
    }
    Ok(routes)
}

fn route_from_claude_provider(
    claude: &crate::app_store::ServiceProviderRecord,
) -> Result<ProtocolRoute, String> {
    validate_router_provider(claude)?;
    let wire_api = wire_api_from_claude_provider(claude);
    let default_model = claude
        .claude_model_mappings
        .iter()
        .find(|mapping| !mapping.upstream_model.trim().is_empty())
        .map(|mapping| mapping.upstream_model.trim().to_string())
        .or_else(|| claude.model.clone())
        .filter(|model| !model.trim().is_empty());
    let mappings = claude.claude_model_mappings.iter()
        .filter(|mapping| !mapping.family.trim().is_empty() && !mapping.upstream_model.trim().is_empty())
        .map(|mapping| ModelMapping {
            claude_model: mapping.family.trim().to_string(),
            upstream_model: mapping.upstream_model.trim().to_string(),
        })
        .collect::<Vec<_>>();
    Ok(ProtocolRoute {
        id: route_id_for_claude_provider(&claude.id),
        name: claude.name.clone(),
        claude_provider_id: claude.id.clone(),
        claude_provider_name: claude.name.clone(),
        upstream_provider_id: claude.id.clone(),
        upstream_provider_name: claude.name.clone(),
        base_url: claude.base_url.clone().unwrap_or_default(),
        auth_header: Some("Authorization".to_string()),
        api_key: claude.api_key.clone(),
        wire_api,
        default_model,
        mappings,
        enabled: claude.is_enabled.unwrap_or(true),
    })
}

fn unresolved_route(
    claude: &crate::app_store::ServiceProviderRecord,
    reason: &str,
) -> ProtocolRoute {
    ProtocolRoute {
        id: route_id_for_claude_provider(&claude.id),
        name: format!("{} -> {}", claude.name, reason),
        claude_provider_id: claude.id.clone(),
        claude_provider_name: claude.name.clone(),
        upstream_provider_id: String::new(),
        upstream_provider_name: reason.to_string(),
        base_url: String::new(),
        auth_header: Some("Authorization".to_string()),
        api_key: String::new(),
        wire_api: WireApi::OpenAiChat,
        default_model: None,
        mappings: Vec::new(),
        enabled: false,
    }
}

fn validate_router_provider(provider: &crate::app_store::ServiceProviderRecord) -> Result<(), String> {
    if provider.claude_api_format == "anthropic_messages" {
        return Err("protocol router requires OpenAI Chat or OpenAI Responses API format".to_string());
    }
    if !provider.is_enabled.unwrap_or(true) {
        return Err(format!("service provider '{}' is disabled", provider.name));
    }
    let base_url = provider.base_url.as_deref().unwrap_or("").trim();
    if base_url.is_empty() {
        return Err(format!("service provider '{}' is missing Base URL", provider.name));
    }
    validate_http_url(base_url, "provider base URL")?;
    if provider.api_key.trim().is_empty() {
        return Err(format!("service provider '{}' is missing API key", provider.name));
    }
    Ok(())
}

fn wire_api_from_claude_provider(provider: &crate::app_store::ServiceProviderRecord) -> WireApi {
    let raw = if provider.claude_api_format == "open_ai_responses" {
        "open_ai_responses"
    } else {
        &provider.claude_api_format
    };
    match crate::app_store::normalize_protocol_router_wire_api(raw).as_str() {
        "open_ai_responses" => WireApi::OpenAiResponses,
        _ => WireApi::OpenAiChat,
    }
}

fn read_stats() -> Result<ProtocolRouterStats, String> {
    let path = stats_path()?;
    if !path.exists() {
        return Ok(ProtocolRouterStats::default());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(ProtocolRouterStats::default());
    }
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn write_stats(stats: &ProtocolRouterStats) -> Result<(), String> {
    let path = stats_path()?;
    let content = serde_json::to_string_pretty(stats).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

fn prune_calls(calls: &mut Vec<ProtocolRouterCallRecord>, retention_days: u64) {
    let cutoff = now_ts().saturating_sub(clamp_retention_days(retention_days) * 24 * 60 * 60);
    calls.retain(|call| call.ts >= cutoff);
}

fn record_call(call: ProtocolRouterCallRecord, retention_days: u64) {
    if let Ok(mut stats) = read_stats() {
        stats.calls.push(call);
        prune_calls(&mut stats.calls, retention_days);
        let _ = write_stats(&stats);
    }
}

#[tauri::command]
pub fn protocol_router_get_config() -> Result<ProtocolRouterConfig, String> {
    read_config()
}

pub async fn protocol_router_autostart() -> Result<ProtocolRouterStatus, String> {
    let config = read_config()?;
    if config.enabled {
        protocol_router_start().await
    } else {
        Ok(status_from_config(&config, false))
    }
}

#[tauri::command]
pub async fn protocol_router_save_config(
    _app: tauri::AppHandle,
    config: ProtocolRouterConfig,
) -> Result<ProtocolRouterConfig, String> {
    validate_config(&config)?;
    write_config(&config)?;
    if config.enabled {
        protocol_router_start().await?;
    } else {
        protocol_router_stop().await?;
    }
    read_config()
}

#[tauri::command]
pub async fn protocol_router_start() -> Result<ProtocolRouterStatus, String> {
    let config = read_config()?;
    validate_config(&config)?;
    let already_running = {
        let guard = state_lock()
            .lock()
            .map_err(|_| "router state lock poisoned".to_string())?;
        guard.as_ref().map(|s| s.port) == Some(config.port)
    };
    if already_running {
        return Ok(status_from_config(&config, true));
    }
    let listener = TcpListener::bind(("127.0.0.1", config.port))
        .await
        .map_err(|e| format!("failed to bind protocol router port {}: {e}", config.port))?;
    let mut guard = state_lock()
        .lock()
        .map_err(|_| "router state lock poisoned".to_string())?;
    if let Some(mut running) = guard.take() {
        if let Some(tx) = running.shutdown.take() {
            let _ = tx.send(());
        }
    }
    let (tx, rx) = oneshot::channel();
    let port = config.port;
    tauri::async_runtime::spawn(run_server(listener, rx));
    *guard = Some(RunningServer {
        port,
        shutdown: Some(tx),
    });
    Ok(status_from_config(&config, true))
}

#[tauri::command]
pub async fn protocol_router_stop() -> Result<ProtocolRouterStatus, String> {
    let config = read_config()?;
    let mut guard = state_lock()
        .lock()
        .map_err(|_| "router state lock poisoned".to_string())?;
    if let Some(mut running) = guard.take() {
        if let Some(tx) = running.shutdown.take() {
            let _ = tx.send(());
        }
    }
    Ok(status_from_config(&config, false))
}

#[tauri::command]
pub fn protocol_router_status() -> Result<ProtocolRouterStatus, String> {
    let config = read_config()?;
    let running = state_lock().lock().map(|g| g.is_some()).unwrap_or(false);
    Ok(status_from_config(&config, running))
}

#[tauri::command]
pub fn protocol_router_rotate_token() -> Result<ProtocolRouterConfig, String> {
    let mut config = read_config()?;
    config.token = generate_token();
    write_config(&config)?;
    read_config()
}

#[tauri::command]
pub async fn protocol_router_test_connection(
    input: ProtocolRouterConnectionTestInput,
) -> Result<ProtocolRouterCallRecord, String> {
    let route_id = if !input.claude_provider_id.trim().is_empty() {
        route_id_for_claude_provider(&input.claude_provider_id)
    } else {
        input.route_id.clone()
    };
    let route = resolve_runtime_route(&route_id)?;
    let requested_model = input
        .model
        .as_deref()
        .filter(|model| !model.trim().is_empty())
        .or(route.default_model.as_deref());
    let model = resolve_model(&route, requested_model);
    if model.trim().is_empty() {
        return Err("route model is required".to_string());
    }
    let started = Instant::now();
    let body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 8
    });
    let result = forward_request(&route, &body, &model).await;
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(UpstreamResult::Json { status, body }) => {
            let (input_tokens, output_tokens, total_tokens) = usage_from_value(&body);
            Ok(ProtocolRouterCallRecord {
                ts: now_ts(),
                route_id: route.id,
                provider: route.upstream_provider_name,
                model,
                endpoint: "/v1/messages".to_string(),
                wire_api: route.wire_api,
                status,
                latency_ms,
                input_tokens,
                output_tokens,
                total_tokens,
                error_summary: if status >= 400 {
                    Some(error_summary(&body))
                } else {
                    None
                },
            })
        }
        Ok(UpstreamResult::Stream { .. }) => Err("test connection does not use streaming".to_string()),
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub fn protocol_router_stats(
    query: Option<StatsQuery>,
) -> Result<ProtocolRouterStatsSummary, String> {
    let config = read_config()?;
    let mut stats = read_stats()?;
    prune_calls(&mut stats.calls, config.retention_days);
    let days = query.and_then(|q| q.days).unwrap_or(config.retention_days);
    let cutoff = now_ts().saturating_sub(days * 24 * 60 * 60);
    let calls = stats
        .calls
        .into_iter()
        .filter(|call| call.ts >= cutoff)
        .collect::<Vec<_>>();
    Ok(summarize_calls(calls))
}

fn status_from_config(config: &ProtocolRouterConfig, running: bool) -> ProtocolRouterStatus {
    ProtocolRouterStatus {
        running,
        enabled: config.enabled,
        port: config.port,
        route_count: config.routes.len(),
    }
}

async fn run_server(listener: TcpListener, mut shutdown: oneshot::Receiver<()>) {
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

async fn handle_connection(mut stream: TcpStream) -> Result<(), String> {
    let request = read_http_request(&mut stream).await?;
    let response = match route_request(request).await {
        Ok(response) => response,
        Err(response) => response,
    };
    let payload = http_response_bytes(response);
    stream
        .write_all(&payload)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

enum UpstreamResult {
    Json { status: u16, body: Value },
    Stream { status: u16, body: Vec<u8> },
}

fn summarize_non_json_response(status: u16, body: &[u8]) -> String {
    let snippet = String::from_utf8_lossy(body)
        .replace('\n', " ")
        .replace('\r', " ")
        .chars()
        .take(240)
        .collect::<String>();
    if snippet.trim().is_empty() {
        format!("upstream returned HTTP {status} with a non-JSON body")
    } else {
        format!("upstream returned HTTP {status} with a non-JSON body: {}", snippet.trim())
    }
}

async fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let mut header_end = None;
    loop {
        let n = stream.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
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
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[(header_end + 4)..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);
    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn route_request(request: HttpRequest) -> Result<HttpResponse, HttpResponse> {
    if request.method != "POST" {
        return Err(json_response(
            405,
            json!({ "error": { "message": "method not allowed" } }),
        ));
    }
    let Some(route_id) = parse_anthropic_route_id(&request.path) else {
        return Err(json_response(
            404,
            json!({ "error": { "message": "route not found" } }),
        ));
    };
    let config =
        read_config().map_err(|e| json_response(500, json!({ "error": { "message": e } })))?;
    if !is_authorized(&request, &config.token) {
        return Err(json_response(
            401,
            json!({ "error": { "message": "invalid router token" } }),
        ));
    }
    let route = resolve_runtime_route(&route_id).map_err(|e| {
        json_response(
            404,
            json!({ "error": { "message": e } }),
        )
    })?;
    let input: Value = serde_json::from_slice(&request.body)
        .map_err(|e| json_response(400, json!({ "error": { "message": e.to_string() } })))?;
    let started = Instant::now();
    let model = resolve_model(&route, input.get("model").and_then(|v| v.as_str()));
    if model.trim().is_empty() {
        return Err(json_response(
            400,
            json!({ "error": { "message": "model is required" } }),
        ));
    }
    let result = forward_request(&route, &input, &model).await;
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(UpstreamResult::Json { status, body: upstream_body }) => {
            let response_body = upstream_to_anthropic(&upstream_body, &model);
            let (input_tokens, output_tokens, total_tokens) = usage_from_value(&upstream_body);
            record_call(
                ProtocolRouterCallRecord {
                    ts: now_ts(),
                    route_id: route.id,
            provider: route.upstream_provider_name,
                    model,
                    endpoint: "/v1/messages".to_string(),
                    wire_api: route.wire_api,
                    status,
                    latency_ms,
                    input_tokens,
                    output_tokens,
                    total_tokens,
                    error_summary: if status >= 400 {
                        Some(error_summary(&upstream_body))
                    } else {
                        None
                    },
                },
                config.retention_days,
            );
            Ok(json_response(status, response_body))
        }
        Ok(UpstreamResult::Stream { status, body }) => {
            record_call(
                ProtocolRouterCallRecord {
                    ts: now_ts(),
                    route_id: route.id,
                    provider: route.upstream_provider_name,
                    model,
                    endpoint: "/v1/messages".to_string(),
                    wire_api: route.wire_api,
                    status,
                    latency_ms,
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    error_summary: if status >= 400 {
                        Some("streaming upstream request failed".to_string())
                    } else {
                        None
                    },
                },
                config.retention_days,
            );
            Ok(sse_response(status, body))
        }
        Err(error) => {
            record_call(
                ProtocolRouterCallRecord {
                    ts: now_ts(),
                    route_id: route.id,
                    provider: route.upstream_provider_name,
                    model,
                    endpoint: "/v1/messages".to_string(),
                    wire_api: route.wire_api,
                    status: 502,
                    latency_ms,
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    error_summary: Some(error.clone()),
                },
                config.retention_days,
            );
            Err(json_response(502, json!({ "error": { "message": error } })))
        }
    }
}

fn parse_anthropic_route_id(path: &str) -> Option<String> {
    let clean = path.split('?').next().unwrap_or(path);
    let parts = clean.trim_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() == 4 && parts[0] == "anthropic" && parts[2] == "v1" && parts[3] == "messages" {
        Some(parts[1].to_string())
    } else {
        None
    }
}

fn resolve_runtime_route(route_id: &str) -> Result<ProtocolRoute, String> {
    let routes = derived_routes()?;
    let route = routes
        .into_iter()
        .find(|route| route.id == route_id)
        .ok_or_else(|| format!("route not configured: {route_id}"))?;
    if !route.enabled {
        let reason = if route.upstream_provider_name.trim().is_empty() {
            "route is disabled".to_string()
        } else {
            route.upstream_provider_name.clone()
        };
        return Err(format!("route '{}' is unavailable: {reason}", route.id));
    }
    validate_http_url(&route.base_url, "upstream base URL")?;
    if route.default_model.as_deref().unwrap_or("").trim().is_empty() && route.mappings.is_empty() {
        return Err(format!("route '{}' has no upstream model mapping", route.id));
    }
    Ok(route)
}

pub(crate) fn route_id_for_claude_provider(provider_id: &str) -> String {
    format!("service-provider-{}", safe_id(provider_id))
}

fn is_authorized(request: &HttpRequest, token: &str) -> bool {
    if token.trim().is_empty() {
        return false;
    }
    let bearer_ok = request
        .headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v == token)
        .unwrap_or(false);
    let anthropic_ok = request
        .headers
        .get("x-api-key")
        .map(|v| v == token)
        .unwrap_or(false);
    bearer_ok || anthropic_ok
}

fn resolve_model(route: &ProtocolRoute, requested: Option<&str>) -> String {
    let raw = requested
        .filter(|m| !m.trim().is_empty())
        .or(route.default_model.as_deref())
        .unwrap_or("")
        .trim()
        .to_string();
    route
        .mappings
        .iter()
        .find(|mapping| mapping.claude_model == raw)
        .map(|mapping| mapping.upstream_model.clone())
        .unwrap_or(raw)
}

async fn forward_request(
    route: &ProtocolRoute,
    input: &Value,
    model: &str,
) -> Result<UpstreamResult, String> {
    let client = Client::new();
    let endpoint = match route.wire_api {
        WireApi::OpenAiChat => "chat/completions",
        WireApi::OpenAiResponses => "responses",
    };
    let url = join_url(&route.base_url, endpoint);
    let upstream_body = match route.wire_api {
        WireApi::OpenAiChat => anthropic_to_openai_chat(input, model),
        WireApi::OpenAiResponses => anthropic_to_openai_responses(input, model),
    };
    let wants_stream = input.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut req = client.post(url).json(&upstream_body);
    let route_api_key = route.api_key.clone();
    if !route_api_key.trim().is_empty() {
        let header = route.auth_header.as_deref().unwrap_or("Authorization");
        if header.eq_ignore_ascii_case("x-api-key") {
            req = req.header(header, route_api_key.trim().to_string());
        } else {
            req = req.header(header, format!("Bearer {}", route_api_key.trim()));
        }
    }
    let response = req.send().await.map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    if wants_stream {
        let bytes = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
        let body = openai_sse_to_anthropic_sse(&bytes, model);
        return Ok(UpstreamResult::Stream { status, body });
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let body = serde_json::from_slice::<Value>(&bytes)
        .map_err(|_| summarize_non_json_response(status, &bytes))?;
    Ok(UpstreamResult::Json { status, body })
}

fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn anthropic_to_openai_chat(input: &Value, model: &str) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = input.get("system") {
        if let Some(text) = content_to_text(system) {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }
    if let Some(items) = input.get("messages").and_then(|v| v.as_array()) {
        for item in items {
            let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let content = item
                .get("content")
                .and_then(content_to_text)
                .unwrap_or_default();
            messages.push(json!({ "role": role, "content": content }));
        }
    }
    let mut body = Map::new();
    body.insert("model".to_string(), Value::String(model.to_string()));
    body.insert("messages".to_string(), Value::Array(messages));
    if let Some(max_tokens) = input.get("max_tokens").cloned() {
        body.insert("max_tokens".to_string(), max_tokens);
    }
    if let Some(temperature) = input.get("temperature").cloned() {
        body.insert("temperature".to_string(), temperature);
    }
    if let Some(stream) = input.get("stream").cloned() {
        body.insert("stream".to_string(), stream);
    }
    if let Some(tools) = anthropic_tools_to_openai(input.get("tools")) {
        body.insert("tools".to_string(), tools);
    }
    if let Some(tool_choice) = input.get("tool_choice").cloned() {
        body.insert("tool_choice".to_string(), anthropic_tool_choice_to_openai(tool_choice));
    }
    Value::Object(body)
}

fn anthropic_to_openai_responses(input: &Value, model: &str) -> Value {
    let mut output = Map::new();
    output.insert("model".to_string(), Value::String(model.to_string()));
    output.insert(
        "input".to_string(),
        Value::String(anthropic_messages_to_prompt(input)),
    );
    if let Some(max_tokens) = input.get("max_tokens").cloned() {
        output.insert("max_output_tokens".to_string(), max_tokens);
    }
    if let Some(temperature) = input.get("temperature").cloned() {
        output.insert("temperature".to_string(), temperature);
    }
    if let Some(stream) = input.get("stream").cloned() {
        output.insert("stream".to_string(), stream);
    }
    if let Some(tools) = anthropic_tools_to_responses(input.get("tools")) {
        output.insert("tools".to_string(), tools);
    }
    Value::Object(output)
}

fn anthropic_messages_to_prompt(input: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(system) = input.get("system").and_then(content_to_text) {
        parts.push(format!("system: {system}"));
    }
    if let Some(items) = input.get("messages").and_then(|v| v.as_array()) {
        for item in items {
            let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let content = item
                .get("content")
                .and_then(content_to_text)
                .unwrap_or_default();
            parts.push(format!("{role}: {content}"));
        }
    }
    parts.join("\n")
}

fn content_to_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                        item.get("text")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    } else if item.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                        item.get("content").and_then(content_to_text)
                    } else if item.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
                        let input = item.get("input").cloned().unwrap_or(Value::Null);
                        Some(format!("tool_use {name}: {input}"))
                    } else if let Some(s) = item.as_str() {
                        Some(s.to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            Some(parts.join("\n"))
        }
        _ => None,
    }
}

fn anthropic_tools_to_openai(tools: Option<&Value>) -> Option<Value> {
    let tools = tools?.as_array()?;
    let mapped = tools
        .iter()
        .filter_map(|tool| {
            let name = tool.get("name").and_then(|v| v.as_str())?;
            let description = tool
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let parameters = tool
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
            Some(json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": parameters
                }
            }))
        })
        .collect::<Vec<_>>();
    if mapped.is_empty() {
        None
    } else {
        Some(Value::Array(mapped))
    }
}

fn anthropic_tools_to_responses(tools: Option<&Value>) -> Option<Value> {
    let tools = tools?.as_array()?;
    let mapped = tools
        .iter()
        .filter_map(|tool| {
            let name = tool.get("name").and_then(|v| v.as_str())?;
            let description = tool
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let parameters = tool
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
            Some(json!({
                "type": "function",
                "name": name,
                "description": description,
                "parameters": parameters
            }))
        })
        .collect::<Vec<_>>();
    if mapped.is_empty() {
        None
    } else {
        Some(Value::Array(mapped))
    }
}

fn anthropic_tool_choice_to_openai(value: Value) -> Value {
    if value.get("type").and_then(|v| v.as_str()) == Some("tool") {
        if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
            return json!({ "type": "function", "function": { "name": name } });
        }
    }
    match value.get("type").and_then(|v| v.as_str()) {
        Some("auto") => Value::String("auto".to_string()),
        Some("any") => Value::String("required".to_string()),
        Some("none") => Value::String("none".to_string()),
        _ => Value::String("auto".to_string()),
    }
}

fn upstream_to_anthropic(value: &Value, model: &str) -> Value {
    if let Some(choice) = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|items| items.first())
    {
        let text = choice
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|v| v.as_str())
            .or_else(|| choice.get("text").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        let mut content = Vec::new();
        if !text.is_empty() {
            content.push(json!({ "type": "text", "text": text }));
        }
        if let Some(tool_calls) = choice
            .get("message")
            .and_then(|message| message.get("tool_calls"))
            .and_then(|v| v.as_array())
        {
            for call in tool_calls {
                let id = call
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("toolu_proxy");
                let function = call.get("function").unwrap_or(&Value::Null);
                let name = function
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool");
                let arguments = function
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or_else(|| json!({}));
                content.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": arguments
                }));
            }
        }
        if content.is_empty() {
            content.push(json!({ "type": "text", "text": "" }));
        }
        let (input_tokens, output_tokens, _) = usage_from_value(value);
        return json!({
            "id": value.get("id").cloned().unwrap_or_else(|| Value::String(format!("msg_{}", uuid::Uuid::new_v4().simple()))),
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": content,
            "stop_reason": if content.iter().any(|block| block.get("type").and_then(|v| v.as_str()) == Some("tool_use")) { "tool_use" } else { "end_turn" },
            "stop_sequence": Value::Null,
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": output_tokens
            }
        });
    }
    let text = value
        .get("output_text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| extract_responses_output_text(value))
        .unwrap_or_default();
    let (input_tokens, output_tokens, _) = usage_from_value(value);
    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| Value::String(format!("msg_{}", uuid::Uuid::new_v4().simple()))),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{ "type": "text", "text": text }],
        "stop_reason": "end_turn",
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens
        }
    })
}

fn extract_responses_output_text(value: &Value) -> Option<String> {
    let output = value.get("output")?.as_array()?;
    let mut parts = Vec::new();
    for item in output {
        if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
            for block in content {
                if let Some(text) = block
                    .get("text")
                    .and_then(|v| v.as_str())
                    .or_else(|| block.get("output_text").and_then(|v| v.as_str()))
                {
                    parts.push(text.to_string());
                }
            }
        }
    }
    Some(parts.join("\n"))
}

fn usage_from_value(value: &Value) -> (u64, u64, u64) {
    let Some(usage) = value.get("usage") else {
        return (0, 0, 0);
    };
    let input = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = usage
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(input + output);
    (input, output, total)
}

fn error_summary(value: &Value) -> String {
    value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(|v| v.as_str())
                .or_else(|| error.as_str())
        })
        .or_else(|| value.get("message").and_then(|v| v.as_str()))
        .unwrap_or("upstream request failed")
        .chars()
        .take(240)
        .collect()
}

fn json_response(status: u16, body: Value) -> HttpResponse {
    let payload = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    HttpResponse {
        status,
        content_type: "application/json",
        body: payload,
    }
}

fn sse_response(status: u16, body: Vec<u8>) -> HttpResponse {
    HttpResponse {
        status,
        content_type: "text/event-stream",
        body,
    }
}

fn http_response_bytes(response: HttpResponse) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        response.status,
        reason_for_status(response.status),
        response.content_type,
        response.body.len()
    );
    [header.into_bytes(), response.body].concat()
}

fn reason_for_status(status: u16) -> &'static str {
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

fn openai_sse_to_anthropic_sse(input: &[u8], model: &str) -> Vec<u8> {
    let raw = String::from_utf8_lossy(input);
    let message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    let mut out = String::new();
    let mut tool_calls: Vec<StreamToolCall> = Vec::new();
    out.push_str(&sse_event(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": Value::Null,
                "stop_sequence": Value::Null,
                "usage": { "input_tokens": 0, "output_tokens": 0 }
            }
        }),
    ));
    out.push_str(&sse_event(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "text", "text": "" }
        }),
    ));

    for line in raw.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(text) = openai_stream_text_delta(&value) {
            if !text.is_empty() {
                out.push_str(&sse_event(
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": { "type": "text_delta", "text": text }
                    }),
                ));
            }
        }
        collect_openai_stream_tool_calls(&value, &mut tool_calls);
    }
    out.push_str(&sse_event(
        "content_block_stop",
        json!({ "type": "content_block_stop", "index": 0 }),
    ));

    for (offset, tool_call) in tool_calls.iter().enumerate() {
        let index = offset + 1;
        out.push_str(&sse_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": index,
                "content_block": {
                    "type": "tool_use",
                    "id": if tool_call.id.is_empty() { format!("toolu_{}", uuid::Uuid::new_v4().simple()) } else { tool_call.id.clone() },
                    "name": if tool_call.name.is_empty() { "tool".to_string() } else { tool_call.name.clone() },
                    "input": {}
                }
            }),
        ));
        if !tool_call.arguments.is_empty() {
            out.push_str(&sse_event(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": tool_call.arguments
                    }
                }),
            ));
        }
        out.push_str(&sse_event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": index }),
        ));
    }

    let stop_reason = if tool_calls.is_empty() {
        "end_turn"
    } else {
        "tool_use"
    };
    out.push_str(&sse_event(
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": stop_reason, "stop_sequence": Value::Null },
            "usage": { "output_tokens": 0 }
        }),
    ));
    out.push_str(&sse_event("message_stop", json!({ "type": "message_stop" })));
    out.into_bytes()
}

fn sse_event(event: &str, data: Value) -> String {
    format!("event: {event}\ndata: {}\n\n", data)
}

fn openai_stream_text_delta(value: &Value) -> Option<String> {
    value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("output_text"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            value
                .get("delta")
                .and_then(|delta| delta.get("text").or_else(|| delta.get("content")))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            value
                .get("delta")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
}

#[derive(Default)]
struct StreamToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn collect_openai_stream_tool_calls(value: &Value, tool_calls: &mut Vec<StreamToolCall>) {
    let Some(calls) = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("tool_calls"))
        .and_then(|v| v.as_array())
    else {
        return;
    };
    for call in calls {
        let index = call.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        while tool_calls.len() <= index {
            tool_calls.push(StreamToolCall::default());
        }
        let target = &mut tool_calls[index];
        if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
            target.id = id.to_string();
        }
        if let Some(function) = call.get("function") {
            if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
                target.name = name.to_string();
            }
            if let Some(arguments) = function.get("arguments").and_then(|v| v.as_str()) {
                target.arguments.push_str(arguments);
            }
        }
    }
}

#[allow(dead_code)]
pub(crate) fn parse_openai_models_catalog(
    value: &Value,
    prefix: Option<&str>,
) -> Result<Vec<CatalogModel>, String> {
    let data = value
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "models response must contain data array".to_string())?;
    let prefix = prefix.unwrap_or("").trim();
    let mut models = Vec::new();
    for item in data {
        let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let id = if prefix.is_empty() || id.starts_with(prefix) {
            id.to_string()
        } else {
            format!("{prefix}{id}")
        };
        models.push(CatalogModel {
            id,
            object: item
                .get("object")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            created: item.get("created").and_then(|v| v.as_u64()),
            owned_by: item
                .get("owned_by")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        });
    }
    Ok(models)
}

fn summarize_calls(calls: Vec<ProtocolRouterCallRecord>) -> ProtocolRouterStatsSummary {
    let mut summary = ProtocolRouterStatsSummary {
        total_calls: calls.len(),
        ..ProtocolRouterStatsSummary::default()
    };
    for call in &calls {
        summary.input_tokens += call.input_tokens;
        summary.output_tokens += call.output_tokens;
        summary.total_tokens += call.total_tokens;
    }
    summary.by_route = aggregate(&calls, |call| call.route_id.clone());
    summary.by_provider = aggregate(&calls, |call| call.provider.clone());
    summary.by_model = aggregate(&calls, |call| call.model.clone());
    summary.calls = calls;
    summary
}

fn aggregate(
    calls: &[ProtocolRouterCallRecord],
    key_fn: impl Fn(&ProtocolRouterCallRecord) -> String,
) -> Vec<AggregateRow> {
    let mut map: HashMap<String, AggregateRow> = HashMap::new();
    for call in calls {
        let key = key_fn(call);
        let row = map.entry(key.clone()).or_insert_with(|| AggregateRow {
            key,
            ..AggregateRow::default()
        });
        row.calls += 1;
        row.input_tokens += call.input_tokens;
        row.output_tokens += call.output_tokens;
        row.total_tokens += call.total_tokens;
    }
    let mut rows = map.into_values().collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| a.key.cmp(&b.key))
    });
    rows
}

pub(crate) fn router_base_url_for_route(route_id: &str) -> Result<String, String> {
    let config = read_config()?;
    Ok(format!(
        "http://127.0.0.1:{}/anthropic/{}/v1",
        config.port,
        safe_id(route_id)
    ))
}

pub(crate) fn router_base_url_for_claude_provider(provider_id: &str) -> Result<String, String> {
    router_base_url_for_route(&route_id_for_claude_provider(provider_id))
}

pub(crate) fn router_token() -> Result<String, String> {
    Ok(read_config()?.token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn parses_openai_models_catalog() {
        let value = json!({
            "object": "list",
            "data": [
                { "id": "kimi-k2.6", "object": "model", "created": 1, "owned_by": "moonshot" },
                { "id": "gpt-5.1", "object": "model", "created": 2, "owned_by": "openai" }
            ]
        });
        let models = parse_openai_models_catalog(&value, None).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "kimi-k2.6");
        assert_eq!(models[1].owned_by.as_deref(), Some("openai"));
    }

    #[test]
    fn parses_opencode_go_fixture_shape() {
        let value = json!({
            "object": "list",
            "data": [
                { "id": "claude-sonnet-4", "object": "model", "created": 0, "owned_by": "opencode-go" }
            ]
        });
        let models = parse_openai_models_catalog(&value, Some("go:")).unwrap();
        assert_eq!(models[0].id, "go:claude-sonnet-4");
    }

    #[test]
    fn opencode_go_style_routes_should_use_openai_chat_endpoint() {
        let provider = crate::app_store::ServiceProviderRecord {
            id: "opencode-go".to_string(),
            name: "OpenCode Go".to_string(),
            tool: "claude".to_string(),
            icon: None,
            api_key: "sk-test".to_string(),
            base_url: Some("https://opencode.ai/zen/go/v1".to_string()),
            model: Some("claude-sonnet-4".to_string()),
            claude_api_format: "open_ai_chat".to_string(),
            claude_connection_mode: "protocol_router".to_string(),
            protocol_router_upstream_provider_id: None,
            protocol_router_wire_api: "open_ai_chat".to_string(),
            claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
            claude_model_mappings: vec![],
            claude_enable_tool_search: None,
            claude_auto_memory_enabled: None,
            claude_always_thinking_enabled: None,
            claude_away_summary_enabled: None,
            claude_include_git_instructions: None,
            claude_enable_attribution: None,
            code: Some("opencode-go".to_string()),
            is_enabled: Some(true),
            provider_key: None,
            favorite_at: None,
            env_managed: Some(true),
            tool_config: Map::new(),
            history: vec![],
            extra: Map::new(),
            fetched_models: None,
        };

        let route = route_from_claude_provider(&provider).unwrap();
        assert_eq!(route.wire_api, WireApi::OpenAiChat);
        assert_eq!(join_url(&route.base_url, "chat/completions"), "https://opencode.ai/zen/go/v1/chat/completions");
    }

    #[test]
    fn converts_anthropic_to_openai_chat() {
        let input = json!({
            "model": "sonnet",
            "system": "be brief",
            "messages": [
                { "role": "user", "content": [{ "type": "text", "text": "hello" }] }
            ],
            "max_tokens": 42
        });
        let output = anthropic_to_openai_chat(&input, "kimi-k2.6");
        assert_eq!(output["model"], "kimi-k2.6");
        assert_eq!(output["messages"][0]["role"], "system");
        assert_eq!(output["messages"][1]["content"], "hello");
        assert_eq!(output["max_tokens"], 42);
    }

    #[test]
    fn converts_openai_chat_to_anthropic() {
        let input = json!({
            "id": "chatcmpl_1",
            "choices": [{ "message": { "content": "hi" } }],
            "usage": { "prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7 }
        });
        let output = upstream_to_anthropic(&input, "kimi-k2.6");
        assert_eq!(output["type"], "message");
        assert_eq!(output["content"][0]["text"], "hi");
        assert_eq!(output["usage"]["input_tokens"], 3);
        assert_eq!(output["usage"]["output_tokens"], 4);
    }

    #[test]
    fn converts_tools_to_openai_chat_tools() {
        let input = json!({
            "model": "sonnet",
            "messages": [{ "role": "user", "content": "use a tool" }],
            "tools": [{
                "name": "read_file",
                "description": "Read a file",
                "input_schema": { "type": "object", "properties": { "path": { "type": "string" } } }
            }]
        });
        let output = anthropic_to_openai_chat(&input, "kimi-k2.6");
        assert_eq!(output["tools"][0]["type"], "function");
        assert_eq!(output["tools"][0]["function"]["name"], "read_file");
    }

    #[test]
    fn converts_openai_tool_call_to_anthropic_tool_use() {
        let input = json!({
            "id": "chatcmpl_1",
            "choices": [{
                "message": {
                    "content": "",
                    "tool_calls": [{
                        "id": "call_1",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"README.md\"}"
                        }
                    }]
                }
            }]
        });
        let output = upstream_to_anthropic(&input, "kimi-k2.6");
        assert_eq!(output["stop_reason"], "tool_use");
        assert_eq!(output["content"][0]["type"], "tool_use");
        assert_eq!(output["content"][0]["name"], "read_file");
        assert_eq!(output["content"][0]["input"]["path"], "README.md");
    }

    #[test]
    fn converts_openai_sse_to_anthropic_sse() {
        let input = br#"data: {"choices":[{"delta":{"content":"hello"}}]}
data: {"choices":[{"delta":{"content":" world"}}]}
data: [DONE]
"#;
        let output = String::from_utf8(openai_sse_to_anthropic_sse(input, "kimi-k2.6")).unwrap();
        assert!(output.contains("event: message_start"));
        assert!(output.contains("event: content_block_delta"));
        assert!(output.contains("hello"));
        assert!(output.contains("world"));
        assert!(output.contains("event: message_stop"));
    }

    #[test]
    fn converts_openai_sse_tool_call_to_anthropic_tool_use_stream() {
        let input = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":"{\"path\""}}]}}]}
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"README.md\"}"}}]}}]}
data: [DONE]
"#;
        let output = String::from_utf8(openai_sse_to_anthropic_sse(input, "kimi-k2.6")).unwrap();
        assert!(output.contains("\"type\":\"tool_use\""));
        assert!(output.contains("\"name\":\"read_file\""));
        assert!(output.contains("input_json_delta"));
        assert!(output.contains("README.md"));
        assert!(output.contains("\"stop_reason\":\"tool_use\""));
    }

    #[test]
    fn prunes_by_retention_days() {
        let now = now_ts();
        let mut calls = vec![
            ProtocolRouterCallRecord {
                ts: now.saturating_sub(40 * 24 * 60 * 60),
                route_id: "old".into(),
                provider: "p".into(),
                model: "m".into(),
                endpoint: "/v1/messages".into(),
                wire_api: WireApi::OpenAiChat,
                status: 200,
                latency_ms: 1,
                input_tokens: 1,
                output_tokens: 1,
                total_tokens: 2,
                error_summary: None,
            },
            ProtocolRouterCallRecord {
                ts: now,
                route_id: "new".into(),
                provider: "p".into(),
                model: "m".into(),
                endpoint: "/v1/messages".into(),
                wire_api: WireApi::OpenAiChat,
                status: 200,
                latency_ms: 1,
                input_tokens: 1,
                output_tokens: 1,
                total_tokens: 2,
                error_summary: None,
            },
        ];
        prune_calls(&mut calls, 30);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].route_id, "new");
    }

    async fn spawn_mock_server(response_body: &'static str) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tauri::async_runtime::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn forwards_openai_chat_to_mock_endpoint() {
        let base = spawn_mock_server(
            r#"{"id":"chatcmpl_mock","choices":[{"message":{"content":"mock ok"}}],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}"#,
        )
        .await;
        let route = ProtocolRoute {
            id: "mock".to_string(),
            name: "Mock".to_string(),
            claude_provider_id: "claude-mock".to_string(),
            claude_provider_name: "Claude Mock".to_string(),
            upstream_provider_id: "mock".to_string(),
            upstream_provider_name: "Mock".to_string(),
            base_url: base,
            auth_header: Some("Authorization".to_string()),
            api_key: String::new(),
            wire_api: WireApi::OpenAiChat,
            default_model: Some("mock-model".to_string()),
            mappings: Vec::new(),
            enabled: true,
        };
        let input = json!({
            "model": "mock-model",
            "messages": [{ "role": "user", "content": "hello" }],
            "max_tokens": 8
        });
        let result = forward_request(&route, &input, "mock-model").await.unwrap();
        let UpstreamResult::Json { status, body } = result else {
            panic!("expected json response");
        };
        assert_eq!(status, 200);
        let anthropic = upstream_to_anthropic(&body, "mock-model");
        assert_eq!(anthropic["content"][0]["text"], "mock ok");
        assert_eq!(anthropic["usage"]["output_tokens"], 3);
    }
}
