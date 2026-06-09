use crate::app_store::{
    default_claude_model_mappings_from_tool_config, ClaudeModelMapping, ProviderHistoryEntry,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub(in crate::app_store) fn strip_legacy_claude_model_keys(tool_config: &mut Map<String, Value>) {
    for key in [
        "claude_haiku_model",
        "claude_sonnet_model",
        "claude_opus_model",
    ] {
        tool_config.remove(key);
    }
}

pub(crate) fn resolved_claude_model_mappings(
    tool_config: &Map<String, Value>,
) -> Vec<ClaudeModelMapping> {
    tool_config
        .get("claude_model_mappings")
        .and_then(|v| serde_json::from_value::<Vec<ClaudeModelMapping>>(v.clone()).ok())
        .filter(|mappings| !mappings.is_empty())
        .unwrap_or_else(|| default_claude_model_mappings_from_tool_config(tool_config))
}

pub(crate) fn resolve_claude_reasoning_effort(tool_config: &Map<String, Value>) -> Option<String> {
    tool_config
        .get("claude_reasoning_effort")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
}

pub(crate) fn normalize_claude_default_model_value(model: Option<&str>) -> Option<String> {
    model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

pub(crate) fn resolve_claude_default_model_from_settings(
    settings: &Map<String, Value>,
) -> Option<String> {
    let env_model = settings
        .get("env")
        .and_then(|v| v.as_object())
        .and_then(|env| env.get("ANTHROPIC_MODEL"))
        .and_then(|v| v.as_str());
    let top_level_model = settings.get("model").and_then(|v| v.as_str());
    normalize_claude_default_model_value(env_model.or(top_level_model))
}

pub(crate) fn resolve_claude_default_model(
    model: Option<&str>,
    tool_config: &Map<String, Value>,
) -> Option<String> {
    let mirrored = tool_config
        .get("claude_default_model")
        .and_then(|v| v.as_str());
    normalize_claude_default_model_value(mirrored.or(model))
}

pub(crate) fn sync_claude_default_model_fields(record: &mut ServiceProviderRecord) {
    if record.tool != "claude" {
        return;
    }

    let normalized = resolve_claude_default_model(record.model.as_deref(), &record.tool_config);
    record.model = normalized.clone();
    if let Some(value) = normalized {
        record
            .tool_config
            .insert("claude_default_model".to_string(), Value::String(value));
    } else {
        record.tool_config.remove("claude_default_model");
    }
}

/// A service provider record — the unified "Service Provider" domain replacing ProviderRecord.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ServiceProviderRecord {
    pub id: String,
    pub name: String,
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub api_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Claude-specific API format: anthropic_messages, open_ai_chat, open_ai_responses
    #[serde(
        default = "default_claude_api_format",
        skip_serializing_if = "is_default_claude_api_format"
    )]
    pub claude_api_format: String,
    /// Claude connection mode: native Anthropic API or local protocol router.
    #[serde(
        default = "default_claude_connection_mode",
        skip_serializing_if = "is_default_claude_connection_mode"
    )]
    pub claude_connection_mode: String,
    /// Upstream AI terminal provider used when Claude connects through the protocol router.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_router_upstream_provider_id: Option<String>,
    /// Upstream wire protocol exposed by OpenAI-compatible providers.
    #[serde(
        default = "default_protocol_router_wire_api",
        skip_serializing_if = "is_default_protocol_router_wire_api"
    )]
    pub protocol_router_wire_api: String,
    /// Which env key to use for auth: ANTHROPIC_AUTH_TOKEN or ANTHROPIC_API_KEY
    #[serde(
        default = "default_claude_auth_env_key",
        skip_serializing_if = "is_default_auth_env_key"
    )]
    pub claude_auth_env_key: String,
    /// Claude model mappings (haiku/sonnet/opus rows)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claude_model_mappings: Vec<ClaudeModelMapping>,
    /// Enable tool search for Claude
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_enable_tool_search: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_auto_memory_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_always_thinking_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_away_summary_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_include_git_instructions: Option<bool>,
    /// Enable attribution (default false = hidden)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_enable_attribution: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_managed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite_at: Option<u64>,
    #[serde(default)]
    pub tool_config: Map<String, Value>,
    #[serde(default)]
    pub history: Vec<ProviderHistoryEntry>,
    #[serde(default)]
    pub extra: Map<String, Value>,
    /// Cached model list fetched from upstream
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_models: Option<Vec<String>>,
}

/// State for all service providers — replaces ProvidersState.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ServiceProvidersState {
    #[serde(default)]
    pub active: HashMap<String, String>,
    #[serde(default)]
    pub providers: Vec<ServiceProviderRecord>,
}

/// Input for creating/updating a service provider — replaces ProviderInput.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ServiceProviderInput {
    pub id: String,
    pub name: String,
    pub tool: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub claude_api_format: Option<String>,
    #[serde(default)]
    pub claude_connection_mode: Option<String>,
    #[serde(default)]
    pub protocol_router_upstream_provider_id: Option<String>,
    #[serde(default)]
    pub protocol_router_wire_api: Option<String>,
    #[serde(default)]
    pub claude_auth_env_key: Option<String>,
    #[serde(default)]
    pub claude_model_mappings: Option<Vec<ClaudeModelMapping>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_enable_tool_search: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_auto_memory_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_always_thinking_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_away_summary_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_include_git_instructions: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_enable_attribution: Option<bool>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub provider_key: Option<String>,
    #[serde(default)]
    pub favorite_at: Option<u64>,
    #[serde(default)]
    pub fields: Map<String, Value>,
    /// Cached models from upstream fetch (optional, for draft support)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_models: Option<Vec<String>>,
}

pub(in crate::app_store) fn default_claude_api_format() -> String {
    "anthropic_messages".to_string()
}

pub(in crate::app_store) fn is_default_claude_api_format(s: &str) -> bool {
    s == "anthropic_messages"
}

pub(in crate::app_store) fn default_claude_connection_mode() -> String {
    "native_anthropic".to_string()
}

pub(in crate::app_store) fn is_default_claude_connection_mode(s: &str) -> bool {
    s == "native_anthropic"
}

pub(in crate::app_store) fn default_protocol_router_wire_api() -> String {
    "open_ai_chat".to_string()
}

pub(in crate::app_store) fn is_default_protocol_router_wire_api(s: &str) -> bool {
    s == "open_ai_chat"
}

pub(crate) fn normalize_protocol_router_wire_api(raw: &str) -> String {
    match raw {
        "open_ai_responses" | "responses" => "open_ai_responses".to_string(),
        _ => "open_ai_chat".to_string(),
    }
}

pub(in crate::app_store) fn normalize_claude_api_format(raw: &str) -> Option<String> {
    match raw.trim() {
        "anthropic_messages" | "anthropic" => Some("anthropic_messages".to_string()),
        "open_ai_chat" | "chat" => Some("open_ai_chat".to_string()),
        "open_ai_responses" | "responses" => Some("open_ai_responses".to_string()),
        _ => None,
    }
}

pub(in crate::app_store) fn infer_claude_connection_mode(
    explicit: Option<&str>,
    claude_api_format: &str,
) -> String {
    match explicit.map(str::trim) {
        Some("protocol_router") => "protocol_router".to_string(),
        Some("native_anthropic") => {
            if claude_api_format == "open_ai_chat" || claude_api_format == "open_ai_responses" {
                "protocol_router".to_string()
            } else {
                "native_anthropic".to_string()
            }
        }
        _ => {
            if claude_api_format == "open_ai_chat" || claude_api_format == "open_ai_responses" {
                "protocol_router".to_string()
            } else {
                "native_anthropic".to_string()
            }
        }
    }
}

pub(in crate::app_store) fn infer_claude_api_format(
    explicit: Option<&str>,
    connection_mode: Option<&str>,
    wire_api: Option<&str>,
) -> String {
    if let Some(value) = explicit.and_then(normalize_claude_api_format) {
        if value == "anthropic_messages"
            && connection_mode.map(str::trim) == Some("protocol_router")
        {
            return normalize_protocol_router_wire_api(wire_api.unwrap_or("open_ai_chat"));
        }
        return value;
    }

    if connection_mode.map(str::trim) == Some("protocol_router") {
        return normalize_protocol_router_wire_api(wire_api.unwrap_or("open_ai_chat"));
    }

    "anthropic_messages".to_string()
}

pub(in crate::app_store) fn infer_protocol_router_wire_api(
    explicit: Option<&str>,
    claude_api_format: &str,
    connection_mode: Option<&str>,
) -> String {
    if let Some(raw) = explicit {
        let normalized = normalize_protocol_router_wire_api(raw);
        if connection_mode.map(str::trim) == Some("protocol_router")
            || claude_api_format == "open_ai_chat"
            || claude_api_format == "open_ai_responses"
        {
            return normalized;
        }
    }

    if claude_api_format == "open_ai_responses" {
        "open_ai_responses".to_string()
    } else {
        "open_ai_chat".to_string()
    }
}

pub(in crate::app_store) fn normalize_service_provider_record(record: &mut ServiceProviderRecord) {
    if record
        .icon
        .as_ref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        record.icon = record
            .tool_config
            .get("icon")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
    }

    let tool_cfg_connection_mode = record
        .tool_config
        .get("claude_connection_mode")
        .and_then(|v| v.as_str());
    let tool_cfg_wire_api = record.tool_config.get("wire_api").and_then(|v| v.as_str());

    let current_connection_mode = Some(record.claude_connection_mode.as_str());
    let current_wire_api = Some(record.protocol_router_wire_api.as_str());
    let inferred_api_format = infer_claude_api_format(
        Some(record.claude_api_format.as_str()),
        current_connection_mode.or(tool_cfg_connection_mode),
        current_wire_api.or(tool_cfg_wire_api),
    );
    let inferred_connection_mode = infer_claude_connection_mode(
        current_connection_mode.or(tool_cfg_connection_mode),
        &inferred_api_format,
    );
    let inferred_wire_api = infer_protocol_router_wire_api(
        current_wire_api.or(tool_cfg_wire_api),
        &inferred_api_format,
        Some(inferred_connection_mode.as_str()),
    );

    record.claude_api_format = inferred_api_format;
    record.claude_connection_mode = inferred_connection_mode;
    record.protocol_router_wire_api = inferred_wire_api;
    sync_claude_default_model_fields(record);
}

pub(in crate::app_store) fn default_claude_auth_env_key() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

pub(in crate::app_store) fn is_default_auth_env_key(s: &str) -> bool {
    s == "ANTHROPIC_API_KEY"
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionInput {
    pub id: Option<String>,
    pub name: String,
    pub working_dir: String,
    pub tool: String,
    #[serde(default)]
    pub tool_session_id: Option<String>,
    #[serde(default)]
    pub runtime_mode: Option<String>,
    #[serde(default)]
    pub runtime_profile_id: Option<String>,
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LauncherItemInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub item_type: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub pin_order: Option<u32>,
    #[serde(default)]
    pub launch_count: Option<u64>,
    #[serde(default)]
    pub last_launched_at: Option<u64>,
    #[serde(default)]
    pub trusted: Option<bool>,
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<u64>,
}
