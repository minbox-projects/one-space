use super::route_id_for_claude_provider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::sync::oneshot;

pub(in crate::protocol_router) const CONFIG_FILE: &str = "protocol_router.json";
pub(in crate::protocol_router) const LEGACY_CONFIG_FILE: &str = "protocol_proxy.json";
pub(in crate::protocol_router) const STATS_FILE: &str = "protocol_router_calls.json";

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

pub(in crate::protocol_router) fn default_true() -> bool {
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

pub(in crate::protocol_router) struct RunningServer {
    pub(in crate::protocol_router) port: u16,
    pub(in crate::protocol_router) shutdown: Option<oneshot::Sender<()>>,
}

pub(in crate::protocol_router) static RUNNING_SERVER: OnceLock<Mutex<Option<RunningServer>>> =
    OnceLock::new();

pub(in crate::protocol_router) fn default_port() -> u16 {
    17687
}

pub(in crate::protocol_router) fn default_retention_days() -> u64 {
    30
}

pub(in crate::protocol_router) fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(in crate::protocol_router) fn clamp_retention_days(days: u64) -> u64 {
    days.clamp(1, 365)
}

pub(in crate::protocol_router) fn generate_token() -> String {
    format!("osp_{}", uuid::Uuid::new_v4().simple())
}

pub(in crate::protocol_router) fn state_lock() -> &'static Mutex<Option<RunningServer>> {
    RUNNING_SERVER.get_or_init(|| Mutex::new(None))
}

pub(in crate::protocol_router) fn config_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(CONFIG_FILE))
}

pub(in crate::protocol_router) fn legacy_config_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(LEGACY_CONFIG_FILE))
}

pub(in crate::protocol_router) fn stats_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(STATS_FILE))
}

pub(in crate::protocol_router) fn read_config() -> Result<ProtocolRouterConfig, String> {
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

pub(in crate::protocol_router) fn write_config(
    config: &ProtocolRouterConfig,
) -> Result<(), String> {
    let path = config_path()?;
    let mut next = config.clone();
    normalize_config(&mut next);
    next.routes.clear();
    let content = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub(in crate::protocol_router) fn normalize_config(config: &mut ProtocolRouterConfig) {
    if config.port == 0 {
        config.port = default_port();
    }
    if config.token.trim().is_empty() {
        config.token = generate_token();
    }
    config.retention_days = clamp_retention_days(config.retention_days);
}

pub(in crate::protocol_router) fn safe_id(raw: &str) -> String {
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

pub(in crate::protocol_router) fn validate_config(
    config: &ProtocolRouterConfig,
) -> Result<(), String> {
    if config.port == 0 {
        return Err("router port must be greater than 0".to_string());
    }
    if config.retention_days < 1 || config.retention_days > 365 {
        return Err("retention days must be between 1 and 365".to_string());
    }
    Ok(())
}

pub(in crate::protocol_router) fn validate_http_url(
    value: &str,
    label: &str,
) -> Result<(), String> {
    let url = reqwest::Url::parse(value).map_err(|e| format!("invalid {label}: {e}"))?;
    match url.scheme() {
        "http" | "https" => Ok(()),
        _ => Err(format!("{label} must use http or https")),
    }
}

pub(in crate::protocol_router) fn read_legacy_runtime_config(
) -> Result<Option<ProtocolRouterConfig>, String> {
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
    let _ = fs::write(
        report_path,
        serde_json::to_string_pretty(&report).unwrap_or_default(),
    );
    Ok(Some(config))
}

pub(in crate::protocol_router) fn derived_routes() -> Result<Vec<ProtocolRoute>, String> {
    let state = crate::app_store::load_service_providers_state()?;
    let mut routes = Vec::new();
    for claude in state
        .providers
        .iter()
        .filter(|provider| provider.tool == "claude")
    {
        let legacy_router = claude.claude_api_format == "open_ai_chat"
            || claude.claude_api_format == "open_ai_responses"
            || claude
                .tool_config
                .get("model_source")
                .and_then(|v| v.as_str())
                == Some("protocol_proxy")
            || claude
                .tool_config
                .get("protocol_proxy_route_id")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
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

pub(in crate::protocol_router) fn route_from_claude_provider(
    claude: &crate::app_store::ServiceProviderRecord,
) -> Result<ProtocolRoute, String> {
    validate_router_provider(claude)?;
    let wire_api = wire_api_from_claude_provider(claude);
    let default_model = claude
        .model
        .clone()
        .or_else(|| {
            claude
                .claude_model_mappings
                .iter()
                .find(|mapping| !mapping.upstream_model.trim().is_empty())
                .map(|mapping| mapping.upstream_model.trim().to_string())
        })
        .filter(|model| !model.trim().is_empty());
    let mappings = claude
        .claude_model_mappings
        .iter()
        .filter(|mapping| {
            !mapping.family.trim().is_empty() && !mapping.upstream_model.trim().is_empty()
        })
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

pub(in crate::protocol_router) fn unresolved_route(
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

pub(in crate::protocol_router) fn validate_router_provider(
    provider: &crate::app_store::ServiceProviderRecord,
) -> Result<(), String> {
    if provider.claude_api_format == "anthropic_messages" {
        return Err(
            "protocol router requires OpenAI Chat or OpenAI Responses API format".to_string(),
        );
    }
    if !provider.is_enabled.unwrap_or(true) {
        return Err(format!("service provider '{}' is disabled", provider.name));
    }
    let base_url = provider.base_url.as_deref().unwrap_or("").trim();
    if base_url.is_empty() {
        return Err(format!(
            "service provider '{}' is missing Base URL",
            provider.name
        ));
    }
    validate_http_url(base_url, "provider base URL")?;
    if provider.api_key.trim().is_empty() {
        return Err(format!(
            "service provider '{}' is missing API key",
            provider.name
        ));
    }
    Ok(())
}

pub(in crate::protocol_router) fn wire_api_from_claude_provider(
    provider: &crate::app_store::ServiceProviderRecord,
) -> WireApi {
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

pub(in crate::protocol_router) fn read_stats() -> Result<ProtocolRouterStats, String> {
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

pub(in crate::protocol_router) fn write_stats(stats: &ProtocolRouterStats) -> Result<(), String> {
    let path = stats_path()?;
    let content = serde_json::to_string_pretty(stats).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub(crate) fn remap_service_provider_route_stats(
    provider_id_map: &HashMap<String, String>,
) -> Result<bool, String> {
    if provider_id_map.is_empty() {
        return Ok(false);
    }
    let path = stats_path()?;
    if !path.exists() {
        return Ok(false);
    }

    let mut stats = read_stats()?;
    let route_id_map = provider_id_map
        .iter()
        .map(|(old_id, new_id)| {
            (
                route_id_for_claude_provider(old_id),
                route_id_for_claude_provider(new_id),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut changed = false;
    for call in &mut stats.calls {
        if let Some(next_route_id) = route_id_map.get(&call.route_id) {
            call.route_id = next_route_id.clone();
            changed = true;
        }
    }

    if changed {
        write_stats(&stats)?;
    }
    Ok(changed)
}

pub(in crate::protocol_router) fn prune_calls(
    calls: &mut Vec<ProtocolRouterCallRecord>,
    retention_days: u64,
) {
    let cutoff = now_ts().saturating_sub(clamp_retention_days(retention_days) * 24 * 60 * 60);
    calls.retain(|call| call.ts >= cutoff);
}

pub(in crate::protocol_router) fn record_call(call: ProtocolRouterCallRecord, retention_days: u64) {
    if let Ok(mut stats) = read_stats() {
        stats.calls.push(call);
        prune_calls(&mut stats.calls, retention_days);
        let _ = write_stats(&stats);
    }
}
