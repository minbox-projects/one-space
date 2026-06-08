use crate::{
    ai_env, ai_news, ai_sessions, config, git, mcp_servers, messages, secrets, storage, workspaces,
};
#[cfg(target_os = "macos")]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

const SCHEMA_VERSION: u32 = 1;
const OUTBOX_DEDUP_WINDOW_SECS: u64 = 3;
const MANAGED_TOOLS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];
const HISTORY_SYNC_TOOLS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];
const HISTORY_SYNC_BASE_PARSER_VERSION: u32 = 1;
const CODEX_HISTORY_TITLE_PARSER_VERSION: u32 = 2;
const OPENCODE_HISTORY_PROJECT_FALLBACK_VERSION: u32 = 2;
const HISTORY_BIND_WINDOW_SECS: u64 = 15 * 60;
const LAUNCHER_EXPORT_VERSION: u32 = 1;
const PROVIDERS_EXPORT_VERSION: u32 = 1;
const LAUNCHER_TYPES: [&str; 5] = ["app", "script", "url", "folder", "internal"];
static SESSION_CREATE_LOCKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static SESSIONS_HISTORY_SYNC_RUNNING: AtomicBool = AtomicBool::new(false);
static SESSIONS_STATE_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiMeta {
    pub schema_version: u32,
    pub revision: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiOk<T> {
    pub ok: bool,
    pub data: T,
    pub meta: ApiMeta,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiErr {
    pub ok: bool,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DashboardCounts {
    pub launcher: usize,
    pub workspaces: usize,
    pub sessions: usize,
    pub ssh: usize,
    pub snippets: usize,
    pub bookmarks: usize,
    pub notes: usize,
    pub ai_news: usize,
    pub environments: usize,
    pub skills: usize,
    pub subagents: usize,
    pub mcp_servers: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_type: Option<String>,
}

fn api_ok<T: Serialize>(data: T, meta: ApiMeta) -> Result<ApiOk<T>, ApiErr> {
    Ok(ApiOk {
        ok: true,
        data,
        meta,
    })
}

fn api_error(code: &str, message: impl Into<String>) -> ApiErr {
    ApiErr {
        ok: false,
        code: code.to_string(),
        message: message.into(),
        details: None,
    }
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn default_session_name_source() -> String {
    "manual".to_string()
}

fn required_history_parser_version(tool: &str) -> u32 {
    if tool.eq_ignore_ascii_case("codex") {
        CODEX_HISTORY_TITLE_PARSER_VERSION
    } else if tool.eq_ignore_ascii_case("opencode") {
        OPENCODE_HISTORY_PROJECT_FALLBACK_VERSION
    } else {
        HISTORY_SYNC_BASE_PARSER_VERSION
    }
}

fn normalize_session_name_source(input: &str) -> String {
    let value = input.trim().to_lowercase();
    if value == "history" {
        "history".to_string()
    } else {
        "manual".to_string()
    }
}

fn sessions_history_days() -> u64 {
    crate::config::get_storage_config()
        .ok()
        .and_then(|cfg| cfg.ai_sessions_history_days)
        .unwrap_or(30)
}

fn session_history_cutoff_ts() -> u64 {
    let history_days = sessions_history_days();
    let now = now_ts();
    now.saturating_sub(history_days * 24 * 60 * 60)
}

fn filter_sessions_by_history_window<'a>(
    sessions: impl Iterator<Item = &'a SessionRecord>,
) -> Vec<SessionRecord> {
    let cutoff_ts = session_history_cutoff_ts();
    let mut filtered = sessions
        .filter(|session| {
            // Favorited sessions are always kept regardless of history window.
            if session.favorited_at.is_some() {
                true
            } else {
                session.last_used_at >= cutoff_ts || session.created_at >= cutoff_ts
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    sort_sessions_for_display(&mut filtered);
    filtered
}

/// Sort sessions for display: favorited first (by favorited_at desc),
/// then non-favorited by last_used_at/created_at desc, with name/id tiebreak.
fn sort_sessions_for_display(sessions: &mut Vec<SessionRecord>) {
    // Pre-compute lowercase names to avoid repeated allocations in comparator.
    let mut keyed: Vec<_> = sessions
        .drain(..)
        .map(|s| {
            let lower = s.name.to_lowercase();
            (
                s.favorited_at.is_some(),
                s.favorited_at,
                s.last_used_at,
                s.created_at,
                lower,
                s.id.clone(),
                s,
            )
        })
        .collect();

    keyed.sort_by(|a, b| {
        let (a_fav, _, a_used, a_created, a_lower, a_id, _) = a;
        let (b_fav, _, b_used, b_created, b_lower, b_id, _) = b;

        match (a_fav, b_fav) {
            (true, true) => {
                b.1.cmp(&a.1)
                    .then_with(|| b_used.cmp(a_used))
                    .then_with(|| b_created.cmp(a_created))
                    .then_with(|| a_lower.cmp(b_lower))
                    .then_with(|| a_id.cmp(b_id))
            }
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => b_used
                .cmp(a_used)
                .then_with(|| b_created.cmp(a_created))
                .then_with(|| a_lower.cmp(b_lower))
                .then_with(|| a_id.cmp(b_id)),
        }
    });

    sessions.extend(keyed.into_iter().map(|t| t.6));
}

fn session_create_locks() -> &'static Mutex<HashSet<String>> {
    SESSION_CREATE_LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn sessions_state_write_lock() -> &'static Mutex<()> {
    SESSIONS_STATE_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_sessions_state_write() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    sessions_state_write_lock()
        .lock()
        .map_err(|_| "sessions state write lock poisoned".to_string())
}

fn acquire_session_create_lock(key: String) -> Result<Option<String>, String> {
    let mut locks = session_create_locks()
        .lock()
        .map_err(|_| "session create lock poisoned".to_string())?;
    if locks.contains(&key) {
        return Ok(None);
    }
    locks.insert(key.clone());
    Ok(Some(key))
}

fn release_session_create_lock(key: &str) {
    if let Ok(mut locks) = session_create_locks().lock() {
        locks.remove(key);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SchemaMeta {
    pub schema_version: u32,
    pub created_at: u64,
    pub last_migrated_at: u64,
    pub revision: u64,
}

impl Default for SchemaMeta {
    fn default() -> Self {
        let now = now_ts();
        Self {
            schema_version: SCHEMA_VERSION,
            created_at: now,
            last_migrated_at: now,
            revision: 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderCore {
    pub id: String,
    pub name: String,
    pub tool: String,
    pub api_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderRuntimePolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderHistoryEntry {
    pub ts: u64,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderRecord {
    pub core: ProviderCore,
    pub runtime_policy: ProviderRuntimePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite_at: Option<u64>,
    #[serde(default)]
    pub tool_config: Map<String, Value>,
    #[serde(default)]
    pub history: Vec<ProviderHistoryEntry>,
    #[serde(default)]
    pub extra: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProvidersState {
    #[serde(default)]
    pub active: HashMap<String, String>,
    #[serde(default)]
    pub providers: Vec<ProviderRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub name: String,
    pub working_dir: String,
    pub tool: String,
    pub tool_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(default = "default_session_name_source")]
    pub name_source: String,
    #[serde(default)]
    pub runtime_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    pub created_at: u64,
    pub last_used_at: u64,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorited_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SessionsHistoryToolState {
    #[serde(default)]
    pub full_backfill_done: bool,
    #[serde(default)]
    pub parser_version: u32,
    #[serde(default)]
    pub last_seen_updated_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_completed_at: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SessionsHistorySyncState {
    #[serde(default)]
    pub tools: HashMap<String, SessionsHistoryToolState>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SessionsState {
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
    #[serde(default)]
    pub history_sync: SessionsHistorySyncState,
    #[serde(default)]
    pub tombstones: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliSessionLookup {
    pub id: String,
    pub tool: String,
    pub tool_session_id: String,
    pub working_dir: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LauncherRecord {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub target: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub pin_order: u32,
    #[serde(default)]
    pub launch_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_launched_at: Option<u64>,
    #[serde(default)]
    pub trusted: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LauncherState {
    #[serde(default)]
    pub items: Vec<LauncherRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EncryptedBlob {
    #[serde(default)]
    pub is_encrypted: bool,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OutboxEvent {
    pub id: String,
    pub domain: String,
    pub reason: String,
    pub created_at: u64,
    pub attempts: u32,
    pub next_retry_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OutboxState {
    #[serde(default)]
    pub events: Vec<OutboxEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<u64>,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub last_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl Default for OutboxState {
    fn default() -> Self {
        Self {
            events: vec![],
            last_run_at: None,
            running: false,
            last_status: "idle".to_string(),
            last_error: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSnapshot {
    pub providers: Value,
    pub sessions: Value,
    pub config: Value,
    pub schema: SchemaMeta,
    pub outbox: OutboxState,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MigrationState {
    pub migrated: bool,
    pub schema_version: u32,
    pub last_migrated_at: Option<u64>,
    pub last_backup_id: Option<String>,
    pub in_progress: bool,
    pub last_error: Option<String>,
}

impl Default for MigrationState {
    fn default() -> Self {
        Self {
            migrated: false,
            schema_version: 0,
            last_migrated_at: None,
            last_backup_id: None,
            in_progress: false,
            last_error: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MigrationReport {
    pub started_at: u64,
    pub finished_at: u64,
    pub success: bool,
    pub backup_id: String,
    pub steps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderInput {
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
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub provider_key: Option<String>,
    #[serde(default)]
    pub favorite_at: Option<u64>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

/// A single model mapping row for Claude service providers.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudeModelMapping {
    /// Model family: haiku, sonnet, opus (read-only in UI)
    pub family: String,
    /// Display name shown in the UI
    #[serde(default)]
    pub display_name: String,
    /// Upstream model identifier sent to the API
    #[serde(default)]
    pub upstream_model: String,
    /// Whether to append [1m] suffix for 1M context support
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_1m: Option<bool>,
    /// Optional Claude Code supported capabilities metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_capabilities: Option<Vec<String>>,
}

pub(crate) fn split_claude_1m_suffix(raw: &str) -> (String, bool) {
    let trimmed = raw.trim();
    if let Some(base) = trimmed.strip_suffix("[1m]") {
        (base.to_string(), true)
    } else {
        (trimmed.to_string(), false)
    }
}

pub(crate) fn claude_model_env_keys_for_family(
    family: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    match family {
        "haiku" => Some((
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_SUPPORTED_CAPABILITIES",
        )),
        "sonnet" => Some((
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES",
        )),
        "opus" => Some((
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_SUPPORTED_CAPABILITIES",
        )),
        _ => None,
    }
}

pub(crate) fn parse_supported_capabilities_csv(raw: &str) -> Option<Vec<String>> {
    let values = raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

pub(crate) fn join_supported_capabilities_csv(values: &[String]) -> Option<String> {
    let values = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values.join(","))
    }
}

pub(crate) fn default_claude_model_mappings_from_tool_config(
    tool_config: &Map<String, Value>,
) -> Vec<ClaudeModelMapping> {
    [
        ("haiku", "Haiku", "claude_haiku_model"),
        ("sonnet", "Sonnet", "claude_sonnet_model"),
        ("opus", "Opus", "claude_opus_model"),
    ]
    .into_iter()
    .map(|(family, display_name, key)| {
        let raw_model = tool_config
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let (upstream_model, supports_1m) = split_claude_1m_suffix(&raw_model);
        ClaudeModelMapping {
            family: family.to_string(),
            display_name: display_name.to_string(),
            upstream_model,
            supports_1m: Some(supports_1m && family != "haiku"),
            supported_capabilities: None,
        }
    })
    .collect()
}

fn strip_legacy_claude_model_keys(tool_config: &mut Map<String, Value>) {
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

fn default_claude_api_format() -> String {
    "anthropic_messages".to_string()
}

fn is_default_claude_api_format(s: &str) -> bool {
    s == "anthropic_messages"
}

fn default_claude_connection_mode() -> String {
    "native_anthropic".to_string()
}

fn is_default_claude_connection_mode(s: &str) -> bool {
    s == "native_anthropic"
}

fn default_protocol_router_wire_api() -> String {
    "open_ai_chat".to_string()
}

fn is_default_protocol_router_wire_api(s: &str) -> bool {
    s == "open_ai_chat"
}

pub(crate) fn normalize_protocol_router_wire_api(raw: &str) -> String {
    match raw {
        "open_ai_responses" | "responses" => "open_ai_responses".to_string(),
        _ => "open_ai_chat".to_string(),
    }
}

fn normalize_claude_api_format(raw: &str) -> Option<String> {
    match raw.trim() {
        "anthropic_messages" | "anthropic" => Some("anthropic_messages".to_string()),
        "open_ai_chat" | "chat" => Some("open_ai_chat".to_string()),
        "open_ai_responses" | "responses" => Some("open_ai_responses".to_string()),
        _ => None,
    }
}

fn infer_claude_connection_mode(explicit: Option<&str>, claude_api_format: &str) -> String {
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

fn infer_claude_api_format(
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

fn infer_protocol_router_wire_api(
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

fn normalize_service_provider_record(record: &mut ServiceProviderRecord) {
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

fn default_claude_auth_env_key() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

fn is_default_auth_env_key(s: &str) -> bool {
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

struct StorageEngine;

impl StorageEngine {
    fn base_dir() -> Result<PathBuf, String> {
        let root = crate::get_data_dir()?;
        let target = root.join("data");
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        Ok(target)
    }

    fn meta_dir() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("meta");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p)
    }

    fn service_providers_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("service_providers");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    fn providers_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("providers");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    fn sessions_path() -> Result<PathBuf, String> {
        // AI sessions are always stored in local app data to keep history
        // independent from user-selected storage backends (git/iCloud/custom path).
        let p = config::get_app_dir()?
            .join("data")
            .join("data")
            .join("sessions");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    fn sessions_path_in_selected_storage() -> Result<PathBuf, String> {
        let root = crate::get_data_dir()?;
        Ok(root.join("data").join("sessions").join("state.json"))
    }

    fn launcher_path() -> Result<PathBuf, String> {
        // Launcher items are always stored in local app data so they do not
        // depend on user-selected storage backends (git/iCloud/custom path).
        let p = config::get_app_dir()?
            .join("data")
            .join("data")
            .join("launcher");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    fn launcher_path_in_selected_storage() -> Result<PathBuf, String> {
        let root = crate::get_data_dir()?;
        Ok(root.join("data").join("launcher").join("state.json"))
    }

    fn secrets_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("secrets");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.enc.json"))
    }

    fn mcp_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("mcp");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    fn content_path(name: &str) -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("content");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join(format!("{}.enc.json", name)))
    }

    fn outbox_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("events");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("outbox.json"))
    }

    fn schema_path() -> Result<PathBuf, String> {
        Ok(Self::meta_dir()?.join("schema.json"))
    }

    fn migration_state_path() -> Result<PathBuf, String> {
        Ok(Self::meta_dir()?.join("migration_state.json"))
    }

    fn migration_report_path() -> Result<PathBuf, String> {
        Ok(Self::meta_dir()?.join("migration_report.json"))
    }

    fn backup_root() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("backups");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p)
    }

    fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = path.with_extension("tmp");
        let mut file = File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temp, path).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T, String> {
        if !path.exists() {
            return Ok(T::default());
        }
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        if content.trim().is_empty() {
            return Ok(T::default());
        }
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
        let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        Self::atomic_write(path, &content)
    }

    fn load_schema() -> Result<SchemaMeta, String> {
        let path = Self::schema_path()?;
        if !path.exists() {
            let schema = SchemaMeta::default();
            Self::write_json(&path, &schema)?;
            return Ok(schema);
        }
        Self::read_json(&path)
    }

    fn bump_revision() -> Result<SchemaMeta, String> {
        let mut schema = Self::load_schema()?;
        schema.revision = schema.revision.saturating_add(1);
        schema.last_migrated_at = now_ts();
        Self::write_json(&Self::schema_path()?, &schema)?;
        Ok(schema)
    }
}

fn migrate_sessions_to_local_if_needed(local_path: &Path) -> Result<(), String> {
    let legacy_path = StorageEngine::sessions_path_in_selected_storage()?;
    if legacy_path == local_path || !legacy_path.exists() || local_path.exists() {
        return Ok(());
    }

    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&legacy_path, local_path).map_err(|e| e.to_string())?;
    Ok(())
}

fn migrate_launcher_to_local_if_needed(local_path: &Path) -> Result<(), String> {
    let legacy_path = StorageEngine::launcher_path_in_selected_storage()?;
    if legacy_path == local_path || !legacy_path.exists() || local_path.exists() {
        return Ok(());
    }

    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&legacy_path, local_path).map_err(|e| e.to_string())?;
    Ok(())
}

struct CryptoService;

impl CryptoService {
    fn encrypt(value: &str) -> Result<String, String> {
        let password = crate::crypto::get_or_init_master_password()?;
        crate::crypto::encrypt(value, &password)
    }

    fn decrypt(value: &str) -> Result<String, String> {
        let password = crate::crypto::get_or_init_master_password()?;
        crate::crypto::decrypt(value, &password)
    }

    fn encrypt_json(value: &Value) -> Result<EncryptedBlob, String> {
        Ok(EncryptedBlob {
            is_encrypted: true,
            data: Self::encrypt(&value.to_string())?,
        })
    }

    fn decrypt_json(blob: &EncryptedBlob) -> Result<Value, String> {
        if !blob.is_encrypted {
            return serde_json::from_str(&blob.data).map_err(|e| e.to_string());
        }
        let plain = Self::decrypt(&blob.data)?;
        serde_json::from_str(&plain).map_err(|e| e.to_string())
    }
}

fn normalize_runtime_mode(input: Option<&str>) -> String {
    let value = input.unwrap_or("").trim().to_lowercase();
    if value == "strict" {
        "strict".to_string()
    } else {
        "shared".to_string()
    }
}

fn session_install_scope_and_root(session: &SessionRecord) -> (String, Option<String>) {
    if normalize_runtime_mode(Some(&session.runtime_mode)) != "strict" {
        return ("global".to_string(), None);
    }
    let raw = session.working_dir.trim();
    if raw.is_empty() {
        return ("project".to_string(), None);
    }
    let root = fs::canonicalize(PathBuf::from(raw))
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| Some(raw.to_string()));
    ("project".to_string(), root)
}

fn normalize_sessions_state(state: &mut SessionsState) -> bool {
    let mut changed = false;
    for session in &mut state.sessions {
        let normalized_name_source = normalize_session_name_source(&session.name_source);
        if session.name_source != normalized_name_source {
            session.name_source = normalized_name_source;
            changed = true;
        }
        let normalized_model_name = session
            .model_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if session.model_name != normalized_model_name {
            session.model_name = normalized_model_name;
            changed = true;
        }
        if session.runtime_mode.trim().is_empty() {
            session.runtime_mode = "shared".to_string();
            changed = true;
        }
        if normalize_runtime_mode(Some(&session.runtime_mode)) != session.runtime_mode {
            session.runtime_mode = normalize_runtime_mode(Some(&session.runtime_mode));
            changed = true;
        }
        if session.runtime_mode == "shared" && session.runtime_profile_id.is_some() {
            session.runtime_profile_id = None;
            changed = true;
        }
    }

    let mut normalized_tombstones = BTreeSet::new();
    for tombstone in state.tombstones.iter() {
        let trimmed = tombstone.trim();
        if trimmed.is_empty() {
            changed = true;
            continue;
        }
        normalized_tombstones.insert(trimmed.to_string());
    }
    if normalized_tombstones != state.tombstones {
        state.tombstones = normalized_tombstones;
        changed = true;
    }

    for tool in HISTORY_SYNC_TOOLS {
        let entry = state
            .history_sync
            .tools
            .entry(tool.to_string())
            .or_insert_with(SessionsHistoryToolState::default);
        if entry.parser_version == 0 && entry.full_backfill_done {
            entry.parser_version = HISTORY_SYNC_BASE_PARSER_VERSION;
            changed = true;
        }
    }
    changed
}

fn service_provider_to_legacy(sp: &ServiceProviderRecord) -> Value {
    let mut map = service_provider_to_value(sp)
        .as_object()
        .cloned()
        .unwrap_or_default();
    for (k, v) in &sp.tool_config {
        map.entry(k.clone()).or_insert_with(|| v.clone());
    }
    for (k, v) in &sp.extra {
        map.entry(k.clone()).or_insert_with(|| v.clone());
    }
    if !sp.history.is_empty() {
        let arr: Vec<Value> = sp
            .history
            .iter()
            .map(|h| {
                json!({
                    "timestamp": h.ts.saturating_mul(1000),
                    "content": h.summary.clone().unwrap_or_default()
                })
            })
            .collect();
        map.insert("history".to_string(), Value::Array(arr));
    }
    Value::Object(map)
}

fn service_providers_to_legacy_view(state: &ServiceProvidersState) -> LegacyProvidersView {
    LegacyProvidersView {
        active_claude: state.active.get("claude").cloned(),
        active_codex: state.active.get("codex").cloned(),
        active_gemini: state.active.get("gemini").cloned(),
        active_opencode: state.active.get("opencode").cloned(),
        providers: state
            .providers
            .iter()
            .map(service_provider_to_legacy)
            .collect(),
    }
}

fn service_providers_to_provider_state(state: &ServiceProvidersState) -> ProvidersState {
    ProvidersState {
        active: state.active.clone(),
        providers: state
            .providers
            .iter()
            .map(service_provider_to_provider_record)
            .collect(),
    }
}

fn service_provider_to_provider_record(sp: &ServiceProviderRecord) -> ProviderRecord {
    let mut tool_config = sp.tool_config.clone();
    if let Some(v) = &sp.icon {
        tool_config.insert("icon".to_string(), Value::String(v.clone()));
    }
    tool_config.insert(
        "claude_api_format".to_string(),
        Value::String(sp.claude_api_format.clone()),
    );
    tool_config.insert(
        "claude_connection_mode".to_string(),
        Value::String(sp.claude_connection_mode.clone()),
    );
    tool_config.insert(
        "claude_auth_env_key".to_string(),
        Value::String(sp.claude_auth_env_key.clone()),
    );
    if !sp.claude_model_mappings.is_empty() {
        if let Ok(value) = serde_json::to_value(&sp.claude_model_mappings) {
            tool_config.insert("claude_model_mappings".to_string(), value);
        }
    }
    strip_legacy_claude_model_keys(&mut tool_config);
    if let Some(v) = sp.claude_enable_tool_search {
        tool_config.insert("claude_enable_tool_search".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.claude_auto_memory_enabled {
        tool_config.insert("claude_auto_memory_enabled".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.claude_always_thinking_enabled {
        tool_config.insert("claude_always_thinking_enabled".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.claude_away_summary_enabled {
        tool_config.insert("claude_away_summary_enabled".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.claude_include_git_instructions {
        tool_config.insert(
            "claude_include_git_instructions".to_string(),
            Value::Bool(v),
        );
    }
    if let Some(v) = sp.claude_enable_attribution {
        tool_config.insert("claude_enable_attribution".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.env_managed {
        tool_config.insert("env_managed".to_string(), Value::Bool(v));
    }

    ProviderRecord {
        core: ProviderCore {
            id: sp.id.clone(),
            name: sp.name.clone(),
            tool: sp.tool.clone(),
            api_key: sp.api_key.clone(),
            code: sp.code.clone(),
            base_url: sp.base_url.clone(),
            model: sp.model.clone(),
        },
        runtime_policy: ProviderRuntimePolicy {
            approval_policy: tool_config
                .get("approval_policy")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            sandbox_mode: tool_config
                .get("sandbox_mode")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        },
        favorite_at: sp.favorite_at,
        tool_config,
        history: sp.history.clone(),
        extra: sp.extra.clone(),
        is_enabled: sp.is_enabled,
        provider_key: sp.provider_key.clone(),
    }
}

fn provider_import_key(tool: &str, provider_id: &str) -> String {
    format!("{}::{}", tool.trim().to_lowercase(), provider_id.trim())
}

fn normalize_provider_name(name: &str) -> String {
    name.trim().to_lowercase()
}

fn provider_input_from_value(value: &Value) -> Result<ProviderInput, String> {
    let obj = value
        .as_object()
        .cloned()
        .ok_or_else(|| "provider must be object".to_string())?;

    let mut input = ProviderInput {
        id: obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        name: obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        tool: obj
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_lowercase(),
        api_key: obj
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        base_url: obj
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        model: obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        is_enabled: obj.get("is_enabled").and_then(|v| v.as_bool()),
        provider_key: obj
            .get("provider_key")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        favorite_at: obj.get("favorite_at").and_then(|v| v.as_u64()),
        code: obj
            .get("code")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_lowercase().to_string())
            .filter(|s| !s.is_empty()),
        fields: extract_fields(&Value::Object(obj.clone())),
    };

    if input.id.is_empty() {
        return Err("provider id required".to_string());
    }
    if input.name.is_empty() {
        return Err("provider name required".to_string());
    }
    if input.tool.is_empty() {
        return Err("provider tool required".to_string());
    }
    if !MANAGED_TOOLS.contains(&input.tool.as_str()) {
        return Err(format!("unsupported provider tool: {}", input.tool));
    }

    input.fields.remove("history");
    Ok(input)
}

fn parse_providers_import_payload(
    import_path: &str,
) -> Result<(HashMap<String, String>, Vec<Value>), String> {
    let raw = fs::read_to_string(import_path).map_err(|e| e.to_string())?;
    let parsed: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let providers = parsed
        .get("providers")
        .and_then(|v| v.as_array().cloned())
        .or_else(|| parsed.as_array().cloned())
        .ok_or_else(|| "import payload must contain providers array".to_string())?;

    let active = parsed
        .as_object()
        .map(extract_active_map_from_snapshot)
        .unwrap_or_default();

    Ok((active, providers))
}

fn find_provider_import_conflict(
    state: &ProvidersState,
    input: &ProviderInput,
) -> Option<ProviderImportConflictMatch> {
    if let Some(existing) = state.providers.iter().find(|p| p.core.id == input.id) {
        return Some(ProviderImportConflictMatch {
            existing_id: existing.core.id.clone(),
            existing_name: existing.core.name.clone(),
            reason: "id".to_string(),
        });
    }

    let normalized_name = normalize_provider_name(&input.name);
    if normalized_name.is_empty() {
        return None;
    }

    state
        .providers
        .iter()
        .find(|p| {
            p.core.tool == input.tool && normalize_provider_name(&p.core.name) == normalized_name
        })
        .map(|existing| ProviderImportConflictMatch {
            existing_id: existing.core.id.clone(),
            existing_name: existing.core.name.clone(),
            reason: "name".to_string(),
        })
}

fn collect_provider_import_candidates(
    state: &ProvidersState,
    provider_values: &[Value],
) -> Result<Vec<ProviderImportCandidate>, String> {
    let mut seen_import_keys: HashSet<String> = HashSet::new();
    let mut candidates = Vec::new();

    for (idx, value) in provider_values.iter().enumerate() {
        let input = provider_input_from_value(value)
            .map_err(|e| format!("provider #{}: {}", idx.saturating_add(1), e))?;
        let import_key = provider_import_key(&input.tool, &input.id);
        if !seen_import_keys.insert(import_key.clone()) {
            return Err(format!(
                "duplicate provider in import file: {} ({})",
                input.name, import_key
            ));
        }
        let conflict = find_provider_import_conflict(state, &input);
        candidates.push(ProviderImportCandidate {
            import_key,
            input,
            conflict,
        });
    }

    Ok(candidates)
}

fn make_imported_provider_id(state: &ProvidersState, preferred: &str) -> String {
    let base = preferred.trim();
    let base = if base.is_empty() {
        "imported-provider"
    } else {
        base
    };
    let sanitized = base
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let prefix = if sanitized.is_empty() {
        "imported-provider".to_string()
    } else {
        sanitized
    };

    loop {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let candidate = format!("{}-{}", prefix, &suffix[..8]);
        if !state.providers.iter().any(|p| p.core.id == candidate) {
            return candidate;
        }
    }
}

fn expand_home_dir_path(path: &str) -> Result<PathBuf, String> {
    if path == "~" {
        return dirs::home_dir().ok_or_else(|| "home directory not found".to_string());
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| "home directory not found".to_string())?;
        return Ok(home.join(stripped));
    }
    Ok(PathBuf::from(path))
}

fn providers_import_preview_from_candidates(
    active: HashMap<String, String>,
    candidates: &[ProviderImportCandidate],
) -> ProvidersImportPreview {
    let items = candidates
        .iter()
        .map(|candidate| ProviderImportPreviewItem {
            import_key: candidate.import_key.clone(),
            id: candidate.input.id.clone(),
            name: candidate.input.name.clone(),
            tool: candidate.input.tool.clone(),
            model: candidate.input.model.clone(),
            conflict: candidate.conflict.is_some(),
            conflict_reason: candidate.conflict.as_ref().map(|item| item.reason.clone()),
            existing_id: candidate
                .conflict
                .as_ref()
                .map(|item| item.existing_id.clone()),
            existing_name: candidate
                .conflict
                .as_ref()
                .map(|item| item.existing_name.clone()),
        })
        .collect::<Vec<_>>();
    let conflicts = items.iter().filter(|item| item.conflict).count();

    ProvidersImportPreview {
        active,
        total: items.len(),
        conflicts,
        items,
    }
}

fn normalize_device_label(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn provider_snapshot_candidates(device_dir: &Path) -> Vec<PathBuf> {
    vec![
        device_dir.join("providers.json"),
        device_dir.join("ai_providers.json"),
        device_dir.join("data").join("providers.json"),
        device_dir.join("data").join("ai_providers.json"),
        device_dir
            .join("shared")
            .join("profile")
            .join("providers.json"),
        device_dir.join("profile").join("providers.json"),
    ]
}

fn read_provider_snapshot_value(path: &Path) -> Option<Value> {
    let content = fs::read_to_string(path).ok()?;
    if content.trim().is_empty() {
        return None;
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if blob.is_encrypted {
            if let Ok(value) = CryptoService::decrypt_json(&blob) {
                return Some(value);
            }
            return None;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&blob.data) {
            return Some(value);
        }
    }

    serde_json::from_str::<Value>(&content).ok()
}

fn extract_active_map_from_snapshot(root: &Map<String, Value>) -> HashMap<String, String> {
    const TOOLS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];
    let mut active = HashMap::new();

    if let Some(active_obj) = root.get("active").and_then(|v| v.as_object()) {
        for tool in TOOLS {
            if let Some(provider_id) = active_obj.get(tool).and_then(|v| v.as_str()) {
                if !provider_id.trim().is_empty() {
                    active.insert(tool.to_string(), provider_id.to_string());
                }
            }
        }
    }

    for tool in TOOLS {
        let key = format!("active_{}", tool);
        if let Some(provider_id) = root.get(&key).and_then(|v| v.as_str()) {
            if !provider_id.trim().is_empty() {
                active.insert(tool.to_string(), provider_id.to_string());
            }
        }
    }

    active
}

fn extract_providers_from_snapshot(root: &Map<String, Value>) -> Vec<SyncedDeviceProviderLite> {
    let mut providers = Vec::new();
    let Some(items) = root.get("providers").and_then(|v| v.as_array()) else {
        return providers;
    };

    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let core = obj.get("core").and_then(|v| v.as_object());
        let field = |name: &str| -> Option<String> {
            core.and_then(|c| c.get(name).and_then(|v| v.as_str()))
                .or_else(|| obj.get(name).and_then(|v| v.as_str()))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        let Some(id) = field("id") else { continue };
        let Some(name) = field("name") else { continue };
        let Some(tool) = field("tool") else { continue };
        if !matches!(tool.as_str(), "claude" | "codex" | "gemini" | "opencode") {
            continue;
        }
        let mut api_key = field("api_key").unwrap_or_default();
        if is_placeholder_string(&api_key) {
            api_key.clear();
        }
        let base_url = field("base_url").filter(|v| !is_placeholder_string(v));
        let model = field("model");
        let provider_key = field("provider_key");
        let is_enabled = core
            .and_then(|c| c.get("is_enabled").and_then(|v| v.as_bool()))
            .or_else(|| obj.get("is_enabled").and_then(|v| v.as_bool()));

        providers.push(SyncedDeviceProviderLite {
            id,
            name,
            tool,
            api_key,
            base_url,
            model,
            provider_key,
            is_enabled,
        });
    }

    providers.sort_by(|a, b| {
        a.tool
            .cmp(&b.tool)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    providers
}

fn provider_snapshot_quality_score(
    providers: &[SyncedDeviceProviderLite],
    active: &HashMap<String, String>,
) -> usize {
    let with_key = providers
        .iter()
        .filter(|p| !p.api_key.trim().is_empty())
        .count();
    let with_model = providers.iter().filter(|p| p.model.is_some()).count();
    let with_base_url = providers.iter().filter(|p| p.base_url.is_some()).count();
    let with_provider_key = providers
        .iter()
        .filter(|p| p.provider_key.is_some())
        .count();

    // Prefer snapshots that include decrypted api_key first, then richer metadata.
    with_key.saturating_mul(10000)
        + with_model.saturating_mul(500)
        + with_base_url.saturating_mul(100)
        + with_provider_key.saturating_mul(20)
        + active.len().saturating_mul(5)
        + providers.len()
}

fn provider_from_input(input: ProviderInput, old: Option<&ProviderRecord>) -> ProviderRecord {
    let mut tool_config = old.map(|o| o.tool_config.clone()).unwrap_or_default();
    let mut extra = old.map(|o| o.extra.clone()).unwrap_or_default();

    for (k, v) in input.fields {
        tool_config.insert(k, v);
    }

    if let Some(o) = old {
        for (k, v) in &o.extra {
            extra.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    let mut history = old.map(|o| o.history.clone()).unwrap_or_default();
    history.insert(
        0,
        ProviderHistoryEntry {
            ts: now_ts(),
            action: if old.is_some() {
                "upsert".to_string()
            } else {
                "create".to_string()
            },
            summary: Some(format!("provider:{} tool:{}", input.id, input.tool)),
        },
    );
    history.truncate(50);

    ProviderRecord {
        core: ProviderCore {
            id: input.id,
            name: input.name,
            tool: input.tool,
            api_key: input.api_key,
            code: input.code,
            base_url: input.base_url,
            model: input.model,
        },
        runtime_policy: ProviderRuntimePolicy {
            approval_policy: tool_config
                .get("approval_policy")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            sandbox_mode: tool_config
                .get("sandbox_mode")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        },
        favorite_at: input
            .favorite_at
            .or_else(|| old.and_then(|o| o.favorite_at)),
        tool_config,
        history,
        extra,
        is_enabled: input.is_enabled,
        provider_key: input.provider_key,
    }
}

pub(crate) fn load_providers_state() -> Result<ProvidersState, String> {
    let path = StorageEngine::providers_path()?;
    if !path.exists() {
        return Ok(ProvidersState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(ProvidersState::default());
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            if let Ok(state) = serde_json::from_value::<ProvidersState>(value) {
                return Ok(state);
            }
        }
    }

    serde_json::from_str::<ProvidersState>(&content).map_err(|e| e.to_string())
}

pub(crate) fn save_providers_state(state: &ProvidersState) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::providers_path()?, &blob)?;
    let _ = write_legacy_cli_providers_snapshot(state);
    StorageEngine::bump_revision()
}

fn write_legacy_cli_providers_snapshot(state: &ProvidersState) -> Result<(), String> {
    let data_dir = crate::get_data_dir()?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let target = data_dir.join("providers.json");

    let providers: Vec<Value> = state
        .providers
        .iter()
        .map(|p| {
            let mut obj = json!({
                "id": p.core.id,
                "name": p.core.name,
                "tool": p.core.tool,
            });
            if let Some(ref code) = p.core.code {
                obj["code"] = json!(code);
            }
            obj
        })
        .collect();

    let payload = json!({
        "active_claude": state.active.get("claude").cloned().unwrap_or_default(),
        "active_codex": state.active.get("codex").cloned().unwrap_or_default(),
        "active_gemini": state.active.get("gemini").cloned().unwrap_or_default(),
        "active_opencode": state.active.get("opencode").cloned().unwrap_or_default(),
        "providers": providers,
    });

    let content = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    crate::atomic_write_string(&target, &content)
}

/// Migrate an old ProvidersState into a ServiceProvidersState.
pub(crate) fn migrate_providers_to_service_providers(old: ProvidersState) -> ServiceProvidersState {
    let providers: Vec<ServiceProviderRecord> = old
        .providers
        .into_iter()
        .map(|p| {
            let is_claude = p.core.tool == "claude";
            let legacy_api_format = p
                .tool_config
                .get("claude_api_format")
                .and_then(|v| v.as_str());
            let legacy_connection_mode = p
                .tool_config
                .get("claude_connection_mode")
                .and_then(|v| v.as_str());
            let legacy_wire_api = p
                .tool_config
                .get("protocol_router_wire_api")
                .and_then(|v| v.as_str())
                .or_else(|| p.tool_config.get("wire_api").and_then(|v| v.as_str()));
            let claude_model_mappings = if is_claude {
                let mappings = resolved_claude_model_mappings(&p.tool_config);
                if mappings.iter().any(|mapping| {
                    !mapping.upstream_model.trim().is_empty()
                        || mapping
                            .supported_capabilities
                            .as_ref()
                            .map(|values| !values.is_empty())
                            .unwrap_or(false)
                }) {
                    mappings
                } else {
                    vec![
                        ClaudeModelMapping {
                            family: "haiku".to_string(),
                            display_name: "Haiku".to_string(),
                            upstream_model: "claude-haiku-4-3-20250514".to_string(),
                            supports_1m: Some(false),
                            supported_capabilities: None,
                        },
                        ClaudeModelMapping {
                            family: "sonnet".to_string(),
                            display_name: "Sonnet".to_string(),
                            upstream_model: "claude-sonnet-4-20250514".to_string(),
                            supports_1m: Some(false),
                            supported_capabilities: None,
                        },
                        ClaudeModelMapping {
                            family: "opus".to_string(),
                            display_name: "Opus".to_string(),
                            upstream_model: "claude-opus-4-20250514".to_string(),
                            supports_1m: Some(false),
                            supported_capabilities: None,
                        },
                    ]
                }
            } else {
                vec![]
            };

            // Determine auth env key: if the old record used api_key (non-empty), keep ANTHROPIC_API_KEY
            let claude_auth_env_key = if is_claude && !p.core.api_key.is_empty() {
                "ANTHROPIC_API_KEY".to_string()
            } else {
                "ANTHROPIC_API_KEY".to_string()
            };

            let inferred_api_format =
                infer_claude_api_format(legacy_api_format, legacy_connection_mode, legacy_wire_api);
            let inferred_connection_mode =
                infer_claude_connection_mode(legacy_connection_mode, &inferred_api_format);
            let mut record = ServiceProviderRecord {
                id: p.core.id,
                name: p.core.name,
                tool: p.core.tool,
                icon: None,
                api_key: p.core.api_key,
                base_url: p.core.base_url,
                model: p.core.model,
                claude_api_format: inferred_api_format.clone(),
                claude_connection_mode: inferred_connection_mode.clone(),
                protocol_router_upstream_provider_id: None,
                protocol_router_wire_api: infer_protocol_router_wire_api(
                    legacy_wire_api,
                    &inferred_api_format,
                    Some(&inferred_connection_mode),
                ),
                claude_auth_env_key,
                claude_model_mappings,
                claude_enable_tool_search: None,
                claude_auto_memory_enabled: None,
                claude_always_thinking_enabled: None,
                claude_away_summary_enabled: None,
                claude_include_git_instructions: None,
                claude_enable_attribution: None,
                code: p.core.code,
                is_enabled: p.is_enabled,
                provider_key: p.provider_key,
                env_managed: None,
                favorite_at: p.favorite_at,
                tool_config: p.tool_config,
                history: p.history,
                extra: p.extra,
                fetched_models: None,
            };
            if !record.claude_model_mappings.is_empty() {
                record.tool_config.insert(
                    "claude_model_mappings".to_string(),
                    serde_json::to_value(&record.claude_model_mappings)
                        .unwrap_or_else(|_| Value::Array(vec![])),
                );
            }
            strip_legacy_claude_model_keys(&mut record.tool_config);
            normalize_service_provider_record(&mut record);
            record
        })
        .collect();

    ServiceProvidersState {
        active: old.active,
        providers,
    }
}

/// Load service providers state, auto-migrating from old providers.json if needed.
pub(crate) fn load_service_providers_state() -> Result<ServiceProvidersState, String> {
    let path = StorageEngine::service_providers_path()?;
    if path.exists() {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if content.trim().is_empty() {
            return Ok(ServiceProvidersState::default());
        }
        if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
            if let Ok(value) = CryptoService::decrypt_json(&blob) {
                if let Ok(mut state) = serde_json::from_value::<ServiceProvidersState>(value) {
                    for provider in state.providers.iter_mut() {
                        normalize_service_provider_record(provider);
                    }
                    return Ok(state);
                }
            }
        }
        let mut state =
            serde_json::from_str::<ServiceProvidersState>(&content).map_err(|e| e.to_string())?;
        for provider in state.providers.iter_mut() {
            normalize_service_provider_record(provider);
        }
        return Ok(state);
    }

    // Try to migrate from old providers.json
    let old_path = StorageEngine::providers_path()?;
    if old_path.exists() {
        let old = load_providers_state()?;
        let new = migrate_providers_to_service_providers(old);
        save_service_providers_internal(&new)?;
        return Ok(new);
    }

    Ok(ServiceProvidersState::default())
}

/// Save service providers state (internal, no side effects).
pub(crate) fn save_service_providers_internal(
    state: &ServiceProvidersState,
) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::service_providers_path()?, &blob)?;
    StorageEngine::bump_revision()
}

fn load_sessions_state() -> Result<SessionsState, String> {
    let path = StorageEngine::sessions_path()?;
    let _ = migrate_sessions_to_local_if_needed(&path);
    if !path.exists() {
        return Ok(SessionsState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(SessionsState::default());
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            if let Ok(mut state) = serde_json::from_value::<SessionsState>(value) {
                let _ = normalize_sessions_state(&mut state);
                return Ok(state);
            }
        }
    }

    let mut state = serde_json::from_str::<SessionsState>(&content).map_err(|e| e.to_string())?;
    let _ = normalize_sessions_state(&mut state);
    Ok(state)
}

fn save_sessions_state(state: &SessionsState) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::sessions_path()?, &blob)?;
    StorageEngine::bump_revision()
}

fn cli_session_lookup_from_record(session: &SessionRecord) -> CliSessionLookup {
    CliSessionLookup {
        id: session.id.trim().to_string(),
        tool: session.tool.trim().to_string(),
        tool_session_id: session.tool_session_id.trim().to_string(),
        working_dir: session.working_dir.trim().to_string(),
    }
}

fn find_cli_session_in_state(state: &SessionsState, query: &str) -> Option<CliSessionLookup> {
    let lookup = query.trim();
    if lookup.is_empty() {
        return None;
    }

    state
        .sessions
        .iter()
        .find(|session| session.tool_session_id.trim() == lookup)
        .or_else(|| {
            state
                .sessions
                .iter()
                .find(|session| session.id.trim() == lookup)
        })
        .map(cli_session_lookup_from_record)
}

pub(crate) fn cli_lookup_session(query: &str) -> Result<Option<CliSessionLookup>, String> {
    let state = load_sessions_state()?;
    Ok(find_cli_session_in_state(&state, query))
}

fn history_tombstone_key(tool: &str, tool_session_id: &str) -> Option<String> {
    let normalized_tool = tool.trim().to_lowercase();
    let normalized_session_id = tool_session_id.trim();
    if normalized_tool.is_empty() || normalized_session_id.is_empty() {
        return None;
    }
    Some(format!("{}::{}", normalized_tool, normalized_session_id))
}

fn history_sync_requires_full_backfill(
    tool: &str,
    tool_state: Option<&SessionsHistoryToolState>,
) -> bool {
    let required_parser_version = required_history_parser_version(tool);
    tool_state
        .map(|tool_state| {
            !tool_state.full_backfill_done || tool_state.parser_version < required_parser_version
        })
        .unwrap_or(true)
}

fn stable_history_session_record_id(tool: &str, tool_session_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tool.trim().to_lowercase().as_bytes());
    hasher.update(b":");
    hasher.update(tool_session_id.trim().as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!(
        "history-{}-{}",
        tool.trim().to_lowercase(),
        &digest[..16.min(digest.len())]
    )
}

fn history_entry_time_secs(entry: &ai_sessions::HistorySessionEntry) -> (u64, u64) {
    let created_at = if entry.created_at_ms > 0 {
        (entry.created_at_ms as u64) / 1000
    } else if entry.updated_at_ms > 0 {
        (entry.updated_at_ms as u64) / 1000
    } else {
        now_ts()
    };
    let updated_at = if entry.updated_at_ms > 0 {
        (entry.updated_at_ms as u64) / 1000
    } else {
        created_at
    };
    (created_at, updated_at)
}

fn normalize_session_working_dir(value: &str) -> String {
    ai_sessions::normalize_working_dir_for_terminal(value)
}

fn same_session_working_dir(left: &str, right: &str) -> bool {
    normalize_session_working_dir(left) == normalize_session_working_dir(right)
}

fn should_bind_history_entry_to_placeholder(
    session: &SessionRecord,
    entry: &ai_sessions::HistorySessionEntry,
) -> bool {
    if session.tool != entry.tool {
        return false;
    }
    if !session.tool_session_id.trim().is_empty() {
        return false;
    }
    if session.status != "pending_bind" && session.status != "unbound" {
        return false;
    }
    if !same_session_working_dir(&session.working_dir, &entry.working_dir) {
        return false;
    }
    let (created_at, updated_at) = history_entry_time_secs(entry);
    let target_ts = if created_at > 0 {
        created_at
    } else {
        updated_at
    };
    if target_ts == 0 {
        return false;
    }
    session.created_at.abs_diff(target_ts) <= HISTORY_BIND_WINDOW_SECS
}

fn placeholder_preference_score(
    session: &SessionRecord,
    entry: &ai_sessions::HistorySessionEntry,
) -> (u8, u64, u64) {
    let (created_at, updated_at) = history_entry_time_secs(entry);
    let target_ts = if created_at > 0 {
        created_at
    } else {
        updated_at
    };
    (
        if session.status == "pending_bind" {
            0
        } else {
            1
        },
        session.created_at.abs_diff(target_ts),
        u64::MAX - session.created_at,
    )
}

#[derive(Debug, Clone, Copy, Default)]
struct SessionsHistorySyncOutcome {
    persisted: bool,
    list_changed: bool,
}

fn merge_history_entry_into_session(
    session: &mut SessionRecord,
    entry: &ai_sessions::HistorySessionEntry,
) -> bool {
    let mut changed = false;
    let (created_at, updated_at) = history_entry_time_secs(entry);
    let history_name = entry.title.trim();
    let history_model = entry
        .model_name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if session.tool_session_id.trim() != entry.tool_session_id.trim() {
        session.tool_session_id = entry.tool_session_id.trim().to_string();
        changed = true;
    }
    if session.status != "active" {
        session.status = "active".to_string();
        changed = true;
    }
    if session.name_source != "manual" && session.name.trim().is_empty() && !history_name.is_empty()
    {
        session.name = history_name.to_string();
        changed = true;
    }
    if session.model_name != history_model {
        session.model_name = history_model;
        changed = true;
    }
    let normalized_working_dir = normalize_session_working_dir(&entry.working_dir);
    if !normalized_working_dir.is_empty() && session.working_dir != normalized_working_dir {
        session.working_dir = normalized_working_dir;
        changed = true;
    }
    if created_at > 0 && session.created_at != created_at {
        session.created_at = created_at;
        changed = true;
    }
    let next_last_used_at = session.last_used_at.max(updated_at.max(created_at));
    if session.last_used_at != next_last_used_at {
        session.last_used_at = next_last_used_at;
        changed = true;
    }
    changed
}

fn apply_history_entries_to_sessions_state(
    state: &mut SessionsState,
    tool: &str,
    entries: Vec<ai_sessions::HistorySessionEntry>,
    synced_at: u64,
) -> SessionsHistorySyncOutcome {
    let mut outcome = SessionsHistorySyncOutcome::default();
    let normalized_tool = tool.trim().to_lowercase();
    let mut session_index_by_tool_session = HashMap::<String, usize>::new();

    for (idx, session) in state.sessions.iter().enumerate() {
        if session.tool != normalized_tool {
            continue;
        }
        let tool_session_id = session.tool_session_id.trim();
        if tool_session_id.is_empty() {
            continue;
        }
        session_index_by_tool_session.insert(tool_session_id.to_string(), idx);
    }

    let mut claimed_placeholders = HashSet::<String>::new();
    let mut max_seen_updated_at_ms = state
        .history_sync
        .tools
        .get(&normalized_tool)
        .map(|tool_state| tool_state.last_seen_updated_at_ms)
        .unwrap_or(0);

    for entry in entries {
        if entry.tool != normalized_tool {
            continue;
        }
        max_seen_updated_at_ms =
            max_seen_updated_at_ms.max(entry.updated_at_ms.max(entry.created_at_ms));
        let Some(tombstone_key) = history_tombstone_key(&entry.tool, &entry.tool_session_id) else {
            continue;
        };
        if state.tombstones.contains(&tombstone_key) {
            continue;
        }

        if let Some(&idx) = session_index_by_tool_session.get(entry.tool_session_id.trim()) {
            if merge_history_entry_into_session(&mut state.sessions[idx], &entry) {
                outcome.persisted = true;
                outcome.list_changed = true;
            }
            continue;
        }

        let placeholder_idx = state
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                !claimed_placeholders.contains(&session.id)
                    && should_bind_history_entry_to_placeholder(session, &entry)
            })
            .min_by_key(|(_, session)| placeholder_preference_score(session, &entry))
            .map(|(idx, _)| idx);

        if let Some(idx) = placeholder_idx {
            claimed_placeholders.insert(state.sessions[idx].id.clone());
            if merge_history_entry_into_session(&mut state.sessions[idx], &entry) {
                outcome.persisted = true;
                outcome.list_changed = true;
            }
            session_index_by_tool_session.insert(entry.tool_session_id.clone(), idx);
            continue;
        }

        let (created_at, updated_at) = history_entry_time_secs(&entry);
        let record = SessionRecord {
            id: stable_history_session_record_id(&entry.tool, &entry.tool_session_id),
            name: entry.title.clone(),
            working_dir: normalize_session_working_dir(&entry.working_dir),
            tool: entry.tool.clone(),
            tool_session_id: entry.tool_session_id.clone(),
            model_name: entry.model_name.clone(),
            name_source: "history".to_string(),
            runtime_mode: "shared".to_string(),
            runtime_profile_id: None,
            preset_id: None,
            created_at,
            last_used_at: updated_at.max(created_at),
            status: "active".to_string(),
            favorited_at: None,
            provider_id: None,
        };
        session_index_by_tool_session.insert(record.tool_session_id.clone(), state.sessions.len());
        state.sessions.push(record);
        outcome.persisted = true;
        outcome.list_changed = true;
    }

    let tool_state = state
        .history_sync
        .tools
        .entry(normalized_tool)
        .or_insert_with(SessionsHistoryToolState::default);
    if !tool_state.full_backfill_done {
        tool_state.full_backfill_done = true;
        outcome.persisted = true;
    }
    let required_parser_version = required_history_parser_version(tool);
    if tool_state.parser_version != required_parser_version {
        tool_state.parser_version = required_parser_version;
        outcome.persisted = true;
    }
    if tool_state.last_seen_updated_at_ms != max_seen_updated_at_ms {
        tool_state.last_seen_updated_at_ms = max_seen_updated_at_ms;
        outcome.persisted = true;
    }
    if tool_state.last_completed_at != Some(synced_at) {
        tool_state.last_completed_at = Some(synced_at);
        outcome.persisted = true;
    }

    if outcome.list_changed {
        sort_sessions_for_display(&mut state.sessions);
    }

    outcome
}

fn sessions_history_sync_tool(tool: String) -> Result<SessionsHistorySyncOutcome, String> {
    let normalized_tool = tool.trim().to_lowercase();
    if !HISTORY_SYNC_TOOLS.contains(&normalized_tool.as_str()) {
        return Ok(SessionsHistorySyncOutcome::default());
    }

    let state_for_cursor = load_sessions_state()?;
    let requires_full_backfill = history_sync_requires_full_backfill(
        &normalized_tool,
        state_for_cursor.history_sync.tools.get(&normalized_tool),
    );
    let min_updated_at_ms = state_for_cursor
        .history_sync
        .tools
        .get(&normalized_tool)
        .and_then(|tool_state| {
            if !requires_full_backfill && tool_state.full_backfill_done {
                Some(tool_state.last_seen_updated_at_ms.saturating_sub(15_000))
            } else {
                None
            }
        });

    let entries =
        ai_sessions::collect_history_sessions_for_tool(&normalized_tool, min_updated_at_ms)?;
    let outcome = {
        let _sessions_state_guard = lock_sessions_state_write()?;
        let mut latest_state = load_sessions_state()?;
        let outcome = apply_history_entries_to_sessions_state(
            &mut latest_state,
            &normalized_tool,
            entries,
            now_ts(),
        );

        if outcome.persisted {
            save_sessions_state(&latest_state)?;
        }
        outcome
    };
    if outcome.persisted {
        let _ = workspaces::sync_from_sessions();
    }

    Ok(outcome)
}

pub(crate) async fn run_sessions_history_sync_pass(app: tauri::AppHandle) -> Result<bool, String> {
    if SESSIONS_HISTORY_SYNC_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(false);
    }

    let mut any_list_change = false;
    let result = async {
        for tool in HISTORY_SYNC_TOOLS {
            let tool_name = tool.to_string();
            match tauri::async_runtime::spawn_blocking(move || {
                sessions_history_sync_tool(tool_name)
            })
            .await
            {
                Ok(Ok(outcome)) => {
                    any_list_change |= outcome.list_changed;
                }
                Ok(Err(err)) => {
                    log::warn!("sessions history sync skipped due to tool error: {}", err);
                }
                Err(err) => {
                    log::warn!("sessions history sync worker join failed: {}", err);
                }
            }
        }
        if any_list_change {
            let _ = app.emit("sessions-updated", ());
            let _ = app.emit("refresh-counts", ());
        }
        Ok(any_list_change)
    }
    .await;

    SESSIONS_HISTORY_SYNC_RUNNING.store(false, Ordering::SeqCst);
    result
}

fn load_launcher_state() -> Result<LauncherState, String> {
    let path = StorageEngine::launcher_path()?;
    let _ = migrate_launcher_to_local_if_needed(&path);
    if !path.exists() {
        return Ok(LauncherState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(LauncherState::default());
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            if let Ok(state) = serde_json::from_value::<LauncherState>(value) {
                return Ok(state);
            }
        }
    }

    serde_json::from_str::<LauncherState>(&content).map_err(|e| e.to_string())
}

fn save_launcher_state(state: &LauncherState) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::launcher_path()?, &blob)?;
    StorageEngine::bump_revision()
}

fn load_outbox_state() -> Result<OutboxState, String> {
    let path = StorageEngine::outbox_path()?;
    if !path.exists() {
        return Ok(OutboxState::default());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(OutboxState::default());
    }

    match serde_json::from_str::<OutboxState>(&content) {
        Ok(state) => Ok(state),
        Err(strict_err) => {
            if let Some(recovered) = parse_first_json_value::<OutboxState>(&content) {
                // Self-heal corrupted trailing bytes and continue.
                let _ = StorageEngine::write_json(&path, &recovered);
                Ok(recovered)
            } else {
                Err(strict_err.to_string())
            }
        }
    }
}

fn save_outbox_state(state: &OutboxState) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::outbox_path()?, state)
}

fn load_migration_state() -> Result<MigrationState, String> {
    StorageEngine::read_json(&StorageEngine::migration_state_path()?)
}

fn save_migration_state(state: &MigrationState) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::migration_state_path()?, state)
}

fn get_meta() -> Result<ApiMeta, String> {
    let schema = StorageEngine::load_schema()?;
    Ok(ApiMeta {
        schema_version: schema.schema_version,
        revision: schema.revision,
    })
}

fn parse_json_array_len(raw: &str) -> usize {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.as_array().map(|arr| arr.len()))
        .unwrap_or(0)
}

fn extract_fields(value: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            match k.as_str() {
                "id" | "name" | "tool" | "api_key" | "base_url" | "model" | "is_enabled"
                | "provider_key" | "code" => {}
                _ => {
                    out.insert(k.clone(), v.clone());
                }
            }
        }
    }
    out
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LegacyProvidersView {
    active_claude: Option<String>,
    active_codex: Option<String>,
    active_gemini: Option<String>,
    active_opencode: Option<String>,
    providers: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderImportPreviewItem {
    pub import_key: String,
    pub id: String,
    pub name: String,
    pub tool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub conflict: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProvidersImportPreview {
    #[serde(default)]
    pub active: HashMap<String, String>,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub conflicts: usize,
    #[serde(default)]
    pub items: Vec<ProviderImportPreviewItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderImportDecision {
    pub import_key: String,
    pub action: String,
}

#[derive(Debug, Clone)]
struct ProviderImportConflictMatch {
    existing_id: String,
    existing_name: String,
    reason: String,
}

#[derive(Debug, Clone)]
struct ProviderImportCandidate {
    import_key: String,
    input: ProviderInput,
    conflict: Option<ProviderImportConflictMatch>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncedDeviceProviderLite {
    pub id: String,
    pub name: String,
    pub tool: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncedDeviceProvidersView {
    pub device_id: String,
    #[serde(default)]
    pub active: HashMap<String, String>,
    #[serde(default)]
    pub providers: Vec<SyncedDeviceProviderLite>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CliInstallCommand {
    pub label: String,
    pub command: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CliInstallGuide {
    pub docs_url: String,
    pub commands: Vec<CliInstallCommand>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct WorkspaceSessionsQueryResult {
    #[serde(default)]
    pub items: Vec<Value>,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub tool_options: Vec<String>,
    #[serde(default)]
    pub model_options: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CliEnvProbeResult {
    pub tool: String,
    pub installed: bool,
    pub version: String,
    pub configured: bool,
    pub importable: bool,
    pub install_guide: CliInstallGuide,
}

pub(crate) fn session_to_legacy(record: &SessionRecord) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("id".into(), json!(record.id));
    map.insert("name".into(), json!(record.name));
    map.insert("working_dir".into(), json!(record.working_dir));
    map.insert("model_type".into(), json!(record.tool));
    map.insert("model_name".into(), json!(record.model_name));
    map.insert("tool_session_id".into(), json!(record.tool_session_id));
    map.insert(
        "runtime_mode".into(),
        json!(normalize_runtime_mode(Some(&record.runtime_mode))),
    );
    map.insert(
        "runtime_profile_id".into(),
        json!(record.runtime_profile_id),
    );
    map.insert("preset_id".into(), json!(record.preset_id));
    map.insert("created_at".into(), json!(record.created_at));
    map.insert("last_used_at".into(), json!(record.last_used_at));
    map.insert("status".into(), json!(record.status));
    if let Some(ts) = record.favorited_at {
        map.insert("favorited_at".into(), json!(ts));
    }
    map.insert("provider_id".into(), json!(record.provider_id));
    Value::Object(map)
}

pub(crate) fn sessions_snapshot_all() -> Result<Vec<SessionRecord>, String> {
    let state = load_sessions_state()?;
    Ok(state.sessions)
}

pub(crate) fn workspace_session_counts_by_root_from_sessions(
    sessions: &[SessionRecord],
) -> HashMap<String, usize> {
    let mut counts = HashMap::<String, usize>::new();
    for session in filter_sessions_by_history_window(sessions.iter()) {
        let normalized_root = ai_sessions::normalize_working_dir_for_terminal(&session.working_dir);
        if normalized_root.trim().is_empty() {
            continue;
        }
        *counts.entry(normalized_root).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn workspace_session_counts_by_root() -> Result<HashMap<String, usize>, String> {
    let sessions = sessions_snapshot_all()?;
    Ok(workspace_session_counts_by_root_from_sessions(&sessions))
}

fn workspace_session_matches_query(record: &SessionRecord, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }

    let haystacks = [
        record.name.trim().to_lowercase(),
        record.tool_session_id.trim().to_lowercase(),
        record
            .model_name
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase(),
        record.working_dir.trim().to_lowercase(),
    ];

    haystacks.iter().any(|value| value.contains(&needle))
}

pub(crate) fn workspace_sessions_query_by_root(
    root_path: &str,
    tool: Option<&str>,
    model_name: Option<&str>,
    query: Option<&str>,
) -> Result<WorkspaceSessionsQueryResult, String> {
    let normalized_root = ai_sessions::normalize_working_dir_for_terminal(root_path);
    if normalized_root.trim().is_empty() {
        return Ok(WorkspaceSessionsQueryResult::default());
    }

    let normalized_tool = tool
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty() && value != "all");
    let normalized_model = model_name
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty() && value != "all");
    let normalized_query = query
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());

    let state = load_sessions_state()?;
    let workspace_sessions =
        filter_sessions_by_history_window(state.sessions.iter().filter(|session| {
            ai_sessions::normalize_working_dir_for_terminal(&session.working_dir) == normalized_root
        }));

    let total = workspace_sessions.len();

    let mut tool_options = workspace_sessions
        .iter()
        .map(|session| session.tool.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    tool_options.sort();
    tool_options.dedup();

    let mut model_options = workspace_sessions
        .iter()
        .filter(|session| {
            normalized_tool.as_ref().map_or(true, |tool_value| {
                session.tool.trim().eq_ignore_ascii_case(tool_value)
            })
        })
        .filter_map(|session| {
            session
                .model_name
                .as_deref()
                .map(|value| value.trim().to_string())
        })
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    model_options.sort();
    model_options.dedup();

    let items = workspace_sessions
        .into_iter()
        .filter(|session| {
            normalized_tool.as_ref().map_or(true, |tool_value| {
                session.tool.trim().eq_ignore_ascii_case(tool_value)
            })
        })
        .filter(|session| {
            normalized_model.as_ref().map_or(true, |model_value| {
                session
                    .model_name
                    .as_deref()
                    .map(|value| value.trim().eq_ignore_ascii_case(model_value))
                    .unwrap_or(false)
            })
        })
        .filter(|session| {
            normalized_query.as_ref().map_or(true, |query_value| {
                workspace_session_matches_query(session, query_value)
            })
        })
        .map(|session| session_to_legacy(&session))
        .collect();

    Ok(WorkspaceSessionsQueryResult {
        items,
        total,
        tool_options,
        model_options,
    })
}

fn launcher_to_legacy(record: &LauncherRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "type": record.item_type,
        "target": record.target,
        "pinned": record.pinned,
        "pin_order": record.pin_order,
        "launch_count": record.launch_count,
        "last_launched_at": record.last_launched_at,
        "trusted": record.trusted,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}

fn is_valid_launcher_type(item_type: &str) -> bool {
    LAUNCHER_TYPES.contains(&item_type)
}

fn sanitize_launcher_record(record: &mut LauncherRecord) -> Result<(), String> {
    record.name = record.name.trim().to_string();
    record.target = record.target.trim().to_string();
    record.item_type = record.item_type.trim().to_lowercase();
    if record.id.trim().is_empty() {
        record.id = uuid::Uuid::new_v4().to_string();
    }
    if record.name.is_empty() {
        return Err("launcher name required".to_string());
    }
    if record.target.is_empty() {
        return Err("launcher target required".to_string());
    }
    if !is_valid_launcher_type(&record.item_type) {
        return Err(format!("invalid launcher type: {}", record.item_type));
    }
    if record.item_type == "app" {
        record.target = normalize_app_target(&record.target)?;
    }
    if record.item_type != "script" {
        record.trusted = true;
    }
    if !record.pinned {
        record.pin_order = 0;
    }
    Ok(())
}

fn normalize_app_target(raw: &str) -> Result<String, String> {
    let mut target = raw.trim().to_string();
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("open -a ") {
        target = target[8..].trim().to_string();
    } else if lower.starts_with("open -a") {
        target = target[7..].trim().to_string();
    }
    target = target
        .trim()
        .trim_matches(is_wrapped_quote_char)
        .trim()
        .to_string();
    if target.is_empty() {
        return Err("app target required".to_string());
    }
    Ok(target)
}

fn is_wrapped_quote_char(c: char) -> bool {
    matches!(c, '"' | '\'' | '`' | '“' | '”' | '‘' | '’')
}

fn launcher_application_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
}

fn resolve_application_bundle_path(app_name: &str) -> Option<PathBuf> {
    let trimmed = app_name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let direct = PathBuf::from(trimmed);
    if direct.exists()
        && direct
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
    {
        return Some(direct);
    }

    let normalized = trimmed.trim_end_matches(".app");
    let normalized_lower = normalized.to_lowercase();
    if normalized_lower.is_empty() {
        return None;
    }

    for root in launcher_application_roots() {
        let exact = root.join(format!("{}.app", normalized));
        if exact.exists() {
            return Some(exact);
        }
    }

    for root in launcher_application_roots() {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if !ext.eq_ignore_ascii_case("app") {
                continue;
            }
            let file_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if file_name.contains(&normalized_lower) || normalized_lower.contains(&file_name) {
                return Some(path);
            }
        }
    }

    None
}

fn normalize_icon_candidate_name(raw: &str) -> Option<String> {
    let name = raw.trim().trim_matches(is_wrapped_quote_char).trim();
    if name.is_empty() {
        return None;
    }
    if name.to_ascii_lowercase().ends_with(".icns") {
        return Some(name.to_string());
    }
    Some(format!("{}.icns", name))
}

fn push_icon_candidate(candidates: &mut Vec<String>, raw: Option<&str>) {
    let Some(value) = raw else {
        return;
    };
    let Some(normalized) = normalize_icon_candidate_name(value) else {
        return;
    };
    if !candidates
        .iter()
        .any(|item| item.eq_ignore_ascii_case(&normalized))
    {
        candidates.push(normalized);
    }
}

fn extract_icon_candidates_from_plist_json(plist: &Value) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    push_icon_candidate(
        &mut candidates,
        plist.get("CFBundleIconFile").and_then(|v| v.as_str()),
    );
    push_icon_candidate(
        &mut candidates,
        plist.get("CFBundleIconName").and_then(|v| v.as_str()),
    );

    if let Some(icon_files) = plist
        .pointer("/CFBundleIcons/CFBundlePrimaryIcon/CFBundleIconFiles")
        .and_then(|v| v.as_array())
    {
        for item in icon_files.iter().rev() {
            push_icon_candidate(&mut candidates, item.as_str());
        }
    }

    if let Some(icon_files) = plist.get("CFBundleIconFiles").and_then(|v| v.as_array()) {
        for item in icon_files.iter().rev() {
            push_icon_candidate(&mut candidates, item.as_str());
        }
    }

    push_icon_candidate(&mut candidates, Some("AppIcon"));
    candidates
}

fn find_icns_path(resources_dir: &Path, candidates: &[String]) -> Option<PathBuf> {
    if !resources_dir.is_dir() {
        return None;
    }

    for candidate in candidates {
        let path = resources_dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    let mut available_icons: Vec<PathBuf> = fs::read_dir(resources_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|s| s.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("icns"))
                .unwrap_or(false)
        })
        .collect();
    available_icons.sort();

    for candidate in candidates {
        let candidate_lower = candidate.to_lowercase();
        if let Some(path) = available_icons.iter().find(|path| {
            path.file_name()
                .and_then(|s| s.to_str())
                .map(|name| name.to_lowercase() == candidate_lower)
                .unwrap_or(false)
        }) {
            return Some(path.clone());
        }
    }

    if let Some(path) = available_icons.iter().find(|path| {
        path.file_name()
            .and_then(|s| s.to_str())
            .map(|name| name.to_ascii_lowercase().contains("appicon"))
            .unwrap_or(false)
    }) {
        return Some(path.clone());
    }

    available_icons.into_iter().next()
}

#[cfg(target_os = "macos")]
fn read_info_plist_json(app_bundle_path: &Path) -> Option<Value> {
    let info_plist = app_bundle_path.join("Contents").join("Info.plist");
    let output = Command::new("plutil")
        .arg("-convert")
        .arg("json")
        .arg("-o")
        .arg("-")
        .arg(info_plist)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice::<Value>(&output.stdout).ok()
}

#[cfg(target_os = "macos")]
fn convert_icns_to_png_data_url(icns_path: &Path) -> Option<String> {
    let output_path = std::env::temp_dir().join(format!(
        "onespace-launcher-icon-{}.png",
        uuid::Uuid::new_v4()
    ));
    let status = Command::new("sips")
        .arg("-s")
        .arg("format")
        .arg("png")
        .arg(icns_path)
        .arg("--out")
        .arg(&output_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        let _ = fs::remove_file(&output_path);
        return None;
    }
    let png = fs::read(&output_path).ok();
    let _ = fs::remove_file(&output_path);
    png.map(|bytes| format!("data:image/png;base64,{}", BASE64_STANDARD.encode(bytes)))
}

#[cfg(target_os = "macos")]
fn resolve_app_icon_data_url(app_name: &str) -> Option<String> {
    let app_bundle_path = resolve_application_bundle_path(app_name)?;
    let resources_dir = app_bundle_path.join("Contents").join("Resources");

    let candidates = read_info_plist_json(&app_bundle_path)
        .map(|plist| extract_icon_candidates_from_plist_json(&plist))
        .unwrap_or_else(Vec::new);
    let icns_path = find_icns_path(&resources_dir, &candidates)?;
    convert_icns_to_png_data_url(&icns_path)
}

#[cfg(not(target_os = "macos"))]
fn resolve_app_icon_data_url(_app_name: &str) -> Option<String> {
    None
}

fn try_open_application(app_name: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if Command::new("open").arg("-a").arg(app_name).spawn().is_ok() {
            return Ok(());
        }

        if let Some(path) = resolve_application_bundle_path(app_name) {
            Command::new("open")
                .arg(&path)
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(());
        }

        Err(format!("Unable to find application named '{}'", app_name))
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", app_name])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new(app_name)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

fn normalize_launcher_pin_order(items: &mut [LauncherRecord]) {
    let mut pinned_idx: Vec<usize> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| if item.pinned { Some(idx) } else { None })
        .collect();
    pinned_idx.sort_by_key(|idx| items[*idx].pin_order);
    for (order, idx) in pinned_idx.into_iter().enumerate() {
        items[idx].pin_order = order as u32;
    }
}

fn sort_launcher_items(items: &mut [LauncherRecord]) {
    normalize_launcher_pin_order(items);
    items.sort_by(|a, b| {
        if a.pinned != b.pinned {
            return if a.pinned {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        if a.pinned && b.pinned {
            return a.pin_order.cmp(&b.pin_order);
        }
        b.last_launched_at
            .unwrap_or(0)
            .cmp(&a.last_launched_at.unwrap_or(0))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
}

fn next_launcher_pin_order(items: &[LauncherRecord]) -> u32 {
    items
        .iter()
        .filter(|item| item.pinned)
        .map(|item| item.pin_order)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

fn merge_launcher_items(existing: &mut Vec<LauncherRecord>, imported: Vec<LauncherRecord>) {
    for incoming in imported {
        if let Some(idx) = existing.iter().position(|it| it.id == incoming.id) {
            existing[idx] = incoming;
        } else {
            existing.push(incoming);
        }
    }
}

fn launcher_record_from_import_input(
    input: LauncherItemInput,
    now: u64,
) -> Result<LauncherRecord, String> {
    let mut record = LauncherRecord {
        id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name,
        item_type: input.item_type,
        target: input.target,
        pinned: input.pinned.unwrap_or(false),
        pin_order: input.pin_order.unwrap_or(0),
        launch_count: input.launch_count.unwrap_or(0),
        last_launched_at: input.last_launched_at,
        trusted: input.trusted.unwrap_or(false),
        created_at: input.created_at.unwrap_or(now),
        updated_at: input.updated_at.unwrap_or(now),
    };
    sanitize_launcher_record(&mut record)?;
    Ok(record)
}

fn is_managed_tool(tool: &str) -> bool {
    MANAGED_TOOLS.contains(&tool)
}

fn provider_env_managed(provider: &ProviderRecord) -> bool {
    if !is_managed_tool(&provider.core.tool) {
        return true;
    }
    provider
        .tool_config
        .get("env_managed")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Read the `_onespace_source_profile` marker from `~/.claude/settings.json`.
/// Returns the profile ID that is currently applied to the global Claude config.
pub(crate) fn read_global_claude_profile_id() -> Option<String> {
    let home_dir = dirs::home_dir()?;
    let path = home_dir.join(".claude").join("settings.json");
    let settings: Map<String, Value> = read_json_object(&path)?;
    settings
        .get("_onespace_source_profile")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn cli_cmd_name(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

fn detect_cli_installation(tool: &str) -> (bool, String) {
    let Some(cmd_name) = cli_cmd_name(tool) else {
        return (false, String::new());
    };

    let probe = crate::cli_probe::probe_cli_version(cmd_name);
    (probe.installed, probe.version)
}

fn read_json_object(path: &Path) -> Option<Map<String, Value>> {
    let content = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<Value>(&content).ok()?;
    value.as_object().cloned()
}

fn parse_first_json_value<T: DeserializeOwned>(content: &str) -> Option<T> {
    let mut stream = serde_json::Deserializer::from_str(content).into_iter::<Value>();
    let first = stream.next()?.ok()?;
    serde_json::from_value::<T>(first).ok()
}

fn cli_has_system_config(tool: &str) -> bool {
    let Some(home_dir) = dirs::home_dir() else {
        return false;
    };

    match tool {
        "claude" => {
            let path = home_dir.join(".claude").join("settings.json");
            let Some(settings) = read_json_object(&path) else {
                return false;
            };
            if let Some(env) = settings.get("env").and_then(|v| v.as_object()) {
                return env.contains_key("ANTHROPIC_API_KEY")
                    || env.contains_key("ANTHROPIC_AUTH_TOKEN")
                    || env.contains_key("ANTHROPIC_BASE_URL")
                    || env.contains_key("ANTHROPIC_MODEL");
            }
            false
        }
        "codex" => {
            let auth_path = home_dir.join(".codex").join("auth.json");
            if let Some(auth) = read_json_object(&auth_path) {
                if auth
                    .get("OPENAI_API_KEY")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                {
                    return true;
                }
            }
            let cfg_path = home_dir.join(".codex").join("config.toml");
            if let Ok(content) = fs::read_to_string(cfg_path) {
                if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                    return doc.get("base_url").is_some()
                        || doc.get("model").is_some()
                        || doc.get("model_provider").is_some()
                        || doc.get("forced_login_method").is_some()
                        || doc.get("approval_policy").is_some()
                        || doc.get("sandbox_mode").is_some();
                }
            }
            false
        }
        "gemini" => {
            let env_path = home_dir.join(".gemini").join(".env");
            if let Ok(content) = fs::read_to_string(env_path) {
                let has_key = content.lines().any(|line| {
                    let line = line.trim();
                    line.starts_with("GEMINI_API_KEY=")
                        || line.starts_with("GOOGLE_GEMINI_BASE_URL=")
                        || line.starts_with("GEMINI_MODEL=")
                });
                if has_key {
                    return true;
                }
            }
            let settings_path = home_dir.join(".gemini").join("settings.json");
            if let Some(settings) = read_json_object(&settings_path) {
                return settings.get("security").is_some() || settings.get("general").is_some();
            }
            false
        }
        "opencode" => {
            let path = home_dir
                .join(".config")
                .join("opencode")
                .join("opencode.json");
            if let Some(settings) = read_json_object(&path) {
                return settings
                    .get("provider")
                    .and_then(|v| v.as_object())
                    .map(|m| !m.is_empty())
                    .unwrap_or(false);
            }
            false
        }
        _ => false,
    }
}

fn install_guide_for(tool: &str) -> CliInstallGuide {
    match tool {
        "claude" => CliInstallGuide {
            docs_url: "https://docs.anthropic.com/en/docs/claude-code".to_string(),
            commands: vec![CliInstallCommand {
                label: "Recommended".to_string(),
                command: "curl -fsSL https://claude.ai/install.sh | bash".to_string(),
            }],
        },
        "codex" => CliInstallGuide {
            docs_url: "https://github.com/openai/codex".to_string(),
            commands: vec![CliInstallCommand {
                label: "Recommended".to_string(),
                command: "bun install -g @openai/codex".to_string(),
            }],
        },
        "gemini" => CliInstallGuide {
            docs_url: "https://github.com/google-gemini/gemini-cli".to_string(),
            commands: vec![CliInstallCommand {
                label: "Recommended".to_string(),
                command: "npm install -g @google/gemini-cli".to_string(),
            }],
        },
        "opencode" => CliInstallGuide {
            docs_url: "https://opencode.ai/docs".to_string(),
            commands: vec![CliInstallCommand {
                label: "Recommended".to_string(),
                command: "curl -fsSL https://opencode.ai/install | bash".to_string(),
            }],
        },
        _ => CliInstallGuide {
            docs_url: String::new(),
            commands: vec![],
        },
    }
}

fn read_system_provider(tool: &str) -> Option<ProviderRecord> {
    if !is_managed_tool(tool) {
        return None;
    }
    let home_dir = dirs::home_dir()?;
    read_system_provider_at_home(tool, &home_dir)
}

fn read_system_provider_at_home(tool: &str, home_dir: &Path) -> Option<ProviderRecord> {
    if !is_managed_tool(tool) {
        return None;
    }
    let mut provider = ProviderRecord::default();
    provider.core.id = format!("default-{}", tool);
    provider.core.tool = tool.to_string();
    provider.core.name = match tool {
        "claude" => "Imported Claude Config".to_string(),
        "codex" => "Imported Codex Config".to_string(),
        "gemini" => "Imported Gemini Config".to_string(),
        _ => "Imported Config".to_string(),
    };
    provider
        .tool_config
        .insert("env_managed".to_string(), Value::Bool(true));

    match tool {
        "claude" => {
            let path = home_dir.join(".claude").join("settings.json");
            let settings = read_json_object(&path)?;
            let normalized_default_model = resolve_claude_default_model_from_settings(&settings);
            if let Some(env) = settings.get("env").and_then(|v| v.as_object()) {
                if let Some(key) = env
                    .get("ANTHROPIC_API_KEY")
                    .and_then(|v| v.as_str())
                    .or_else(|| env.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()))
                {
                    provider.core.api_key = key.to_string();
                }
                if let Some(v) = env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()) {
                    provider.core.base_url = Some(v.to_string());
                }
                let mut claude_model_mappings = Vec::new();
                for family in ["haiku", "sonnet", "opus"] {
                    let Some((model_key, name_key, capabilities_key)) =
                        claude_model_env_keys_for_family(family)
                    else {
                        continue;
                    };
                    let raw_model = env.get(model_key).and_then(|v| v.as_str()).unwrap_or("");
                    let (upstream_model, supports_1m) = split_claude_1m_suffix(raw_model);
                    let display_name = env
                        .get(name_key)
                        .and_then(|v| v.as_str())
                        .unwrap_or(match family {
                            "haiku" => "Haiku",
                            "sonnet" => "Sonnet",
                            "opus" => "Opus",
                            _ => "",
                        })
                        .to_string();
                    let supported_capabilities = env
                        .get(capabilities_key)
                        .and_then(|v| v.as_str())
                        .and_then(parse_supported_capabilities_csv);
                    if !upstream_model.is_empty()
                        || !display_name.is_empty()
                        || supported_capabilities.is_some()
                    {
                        claude_model_mappings.push(ClaudeModelMapping {
                            family: family.to_string(),
                            display_name,
                            upstream_model,
                            supports_1m: Some(supports_1m && family != "haiku"),
                            supported_capabilities,
                        });
                    }
                }
                if !claude_model_mappings.is_empty() {
                    provider.tool_config.insert(
                        "claude_model_mappings".to_string(),
                        serde_json::to_value(&claude_model_mappings)
                            .unwrap_or_else(|_| Value::Array(vec![])),
                    );
                }
                for (src, dst) in [
                    ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "claude_haiku_model"),
                    ("ANTHROPIC_DEFAULT_SONNET_MODEL", "claude_sonnet_model"),
                    ("ANTHROPIC_DEFAULT_OPUS_MODEL", "claude_opus_model"),
                    ("CLAUDE_CODE_EFFORT_LEVEL", "claude_reasoning_effort"),
                ] {
                    if let Some(v) = env.get(src).and_then(|v| v.as_str()) {
                        provider
                            .tool_config
                            .insert(dst.to_string(), Value::String(v.to_string()));
                    }
                }
            }
            provider.core.model = normalized_default_model.clone();
            if let Some(model) = normalized_default_model {
                provider
                    .tool_config
                    .insert("claude_default_model".to_string(), Value::String(model));
            } else {
                provider.tool_config.remove("claude_default_model");
            }
            for (src, dst) in [
                ("dangerouslySkipPermissions", "dangerously_skip_permissions"),
                ("enableAllMemoryFeatures", "enable_all_memory_features"),
                ("enableMcp", "enable_mcp"),
            ] {
                if let Some(v) = settings.get(src).and_then(|v| v.as_bool()) {
                    provider.tool_config.insert(dst.to_string(), Value::Bool(v));
                }
            }
            for (src, dst) in [
                ("allowedTools", "allowed_tools"),
                ("blockedTools", "blocked_tools"),
            ] {
                if let Some(v) = settings.get(src) {
                    provider.tool_config.insert(dst.to_string(), v.clone());
                }
            }
            if let Some(v) = settings.get("maxSessionTurns").and_then(|v| v.as_u64()) {
                provider
                    .tool_config
                    .insert("max_session_turns".to_string(), Value::Number(v.into()));
            }
        }
        "codex" => {
            let auth_path = home_dir.join(".codex").join("auth.json");
            if let Some(auth) = read_json_object(&auth_path) {
                if let Some(v) = auth.get("OPENAI_API_KEY").and_then(|v| v.as_str()) {
                    provider.core.api_key = v.to_string();
                }
            }
            let config_path = home_dir.join(".codex").join("config.toml");
            if let Ok(content) = fs::read_to_string(config_path) {
                if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                    let active_model_provider = doc
                        .get("model_provider")
                        .and_then(|v| v.as_str())
                        .and_then(|id| {
                            doc.get("model_providers")
                                .and_then(|v| v.as_table())
                                .and_then(|table| table.get(id.trim()))
                                .and_then(|v| v.as_table())
                        });
                    if let Some(active_provider) = active_model_provider {
                        if let Some(v) = active_provider.get("base_url").and_then(|v| v.as_str()) {
                            provider.core.base_url = Some(v.to_string());
                        }
                        if let Some(wire_api) =
                            active_provider.get("wire_api").and_then(|v| v.as_str())
                        {
                            provider.tool_config.insert(
                                "wire_api".to_string(),
                                Value::String(wire_api.to_string()),
                            );
                        }
                    }
                    if let Some(v) = doc.get("base_url").and_then(|v| v.as_str()) {
                        if provider.core.base_url.is_none() {
                            provider.core.base_url = Some(v.to_string());
                        }
                    }
                    if let Some(v) = doc.get("model").and_then(|v| v.as_str()) {
                        provider.core.model = Some(v.to_string());
                    }
                    if let Some(v) = doc.get("forced_login_method").and_then(|v| v.as_str()) {
                        provider
                            .tool_config
                            .insert("codex_auth_mode".to_string(), Value::String(v.to_string()));
                    }
                    for k in [
                        "disable_response_storage",
                        "personality",
                        "model_reasoning_effort",
                        "model_reasoning_summary",
                        "approval_policy",
                        "sandbox_mode",
                    ] {
                        if let Some(v) = doc.get(k) {
                            if let Some(b) = v.as_bool() {
                                provider.tool_config.insert(k.to_string(), Value::Bool(b));
                            } else if let Some(s) = v.as_str() {
                                provider
                                    .tool_config
                                    .insert(k.to_string(), Value::String(s.to_string()));
                            }
                        }
                    }
                    if let Some(mp) = doc.get("model_providers").and_then(|v| v.as_table()) {
                        if let Some(default) = mp.get("default") {
                            if let Some(wire_api) = default.get("wire_api").and_then(|v| v.as_str())
                            {
                                provider
                                    .tool_config
                                    .entry("wire_api".to_string())
                                    .or_insert(Value::String(wire_api.to_string()));
                            }
                        }
                    }
                }
            }
        }
        "gemini" => {
            let env_path = home_dir.join(".gemini").join(".env");
            if let Ok(content) = fs::read_to_string(env_path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        let key = k.trim();
                        let val = v.trim().to_string();
                        match key {
                            "GEMINI_API_KEY" => provider.core.api_key = val,
                            "GOOGLE_GEMINI_BASE_URL" => provider.core.base_url = Some(val),
                            "GEMINI_MODEL" => provider.core.model = Some(val),
                            _ => {}
                        }
                    }
                }
            }
            let settings_path = home_dir.join(".gemini").join("settings.json");
            if let Some(settings) = read_json_object(&settings_path) {
                if let Some(v) = settings.get("theme") {
                    provider.tool_config.insert("theme".to_string(), v.clone());
                }
                if let Some(general) = settings.get("general").and_then(|v| v.as_object()) {
                    if let Some(v) = general.get("vimMode").and_then(|v| v.as_bool()) {
                        provider
                            .tool_config
                            .insert("vim_mode".to_string(), Value::Bool(v));
                    }
                    if let Some(v) = general.get("defaultApprovalMode").and_then(|v| v.as_str()) {
                        provider.tool_config.insert(
                            "default_approval_mode".to_string(),
                            Value::String(v.to_string()),
                        );
                    }
                }
                if let Some(auth_type) = settings
                    .get("security")
                    .and_then(|v| v.as_object())
                    .and_then(|s| s.get("auth"))
                    .and_then(|v| v.as_object())
                    .and_then(|a| a.get("selectedType"))
                    .and_then(|v| v.as_str())
                {
                    provider.tool_config.insert(
                        "gemini_auth_type".to_string(),
                        Value::String(auth_type.to_string()),
                    );
                }
            }
        }
        _ => return None,
    }

    Some(provider)
}

fn render_claude_to_dir(
    provider: &ProviderRecord,
    target_dir: &Path,
) -> Result<Vec<(PathBuf, String)>, String> {
    let settings_path = target_dir.join("settings.json");
    let is_global_dir = target_dir.ends_with(".claude");
    let mut settings = Map::new();

    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    let bool_fields = [
        ("dangerously_skip_permissions", "dangerouslySkipPermissions"),
        ("enable_all_memory_features", "enableAllMemoryFeatures"),
        ("enable_mcp", "enableMcp"),
    ];

    for (src, dst) in bool_fields {
        if let Some(v) = provider.tool_config.get(src).and_then(|v| v.as_bool()) {
            settings.insert(dst.to_string(), Value::Bool(v));
        } else {
            settings.remove(dst);
        }
    }

    for (src, dst) in [
        ("allowed_tools", "allowedTools"),
        ("blocked_tools", "blockedTools"),
    ] {
        if let Some(v) = provider.tool_config.get(src) {
            settings.insert(dst.to_string(), v.clone());
        } else {
            settings.remove(dst);
        }
    }

    if let Some(turns) = provider
        .tool_config
        .get("max_session_turns")
        .and_then(|v| v.as_u64())
    {
        settings.insert("maxSessionTurns".to_string(), Value::Number(turns.into()));
    }

    let mut env = settings
        .remove("env")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    env.insert(
        "ANTHROPIC_API_KEY".to_string(),
        Value::String(provider.core.api_key.clone()),
    );
    env.remove("ANTHROPIC_AUTH_TOKEN");

    if let Some(base_url) = &provider.core.base_url {
        if !base_url.is_empty() {
            env.insert(
                "ANTHROPIC_BASE_URL".to_string(),
                Value::String(base_url.clone()),
            );
        }
    } else {
        env.remove("ANTHROPIC_BASE_URL");
    }

    if let Some(v) = resolve_claude_default_model(provider.core.model.as_deref(), &provider.tool_config)
    {
        settings.insert("model".to_string(), Value::String(v.clone()));
        env.insert("ANTHROPIC_MODEL".to_string(), Value::String(v));
    } else {
        settings.remove("model");
        env.remove("ANTHROPIC_MODEL");
    }

    let claude_model_mappings = resolved_claude_model_mappings(&provider.tool_config);
    for family in ["haiku", "sonnet", "opus"] {
        let Some((model_key, name_key, capabilities_key)) =
            claude_model_env_keys_for_family(family)
        else {
            continue;
        };
        let mapping = claude_model_mappings
            .iter()
            .find(|mapping| mapping.family == family);
        if let Some(mapping) = mapping {
            let mut upstream_model = mapping.upstream_model.clone();
            if mapping.supports_1m.unwrap_or(false)
                && family != "haiku"
                && !upstream_model.contains("[1m]")
            {
                upstream_model.push_str("[1m]");
            }
            if upstream_model.trim().is_empty() {
                env.remove(model_key);
            } else {
                env.insert(model_key.to_string(), Value::String(upstream_model));
            }
            if mapping.display_name.trim().is_empty() {
                env.remove(name_key);
            } else {
                env.insert(
                    name_key.to_string(),
                    Value::String(mapping.display_name.clone()),
                );
            }
            if let Some(capabilities) = mapping
                .supported_capabilities
                .as_ref()
                .and_then(|values| join_supported_capabilities_csv(values))
            {
                env.insert(capabilities_key.to_string(), Value::String(capabilities));
            } else {
                env.remove(capabilities_key);
            }
        } else {
            env.remove(model_key);
            env.remove(name_key);
            env.remove(capabilities_key);
        }
    }

    if let Some(effort) = resolve_claude_reasoning_effort(&provider.tool_config) {
        env.insert(
            "CLAUDE_CODE_EFFORT_LEVEL".to_string(),
            Value::String(effort),
        );
    } else {
        env.remove("CLAUDE_CODE_EFFORT_LEVEL");
    }

    settings.insert("env".to_string(), Value::Object(env));

    // Internal marker: track which onespace profile is applied to the global Claude config.
    // Only written to ~/.claude, not to profile-specific directories.
    if is_global_dir {
        settings.insert(
            "_onespace_source_profile".to_string(),
            Value::String(provider.core.id.clone()),
        );
    } else {
        settings.remove("_onespace_source_profile");
    }

    let content =
        serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;
    Ok(vec![(settings_path, content)])
}

fn render_claude(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    render_claude_to_dir(provider, &home_dir.join(".claude"))
}

fn render_claude_reset_to_unmanaged() -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let settings_path = home_dir.join(".claude").join("settings.json");
    let mut settings = Map::new();

    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    // Remove the onespace source marker when resetting global config.
    settings.remove("_onespace_source_profile");

    for key in [
        "dangerouslySkipPermissions",
        "enableAllMemoryFeatures",
        "enableMcp",
        "allowedTools",
        "blockedTools",
        "maxSessionTurns",
    ] {
        settings.remove(key);
    }

    if let Some(env) = settings.get_mut("env").and_then(|v| v.as_object_mut()) {
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_SUPPORTED_CAPABILITIES",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_SUPPORTED_CAPABILITIES",
            "CLAUDE_CODE_EFFORT_LEVEL",
        ] {
            env.remove(key);
        }
        if env.is_empty() {
            settings.remove("env");
        }
    }

    let content =
        serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;
    Ok(vec![(settings_path, content)])
}

fn sanitize_codex_model_provider_id(raw: &str) -> String {
    let mut out = String::new();
    let mut last_was_separator = false;

    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            out.push('_');
            last_was_separator = true;
        }
    }

    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "onespace_provider".to_string()
    } else {
        format!("onespace_{}", trimmed)
    }
}

fn is_onespace_codex_model_provider_id(id: &str) -> bool {
    id.trim().starts_with("onespace_")
}

fn codex_auth_mode(provider: &ProviderRecord) -> Option<&'static str> {
    if let Some(mode) = provider
        .tool_config
        .get("codex_auth_mode")
        .or_else(|| provider.tool_config.get("auth_mode"))
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_lowercase())
    {
        return match mode.as_str() {
            "api" | "api_key" | "apikey" => Some("api"),
            "chatgpt" | "login" => Some("chatgpt"),
            "none" | "disabled" => None,
            _ => None,
        };
    }

    if provider.core.api_key.trim().is_empty() {
        None
    } else {
        Some("api")
    }
}

fn render_codex_auth(
    auth_path: &Path,
    provider: &ProviderRecord,
    auth_mode: Option<&str>,
) -> Result<Option<(PathBuf, String)>, String> {
    let Some(auth_mode) = auth_mode else {
        return Ok(None);
    };

    let mut auth = if auth_path.exists() {
        read_json_object(auth_path).unwrap_or_default()
    } else {
        Map::new()
    };

    match auth_mode {
        "api" => {
            auth.insert(
                "OPENAI_API_KEY".to_string(),
                Value::String(provider.core.api_key.clone()),
            );
        }
        "chatgpt" => {
            auth.remove("OPENAI_API_KEY");
        }
        _ => return Ok(None),
    }

    Ok(Some((
        auth_path.to_path_buf(),
        serde_json::to_string_pretty(&Value::Object(auth)).map_err(|e| e.to_string())?,
    )))
}

fn set_toml_table_string(table: &mut toml_edit::Table, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        table[key] = toml_edit::value(value.to_string());
    } else {
        table.remove(key);
    }
}

fn set_toml_table_bool(table: &mut toml_edit::Table, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        table[key] = toml_edit::value(value);
    } else {
        table.remove(key);
    }
}

fn render_codex_model_provider(
    doc: &mut toml_edit::DocumentMut,
    provider: &ProviderRecord,
    provider_id: &str,
    auth_mode: Option<&str>,
) {
    if !doc.contains_key("model_providers") {
        doc["model_providers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let Some(providers) = doc["model_providers"].as_table_mut() else {
        return;
    };

    if !providers.contains_key(provider_id) {
        providers.insert(provider_id, toml_edit::Item::Table(toml_edit::Table::new()));
    }

    let Some(provider_table) = providers
        .get_mut(provider_id)
        .and_then(|item| item.as_table_mut())
    else {
        return;
    };

    set_toml_table_string(provider_table, "name", Some(&provider.core.name));
    set_toml_table_string(
        provider_table,
        "base_url",
        provider.core.base_url.as_deref(),
    );
    set_toml_table_string(
        provider_table,
        "wire_api",
        provider
            .tool_config
            .get("wire_api")
            .and_then(|v| v.as_str())
            .or(Some("responses")),
    );
    set_toml_table_bool(
        provider_table,
        "requires_openai_auth",
        (auth_mode == Some("api")).then_some(true),
    );
}

fn render_codex(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    render_codex_at_home(provider, &home_dir)
}

fn render_codex_at_home(
    provider: &ProviderRecord,
    home_dir: &Path,
) -> Result<Vec<(PathBuf, String)>, String> {
    let codex_dir = home_dir.join(".codex");
    let auth_path = codex_dir.join("auth.json");
    let config_path = codex_dir.join("config.toml");

    let auth_mode = codex_auth_mode(provider);

    let mut toml_str = String::new();
    if config_path.exists() {
        toml_str = fs::read_to_string(&config_path).unwrap_or_default();
    }
    let mut doc = toml_str
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|_| toml_edit::DocumentMut::new());

    doc.remove("base_url");
    doc.remove("preferred_auth_method");

    match auth_mode {
        Some("api") => doc["forced_login_method"] = toml_edit::value("api"),
        Some("chatgpt") => doc["forced_login_method"] = toml_edit::value("chatgpt"),
        _ => {
            doc.remove("forced_login_method");
        }
    }

    let custom_provider_id = provider
        .core
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|_| sanitize_codex_model_provider_id(&provider.core.id));
    if let Some(provider_id) = custom_provider_id.as_deref() {
        render_codex_model_provider(&mut doc, provider, provider_id, auth_mode);
        doc["model_provider"] = toml_edit::value(provider_id.to_string());
    } else {
        doc["model_provider"] = toml_edit::value("openai");
    }

    if let Some(v) = &provider.core.model {
        doc["model"] = toml_edit::value(v.clone());
    } else {
        doc.remove("model");
    }

    for (k, toml_key) in [
        ("disable_response_storage", "disable_response_storage"),
        ("personality", "personality"),
        ("model_reasoning_effort", "model_reasoning_effort"),
        ("model_reasoning_summary", "model_reasoning_summary"),
        ("approval_policy", "approval_policy"),
        ("sandbox_mode", "sandbox_mode"),
    ] {
        if let Some(value) = provider.tool_config.get(k) {
            match value {
                Value::Bool(b) => doc[toml_key] = toml_edit::value(*b),
                Value::String(s) => doc[toml_key] = toml_edit::value(s.clone()),
                _ => {}
            }
        }
    }

    let mut outputs = Vec::new();
    if let Some(auth_output) = render_codex_auth(&auth_path, provider, auth_mode)? {
        outputs.push(auth_output);
    }
    outputs.push((config_path, doc.to_string()));
    Ok(outputs)
}

fn render_codex_reset_to_unmanaged() -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    render_codex_reset_to_unmanaged_at_home(&home_dir)
}

fn render_codex_reset_to_unmanaged_at_home(
    home_dir: &Path,
) -> Result<Vec<(PathBuf, String)>, String> {
    let codex_dir = home_dir.join(".codex");
    let auth_path = codex_dir.join("auth.json");
    let config_path = codex_dir.join("config.toml");
    let mut outputs = Vec::new();

    if auth_path.exists() {
        let mut auth = read_json_object(&auth_path).unwrap_or_default();
        auth.remove("OPENAI_API_KEY");
        outputs.push((
            auth_path,
            serde_json::to_string_pretty(&Value::Object(auth)).map_err(|e| e.to_string())?,
        ));
    }

    if config_path.exists() {
        let content = fs::read_to_string(&config_path).unwrap_or_default();
        let mut doc = content
            .parse::<toml_edit::DocumentMut>()
            .unwrap_or_else(|_| toml_edit::DocumentMut::new());
        let active_model_provider = doc
            .get("model_provider")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string());
        let active_is_onespace = active_model_provider
            .as_deref()
            .map(is_onespace_codex_model_provider_id)
            .unwrap_or(false);

        for key in [
            "base_url",
            "disable_response_storage",
            "personality",
            "model_reasoning_effort",
            "model_reasoning_summary",
            "approval_policy",
            "sandbox_mode",
        ] {
            doc.remove(key);
        }

        if active_is_onespace {
            doc.remove("model");
            doc.remove("model_provider");
            doc.remove("forced_login_method");
            doc.remove("preferred_auth_method");
        }

        if let Some(providers) = doc
            .get_mut("model_providers")
            .and_then(|v| v.as_table_mut())
        {
            if let Some(provider_id) = active_model_provider.as_deref() {
                if is_onespace_codex_model_provider_id(provider_id) {
                    providers.remove(provider_id);
                }
            }
        }

        outputs.push((config_path, doc.to_string()));
    }

    Ok(outputs)
}

fn render_gemini(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let gemini_dir = home_dir.join(".gemini");
    let env_path = gemini_dir.join(".env");
    let settings_path = gemini_dir.join("settings.json");

    let mut env_map = std::collections::BTreeMap::new();
    env_map.insert("GEMINI_API_KEY".to_string(), provider.core.api_key.clone());
    if let Some(v) = &provider.core.base_url {
        env_map.insert("GOOGLE_GEMINI_BASE_URL".to_string(), v.clone());
    }
    if let Some(v) = &provider.core.model {
        env_map.insert("GEMINI_MODEL".to_string(), v.clone());
    }

    let mut env_content = String::new();
    for (k, v) in env_map {
        env_content.push_str(&format!("{}={}\n", k, v));
    }

    let mut settings = Map::new();
    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    for field in ["theme"] {
        if let Some(v) = provider.tool_config.get(field) {
            settings.insert(field.to_string(), v.clone());
        }
    }

    if let Some(v) = provider
        .tool_config
        .get("vim_mode")
        .and_then(|v| v.as_bool())
    {
        let mut general = settings
            .remove("general")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        general.insert("vimMode".to_string(), Value::Bool(v));
        if let Some(mode) = provider
            .tool_config
            .get("default_approval_mode")
            .and_then(|v| v.as_str())
        {
            general.insert(
                "defaultApprovalMode".to_string(),
                Value::String(mode.to_string()),
            );
        }
        settings.insert("general".to_string(), Value::Object(general));
    }

    if let Some(auth_type) = provider
        .tool_config
        .get("gemini_auth_type")
        .and_then(|v| v.as_str())
    {
        let mut security = settings
            .remove("security")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let mut auth = security
            .remove("auth")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        auth.insert(
            "selectedType".to_string(),
            Value::String(auth_type.to_string()),
        );
        security.insert("auth".to_string(), Value::Object(auth));
        settings.insert("security".to_string(), Value::Object(security));
    }

    Ok(vec![
        (env_path, env_content),
        (
            settings_path,
            serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?,
        ),
    ])
}

fn render_gemini_reset_to_unmanaged() -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let gemini_dir = home_dir.join(".gemini");
    let env_path = gemini_dir.join(".env");
    let settings_path = gemini_dir.join("settings.json");
    let mut outputs = Vec::new();

    if env_path.exists() {
        let content = fs::read_to_string(&env_path).unwrap_or_default();
        let mut env_map = std::collections::BTreeMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim();
                if key == "GEMINI_API_KEY"
                    || key == "GOOGLE_GEMINI_BASE_URL"
                    || key == "GEMINI_MODEL"
                {
                    continue;
                }
                env_map.insert(key.to_string(), v.trim().to_string());
            }
        }
        let mut new_content = String::new();
        for (k, v) in env_map {
            new_content.push_str(&format!("{}={}\n", k, v));
        }
        outputs.push((env_path, new_content));
    }

    if settings_path.exists() {
        let mut settings = read_json_object(&settings_path).unwrap_or_default();
        settings.remove("theme");

        if let Some(general) = settings.get_mut("general").and_then(|v| v.as_object_mut()) {
            general.remove("vimMode");
            general.remove("defaultApprovalMode");
            if general.is_empty() {
                settings.remove("general");
            }
        }

        if let Some(security) = settings.get_mut("security").and_then(|v| v.as_object_mut()) {
            if let Some(auth) = security.get_mut("auth").and_then(|v| v.as_object_mut()) {
                auth.remove("selectedType");
                if auth.is_empty() {
                    security.remove("auth");
                }
            }
            if security.is_empty() {
                settings.remove("security");
            }
        }

        outputs.push((
            settings_path,
            serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?,
        ));
    }

    Ok(outputs)
}

fn render_opencode(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let path = home_dir
        .join(".config")
        .join("opencode")
        .join("opencode.json");

    let mut settings = Map::new();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    settings
        .entry("$schema".to_string())
        .or_insert(Value::String("https://opencode.ai/config.json".to_string()));

    if let Some(v) = provider
        .tool_config
        .get("opencode_default_model")
        .and_then(|v| v.as_str())
    {
        settings.insert("model".to_string(), Value::String(v.to_string()));
    }

    if let Some(v) = provider
        .tool_config
        .get("opencode_default_agent")
        .and_then(|v| v.as_str())
    {
        let mut agent = settings
            .remove("agent")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        agent.insert("default".to_string(), Value::String(v.to_string()));
        settings.insert("agent".to_string(), Value::Object(agent));
    }

    if let Some(v) = provider
        .tool_config
        .get("opencode_sessions_dir")
        .and_then(|v| v.as_str())
    {
        let mut sessions = settings
            .remove("sessions")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        sessions.insert("dir".to_string(), Value::String(v.to_string()));
        settings.insert("sessions".to_string(), Value::Object(sessions));
    }

    let mut providers = settings
        .remove("provider")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    let provider_key = provider
        .provider_key
        .clone()
        .or_else(|| {
            if provider.core.id == "default-opencode" {
                Some("onespace_provider".to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| provider.core.id.clone());

    let mut provider_obj = provider.tool_config.clone();
    provider_obj.insert(
        "name".to_string(),
        Value::String(provider.core.name.clone()),
    );
    providers.insert(provider_key, Value::Object(provider_obj));

    settings.insert("provider".to_string(), Value::Object(providers));

    Ok(vec![(
        path,
        serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?,
    )])
}

fn render_projection(provider: &ProviderRecord) -> Result<Vec<(PathBuf, String)>, String> {
    if !provider_env_managed(provider) {
        return match provider.core.tool.as_str() {
            "claude" => render_claude_reset_to_unmanaged(),
            "codex" => render_codex_reset_to_unmanaged(),
            "gemini" => render_gemini_reset_to_unmanaged(),
            _ => Err(format!(
                "Unsupported tool for unmanaged reset: {}",
                provider.core.tool
            )),
        };
    }

    match provider.core.tool.as_str() {
        "claude" => render_claude(provider),
        "codex" => render_codex(provider),
        "gemini" => render_gemini(provider),
        "opencode" => render_opencode(provider),
        other => Err(format!("Unsupported tool: {}", other)),
    }
}

fn apply_projection(provider: &ProviderRecord) -> Result<(), String> {
    let renders = render_projection(provider)?;
    for (path, content) in renders {
        StorageEngine::atomic_write(&path, &content)?;
    }
    Ok(())
}

fn build_projection_diff(provider: &ProviderRecord) -> Result<Vec<Value>, String> {
    let renders = render_projection(provider)?;
    let mut diffs = Vec::new();

    for (path, desired) in renders {
        let current = if path.exists() {
            fs::read_to_string(&path).unwrap_or_default()
        } else {
            String::new()
        };
        if current != desired {
            diffs.push(json!({
                "path": path.to_string_lossy(),
                "current": current,
                "desired": desired
            }));
        }
    }

    Ok(diffs)
}

static SYNC_RUNNING: AtomicBool = AtomicBool::new(false);

struct SyncRunningGuard;

impl Drop for SyncRunningGuard {
    fn drop(&mut self) {
        SYNC_RUNNING.store(false, Ordering::SeqCst);
    }
}

fn file_modified_ts(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

fn placeholder_for(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.icloud", path.to_string_lossy()))
}

fn atomic_copy(src: &Path, dst: &Path) -> Result<(), String> {
    let bytes = fs::read(src).map_err(|e| e.to_string())?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = dst.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dst).map_err(|e| e.to_string())
}

fn sync_file_bidirectional(
    local: &Path,
    shared: &Path,
    warnings: &mut Vec<String>,
    label: &str,
) -> Result<(), String> {
    let local_ts = file_modified_ts(local);
    let shared_ts = file_modified_ts(shared);
    let shared_pending_download = shared_ts.is_none() && placeholder_for(shared).exists();

    if shared_pending_download {
        warnings.push(format!(
            "{}: shared file pending download ({})",
            label,
            placeholder_for(shared).display()
        ));
    }

    match (local_ts, shared_ts) {
        (Some(l), Some(s)) if s > l => {
            if let Err(err) = atomic_copy(shared, local) {
                warnings.push(format!(
                    "{}: skip importing shared copy {} -> {} ({})",
                    label,
                    shared.display(),
                    local.display(),
                    err
                ));
            }
        }
        (Some(l), Some(s)) if l > s => {
            atomic_copy(local, shared)?;
        }
        (None, Some(_)) => {
            if let Err(err) = atomic_copy(shared, local) {
                warnings.push(format!(
                    "{}: skip importing shared copy {} -> {} ({})",
                    label,
                    shared.display(),
                    local.display(),
                    err
                ));
            }
        }
        (Some(_), None) => {
            if shared_pending_download {
                warnings.push(format!(
                    "{}: skip exporting while shared file is pending download",
                    label
                ));
            } else {
                atomic_copy(local, shared)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn walk_files_recursive(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Ok(vec![]);
    }

    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf();
                files.push(rel);
            }
        }
    }
    Ok(files)
}

fn strip_icloud_suffix(path: &Path) -> Option<PathBuf> {
    let file_name = path.file_name()?.to_string_lossy();
    let stripped = file_name.strip_suffix(".icloud")?;
    let mut out = path.to_path_buf();
    out.set_file_name(stripped);
    Some(out)
}

fn sync_directory_bidirectional(
    local_root: &Path,
    shared_root: &Path,
    warnings: &mut Vec<String>,
    label: &str,
) -> Result<(), String> {
    fs::create_dir_all(local_root).map_err(|e| e.to_string())?;
    fs::create_dir_all(shared_root).map_err(|e| e.to_string())?;

    let local_files_raw = walk_files_recursive(local_root)?;
    let shared_files_raw = walk_files_recursive(shared_root)?;

    let local_files: Vec<PathBuf> = local_files_raw
        .into_iter()
        .filter(|p| strip_icloud_suffix(p).is_none())
        .collect();

    let mut shared_files = Vec::new();
    let mut shared_pending = HashSet::<PathBuf>::new();
    for rel in shared_files_raw {
        if let Some(real_rel) = strip_icloud_suffix(&rel) {
            shared_pending.insert(real_rel);
        } else {
            shared_files.push(rel);
        }
    }

    let mut union = BTreeSet::<PathBuf>::new();
    for rel in &local_files {
        union.insert(rel.clone());
    }
    for rel in &shared_files {
        union.insert(rel.clone());
    }
    for rel in &shared_pending {
        union.insert(rel.clone());
    }

    for rel in union {
        let local = local_root.join(&rel);
        let shared = shared_root.join(&rel);
        let local_ts = file_modified_ts(&local);
        let shared_ts = file_modified_ts(&shared);
        let shared_pending_download = (shared_ts.is_none() && placeholder_for(&shared).exists())
            || shared_pending.contains(&rel);

        if shared_pending_download {
            warnings.push(format!(
                "{}:{}: shared file pending download ({})",
                label,
                rel.display(),
                placeholder_for(&shared).display()
            ));
        }

        match (local_ts, shared_ts) {
            (Some(l), Some(s)) if s > l => {
                if let Err(err) = atomic_copy(&shared, &local) {
                    warnings.push(format!(
                        "{}:{}: skip importing shared copy {} -> {} ({})",
                        label,
                        rel.display(),
                        shared.display(),
                        local.display(),
                        err
                    ));
                }
            }
            (Some(l), Some(s)) if l > s => {
                atomic_copy(&local, &shared)?;
            }
            (None, Some(_)) => {
                if let Err(err) = atomic_copy(&shared, &local) {
                    warnings.push(format!(
                        "{}:{}: skip importing shared copy {} -> {} ({})",
                        label,
                        rel.display(),
                        shared.display(),
                        local.display(),
                        err
                    ));
                }
            }
            (Some(_), None) => {
                if shared_pending_download {
                    warnings.push(format!(
                        "{}:{}: skip exporting while shared file is pending download",
                        label,
                        rel.display()
                    ));
                } else {
                    atomic_copy(&local, &shared)?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn shared_profile_path(cfg: &config::StorageConfig, name: &str) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("profile")
        .join(name);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

fn shared_content_path(cfg: &config::StorageConfig, file_name: &str) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("content")
        .join(file_name);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

fn shared_news_path(cfg: &config::StorageConfig, file_name: &str) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("news")
        .join(file_name);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

fn local_workflow_presets_path() -> Result<PathBuf, String> {
    Ok(config::get_local_data_dir()?.join("workflow_presets.json"))
}

fn local_skills_repository_root() -> Result<PathBuf, String> {
    Ok(crate::get_data_dir()?.join("data").join("skills"))
}

fn local_subagents_repository_root() -> Result<PathBuf, String> {
    Ok(crate::get_data_dir()?.join("data").join("subagents"))
}

fn local_ai_news_path() -> Result<PathBuf, String> {
    ai_news::ai_news_local_path()
}

fn shared_skills_repository_root(cfg: &config::StorageConfig) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("profile")
        .join("skills_repository");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn shared_subagents_repository_root(cfg: &config::StorageConfig) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("profile")
        .join("subagents_repository");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn key_looks_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("key")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("auth")
}

fn is_placeholder_string(value: &str) -> bool {
    value.starts_with('$') || value.starts_with("${")
}

fn placeholder_for_key(key: &str) -> String {
    let normalized = key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("${}", normalized)
}

fn sanitize_value_for_shared(key_hint: Option<&str>, value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut out = Map::new();
            for (k, v) in obj {
                out.insert(k.clone(), sanitize_value_for_shared(Some(k), v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| sanitize_value_for_shared(key_hint, v))
                .collect(),
        ),
        Value::String(s) => {
            if key_hint.map(key_looks_sensitive).unwrap_or(false) && !s.is_empty() {
                Value::String(placeholder_for_key(key_hint.unwrap_or("SECRET")))
            } else {
                Value::String(s.clone())
            }
        }
        _ => value.clone(),
    }
}

fn sanitize_map_for_shared(source: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in source {
        out.insert(k.clone(), sanitize_value_for_shared(Some(k), v));
    }
    out
}

fn merge_sensitive_maps(
    incoming: &Map<String, Value>,
    existing: &Map<String, Value>,
) -> Map<String, Value> {
    let mut merged = incoming.clone();
    for (k, old_v) in existing {
        let should_restore = key_looks_sensitive(k)
            && match merged.get(k) {
                None => true,
                Some(Value::String(s)) => s.is_empty() || is_placeholder_string(s),
                Some(_) => false,
            };
        if should_restore {
            merged.insert(k.clone(), old_v.clone());
        }
    }
    merged
}

fn export_local_providers_to_shared(path: &Path) -> Result<(), String> {
    let mut state = load_providers_state()?;
    for provider in &mut state.providers {
        provider.core.api_key.clear();
        provider.tool_config = sanitize_map_for_shared(&provider.tool_config);
        provider.extra = sanitize_map_for_shared(&provider.extra);
    }
    StorageEngine::write_json(path, &state)
}

fn import_shared_providers_to_local(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let incoming: ProvidersState = StorageEngine::read_json(path)?;

    let mut local = load_providers_state()?;
    let before = serde_json::to_value(&local).unwrap_or(Value::Null);
    let incoming_keys: HashSet<(String, String)> = incoming
        .providers
        .iter()
        .map(|p| (p.core.id.clone(), p.core.tool.clone()))
        .collect();

    for in_provider in &incoming.providers {
        if let Some(existing) = local
            .providers
            .iter_mut()
            .find(|p| p.core.id == in_provider.core.id && p.core.tool == in_provider.core.tool)
        {
            let old_api_key = existing.core.api_key.clone();
            let old_tool_cfg = existing.tool_config.clone();
            let old_extra = existing.extra.clone();
            let old_history = existing.history.clone();

            *existing = in_provider.clone();
            existing.core.api_key = old_api_key;
            existing.tool_config = merge_sensitive_maps(&existing.tool_config, &old_tool_cfg);
            existing.extra = merge_sensitive_maps(&existing.extra, &old_extra);
            if !old_history.is_empty() {
                existing.history = old_history;
            }
        } else {
            let mut inserted = in_provider.clone();
            inserted.core.api_key = String::new();
            local.providers.push(inserted);
        }
    }

    // Propagate deletions from shared profile to local mirror.
    local
        .providers
        .retain(|p| incoming_keys.contains(&(p.core.id.clone(), p.core.tool.clone())));

    if !incoming.active.is_empty() && incoming.active != local.active {
        local.active = incoming.active.clone();
    }

    // Prevent active pointer drift after deletions.
    local.active.retain(|tool, provider_id| {
        local
            .providers
            .iter()
            .any(|p| p.core.tool == *tool && p.core.id == *provider_id)
    });

    // Also backfill missing active keys if shared config omitted active map.
    if incoming.active.is_empty() {
        for provider in &local.providers {
            local
                .active
                .entry(provider.core.tool.clone())
                .or_insert_with(|| provider.core.id.clone());
        }
    }

    let after = serde_json::to_value(&local).unwrap_or(Value::Null);
    if before != after {
        let _ = save_providers_state(&local)?;
    }
    Ok(())
}

fn sanitize_mcp_for_shared(state: &mcp_servers::MCPServersState) -> mcp_servers::MCPServersState {
    let mut out = state.clone();
    out.is_encrypted = false;
    for server in &mut out.servers {
        if let Some(env) = server.env.as_mut() {
            let keys: Vec<String> = env.keys().cloned().collect();
            for key in keys {
                env.insert(key.clone(), placeholder_for_key(&key));
            }
        }
        if let Some(headers) = server.headers.as_mut() {
            let keys: Vec<String> = headers.keys().cloned().collect();
            for key in keys {
                headers.insert(key.clone(), placeholder_for_key(&key));
            }
        }
    }
    out
}

fn merge_sensitive_string_maps(
    incoming: &Option<HashMap<String, String>>,
    existing: &Option<HashMap<String, String>>,
) -> Option<HashMap<String, String>> {
    match (incoming, existing) {
        (None, None) => None,
        (Some(map), None) => Some(map.clone()),
        (None, Some(map)) => Some(map.clone()),
        (Some(in_map), Some(old_map)) => {
            let mut merged = in_map.clone();
            for (k, old_val) in old_map {
                let keep_old = match merged.get(k) {
                    None => true,
                    Some(v) => v.is_empty() || is_placeholder_string(v),
                };
                if keep_old {
                    merged.insert(k.clone(), old_val.clone());
                }
            }
            Some(merged)
        }
    }
}

fn export_local_mcp_to_shared(path: &Path) -> Result<(), String> {
    let local_state = mcp_servers::get_mcp_servers()?;
    let shared = sanitize_mcp_for_shared(&local_state);
    StorageEngine::write_json(path, &shared)
}

fn import_shared_mcp_to_local(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let incoming: mcp_servers::MCPServersState = StorageEngine::read_json(path)?;

    let mut local_state = mcp_servers::get_mcp_servers().unwrap_or_default();
    let before = serde_json::to_value(&local_state).unwrap_or(Value::Null);
    let incoming_ids: HashSet<String> = incoming.servers.iter().map(|s| s.id.clone()).collect();

    for incoming_server in incoming.servers {
        if let Some(existing) = local_state
            .servers
            .iter_mut()
            .find(|s| s.id == incoming_server.id)
        {
            existing.name = incoming_server.name.clone();
            existing.description = incoming_server.description.clone();
            existing.config_key = incoming_server.config_key.clone();
            existing.transport = incoming_server.transport.clone();
            existing.command = incoming_server.command.clone();
            existing.args = incoming_server.args.clone();
            existing.cwd = incoming_server.cwd.clone();
            existing.url = incoming_server.url.clone();
            existing.http_url = incoming_server.http_url.clone();
            existing.timeout = incoming_server.timeout;
            existing.trust = incoming_server.trust;
            existing.linked_provider_ids = incoming_server.linked_provider_ids.clone();
            existing.env = merge_sensitive_string_maps(&incoming_server.env, &existing.env);
            existing.headers =
                merge_sensitive_string_maps(&incoming_server.headers, &existing.headers);
            existing.updated_at = chrono::Utc::now();
        } else {
            let mut inserted = incoming_server.clone();
            inserted.created_at = chrono::Utc::now();
            inserted.updated_at = chrono::Utc::now();
            local_state.servers.push(inserted);
        }
    }

    // Propagate deletions from shared profile to local mirror.
    local_state
        .servers
        .retain(|server| incoming_ids.contains(&server.id));

    let after = serde_json::to_value(&local_state).unwrap_or(Value::Null);
    if before != after {
        local_state.is_encrypted = true;
        for server in &mut local_state.servers {
            let _ = mcp_servers::encrypt_sensitive_data(server);
        }
        StorageEngine::write_json(&StorageEngine::mcp_path()?, &local_state)?;
    }

    Ok(())
}

fn sync_providers_profile(
    cfg: &config::StorageConfig,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let local = StorageEngine::providers_path()?;
    let shared = shared_profile_path(cfg, "providers.json")?;
    let local_ts = file_modified_ts(&local);
    let shared_ts = file_modified_ts(&shared);
    let shared_pending_download = shared_ts.is_none() && placeholder_for(&shared).exists();

    match (local_ts, shared_ts) {
        (Some(l), Some(s)) if s > l => import_shared_providers_to_local(&shared)?,
        (Some(l), Some(s)) if l > s => export_local_providers_to_shared(&shared)?,
        (None, Some(_)) => import_shared_providers_to_local(&shared)?,
        (Some(_), None) => {
            if shared_pending_download {
                warnings.push(
                    "providers: skip export while shared file is pending download".to_string(),
                );
            } else {
                export_local_providers_to_shared(&shared)?;
            }
        }
        _ => {}
    }

    if shared_pending_download {
        let ph = placeholder_for(&shared);
        warnings.push(format!(
            "providers: shared file pending download ({})",
            ph.display()
        ));
    }

    Ok(())
}

fn sync_mcp_profile(cfg: &config::StorageConfig, warnings: &mut Vec<String>) -> Result<(), String> {
    let local = StorageEngine::mcp_path()?;
    let shared = shared_profile_path(cfg, "mcp.json")?;
    let local_ts = file_modified_ts(&local);
    let shared_ts = file_modified_ts(&shared);
    let shared_pending_download = shared_ts.is_none() && placeholder_for(&shared).exists();

    match (local_ts, shared_ts) {
        (Some(l), Some(s)) if s > l => import_shared_mcp_to_local(&shared)?,
        (Some(l), Some(s)) if l > s => export_local_mcp_to_shared(&shared)?,
        (None, Some(_)) => import_shared_mcp_to_local(&shared)?,
        (Some(_), None) => {
            if shared_pending_download {
                warnings.push("mcp: skip export while shared file is pending download".to_string());
            } else {
                export_local_mcp_to_shared(&shared)?;
            }
        }
        _ => {}
    }

    if shared_pending_download {
        let ph = placeholder_for(&shared);
        warnings.push(format!(
            "mcp: shared file pending download ({})",
            ph.display()
        ));
    }

    Ok(())
}

fn run_local_shared_sync(cfg: &config::StorageConfig) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    let policy = cfg.sync_policy.clone();

    if policy.providers {
        sync_providers_profile(cfg, &mut warnings)?;
    }

    if policy.mcp {
        sync_mcp_profile(cfg, &mut warnings)?;
    }

    if policy.workflow_presets {
        let local = local_workflow_presets_path()?;
        let shared = shared_profile_path(cfg, "workflow_presets.json")?;
        sync_file_bidirectional(&local, &shared, &mut warnings, "workflow_presets")?;
    }

    if policy.skills_sources || policy.subagents_sources {
        let local = config::shared_profile_local_path()?;
        let shared = shared_profile_path(cfg, "skills_sources.json")?;
        sync_file_bidirectional(&local, &shared, &mut warnings, "skills_sources")?;
    }

    if policy.skills_repository {
        let local = local_skills_repository_root()?;
        let shared = shared_skills_repository_root(cfg)?;
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")?;
    }

    if policy.subagents_repository {
        let local = local_subagents_repository_root()?;
        let shared = shared_subagents_repository_root(cfg)?;
        sync_directory_bidirectional(&local, &shared, &mut warnings, "subagents_repository")?;
    }

    if policy.content {
        for name in ["notes", "bookmarks", "snippets"] {
            let local = StorageEngine::content_path(name)?;
            let shared = shared_content_path(cfg, &format!("{}.enc.json", name))?;
            sync_file_bidirectional(&local, &shared, &mut warnings, name)?;
        }
    }

    if policy.ai_news {
        let local = local_ai_news_path()?;
        let shared = shared_news_path(cfg, "ai_news.json")?;
        sync_file_bidirectional(&local, &shared, &mut warnings, "ai_news")?;
    }

    Ok(warnings)
}

fn emit_sync_status(app: &tauri::AppHandle, status: &str, message: Option<&str>) {
    let payload = json!({
        "status": status,
        "message": message.unwrap_or_default(),
    });
    let _ = app.emit("git-sync-status", payload);
}

async fn run_sync_pipeline(
    app: &tauri::AppHandle,
    cfg: config::StorageConfig,
) -> Result<Vec<String>, String> {
    if cfg.storage_type == "git" {
        emit_sync_status(app, "pulling", Some("Pulling from shared storage..."));
        let cfg_for_pull = cfg.clone();
        tauri::async_runtime::spawn_blocking(move || git::init_or_pull_git_repo(&cfg_for_pull))
            .await
            .map_err(|e| e.to_string())??;
    } else {
        emit_sync_status(app, "pulling", Some("Syncing local mirror..."));
    }

    let warnings = run_local_shared_sync(&cfg)?;

    if cfg.storage_type == "git" {
        emit_sync_status(app, "pushing", Some("Pushing local mirror updates..."));
        let cfg_for_push = cfg.clone();
        tauri::async_runtime::spawn_blocking(move || git::commit_and_push(&cfg_for_push))
            .await
            .map_err(|e| e.to_string())??;
    }

    Ok(warnings)
}

async fn process_sync_queue_impl(app: tauri::AppHandle, force_run: bool) -> Result<(), String> {
    if SYNC_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }
    let _guard = SyncRunningGuard;
    let mut outbox = load_outbox_state()?;
    let previous_sync_error = outbox.last_error.clone();
    outbox.running = true;
    outbox.last_status = "running".to_string();
    save_outbox_state(&outbox)?;

    let now = now_ts();
    let mut due = Vec::new();
    let mut keep = Vec::new();
    for ev in outbox.events.into_iter() {
        if ev.next_retry_at <= now {
            due.push(ev);
        } else {
            keep.push(ev);
        }
    }

    let should_run = force_run || !due.is_empty();
    let mut last_error = None;

    if should_run {
        let cfg = config::get_config()?;
        let storage_type = cfg.storage_type.clone();
        match run_sync_pipeline(&app, cfg).await {
            Ok(warnings) => {
                if !warnings.is_empty() {
                    eprintln!("sync warnings: {}", warnings.join(" | "));
                }
                emit_sync_status(&app, "success", Some("Synced successfully"));
                if previous_sync_error.is_some() {
                    messages::record_message_silent(
                        &app,
                        messages::MessageCreateInput {
                            source: "sync".to_string(),
                            category: "recovery".to_string(),
                            severity: "success".to_string(),
                            title: messages::localized("同步恢复成功", "Sync recovered"),
                            summary: Some(if messages::current_language_is_zh() {
                                format!("{} 同步已从失败状态恢复", storage_type)
                            } else {
                                format!("{} sync recovered from a failed state", storage_type)
                            }),
                            detail: previous_sync_error,
                            dedupe_key: Some(format!("sync:recovery:{}", storage_type)),
                            target: Some(messages::MessageTarget {
                                tab: "settings".to_string(),
                                section: Some("storage".to_string()),
                                entity_id: None,
                            }),
                            metadata: Some(json!({ "storage_type": storage_type })),
                        },
                    );
                }
                let _ = app.emit("refresh-ai-providers", ());
            }
            Err(err) => {
                last_error = Some(err.clone());
                emit_sync_status(&app, "error", Some(&err));
                messages::record_message_silent(
                    &app,
                    messages::MessageCreateInput {
                        source: "sync".to_string(),
                        category: "storage".to_string(),
                        severity: "error".to_string(),
                        title: messages::localized("同步失败", "Sync failed"),
                        summary: Some(err.clone()),
                        detail: Some(err.clone()),
                        dedupe_key: Some(format!("sync:error:{}", storage_type)),
                        target: Some(messages::MessageTarget {
                            tab: "settings".to_string(),
                            section: Some("storage".to_string()),
                            entity_id: None,
                        }),
                        metadata: Some(json!({ "storage_type": storage_type })),
                    },
                );
                for mut ev in due {
                    ev.attempts = ev.attempts.saturating_add(1);
                    let backoff = 2u64.saturating_pow(ev.attempts.min(8));
                    ev.next_retry_at = now_ts().saturating_add(backoff);
                    ev.last_error = Some(err.clone());
                    keep.push(ev);
                }
            }
        }
    } else {
        keep.extend(due);
    }

    outbox.events = keep;
    outbox.last_run_at = Some(now_ts());
    outbox.running = false;
    if let Some(err) = last_error {
        outbox.last_status = "error".to_string();
        outbox.last_error = Some(err);
    } else {
        outbox.last_status = "success".to_string();
        outbox.last_error = None;
    }
    save_outbox_state(&outbox)?;
    Ok(())
}

async fn process_sync_queue(app: tauri::AppHandle) -> Result<(), String> {
    process_sync_queue_impl(app, false).await
}

fn enqueue_sync_event(domain: &str, reason: &str) -> Result<(), String> {
    let mut outbox = load_outbox_state()?;
    let now = now_ts();

    let is_dup = outbox.events.iter().any(|e| {
        e.domain == domain
            && now.saturating_sub(e.created_at) <= OUTBOX_DEDUP_WINDOW_SECS
            && e.last_error.is_none()
    });

    if !is_dup {
        outbox.events.push(OutboxEvent {
            id: format!("evt-{}", uuid::Uuid::new_v4()),
            domain: domain.to_string(),
            reason: reason.to_string(),
            created_at: now,
            attempts: 0,
            next_retry_at: now,
            last_error: None,
        });
    }
    save_outbox_state(&outbox)
}

fn copy_if_exists(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(src, dst).map_err(|e| e.to_string())?;
    Ok(())
}

fn backup_legacy_files(backup_id: &str) -> Result<PathBuf, String> {
    let backup_root = StorageEngine::backup_root()?.join(backup_id);
    fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;

    let data_dir = crate::get_data_dir()?;
    let app_dir = config::get_app_dir()?;

    let files = vec![
        data_dir.join("ai_providers.json"),
        data_dir.join("ai_sessions.json"),
        data_dir.join("secrets.json"),
        data_dir.join("snippets.json"),
        data_dir.join("bookmarks.json"),
        data_dir.join("notes.json"),
        data_dir.join("mcp_servers.json"),
        app_dir.join("config.json"),
    ];

    for file in files {
        if file.exists() {
            let rel = file
                .file_name()
                .ok_or("invalid file")?
                .to_string_lossy()
                .to_string();
            copy_if_exists(&file, &backup_root.join(rel))?;
        }
    }

    Ok(backup_root)
}

fn build_new_providers_from_legacy() -> Result<ProvidersState, String> {
    let legacy = ai_env::get_ai_providers()?;
    let mut active = HashMap::new();
    if let Some(v) = legacy.active_claude {
        active.insert("claude".to_string(), v);
    }
    if let Some(v) = legacy.active_codex {
        active.insert("codex".to_string(), v);
    }
    if let Some(v) = legacy.active_gemini {
        active.insert("gemini".to_string(), v);
    }
    if let Some(v) = legacy.active_opencode {
        active.insert("opencode".to_string(), v);
    }

    let mut providers = Vec::new();
    for p in legacy.providers {
        let value = serde_json::to_value(&p).map_err(|e| e.to_string())?;
        let obj = value.as_object().cloned().unwrap_or_default();

        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let tool = obj
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let api_key = obj
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let base_url = obj
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let model = obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let is_enabled = obj.get("is_enabled").and_then(|v| v.as_bool());
        let provider_key = obj
            .get("provider_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Skip legacy auto-imported managed default providers when the corresponding CLI
        // binary is not installed on this machine.
        if is_managed_tool(&tool) && id == format!("default-{}", tool) {
            let (installed, _) = detect_cli_installation(&tool);
            if !installed {
                continue;
            }
        }

        let mut tool_config = Map::new();
        for (k, v) in &obj {
            match k.as_str() {
                "id"
                | "name"
                | "tool"
                | "api_key"
                | "base_url"
                | "model"
                | "is_enabled"
                | "provider_key"
                | "history"
                | "claude_haiku_model"
                | "claude_sonnet_model"
                | "claude_opus_model" => {}
                _ => {
                    tool_config.insert(k.clone(), v.clone());
                }
            }
        }

        if tool == "claude" {
            if !tool_config.contains_key("claude_model_mappings") {
                let mut legacy_tool_config = tool_config.clone();
                for legacy_key in [
                    "claude_haiku_model",
                    "claude_sonnet_model",
                    "claude_opus_model",
                ] {
                    if let Some(value) = obj.get(legacy_key) {
                        legacy_tool_config.insert(legacy_key.to_string(), value.clone());
                    }
                }
                let mappings = resolved_claude_model_mappings(&legacy_tool_config);
                if mappings
                    .iter()
                    .any(|mapping| !mapping.upstream_model.trim().is_empty())
                {
                    tool_config.insert(
                        "claude_model_mappings".to_string(),
                        serde_json::to_value(&mappings).unwrap_or_else(|_| Value::Array(vec![])),
                    );
                }
            }
            strip_legacy_claude_model_keys(&mut tool_config);
        }

        let mut history = Vec::new();
        if let Some(Value::Array(arr)) = obj.get("history") {
            for item in arr {
                if let Some(ts) = item.get("timestamp").and_then(|v| v.as_u64()) {
                    let summary = item
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    history.push(ProviderHistoryEntry {
                        ts: ts / 1000,
                        action: "legacy-import".to_string(),
                        summary,
                    });
                }
            }
        }

        providers.push(ProviderRecord {
            core: ProviderCore {
                id,
                name,
                tool,
                api_key,
                code: None,
                base_url,
                model,
            },
            runtime_policy: ProviderRuntimePolicy {
                approval_policy: tool_config
                    .get("approval_policy")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                sandbox_mode: tool_config
                    .get("sandbox_mode")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            },
            favorite_at: obj.get("favorite_at").and_then(|v| v.as_u64()),
            tool_config,
            history,
            extra: Map::new(),
            is_enabled,
            provider_key,
        });
    }

    // Keep active bindings only when the provider still exists after filtering.
    active.retain(|tool, provider_id| {
        providers
            .iter()
            .any(|p| p.core.tool == *tool && p.core.id == *provider_id)
    });

    Ok(ProvidersState { active, providers })
}

fn build_new_sessions_from_legacy() -> Result<SessionsState, String> {
    let legacy = ai_sessions::get_ai_sessions()?;
    let sessions = legacy
        .into_iter()
        .map(|s| SessionRecord {
            id: s.id,
            name: s.name,
            working_dir: s.working_dir,
            tool: s.model_type,
            tool_session_id: s.tool_session_id,
            model_name: None,
            name_source: "manual".to_string(),
            runtime_mode: "shared".to_string(),
            runtime_profile_id: None,
            preset_id: None,
            created_at: s.created_at,
            last_used_at: s.created_at,
            status: "active".to_string(),
            favorited_at: None,
            provider_id: None,
        })
        .collect();
    Ok(SessionsState {
        sessions,
        ..SessionsState::default()
    })
}

fn migrate_content_file(read: fn() -> Result<String, String>, name: &str) -> Result<(), String> {
    let content = read()?;
    let parsed: Value = serde_json::from_str(&content).unwrap_or_else(|_| Value::Array(vec![]));
    let encrypted = CryptoService::encrypt_json(&parsed)?;
    StorageEngine::write_json(&StorageEngine::content_path(name)?, &encrypted)
}

fn migrate_secrets() -> Result<(), String> {
    let data_dir = crate::get_data_dir()?;
    let legacy_path = data_dir.join("secrets.json");
    if !legacy_path.exists() {
        let empty = CryptoService::encrypt_json(&json!({}))?;
        return StorageEngine::write_json(&StorageEngine::secrets_path()?, &empty);
    }

    let content = fs::read_to_string(&legacy_path).map_err(|e| e.to_string())?;
    let mut legacy: secrets::Secrets = serde_json::from_str(&content).unwrap_or_default();

    let mut map = Map::new();
    if legacy.is_encrypted {
        for (k, v) in legacy.values.drain() {
            let dec = CryptoService::decrypt(&v).unwrap_or(v);
            map.insert(k, Value::String(dec));
        }
    } else {
        for (k, v) in legacy.values.drain() {
            map.insert(k, Value::String(v));
        }
    }

    let encrypted = CryptoService::encrypt_json(&Value::Object(map))?;
    StorageEngine::write_json(&StorageEngine::secrets_path()?, &encrypted)
}

fn migrate_mcp() -> Result<(), String> {
    let mut state = mcp_servers::get_mcp_servers().unwrap_or_default();
    state.is_encrypted = true;
    for server in state.servers.iter_mut() {
        let _ = mcp_servers::encrypt_sensitive_data(server);
    }
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    StorageEngine::write_json(&StorageEngine::mcp_path()?, &value)
}

fn migrate_config_shadow() -> Result<(), String> {
    let mut cfg = config::get_config()?;
    cfg.http_token = None;
    if let Some(ref mut proxy) = cfg.proxy {
        proxy.proxy_password = None;
    }
    let value = serde_json::to_value(cfg).map_err(|e| e.to_string())?;
    let path = StorageEngine::meta_dir()?.join("config_shadow.json");
    StorageEngine::write_json(&path, &value)
}

fn write_migration_report(report: &MigrationReport) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::migration_report_path()?, report)
}

fn run_migration_impl() -> Result<MigrationState, String> {
    let mut state = load_migration_state().unwrap_or_default();
    let schema = StorageEngine::load_schema().unwrap_or_default();
    if state.migrated && schema.schema_version == SCHEMA_VERSION {
        return Ok(state);
    }

    state.in_progress = true;
    state.last_error = None;
    save_migration_state(&state)?;

    let started = now_ts();
    let backup_id = format!("backup-{}", started);
    let mut steps = Vec::new();

    let result = (|| -> Result<(), String> {
        let _backup_dir = backup_legacy_files(&backup_id)?;
        steps.push("backup".to_string());

        migrate_config_shadow()?;
        steps.push("config".to_string());

        let providers = build_new_providers_from_legacy()?;
        let providers_blob = CryptoService::encrypt_json(
            &serde_json::to_value(&providers).map_err(|e| e.to_string())?,
        )?;
        StorageEngine::write_json(&StorageEngine::providers_path()?, &providers_blob)?;
        steps.push("providers".to_string());

        let sessions = build_new_sessions_from_legacy()?;
        let sessions_blob = CryptoService::encrypt_json(
            &serde_json::to_value(&sessions).map_err(|e| e.to_string())?,
        )?;
        StorageEngine::write_json(&StorageEngine::sessions_path()?, &sessions_blob)?;
        steps.push("sessions".to_string());

        migrate_secrets()?;
        steps.push("secrets".to_string());

        migrate_mcp()?;
        steps.push("mcp".to_string());

        migrate_content_file(storage::read_snippets, "snippets")?;
        migrate_content_file(storage::read_bookmarks, "bookmarks")?;
        migrate_content_file(storage::read_notes, "notes")?;
        steps.push("content".to_string());

        let mut schema = StorageEngine::load_schema()?;
        schema.schema_version = SCHEMA_VERSION;
        schema.last_migrated_at = now_ts();
        schema.revision = schema.revision.saturating_add(1);
        StorageEngine::write_json(&StorageEngine::schema_path()?, &schema)?;

        let outbox = OutboxState::default();
        save_outbox_state(&outbox)?;

        Ok(())
    })();

    let finished = now_ts();

    match result {
        Ok(_) => {
            state.migrated = true;
            state.schema_version = SCHEMA_VERSION;
            state.last_migrated_at = Some(finished);
            state.last_backup_id = Some(backup_id.clone());
            state.in_progress = false;
            state.last_error = None;
            save_migration_state(&state)?;

            write_migration_report(&MigrationReport {
                started_at: started,
                finished_at: finished,
                success: true,
                backup_id,
                steps,
                error: None,
            })?;
            Ok(state)
        }
        Err(err) => {
            state.in_progress = false;
            state.last_error = Some(err.clone());
            save_migration_state(&state)?;

            let _ = write_migration_report(&MigrationReport {
                started_at: started,
                finished_at: finished,
                success: false,
                backup_id,
                steps,
                error: Some(err.clone()),
            });
            Err(err)
        }
    }
}

fn rollback_from_backup(backup_id: &str) -> Result<(), String> {
    let backup_dir = StorageEngine::backup_root()?.join(backup_id);
    if !backup_dir.exists() {
        return Err("Backup not found".to_string());
    }

    let data_dir = crate::get_data_dir()?;
    let app_dir = config::get_app_dir()?;

    for file in [
        "ai_providers.json",
        "ai_sessions.json",
        "secrets.json",
        "snippets.json",
        "bookmarks.json",
        "notes.json",
        "mcp_servers.json",
    ] {
        let src = backup_dir.join(file);
        let dst = data_dir.join(file);
        if src.exists() {
            copy_if_exists(&src, &dst)?;
        }
    }

    let cfg = backup_dir.join("config.json");
    if cfg.exists() {
        copy_if_exists(&cfg, &app_dir.join("config.json"))?;
    }

    Ok(())
}

fn cleanup_legacy_root_files() -> Result<(), String> {
    let data_dir = crate::get_data_dir()?;
    let checks = vec![
        (
            data_dir.join("ai_providers.json"),
            StorageEngine::providers_path()?,
        ),
        (
            data_dir.join("ai_sessions.json"),
            StorageEngine::sessions_path()?,
        ),
        (
            data_dir.join("secrets.json"),
            StorageEngine::secrets_path()?,
        ),
        (
            data_dir.join("snippets.json"),
            StorageEngine::content_path("snippets")?,
        ),
        (
            data_dir.join("bookmarks.json"),
            StorageEngine::content_path("bookmarks")?,
        ),
        (
            data_dir.join("notes.json"),
            StorageEngine::content_path("notes")?,
        ),
        (
            data_dir.join("mcp_servers.json"),
            StorageEngine::mcp_path()?,
        ),
    ];

    for (legacy, new_path) in checks {
        if legacy.exists() && new_path.exists() {
            let _ = fs::remove_file(legacy);
        }
    }
    Ok(())
}

fn rotate_encrypted_blob_file(path: &Path, old_pass: &str, new_pass: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(());
    }

    let plain_json = if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if blob.is_encrypted {
            match crate::crypto::decrypt(&blob.data, old_pass) {
                Ok(plain) => plain,
                Err(err) => {
                    // Do not fail the whole password-change flow for one incompatible/corrupted file.
                    // If payload itself looks like JSON, treat it as legacy plain content; otherwise skip.
                    if serde_json::from_str::<Value>(&blob.data).is_ok() {
                        eprintln!(
                            "rotate_encrypted_blob_file: decrypt failed but blob data is plain JSON, path={}, err={}",
                            path.display(),
                            err
                        );
                        blob.data
                    } else {
                        eprintln!(
                            "rotate_encrypted_blob_file: skip file due to decrypt failure, path={}, err={}",
                            path.display(),
                            err
                        );
                        return Ok(());
                    }
                }
            }
        } else {
            blob.data
        }
    } else {
        content
    };

    let parsed: Value = match serde_json::from_str(&plain_json) {
        Ok(v) => v,
        Err(err) => {
            eprintln!(
                "rotate_encrypted_blob_file: skip file due to invalid json, path={}, err={}",
                path.display(),
                err
            );
            return Ok(());
        }
    };
    let encrypted = crate::crypto::encrypt(&parsed.to_string(), new_pass)?;
    let blob = EncryptedBlob {
        is_encrypted: true,
        data: encrypted,
    };
    StorageEngine::write_json(path, &blob)
}

fn rotate_mcp_state_password(old_pass: &str, new_pass: &str) -> Result<(), String> {
    let path = StorageEngine::mcp_path()?;
    if !path.exists() {
        return Ok(());
    }
    let mut state: mcp_servers::MCPServersState = StorageEngine::read_json(&path)?;
    if state.servers.is_empty() {
        return Ok(());
    }

    for server in state.servers.iter_mut() {
        if let Some(ref mut env) = server.env {
            for (_, value) in env.iter_mut() {
                if value.is_empty() || value.starts_with('$') || value.starts_with("${") {
                    continue;
                }
                let plain =
                    crate::crypto::decrypt(value, old_pass).unwrap_or_else(|_| value.clone());
                *value = crate::crypto::encrypt(&plain, new_pass)?;
            }
        }

        if let Some(ref mut headers) = server.headers {
            for (key, value) in headers.iter_mut() {
                let k = key.to_lowercase();
                if !(k.contains("auth")
                    || k.contains("key")
                    || k.contains("token")
                    || k.contains("secret"))
                {
                    continue;
                }
                if value.is_empty() || value.starts_with('$') || value.starts_with("${") {
                    continue;
                }
                let plain =
                    crate::crypto::decrypt(value, old_pass).unwrap_or_else(|_| value.clone());
                *value = crate::crypto::encrypt(&plain, new_pass)?;
            }
        }
    }
    state.is_encrypted = true;
    StorageEngine::write_json(&path, &state)
}

pub fn rotate_master_password_data(old_pass: &str, new_pass: &str) -> Result<(), String> {
    rotate_encrypted_blob_file(&StorageEngine::providers_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::sessions_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::launcher_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::secrets_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(
        &StorageEngine::content_path("snippets")?,
        old_pass,
        new_pass,
    )?;
    rotate_encrypted_blob_file(
        &StorageEngine::content_path("bookmarks")?,
        old_pass,
        new_pass,
    )?;
    rotate_encrypted_blob_file(&StorageEngine::content_path("notes")?, old_pass, new_pass)?;
    rotate_mcp_state_password(old_pass, new_pass)?;
    Ok(())
}

pub fn ensure_migrated_on_startup() -> Result<(), String> {
    run_migration_impl().map(|_| ())?;
    // Keep startup side-effects minimal: do not create a new local key before onboarding.
    let key_path = crate::crypto::get_local_key_path()?;
    if key_path.exists() {
        let pass = crate::crypto::get_or_init_master_password()?;
        rotate_master_password_data(&pass, &pass)?;
    }
    cleanup_legacy_root_files()?;
    Ok(())
}

#[tauri::command]
pub fn storage_get_snapshot() -> Result<ApiOk<AppSnapshot>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let providers = service_providers_to_legacy_view(
        &load_service_providers_state().map_err(|e| api_error("io_error", e))?,
    );
    let sessions = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let cfg = config::get_storage_config().map_err(|e| api_error("config_error", e))?;
    let schema = StorageEngine::load_schema().map_err(|e| api_error("io_error", e))?;
    let outbox = load_outbox_state().map_err(|e| api_error("io_error", e))?;

    api_ok(
        AppSnapshot {
            providers: serde_json::to_value(providers)
                .map_err(|e| api_error("serialize_error", e.to_string()))?,
            sessions: Value::Array(sessions.sessions.iter().map(session_to_legacy).collect()),
            config: serde_json::to_value(cfg)
                .map_err(|e| api_error("serialize_error", e.to_string()))?,
            schema,
            outbox,
        },
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn providers_list() -> Result<ApiOk<LegacyProvidersView>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let legacy_state = service_providers_to_provider_state(&state);
    let _ = write_legacy_cli_providers_snapshot(&legacy_state);
    api_ok(
        service_providers_to_legacy_view(&state),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn providers_list_synced_other_devices() -> Result<ApiOk<Vec<SyncedDeviceProvidersView>>, ApiErr>
{
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let cfg = config::get_storage_config().map_err(|e| api_error("config_error", e))?;

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(root) = config::resolve_shared_storage_root(&cfg) {
        roots.push(root);
    }
    if let Ok(shared) = config::get_shared_data_dir_for(&cfg) {
        if !roots.iter().any(|p| p == &shared) {
            roots.push(shared);
        }
    }

    let current_device = normalize_device_label(&crate::get_hostname());
    let mut seen_devices: HashSet<String> = HashSet::new();
    let mut devices: Vec<SyncedDeviceProvidersView> = Vec::new();
    let skip_dirs: HashSet<&str> = [
        "shared", "profile", "content", "meta", "data", "backup", "backups", ".git",
    ]
    .into_iter()
    .collect();

    for root in roots {
        if !root.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let device_id = entry.file_name().to_string_lossy().trim().to_string();
            if device_id.is_empty() {
                continue;
            }
            let normalized = normalize_device_label(&device_id);
            if normalized.is_empty()
                || normalized == current_device
                || skip_dirs.contains(normalized.as_str())
                || seen_devices.contains(&normalized)
            {
                continue;
            }

            let mut matched: Option<(usize, SyncedDeviceProvidersView)> = None;
            for candidate in provider_snapshot_candidates(&path) {
                if !candidate.exists() {
                    continue;
                }
                let Some(value) = read_provider_snapshot_value(&candidate) else {
                    continue;
                };
                let Some(root_obj) = value.as_object() else {
                    continue;
                };
                let providers = extract_providers_from_snapshot(root_obj);
                if providers.is_empty() {
                    continue;
                }
                let active = extract_active_map_from_snapshot(root_obj);
                let score = provider_snapshot_quality_score(&providers, &active);
                let view = SyncedDeviceProvidersView {
                    device_id: device_id.clone(),
                    active,
                    providers,
                };
                match &matched {
                    Some((best_score, _)) if *best_score >= score => {}
                    _ => matched = Some((score, view)),
                }
            }

            if let Some((_, view)) = matched {
                seen_devices.insert(normalized);
                devices.push(view);
            }
        }
    }

    devices.sort_by(|a, b| a.device_id.to_lowercase().cmp(&b.device_id.to_lowercase()));
    api_ok(devices, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn dashboard_counts() -> Result<ApiOk<DashboardCounts>, ApiErr> {
    run_migration_impl().map_err(|e| api_error("migration_failed", e))?;
    let counts = tauri::async_runtime::spawn_blocking(compute_dashboard_counts)
        .await
        .map_err(|e| api_error("task_join_error", e.to_string()))?
        .map_err(|e| api_error("io_error", e))?;
    api_ok(counts, get_meta().map_err(|e| api_error("io_error", e))?)
}

fn compute_dashboard_counts() -> Result<DashboardCounts, String> {
    let launcher = load_launcher_state().map(|s| s.items.len())?;
    let workspaces = workspaces::workspace_count_fast().unwrap_or(0);
    let sessions_state = load_sessions_state()?;
    let sessions = filter_sessions_by_history_window(sessions_state.sessions.iter()).len();

    let environments = load_service_providers_state().map(|s| s.providers.len())?;

    let ssh = crate::get_ssh_hosts().map(|hosts| hosts.len()).unwrap_or(0);
    let snippets = storage::read_snippets()
        .map(|raw| parse_json_array_len(&raw))
        .unwrap_or(0);
    let bookmarks = storage::read_bookmarks()
        .map(|raw| parse_json_array_len(&raw))
        .unwrap_or(0);
    let notes = storage::read_notes()
        .map(|raw| parse_json_array_len(&raw))
        .unwrap_or(0);
    let ai_news = crate::ai_news::ai_news_count_fast().unwrap_or(0);
    let skills = crate::skills::skills_installed_count_all_scopes().unwrap_or(0);
    let subagents = crate::subagents::subagents_installed_asset_count_all_scopes().unwrap_or(0);
    let mcp_servers = crate::mcp_servers::get_mcp_servers_count_fast().unwrap_or(0);
    let storage_type = config::get_storage_config()
        .ok()
        .map(|cfg| cfg.storage_type);

    Ok(DashboardCounts {
        launcher,
        workspaces,
        sessions,
        ssh,
        snippets,
        bookmarks,
        notes,
        ai_news,
        environments,
        skills,
        subagents,
        mcp_servers,
        storage_type,
    })
}

#[tauri::command]
pub async fn cli_env_probe(tool: String) -> Result<ApiOk<CliEnvProbeResult>, ApiErr> {
    let probe_tool = tool.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let (installed, version) = detect_cli_installation(&probe_tool);
        let configured = cli_has_system_config(&probe_tool);
        CliEnvProbeResult {
            tool: probe_tool.clone(),
            installed,
            version,
            configured,
            importable: is_managed_tool(&probe_tool) && installed && configured,
            install_guide: install_guide_for(&probe_tool),
        }
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?;

    api_ok(result, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn providers_auto_import_from_system(
    app: tauri::AppHandle,
    tool: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    if !is_managed_tool(&tool) {
        return Err(api_error(
            "invalid_tool",
            "tool does not support env managed import",
        ));
    }

    let (installed, _) = detect_cli_installation(&tool);
    if !installed {
        return api_ok(
            json!({ "imported": false, "reason": "not_installed" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }
    if !cli_has_system_config(&tool) {
        return api_ok(
            json!({ "imported": false, "reason": "not_configured" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let mut state = load_providers_state().map_err(|e| api_error("io_error", e))?;
    let default_id = format!("default-{}", tool);
    if state.active.get(&tool).is_some() {
        return api_ok(
            json!({ "imported": false, "reason": "active_exists" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }
    if state.providers.iter().any(|p| p.core.id == default_id) {
        return api_ok(
            json!({ "imported": false, "reason": "provider_exists" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let provider = read_system_provider(&tool).ok_or_else(|| {
        api_error(
            "import_failed",
            "failed to parse system config for selected tool",
        )
    })?;
    let api_key = provider.core.api_key.trim();
    let base_url = provider
        .core
        .base_url
        .as_deref()
        .map(|v| v.trim())
        .unwrap_or("");
    let mut missing_fields: Vec<&str> = Vec::new();
    if api_key.is_empty() {
        missing_fields.push("api_key");
    }
    if base_url.is_empty() {
        missing_fields.push("base_url");
    }
    let should_activate = missing_fields.is_empty();
    let provider_id = provider.core.id.clone();
    state.providers.push(provider);
    if should_activate {
        state.active.insert(tool.clone(), provider_id.clone());
    }
    let schema = save_providers_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("providers", "auto_import_system_config")
        .map_err(|e| api_error("sync_error", e))?;

    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({
            "imported": true,
            "provider_id": provider_id,
            "tool": tool,
            "activated": should_activate,
            "missing_fields": missing_fields
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn providers_set_env_managed(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
    enabled: bool,
) -> Result<ApiOk<Value>, ApiErr> {
    if !is_managed_tool(&tool) {
        return Err(api_error(
            "invalid_tool",
            "tool does not support env managed switch",
        ));
    }
    service_providers_set_env_managed(app, provider_id, enabled).await
}

#[tauri::command]
pub async fn providers_upsert(
    app: tauri::AppHandle,
    provider: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    service_providers_upsert(app, provider).await
}

#[tauri::command]
pub async fn providers_delete(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    service_providers_delete(app, provider_id).await
}

#[tauri::command]
pub async fn providers_set_active(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    service_providers_set_active(app, tool, provider_id).await
}

#[tauri::command]
pub fn claude_profile_list() -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let legacy_state = ProvidersState {
        active: state.active.clone(),
        providers: state
            .providers
            .iter()
            .map(service_provider_to_provider_record)
            .collect(),
    };
    let profiles = crate::claude_profiles::list_claude_profiles(&legacy_state);
    api_ok(
        serde_json::to_value(profiles).map_err(|e| api_error("serialize_error", e.to_string()))?,
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn claude_profile_resolve(query: String) -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let legacy_state = ProvidersState {
        active: state.active.clone(),
        providers: state
            .providers
            .iter()
            .map(service_provider_to_provider_record)
            .collect(),
    };
    let profile = crate::claude_profiles::resolve_claude_profile(&legacy_state, &query)
        .ok_or_else(|| api_error("not_found", format!("Claude profile not found: {query}")))?;
    let config_dir = crate::claude_profiles::get_claude_profiles_dir()
        .map(|d| d.join(crate::claude_profiles::resolve_claude_dir_name(&profile)))
        .map_err(|e| api_error("io_error", e))?;
    let mut obj = serde_json::to_value(&profile)
        .map_err(|e| api_error("serialize_error", e.to_string()))?
        .as_object()
        .cloned()
        .unwrap_or_default();
    obj.insert(
        "claude_config_dir".to_string(),
        Value::String(config_dir.to_string_lossy().to_string()),
    );
    api_ok(
        Value::Object(obj),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn claude_profile_set_default(profile_id: String) -> Result<ApiOk<Value>, ApiErr> {
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let exists = state
        .providers
        .iter()
        .any(|p| p.id == profile_id && p.tool == "claude");
    if !exists {
        return Err(api_error(
            "invalid_payload",
            format!("Claude service provider not found: {profile_id}"),
        ));
    }
    state
        .active
        .insert("claude".to_string(), profile_id.clone());
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    api_ok(
        json!({ "profile_id": profile_id, "set_default": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn get_claude_config_dir(provider_id: String) -> Result<String, String> {
    resolve_claude_config_dir_for_provider_id(&provider_id).map(|d| d.to_string_lossy().to_string())
}

#[tauri::command]
pub fn claude_profile_materialize(provider_id: String) -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == "claude")
        .cloned()
        .ok_or_else(|| {
            api_error(
                "not_found",
                format!("Claude service provider not found: {provider_id}"),
            )
        })?;
    let legacy_provider = service_provider_to_provider_record(&provider);
    let profile_dir = crate::claude_profiles::get_claude_profiles_dir()
        .map(|d| {
            d.join(crate::claude_profiles::resolve_claude_dir_name(
                &legacy_provider,
            ))
        })
        .map_err(|e| api_error("profile_failed", e))?;
    crate::claude_profiles::materialize_claude_settings_sp(&provider, &profile_dir)
        .map_err(|e| api_error("profile_failed", e))?;
    api_ok(
        json!({ "materialized": true, "config_dir": profile_dir.to_string_lossy().to_string() }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn providers_export(output_path: String) -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let legacy = service_providers_to_legacy_view(&state);
    let payload = json!({
        "format": "onespace-service-providers",
        "version": PROVIDERS_EXPORT_VERSION,
        "exported_at": now_ts(),
        "active": state.active,
        "active_claude": legacy.active_claude,
        "active_codex": legacy.active_codex,
        "active_gemini": legacy.active_gemini,
        "active_opencode": legacy.active_opencode,
        "providers": legacy.providers,
    });

    let content = serde_json::to_string_pretty(&payload)
        .map_err(|e| api_error("serialize_error", e.to_string()))?;
    let expanded_output_path =
        expand_home_dir_path(&output_path).map_err(|e| api_error("io_error", e))?;
    let final_output_path = if expanded_output_path.is_dir() {
        expanded_output_path.join("onespace-ai-environments-export.json")
    } else {
        expanded_output_path
    };
    StorageEngine::atomic_write(&final_output_path, &content)
        .map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({
            "path": final_output_path.to_string_lossy().to_string(),
            "count": payload
                .get("providers")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0)
        }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn providers_import_preview(
    import_path: String,
) -> Result<ApiOk<ProvidersImportPreview>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let service_state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let state = service_providers_to_provider_state(&service_state);
    let import_path = expand_home_dir_path(&import_path)
        .map_err(|e| api_error("invalid_payload", e))?
        .to_string_lossy()
        .to_string();
    let (active, providers) = parse_providers_import_payload(&import_path)
        .map_err(|e| api_error("invalid_payload", e))?;
    let candidates = collect_provider_import_candidates(&state, &providers)
        .map_err(|e| api_error("invalid_payload", e))?;

    api_ok(
        providers_import_preview_from_candidates(active, &candidates),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn providers_import_apply(
    app: tauri::AppHandle,
    import_path: String,
    decisions: Vec<ProviderImportDecision>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let service_state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let mut state = service_providers_to_provider_state(&service_state);
    let import_path = expand_home_dir_path(&import_path)
        .map_err(|e| api_error("invalid_payload", e))?
        .to_string_lossy()
        .to_string();
    let (active_map, providers) = parse_providers_import_payload(&import_path)
        .map_err(|e| api_error("invalid_payload", e))?;
    let candidates = collect_provider_import_candidates(&state, &providers)
        .map_err(|e| api_error("invalid_payload", e))?;

    let decision_map =
        decisions
            .into_iter()
            .try_fold(HashMap::<String, String>::new(), |mut acc, decision| {
                let action = decision.action.trim().to_lowercase();
                if action != "overwrite" && action != "new" {
                    return Err(api_error(
                        "invalid_payload",
                        format!("invalid import action: {}", decision.action),
                    ));
                }
                acc.insert(decision.import_key, action);
                Ok(acc)
            })?;

    let mut final_id_map: HashMap<String, String> = HashMap::new();
    let mut overwritten = 0usize;
    let mut created = 0usize;

    for candidate in candidates {
        let mut input = candidate.input.clone();
        let action = if candidate.conflict.is_some() {
            decision_map
                .get(&candidate.import_key)
                .map(|v| v.as_str())
                .ok_or_else(|| {
                    api_error(
                        "invalid_payload",
                        format!("missing import decision for {}", candidate.import_key),
                    )
                })?
        } else {
            "new"
        };

        let final_id = if let Some(conflict) = &candidate.conflict {
            if action == "overwrite" {
                let target_id = conflict.existing_id.clone();
                let Some(pos) = state.providers.iter().position(|p| p.core.id == target_id) else {
                    return Err(api_error(
                        "not_found",
                        format!("provider to overwrite not found: {}", target_id),
                    ));
                };
                input.id = target_id.clone();
                let old_record = state.providers.get(pos).cloned();
                let record = provider_from_input(input, old_record.as_ref());
                state.providers[pos] = record;
                overwritten = overwritten.saturating_add(1);
                target_id
            } else {
                if state.providers.iter().any(|p| p.core.id == input.id) {
                    input.id = make_imported_provider_id(&state, &input.id);
                }
                let final_id = input.id.clone();
                let record = provider_from_input(input, None);
                state.providers.push(record);
                created = created.saturating_add(1);
                final_id
            }
        } else {
            if state.providers.iter().any(|p| p.core.id == input.id) {
                input.id = make_imported_provider_id(&state, &input.id);
            }
            let final_id = input.id.clone();
            let record = provider_from_input(input, None);
            state.providers.push(record);
            created = created.saturating_add(1);
            final_id
        };

        final_id_map.insert(candidate.import_key, final_id);
    }

    let mut active_restored = 0usize;
    for (tool, imported_provider_id) in active_map {
        let key = provider_import_key(&tool, &imported_provider_id);
        if let Some(final_id) = final_id_map.get(&key) {
            state.active.insert(tool, final_id.clone());
            active_restored = active_restored.saturating_add(1);
        }
    }
    state.active.retain(|tool, provider_id| {
        state
            .providers
            .iter()
            .any(|p| p.core.tool == *tool && p.core.id == *provider_id)
    });

    let next_service_state = migrate_providers_to_service_providers(state.clone());
    let schema = save_service_providers_internal(&next_service_state)
        .map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "providers_import_apply")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({
            "imported": overwritten.saturating_add(created),
            "overwritten": overwritten,
            "created": created,
            "active_restored": active_restored,
            "total": state.providers.len(),
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

// ─── Service Providers commands (new unified domain) ───────────────────────────

fn service_provider_to_value(sp: &ServiceProviderRecord) -> Value {
    let mut obj = json!({
        "id": sp.id,
        "name": sp.name,
        "tool": sp.tool,
        "api_key": sp.api_key,
    });
    if let Some(ref v) = sp.icon {
        obj["icon"] = json!(v);
    }
    if let Some(ref v) = sp.base_url {
        obj["base_url"] = json!(v);
    }
    if let Some(ref v) = sp.model {
        obj["model"] = json!(v);
        if sp.tool == "claude" {
            obj["claude_default_model"] = json!(v);
        }
    } else if sp.tool == "claude" {
        obj["claude_default_model"] = Value::Null;
    }
    if let Some(ref v) = sp.code {
        obj["code"] = json!(v);
    }
    if let Some(ref v) = sp.is_enabled {
        obj["is_enabled"] = json!(v);
    }
    if let Some(ref v) = sp.provider_key {
        obj["provider_key"] = json!(v);
    }
    if let Some(ref v) = sp.env_managed {
        obj["env_managed"] = json!(v);
    }
    if let Some(v) = sp.favorite_at {
        obj["favorite_at"] = json!(v);
    }
    if !sp.claude_model_mappings.is_empty() {
        obj["claude_model_mappings"] =
            serde_json::to_value(&sp.claude_model_mappings).unwrap_or(json!([]));
    }
    if let Some(ref v) = sp.claude_enable_tool_search {
        obj["claude_enable_tool_search"] = json!(v);
    }
    if let Some(ref v) = sp.claude_auto_memory_enabled {
        obj["claude_auto_memory_enabled"] = json!(v);
    }
    if let Some(ref v) = sp.claude_always_thinking_enabled {
        obj["claude_always_thinking_enabled"] = json!(v);
    }
    if let Some(ref v) = sp.claude_away_summary_enabled {
        obj["claude_away_summary_enabled"] = json!(v);
    }
    if let Some(ref v) = sp.claude_include_git_instructions {
        obj["claude_include_git_instructions"] = json!(v);
    }
    if let Some(ref v) = sp.claude_enable_attribution {
        obj["claude_enable_attribution"] = json!(v);
    }
    if sp.claude_api_format != "anthropic_messages" {
        obj["claude_api_format"] = json!(sp.claude_api_format);
    }
    if sp.claude_connection_mode != "native_anthropic" {
        obj["claude_connection_mode"] = json!(sp.claude_connection_mode);
    }
    if let Some(ref v) = sp.protocol_router_upstream_provider_id {
        obj["protocol_router_upstream_provider_id"] = json!(v);
    }
    if sp.protocol_router_wire_api != "open_ai_chat" {
        obj["protocol_router_wire_api"] = json!(sp.protocol_router_wire_api);
    }
    if sp.claude_auth_env_key != "ANTHROPIC_API_KEY" {
        obj["claude_auth_env_key"] = json!(sp.claude_auth_env_key);
    }
    if let Some(ref v) = sp.fetched_models {
        obj["fetched_models"] = json!(v);
    }
    // Pass through tool_config for backward compatibility
    if !sp.tool_config.is_empty() {
        obj["tool_config"] = serde_json::to_value(&sp.tool_config).unwrap_or(json!({}));
    }
    obj
}

fn service_provider_from_value(
    val: Value,
    existing: Option<&ServiceProviderRecord>,
) -> ServiceProviderRecord {
    let obj = val.as_object().cloned().unwrap_or_default();
    let explicit_empty_claude_default_model = obj
        .get("claude_default_model")
        .map(|value| match value {
            Value::String(s) => s.trim().is_empty(),
            Value::Null => true,
            _ => false,
        })
        .unwrap_or(false);
    let mut tool_config = existing.map(|e| e.tool_config.clone()).unwrap_or_default();
    // Merge any fields from the input that aren't top-level fields
    let top_level_keys: HashSet<&str> = [
        "id",
        "name",
        "tool",
        "api_key",
        "icon",
        "base_url",
        "model",
        "code",
        "is_enabled",
        "provider_key",
        "env_managed",
        "favorite_at",
        "claude_api_format",
        "claude_connection_mode",
        "protocol_router_upstream_provider_id",
        "protocol_router_wire_api",
        "claude_auth_env_key",
        "claude_default_model",
        "claude_reasoning_effort",
        "claude_model_mappings",
        "claude_enable_tool_search",
        "claude_auto_memory_enabled",
        "claude_always_thinking_enabled",
        "claude_away_summary_enabled",
        "claude_include_git_instructions",
        "claude_enable_attribution",
        "fetched_models",
        "tool_config",
    ]
    .into_iter()
    .collect();
    for (k, v) in &obj {
        if !top_level_keys.contains(k.as_str()) {
            tool_config.insert(k.clone(), v.clone());
        }
    }
    // If tool_config was explicitly provided, merge it
    if let Some(tc) = obj.get("tool_config").and_then(|v| v.as_object()) {
        for (k, v) in tc {
            tool_config.insert(k.clone(), v.clone());
        }
    }
    for mirrored_key in ["claude_default_model", "claude_reasoning_effort"] {
        if let Some(value) = obj.get(mirrored_key) {
            let keep_value = match value {
                Value::String(s) => !s.trim().is_empty(),
                Value::Null => false,
                _ => true,
            };
            if keep_value {
                tool_config.insert(mirrored_key.to_string(), value.clone());
            } else {
                tool_config.remove(mirrored_key);
            }
        }
    }

    let mut record = ServiceProviderRecord {
        id: obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        name: obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        tool: obj
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        icon: obj
            .get("icon")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        api_key: obj
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        base_url: obj
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model: obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        claude_api_format: infer_claude_api_format(
            obj.get("claude_api_format")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("claude_api_format"))
                        .and_then(|v| v.as_str())
                }),
            obj.get("claude_connection_mode")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("claude_connection_mode"))
                        .and_then(|v| v.as_str())
                }),
            obj.get("protocol_router_wire_api")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("wire_api").and_then(|v| v.as_str()))
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("wire_api"))
                        .and_then(|v| v.as_str())
                })
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("protocol_router_wire_api"))
                        .and_then(|v| v.as_str())
                }),
        ),
        claude_connection_mode: "native_anthropic".to_string(),
        protocol_router_upstream_provider_id: obj
            .get("protocol_router_upstream_provider_id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        protocol_router_wire_api: infer_protocol_router_wire_api(
            obj.get("protocol_router_wire_api")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("wire_api").and_then(|v| v.as_str()))
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("wire_api"))
                        .and_then(|v| v.as_str())
                })
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("protocol_router_wire_api"))
                        .and_then(|v| v.as_str())
                }),
            obj.get("claude_api_format")
                .and_then(|v| v.as_str())
                .unwrap_or("anthropic_messages"),
            obj.get("claude_connection_mode")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("claude_connection_mode"))
                        .and_then(|v| v.as_str())
                }),
        ),
        claude_auth_env_key: obj
            .get("claude_auth_env_key")
            .and_then(|v| v.as_str())
            .unwrap_or("ANTHROPIC_API_KEY")
            .to_string(),
        claude_model_mappings: obj
            .get("claude_model_mappings")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        claude_enable_tool_search: obj
            .get("claude_enable_tool_search")
            .and_then(|v| v.as_bool()),
        claude_auto_memory_enabled: obj
            .get("claude_auto_memory_enabled")
            .and_then(|v| v.as_bool()),
        claude_always_thinking_enabled: obj
            .get("claude_always_thinking_enabled")
            .and_then(|v| v.as_bool()),
        claude_away_summary_enabled: obj
            .get("claude_away_summary_enabled")
            .and_then(|v| v.as_bool()),
        claude_include_git_instructions: obj
            .get("claude_include_git_instructions")
            .and_then(|v| v.as_bool()),
        claude_enable_attribution: obj
            .get("claude_enable_attribution")
            .and_then(|v| v.as_bool()),
        code: obj
            .get("code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        is_enabled: obj.get("is_enabled").and_then(|v| v.as_bool()),
        provider_key: obj
            .get("provider_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        env_managed: obj.get("env_managed").and_then(|v| v.as_bool()),
        favorite_at: obj
            .get("favorite_at")
            .and_then(|v| v.as_u64())
            .or_else(|| existing.and_then(|e| e.favorite_at)),
        tool_config,
        history: existing.map(|e| e.history.clone()).unwrap_or_default(),
        extra: existing.map(|e| e.extra.clone()).unwrap_or_default(),
        fetched_models: obj
            .get("fetched_models")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
    };
    if explicit_empty_claude_default_model {
        record.model = None;
        record.tool_config.remove("claude_default_model");
    }
    normalize_service_provider_record(&mut record);
    record
}

#[tauri::command]
pub fn service_providers_list() -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let providers: Vec<Value> = state
        .providers
        .iter()
        .map(service_provider_to_value)
        .collect();
    let payload = json!({
        "active": state.active,
        "active_claude": state.active.get("claude"),
        "active_codex": state.active.get("codex"),
        "active_gemini": state.active.get("gemini"),
        "active_opencode": state.active.get("opencode"),
        "providers": providers,
    });
    api_ok(payload, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn service_providers_upsert(
    app: tauri::AppHandle,
    provider: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    let obj = provider
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "provider must be object"))?;
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tool = obj
        .get("tool")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if id.is_empty() || tool.is_empty() {
        return Err(api_error("invalid_payload", "provider id/tool required"));
    }

    // Validate code for Claude
    if tool == "claude" {
        if let Some(ref code) = obj.get("code").and_then(|v| v.as_str()) {
            let code_trim = code.trim().to_lowercase();
            if code_trim.is_empty() {
                return Err(api_error("invalid_payload", "code is required"));
            }
            if !code_trim
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(api_error(
                    "invalid_payload",
                    "code can only contain ASCII letters, numbers, hyphens, and underscores",
                ));
            }
        }
    }

    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;

    // Validate code uniqueness
    if tool == "claude" {
        if let Some(ref code) = obj.get("code").and_then(|v| v.as_str()) {
            let code_trim = code.trim().to_lowercase();
            let duplicate = state.providers.iter().any(|p| {
                p.tool == "claude"
                    && p.id != id
                    && p.code.as_ref().map(|c| c.trim().to_lowercase()) == Some(code_trim.clone())
            });
            if duplicate {
                return Err(api_error(
                    "invalid_payload",
                    format!(
                        "code '{}' is already used by another Claude service provider",
                        code_trim
                    ),
                ));
            }
        }
    }

    let existing = state.providers.iter().find(|p| p.id == id).cloned();

    // Handle secret placeholder: if api_key is ******** and existing has a real key, preserve it
    let mut record = service_provider_from_value(Value::Object(obj), existing.as_ref());
    if record.api_key == "********" {
        if let Some(ref ex) = existing {
            if !ex.api_key.is_empty() && ex.api_key != "********" {
                record.api_key = ex.api_key.clone();
            }
        }
    }

    // Ensure default claude fields
    if record.claude_api_format.is_empty() {
        record.claude_api_format = "anthropic_messages".to_string();
    }
    if record.claude_connection_mode.is_empty() {
        record.claude_connection_mode = "native_anthropic".to_string();
    }
    if record.tool == "claude"
        && (record.claude_api_format == "open_ai_chat"
            || record.claude_api_format == "open_ai_responses")
    {
        record.claude_connection_mode = "protocol_router".to_string();
        record.protocol_router_wire_api =
            normalize_protocol_router_wire_api(&record.claude_api_format);
    }
    record.protocol_router_wire_api =
        normalize_protocol_router_wire_api(&record.protocol_router_wire_api);
    if record.claude_auth_env_key.is_empty() {
        record.claude_auth_env_key = "ANTHROPIC_API_KEY".to_string();
    }

    if let Some(pos) = state.providers.iter().position(|p| p.id == record.id) {
        state.providers[pos] = record.clone();
    } else {
        state.providers.push(record.clone());
    }

    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_upsert")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        service_provider_to_legacy(&record),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_providers_delete(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    state.providers.retain(|p| p.id != provider_id);
    state.active.retain(|_, v| v != &provider_id);
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_delete")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_providers_set_active(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    state.active.insert(tool.clone(), provider_id.clone());
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_set_active")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "tool": tool, "provider_id": provider_id }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_providers_set_env_managed(
    app: tauri::AppHandle,
    provider_id: String,
    env_managed: bool,
) -> Result<ApiOk<Value>, ApiErr> {
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    if let Some(p) = state.providers.iter_mut().find(|p| p.id == provider_id) {
        p.env_managed = Some(env_managed);
    }
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_set_env_managed")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "provider_id": provider_id, "env_managed": env_managed }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

fn set_service_provider_favorite_impl(
    state: &mut ServiceProvidersState,
    provider_id: &str,
    favorite: bool,
) -> Result<ServiceProviderRecord, ApiErr> {
    let record = state
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| api_error("not_found", "service provider not found"))?;

    if favorite {
        if record.favorite_at.is_none() {
            record.favorite_at = Some(now_ts());
        }
    } else {
        record.favorite_at = None;
    }

    Ok(record.clone())
}

#[tauri::command]
pub async fn service_providers_set_favorite(
    app: tauri::AppHandle,
    provider_id: String,
    favorite: bool,
) -> Result<ApiOk<Value>, ApiErr> {
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let updated = set_service_provider_favorite_impl(&mut state, &provider_id, favorite)?;
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_set_favorite")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        service_provider_to_legacy(&updated),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn service_providers_export(output_path: String) -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let providers: Vec<Value> = state
        .providers
        .iter()
        .map(service_provider_to_value)
        .collect();
    let payload = json!({
        "format": "onespace-service-providers",
        "version": PROVIDERS_EXPORT_VERSION,
        "exported_at": now_ts(),
        "active": state.active,
        "providers": providers,
    });
    let content = serde_json::to_string_pretty(&payload)
        .map_err(|e| api_error("serialize_error", e.to_string()))?;
    let expanded_output_path =
        expand_home_dir_path(&output_path).map_err(|e| api_error("io_error", e))?;
    let final_output_path = if expanded_output_path.is_dir() {
        expanded_output_path.join("onespace-service-providers-export.json")
    } else {
        expanded_output_path
    };
    StorageEngine::atomic_write(&final_output_path, &content)
        .map_err(|e| api_error("io_error", e))?;
    api_ok(
        json!({
            "path": final_output_path.to_string_lossy().to_string(),
            "count": payload.get("providers").and_then(|v| v.as_array()).map(|arr| arr.len()).unwrap_or(0)
        }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn service_providers_import_preview(import_path: String) -> Result<ApiOk<Value>, ApiErr> {
    // Accept both old providers format and new service_providers format
    let expanded =
        expand_home_dir_path(&import_path).map_err(|e| api_error("invalid_payload", e))?;
    let content =
        fs::read_to_string(&expanded).map_err(|e| api_error("io_error", e.to_string()))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| api_error("invalid_payload", e.to_string()))?;
    // Return preview as-is for frontend to display
    api_ok(value, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn service_providers_import_apply(
    app: tauri::AppHandle,
    import_path: String,
) -> Result<ApiOk<Value>, ApiErr> {
    let expanded =
        expand_home_dir_path(&import_path).map_err(|e| api_error("invalid_payload", e))?;
    let content =
        fs::read_to_string(&expanded).map_err(|e| api_error("io_error", e.to_string()))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| api_error("invalid_payload", e.to_string()))?;

    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;

    // Try to parse as new service_providers format
    if let Ok(imported) = serde_json::from_value::<ServiceProvidersState>(value.clone()) {
        for sp in imported.providers {
            if let Some(pos) = state.providers.iter().position(|p| p.id == sp.id) {
                state.providers[pos] = sp;
            } else {
                state.providers.push(sp);
            }
        }
        for (tool, pid) in imported.active {
            state.active.insert(tool, pid);
        }
    } else if let Some(obj) = value.as_object() {
        // Try legacy providers format
        if let Some(providers_arr) = obj.get("providers").and_then(|v| v.as_array()) {
            for pval in providers_arr {
                let record = service_provider_from_value(pval.clone(), None);
                if let Some(pos) = state.providers.iter().position(|p| p.id == record.id) {
                    state.providers[pos] = record;
                } else {
                    state.providers.push(record);
                }
            }
        }
        if let Some(active) = obj.get("active").and_then(|v| v.as_object()) {
            for (tool, pid) in active {
                if let Some(pid_str) = pid.as_str() {
                    state.active.insert(tool.clone(), pid_str.to_string());
                }
            }
        }
    }

    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_import_apply")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({
            "imported": state.providers.len(),
            "total": state.providers.len(),
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn service_providers_list_synced_other_devices() -> Result<ApiOk<Vec<Value>>, ApiErr> {
    let cfg = config::get_storage_config().map_err(|e| api_error("config_error", e))?;
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(root) = config::resolve_shared_storage_root(&cfg) {
        roots.push(root);
    }
    if let Ok(shared) = config::get_shared_data_dir_for(&cfg) {
        if !roots.iter().any(|p| p == &shared) {
            roots.push(shared);
        }
    }
    let current_device = normalize_device_label(&crate::get_hostname());
    let mut seen_devices: HashSet<String> = HashSet::new();
    let mut devices: Vec<Value> = Vec::new();
    let skip_dirs: HashSet<&str> = [
        "shared", "profile", "content", "meta", "data", "backup", "backups", ".git",
    ]
    .into_iter()
    .collect();

    for root in roots {
        if !root.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let device_id = entry.file_name().to_string_lossy().trim().to_string();
            if device_id.is_empty() {
                continue;
            }
            let normalized = normalize_device_label(&device_id);
            if normalized.is_empty()
                || normalized == current_device
                || skip_dirs.contains(normalized.as_str())
                || seen_devices.contains(&normalized)
            {
                continue;
            }
            // Try service_providers first, then legacy providers
            let candidate_paths = [
                path.join("service_providers").join("state.json"),
                path.join("providers").join("state.json"),
                path.join("providers.json"),
                path.join("ai_providers.json"),
            ];
            for cp in &candidate_paths {
                if !cp.exists() {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(cp) {
                    if let Ok(val) = serde_json::from_str::<Value>(&content) {
                        let mut lite_providers = Vec::new();
                        if let Some(providers_arr) = val.get("providers").and_then(|v| v.as_array())
                        {
                            for pv in providers_arr {
                                lite_providers.push(json!({
                                    "id": pv.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                                    "name": pv.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                    "tool": pv.get("tool").and_then(|v| v.as_str()).unwrap_or(""),
                                }));
                            }
                        }
                        devices.push(json!({
                            "device_id": normalized,
                            "providers": lite_providers,
                        }));
                        seen_devices.insert(normalized);
                        break;
                    }
                }
            }
        }
    }
    api_ok(devices, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn service_providers_auto_import_from_system(
    _app: tauri::AppHandle,
    _tool: String,
) -> Result<ApiOk<Value>, ApiErr> {
    // Stub: return empty for now. Full implementation would scan system config.
    api_ok(
        json!({ "imported": false, "reason": "not implemented" }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

// ─── End service_providers commands ────────────────────────────────────────────

#[tauri::command]
pub fn launcher_list() -> Result<ApiOk<Vec<Value>>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    sort_launcher_items(&mut state.items);
    api_ok(
        state.items.iter().map(launcher_to_legacy).collect(),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn launcher_upsert(_app: tauri::AppHandle, item: Value) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let obj = item
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "launcher item must be object"))?;

    let req_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let req_name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let req_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_type").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let req_target = obj
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("command").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let req_pinned = obj.get("pinned").and_then(|v| v.as_bool());
    let req_pin_order = obj
        .get("pin_order")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let req_trusted = obj.get("trusted").and_then(|v| v.as_bool());

    let now = now_ts();
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let item_id = req_id
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let existing = state.items.iter().find(|it| it.id == item_id).cloned();

    let mut record = LauncherRecord {
        id: item_id,
        name: req_name
            .or_else(|| existing.as_ref().map(|it| it.name.clone()))
            .unwrap_or_default(),
        item_type: req_type
            .or_else(|| existing.as_ref().map(|it| it.item_type.clone()))
            .unwrap_or_default(),
        target: req_target
            .or_else(|| existing.as_ref().map(|it| it.target.clone()))
            .unwrap_or_default(),
        pinned: req_pinned
            .unwrap_or_else(|| existing.as_ref().map(|it| it.pinned).unwrap_or(false)),
        pin_order: req_pin_order
            .unwrap_or_else(|| existing.as_ref().map(|it| it.pin_order).unwrap_or(0)),
        launch_count: existing.as_ref().map(|it| it.launch_count).unwrap_or(0),
        last_launched_at: existing.as_ref().and_then(|it| it.last_launched_at),
        trusted: req_trusted
            .unwrap_or_else(|| existing.as_ref().map(|it| it.trusted).unwrap_or(false)),
        created_at: existing.as_ref().map(|it| it.created_at).unwrap_or(now),
        updated_at: now,
    };
    if let Err(err) = sanitize_launcher_record(&mut record) {
        if let Some(old) = &existing {
            if record.name.trim().is_empty() {
                record.name = old.name.clone();
            }
            if record.target.trim().is_empty() {
                record.target = old.target.clone();
            }
            if !is_valid_launcher_type(&record.item_type) {
                record.item_type = old.item_type.clone();
            }
            sanitize_launcher_record(&mut record).map_err(|e| api_error("invalid_payload", e))?;
        } else {
            return Err(api_error("invalid_payload", err));
        }
    }

    if record.pinned {
        let was_pinned = existing.as_ref().map(|it| it.pinned).unwrap_or(false);
        if !was_pinned && req_pin_order.is_none() {
            record.pin_order = next_launcher_pin_order(&state.items);
        }
    } else {
        record.pin_order = 0;
    }

    if let Some(pos) = state.items.iter().position(|it| it.id == record.id) {
        state.items[pos] = record.clone();
    } else {
        state.items.push(record.clone());
    }

    normalize_launcher_pin_order(&mut state.items);
    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        launcher_to_legacy(&record),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_delete(
    _app: tauri::AppHandle,
    payload: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_id = obj
        .get("itemId")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if item_id.is_empty() {
        return Err(api_error("invalid_payload", "itemId required"));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    state.items.retain(|it| it.id != item_id);
    normalize_launcher_pin_order(&mut state.items);
    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_reorder(
    _app: tauri::AppHandle,
    ids: Vec<String>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;

    let mut ordered_ids: Vec<String> = ids
        .into_iter()
        .filter(|id| state.items.iter().any(|it| it.id == *id && it.pinned))
        .collect();
    let current_pinned: Vec<String> = state
        .items
        .iter()
        .filter(|it| it.pinned)
        .map(|it| it.id.clone())
        .collect();
    for id in current_pinned {
        if !ordered_ids.iter().any(|x| x == &id) {
            ordered_ids.push(id);
        }
    }

    for item in state.items.iter_mut() {
        if !item.pinned {
            continue;
        }
        if let Some(pos) = ordered_ids.iter().position(|id| id == &item.id) {
            item.pin_order = pos as u32;
            item.updated_at = now_ts();
        }
    }

    normalize_launcher_pin_order(&mut state.items);
    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({ "reordered": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_mark_launched(
    _app: tauri::AppHandle,
    payload: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_id = obj
        .get("itemId")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if item_id.is_empty() {
        return Err(api_error("invalid_payload", "itemId required"));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let now = now_ts();
    let mut found = false;
    for item in state.items.iter_mut() {
        if item.id == item_id {
            item.launch_count = item.launch_count.saturating_add(1);
            item.last_launched_at = Some(now);
            item.updated_at = now;
            found = true;
            break;
        }
    }

    if !found {
        return Err(api_error("not_found", "launcher item not found"));
    }

    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({ "launched": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_set_trust(
    _app: tauri::AppHandle,
    payload: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_id = obj
        .get("itemId")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if item_id.is_empty() {
        return Err(api_error("invalid_payload", "itemId required"));
    }
    let trusted = obj
        .get("trusted")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| api_error("invalid_payload", "trusted bool required"))?;
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let mut found = false;
    for item in state.items.iter_mut() {
        if item.id == item_id {
            if item.item_type != "script" {
                return Err(api_error(
                    "invalid_payload",
                    "only script item supports trust switch",
                ));
            }
            item.trusted = trusted;
            item.updated_at = now_ts();
            found = true;
            break;
        }
    }

    if !found {
        return Err(api_error("not_found", "launcher item not found"));
    }

    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({ "trusted": trusted }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn launcher_export(
    output_path: String,
    item_ids: Option<Vec<String>>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let selected_ids = item_ids.unwrap_or_default();
    let mut exported: Vec<LauncherRecord> = state
        .items
        .iter()
        .filter(|item| selected_ids.is_empty() || selected_ids.iter().any(|id| id == &item.id))
        .cloned()
        .collect();
    sort_launcher_items(&mut exported);

    let payload = json!({
        "version": LAUNCHER_EXPORT_VERSION,
        "exported_at": now_ts(),
        "items": exported,
    });

    let content = serde_json::to_string_pretty(&payload)
        .map_err(|e| api_error("serialize_error", e.to_string()))?;
    StorageEngine::atomic_write(Path::new(&output_path), &content)
        .map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({
            "path": output_path,
            "count": payload
                .get("items")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0)
        }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn launcher_import(
    _app: tauri::AppHandle,
    import_path: String,
    mode: Option<String>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let raw = fs::read_to_string(&import_path).map_err(|e| api_error("io_error", e.to_string()))?;
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|e| api_error("invalid_payload", e.to_string()))?;
    let items_val = parsed
        .get("items")
        .and_then(|v| v.as_array().cloned())
        .or_else(|| parsed.as_array().cloned())
        .ok_or_else(|| api_error("invalid_payload", "import payload must contain items array"))?;

    let now = now_ts();
    let mut imported_records: Vec<LauncherRecord> = Vec::new();
    for item in items_val {
        let input: LauncherItemInput = serde_json::from_value(item)
            .map_err(|e| api_error("invalid_payload", format!("invalid launcher item: {}", e)))?;
        let mut record = launcher_record_from_import_input(input, now)
            .map_err(|e| api_error("invalid_payload", e))?;
        record.updated_at = now;
        imported_records.push(record);
    }
    let imported_count = imported_records.len();

    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let mode = mode.unwrap_or_else(|| "merge".to_string()).to_lowercase();
    if mode == "replace" {
        state.items = imported_records;
    } else {
        merge_launcher_items(&mut state.items, imported_records);
    }
    normalize_launcher_pin_order(&mut state.items);

    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({
            "imported": true,
            "mode": mode,
            "count": imported_count,
            "total": state.items.len()
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn launcher_execute(payload: Value) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_type").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let target = obj
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("command").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();

    if target.is_empty() {
        return Err(api_error("invalid_payload", "launcher target required"));
    }
    if !is_valid_launcher_type(&item_type) || item_type == "internal" {
        return Err(api_error(
            "invalid_payload",
            "unsupported launcher type for execute",
        ));
    }

    let run_result: Result<(), String> = match item_type.as_str() {
        "url" | "folder" => crate::open_path_with_system(&target),
        "app" => match normalize_app_target(&target) {
            Ok(app_name) => try_open_application(&app_name),
            Err(e) => Err(e),
        },
        "script" => Command::new("sh")
            .arg("-c")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => Err("unsupported launcher type".to_string()),
    };

    run_result.map_err(|e| api_error("launch_failed", e))?;
    api_ok(
        json!({ "launched": true }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn launcher_resolve_app_icon(target: String) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let normalized_target =
        normalize_app_target(&target).unwrap_or_else(|_| target.trim().to_string());
    let data_url = resolve_app_icon_data_url(&normalized_target);

    api_ok(
        json!({ "data_url": data_url }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn sessions_list() -> Result<ApiOk<Vec<Value>>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let _sessions_state_guard =
        lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let mut normalized = false;

    let mut owner_by_tool_session = HashMap::<(String, String), String>::new();
    for session in state.sessions.iter_mut() {
        let tool_session_id = session.tool_session_id.trim();
        if tool_session_id.is_empty() {
            continue;
        }
        let key = (session.tool.clone(), tool_session_id.to_string());
        if let Some(owner_id) = owner_by_tool_session.get(&key) {
            if owner_id != &session.id {
                session.tool_session_id.clear();
                session.status = "unbound".to_string();
                normalized = true;
                continue;
            }
        } else {
            owner_by_tool_session.insert(key, session.id.clone());
        }
    }
    if normalized {
        let _ = save_sessions_state(&state);
    }

    let filtered = filter_sessions_by_history_window(state.sessions.iter());
    api_ok(
        filtered.iter().map(session_to_legacy).collect(),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

/// 通过 provider_id 解析 Claude profile 的配置目录路径。
/// 加载 providers state，查找对应 provider，使用其 code 或 id 作为目录名。
fn resolve_claude_config_dir_for_provider_id(provider_id: &str) -> Result<PathBuf, String> {
    let state = load_service_providers_state()?;
    let provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == "claude")
        .ok_or_else(|| format!("Claude service provider not found: {provider_id}"))?;
    let legacy_provider = service_provider_to_provider_record(provider);
    let dir_name = crate::claude_profiles::resolve_claude_dir_name(&legacy_provider);
    Ok(crate::claude_profiles::get_claude_profiles_dir()?.join(&dir_name))
}

fn launch_options_for_session(
    record: &SessionRecord,
) -> Result<ai_sessions::LaunchOptions, String> {
    let mode = normalize_runtime_mode(Some(&record.runtime_mode));
    let mut env: HashMap<String, String> = HashMap::new();

    if mode == "strict" {
        let profile_id = record
            .runtime_profile_id
            .clone()
            .ok_or_else(|| "strict runtime profile id is required".to_string())?;
        let strict_env = crate::runtime_profiles::runtime_env_for_profile(&profile_id)?;
        env.extend(strict_env);
    }

    if record.tool == "claude" {
        if let Some(provider_id) = &record.provider_id {
            let config_dir = resolve_claude_config_dir_for_provider_id(provider_id)?;
            env.insert(
                "CLAUDE_CONFIG_DIR".to_string(),
                config_dir.to_string_lossy().to_string(),
            );
        }
    }

    if env.is_empty() {
        Ok(ai_sessions::LaunchOptions::default())
    } else {
        Ok(ai_sessions::LaunchOptions { env: Some(env) })
    }
}

fn lookup_env_for_session(record: &SessionRecord) -> Option<HashMap<String, String>> {
    let mode = normalize_runtime_mode(Some(&record.runtime_mode));
    let mut env: HashMap<String, String> = HashMap::new();

    if mode == "strict" {
        let profile_id = record.runtime_profile_id.as_ref()?;
        let strict_env = crate::runtime_profiles::runtime_env_for_profile(profile_id).ok()?;
        env.extend(strict_env);
    }

    if record.tool == "claude" {
        if let Some(provider_id) = &record.provider_id {
            if let Ok(config_dir) = resolve_claude_config_dir_for_provider_id(provider_id) {
                env.insert(
                    "CLAUDE_CONFIG_DIR".to_string(),
                    config_dir.to_string_lossy().to_string(),
                );
            }
        }
    }

    if env.is_empty() {
        None
    } else {
        Some(env)
    }
}

fn apply_resolved_session_id_after_create(
    session: &mut SessionRecord,
    resolved_tool_session_id: Option<&str>,
    now: u64,
) {
    session.last_used_at = now;
    if let Some(tool_session_id) = resolved_tool_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        session.tool_session_id = tool_session_id.to_string();
        session.status = "active".to_string();
    } else {
        session.tool_session_id.clear();
        session.status = "pending_bind".to_string();
    }
}

#[tauri::command]
pub async fn sessions_create(
    app: tauri::AppHandle,
    session: SessionInput,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let now = now_ts();
    let id = session
        .id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let runtime_mode = normalize_runtime_mode(session.runtime_mode.as_deref());
    let runtime_profile_id = if runtime_mode == "strict" {
        session.runtime_profile_id.clone().and_then(|v| {
            if v.trim().is_empty() {
                None
            } else {
                Some(v.trim().to_string())
            }
        })
    } else {
        None
    };

    let resolved_working_dir = resolve_working_dir_for_session_create(&session);
    let normalized_working_dir =
        ai_sessions::normalize_working_dir_for_terminal(&resolved_working_dir);

    let record = SessionRecord {
        id,
        name: String::new(),
        working_dir: normalized_working_dir.clone(),
        tool: session.tool.clone(),
        tool_session_id: session
            .tool_session_id
            .clone()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_default(),
        model_name: None,
        name_source: "history".to_string(),
        runtime_mode,
        runtime_profile_id,
        preset_id: session.preset_id.clone().and_then(|v| {
            if v.trim().is_empty() {
                None
            } else {
                Some(v.trim().to_string())
            }
        }),
        created_at: now,
        last_used_at: now,
        status: "pending_bind".to_string(),
        favorited_at: None,
        provider_id: session.provider_id.clone().and_then(|v| {
            if v.trim().is_empty() {
                None
            } else {
                Some(v.trim().to_string())
            }
        }),
    };

    let launch_options =
        launch_options_for_session(&record).map_err(|e| api_error("launch_failed", e))?;
    let create_lock_key = format!(
        "{}|{}|{}|{}|{}",
        record.tool.trim().to_lowercase(),
        record.working_dir.as_str(),
        record.runtime_mode.as_str(),
        record.runtime_profile_id.as_deref().unwrap_or_default(),
        record.preset_id.as_deref().unwrap_or_default()
    );
    let create_lock_key =
        match acquire_session_create_lock(create_lock_key).map_err(|e| api_error("io_error", e))? {
            Some(key) => key,
            None => {
                return Err(api_error(
                    "SESSION_CREATE_DUPLICATED",
                    "duplicate create request in progress",
                ))
            }
        };

    let create_result: Result<ApiOk<Value>, ApiErr> = (|| {
        {
            let _sessions_state_guard =
                lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
            let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
            state.sessions.push(record.clone());
            save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
        }

        workspaces::apply_workspace_mcp_for_session(&normalized_working_dir, &session.tool)
            .map_err(|e| api_error("workspace_mcp_apply_failed", e))?;

        let resolved_tool_session_id =
            match ai_sessions::launch_native_session_for_create_with_options(
                &normalized_working_dir,
                &session.tool,
                session.tool_session_id.as_deref(),
                &launch_options,
            ) {
                Ok(tool_session_id) => tool_session_id,
                Err(e) => {
                    {
                        let _sessions_state_guard = lock_sessions_state_write()
                            .map_err(|err| api_error("io_error", err))?;
                        let mut rollback =
                            load_sessions_state().map_err(|err| api_error("io_error", err))?;
                        rollback.sessions.retain(|s| s.id != record.id);
                        let _ = save_sessions_state(&rollback);
                    }
                    return Err(api_error("launch_failed", e));
                }
            };

        let (schema, final_record) = {
            let _sessions_state_guard =
                lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
            let mut latest_state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
            let now = now_ts();
            let mut final_record: Option<SessionRecord> = None;
            for item in latest_state.sessions.iter_mut() {
                if item.id != record.id {
                    continue;
                }
                apply_resolved_session_id_after_create(
                    item,
                    resolved_tool_session_id.as_deref(),
                    now,
                );
                final_record = Some(item.clone());
                break;
            }

            let final_record = final_record
                .ok_or_else(|| api_error("not_found", "session not found after create"))?;
            let schema =
                save_sessions_state(&latest_state).map_err(|e| api_error("io_error", e))?;
            (schema, final_record)
        };
        workspaces::schedule_sync_from_sessions(app.clone());

        api_ok(
            session_to_legacy(&final_record),
            ApiMeta {
                schema_version: schema.schema_version,
                revision: schema.revision,
            },
        )
    })();

    release_session_create_lock(&create_lock_key);
    create_result
}

#[tauri::command]
pub async fn sessions_update(
    app: tauri::AppHandle,
    session: SessionInput,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let id = session
        .id
        .clone()
        .ok_or_else(|| api_error("invalid_payload", "session.id required"))?;
    let _sessions_state_guard =
        lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;

    // Reload from disk right before saving to avoid overwriting concurrent changes
    // (e.g., history sync adding new sessions, concurrent favorite changes).
    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;

    // Capture the values we want to apply.
    let update_name = session.name.clone();
    let update_name_source = "manual".to_string();
    let update_working_dir = ai_sessions::normalize_working_dir_for_terminal(&session.working_dir);
    let update_tool = session.tool.clone();
    let update_runtime_mode = session.runtime_mode.is_some();
    let update_runtime_mode_val = normalize_runtime_mode(session.runtime_mode.as_deref());
    let update_runtime_profile_id = session.runtime_profile_id.clone().and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let update_preset_id = session.preset_id.clone().and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let update_last_used = now_ts();

    let mut found = false;
    for s in state.sessions.iter_mut() {
        if s.id == id {
            if let Some(tool_session_id) = &session.tool_session_id {
                let requested = tool_session_id.trim();
                if requested != s.tool_session_id {
                    return Err(api_error(
                        "IMMUTABLE_FIELD",
                        "tool_session_id is system-managed and cannot be updated",
                    ));
                }
            }
            if let Some(status) = &session.status {
                let requested_status = status.trim();
                if !requested_status.is_empty() && requested_status != s.status {
                    return Err(api_error(
                        "IMMUTABLE_FIELD",
                        "status is system-managed and cannot be updated",
                    ));
                }
            }
            if let Some(provider_id) = &session.provider_id {
                if !provider_id.trim().is_empty()
                    && s.provider_id.as_deref() != Some(provider_id.trim())
                {
                    return Err(api_error(
                        "IMMUTABLE_FIELD",
                        "provider_id is system-managed and cannot be updated",
                    ));
                }
            }
            if s.name != update_name {
                s.name = update_name.clone();
                s.name_source = update_name_source.clone();
            }
            s.working_dir = update_working_dir.clone();
            s.tool = update_tool.clone();
            if update_runtime_mode {
                s.runtime_mode = update_runtime_mode_val.clone();
                if s.runtime_mode != "strict" {
                    s.runtime_profile_id = None;
                }
            }
            if session.runtime_profile_id.is_some() {
                s.runtime_profile_id = update_runtime_profile_id.clone();
            }
            if session.preset_id.is_some() {
                s.preset_id = update_preset_id.clone();
            }
            s.last_used_at = update_last_used;
            found = true;
            break;
        }
    }

    if !found {
        return Err(api_error("not_found", "session not found"));
    }

    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
    workspaces::schedule_sync_from_sessions(app);

    let updated = state
        .sessions
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| api_error("not_found", "session not found"))?;

    api_ok(
        session_to_legacy(&updated),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn sessions_delete(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let _sessions_state_guard =
        lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;

    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let tombstone_key = state
        .sessions
        .iter()
        .find(|s| s.id == session_id)
        .and_then(|session| history_tombstone_key(&session.tool, &session.tool_session_id));
    if let Some(key) = &tombstone_key {
        state.tombstones.insert(key.clone());
    }
    state.sessions.retain(|s| s.id != session_id);
    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
    workspaces::schedule_sync_from_sessions(app);

    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

/// Read the configured permission mode for a given tool. Defaults to Default.
fn resolve_permission_mode_for_tool(tool: &str) -> ai_sessions::TerminalPermissionMode {
    let key = tool.trim().to_lowercase();
    if let Ok(cfg) = crate::config::get_config() {
        if let Some(modes) = &cfg.ai_model_permission_modes {
            if let Some(value) = modes.get(&key) {
                return ai_sessions::TerminalPermissionMode::from_str(value);
            }
        }
    }
    ai_sessions::TerminalPermissionMode::Default
}

fn resolve_working_dir_for_session_create(session: &SessionInput) -> String {
    let provided = session.working_dir.trim();
    if !provided.is_empty() {
        return provided.to_string();
    }

    let is_claude_provider_launch = session.tool.trim().eq_ignore_ascii_case("claude")
        && session
            .provider_id
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);

    if !is_claude_provider_launch {
        return String::new();
    }

    crate::config::get_config()
        .ok()
        .and_then(|cfg| cfg.claude_provider_launch_dir)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
}

/// Validate caller's permission mode against config and resolve the effective mode.
/// - config=default, caller=full_access → INVALID_PERMISSION_MODE
/// - config=full_access, caller=missing  → PERMISSION_CONFIRMATION_REQUIRED
/// - config=full_access, caller=default  → Default
/// - config=full_access, caller=full_access → FullAccess
/// - config=default, caller=default or missing → Default
fn validate_and_resolve_permission_mode(
    config_mode: &ai_sessions::TerminalPermissionMode,
    caller_mode: Option<&str>,
) -> Result<ai_sessions::TerminalPermissionMode, ApiErr> {
    // Strictly parse caller mode — only 'default' and 'full_access' are valid
    let parsed_caller = caller_mode
        .map(|v| match ai_sessions::TerminalPermissionMode::from_str(v) {
            ai_sessions::TerminalPermissionMode::Default => {
                // from_str maps unknown values to Default, but we want strict validation
                if v == "default" {
                    Ok(ai_sessions::TerminalPermissionMode::Default)
                } else {
                    Err(api_error(
                        "INVALID_PERMISSION_MODE",
                        "permission_mode must be 'default' or 'full_access'",
                    ))
                }
            }
            ai_sessions::TerminalPermissionMode::FullAccess => {
                if v == "full_access" {
                    Ok(ai_sessions::TerminalPermissionMode::FullAccess)
                } else {
                    Err(api_error(
                        "INVALID_PERMISSION_MODE",
                        "permission_mode must be 'default' or 'full_access'",
                    ))
                }
            }
        })
        .transpose()?;

    match (config_mode, parsed_caller) {
        // config default: caller cannot elevate to full_access
        (
            ai_sessions::TerminalPermissionMode::Default,
            Some(ai_sessions::TerminalPermissionMode::FullAccess),
        ) => Err(api_error(
            "INVALID_PERMISSION_MODE",
            "cannot elevate to full_access when tool is configured as default",
        )),
        // config full_access: caller must confirm
        (ai_sessions::TerminalPermissionMode::FullAccess, None) => Err(api_error(
            "PERMISSION_CONFIRMATION_REQUIRED",
            "tool is configured as full_access; caller must confirm permission mode",
        )),
        // config full_access with explicit caller confirmation
        (_, Some(mode)) => Ok(mode),
        // config default with explicit default or missing → Default
        _ => Ok(ai_sessions::TerminalPermissionMode::Default),
    }
}

#[tauri::command]
pub fn sessions_launch(
    app: tauri::AppHandle,
    session_id: String,
    permission_mode: Option<String>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let _sessions_state_guard =
        lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;

    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let now = now_ts();
    let mut target: Option<SessionRecord> = None;

    for s in state.sessions.iter_mut() {
        if s.id == session_id {
            s.last_used_at = now;
            target = Some(s.clone());
            break;
        }
    }

    let mut target = target.ok_or_else(|| api_error("not_found", "session not found"))?;

    if target.status == "unbound"
        || target.status == "pending_bind"
        || target.tool_session_id.trim().is_empty()
    {
        let mut occupied_ids = HashSet::<String>::new();
        for s in state.sessions.iter() {
            if s.id == target.id || s.tool != target.tool {
                continue;
            }
            let existing_id = s.tool_session_id.trim();
            if existing_id.is_empty() {
                continue;
            }
            occupied_ids.insert(existing_id.to_string());
        }
        let lookup_env = lookup_env_for_session(&target);
        if let Some(bound_id) = ai_sessions::resolve_native_session_id_for_existing(
            &target.tool,
            &target.working_dir,
            lookup_env.as_ref(),
            Some((target.created_at as i64) * 1000),
            Some(&occupied_ids),
            target.status == "pending_bind",
        ) {
            for s in state.sessions.iter_mut() {
                if s.id == target.id {
                    s.tool_session_id = bound_id.clone();
                    s.status = "active".to_string();
                    target.tool_session_id = bound_id.clone();
                    target.status = "active".to_string();
                    break;
                }
            }
        } else {
            return Err(api_error(
                "SESSION_ID_MISSING",
                "session tool_session_id is empty; create a new session",
            ));
        }
    }

    if state.sessions.iter().any(|s| {
        s.id != target.id
            && s.tool == target.tool
            && !s.tool_session_id.trim().is_empty()
            && s.tool_session_id == target.tool_session_id
    }) {
        return Err(api_error(
            "SESSION_ID_CONFLICT",
            "tool_session_id is already bound to another session",
        ));
    }

    // Resolve permission mode from config and validate caller's request
    let config_perm_mode = resolve_permission_mode_for_tool(&target.tool);
    let resolved_perm_mode =
        validate_and_resolve_permission_mode(&config_perm_mode, permission_mode.as_deref())
            .map_err(|e| e)?;

    let (install_scope, install_project_root) = session_install_scope_and_root(&target);
    crate::skills::skills_reconcile_for_tool(
        &target.tool,
        Some(install_scope.as_str()),
        install_project_root.as_deref(),
    )
    .map_err(|e| api_error("skills_preflight_failed", e))?;
    crate::subagents::subagents_reconcile_for_tool(
        &target.tool,
        Some(install_scope.as_str()),
        install_project_root.as_deref(),
    )
    .map_err(|e| api_error("subagents_preflight_failed", e))?;
    workspaces::apply_workspace_mcp_for_session(&target.working_dir, &target.tool)
        .map_err(|e| api_error("workspace_mcp_apply_failed", e))?;

    let launch_options =
        launch_options_for_session(&target).map_err(|e| api_error("launch_failed", e))?;

    ai_sessions::launch_native_session_with_options(
        &target.working_dir,
        &target.tool,
        &target.tool_session_id,
        resolved_perm_mode,
        &launch_options,
    )
    .map_err(|e| {
        if e.contains("Unsupported model type") {
            api_error("CLI_UNSUPPORTED", e)
        } else {
            api_error("RESUME_FAILED", e)
        }
    })?;

    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
    workspaces::schedule_sync_from_sessions(app);

    api_ok(
        session_to_legacy(&target),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn projection_dry_run(tool: String, provider_id: String) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let service_provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == tool)
        .ok_or_else(|| api_error("not_found", "provider not found"))?;
    let provider = service_provider_to_provider_record(service_provider);

    let diffs = build_projection_diff(&provider).map_err(|e| api_error("projection_failed", e))?;
    api_ok(
        json!({ "changes": diffs }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn projection_apply(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let service_provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == tool)
        .cloned()
        .ok_or_else(|| api_error("not_found", "provider not found"))?;

    if tool == "claude" {
        // Dual-write for Claude: profile dir + global ~/.claude
        let provider = service_provider_to_provider_record(&service_provider);
        let profile_dir = crate::claude_profiles::get_claude_profiles_dir()
            .map(|d| d.join(crate::claude_profiles::resolve_claude_dir_name(&provider)))
            .map_err(|e| api_error("projection_failed", e))?;
        crate::claude_profiles::materialize_claude_settings_sp(&service_provider, &profile_dir)
            .map_err(|e| api_error("projection_failed", e))?;
    }

    let provider = service_provider_to_provider_record(&service_provider);
    apply_projection(&provider).map_err(|e| api_error("projection_failed", e))?;

    enqueue_sync_event("projection", "projection_apply").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "applied": true }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn sync_enqueue(app: tauri::AppHandle, reason: String) -> Result<ApiOk<Value>, ApiErr> {
    enqueue_sync_event("manual", &reason).map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "queued": true }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn sync_run_now(app: tauri::AppHandle) -> Result<ApiOk<Value>, ApiErr> {
    process_sync_queue_impl(app, true)
        .await
        .map_err(|e| api_error("sync_error", e))?;
    let outbox = load_outbox_state().map_err(|e| api_error("io_error", e))?;
    api_ok(
        serde_json::to_value(outbox).map_err(|e| api_error("serialize_error", e.to_string()))?,
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn sync_status() -> Result<ApiOk<OutboxState>, ApiErr> {
    let outbox = load_outbox_state().map_err(|e| api_error("io_error", e))?;
    api_ok(outbox, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub fn migration_status() -> Result<ApiOk<MigrationState>, ApiErr> {
    let state = load_migration_state().map_err(|e| api_error("io_error", e))?;
    api_ok(
        state,
        get_meta().unwrap_or(ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision: 0,
        }),
    )
}

#[tauri::command]
pub fn migration_run() -> Result<ApiOk<MigrationState>, ApiErr> {
    let state = run_migration_impl().map_err(|e| api_error("migration_failed", e))?;
    api_ok(state, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub fn migration_rollback(backup_id: String) -> Result<ApiOk<Value>, ApiErr> {
    rollback_from_backup(&backup_id).map_err(|e| api_error("rollback_failed", e))?;
    let mut state = load_migration_state().map_err(|e| api_error("io_error", e))?;
    state.migrated = false;
    state.last_error = None;
    save_migration_state(&state).map_err(|e| api_error("io_error", e))?;
    api_ok(
        json!({ "rolled_back": true, "backup_id": backup_id }),
        get_meta().unwrap_or(ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision: 0,
        }),
    )
}

/// Core favorite logic, extracted for testability without Tauri runtime.
#[allow(dead_code)]
fn set_session_favorite_impl(
    state: &mut SessionsState,
    session_id: &str,
    favorite: bool,
) -> Result<SessionRecord, ApiErr> {
    let record = state
        .sessions
        .iter_mut()
        .find(|s| s.id == session_id)
        .ok_or_else(|| api_error("not_found", "session not found"))?;

    if favorite {
        if record.favorited_at.is_none() {
            record.favorited_at = Some(now_ts());
        }
    } else {
        record.favorited_at = None;
    }

    let updated = record.clone();
    Ok(updated)
}

/// Set or unset the favorite status of a session.
/// When setting favorite, records the current timestamp as favorited_at.
/// Re-setting favorite to true keeps the original timestamp (idempotent).
#[tauri::command]
pub async fn sessions_set_favorite(
    app: tauri::AppHandle,
    session_id: String,
    favorite: bool,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let updated = {
        let _sessions_state_guard =
            lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
        let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
        let updated = set_session_favorite_impl(&mut state, &session_id, favorite)?;
        save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
        updated
    };

    let _ = app.emit("sessions-updated", ());

    api_ok(
        session_to_legacy(&updated),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, thread::sleep, time::Duration};

    fn make_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "onespace-app-store-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ))
    }

    fn write_test_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, content).expect("write file");
    }

    fn with_temp_dir<T>(name: &str, f: impl FnOnce(&Path) -> T) -> T {
        let temp_home = make_temp_dir(name);
        fs::create_dir_all(&temp_home).expect("create temp home");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&temp_home)));
        let _ = fs::remove_dir_all(&temp_home);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    fn codex_provider(
        id: &str,
        name: &str,
        key: &str,
        base_url: &str,
        model: &str,
    ) -> ProviderRecord {
        let mut tool_config = Map::new();
        tool_config.insert(
            "wire_api".to_string(),
            Value::String("responses".to_string()),
        );
        tool_config.insert(
            "model_reasoning_effort".to_string(),
            Value::String("high".to_string()),
        );
        tool_config.insert(
            "approval_policy".to_string(),
            Value::String("never".to_string()),
        );
        tool_config.insert(
            "sandbox_mode".to_string(),
            Value::String("workspace-write".to_string()),
        );
        ProviderRecord {
            core: ProviderCore {
                id: id.to_string(),
                name: name.to_string(),
                tool: "codex".to_string(),
                api_key: key.to_string(),
                code: None,
                base_url: Some(base_url.to_string()),
                model: Some(model.to_string()),
            },
            runtime_policy: ProviderRuntimePolicy {
                approval_policy: Some("never".to_string()),
                sandbox_mode: Some("workspace-write".to_string()),
            },
            favorite_at: None,
            tool_config,
            ..ProviderRecord::default()
        }
    }

    fn rendered_content(outputs: &[(PathBuf, String)], suffix: &str) -> String {
        outputs
            .iter()
            .find(|(path, _)| path.ends_with(suffix))
            .map(|(_, content)| content.clone())
            .unwrap_or_else(|| panic!("missing rendered output for {}", suffix))
    }

    #[test]
    fn codex_projection_preserves_login_auth_and_uses_model_provider() {
        with_temp_dir("codex-projection-login-preserve", |home| {
            let codex_dir = home.join(".codex");
            fs::create_dir_all(&codex_dir).expect("create codex dir");
            write_test_file(
                &codex_dir.join("auth.json"),
                r#"{
  "OPENAI_API_KEY": "old-key",
  "tokens": {"id_token": "login-token"},
  "account_id": "acct_123"
}"#,
            );
            write_test_file(
                &codex_dir.join("config.toml"),
                r#"preferred_auth_method = "login"
model = "old-model"
model_provider = "ollama_lan"

[model_providers.ollama_lan]
name = "Ollama LAN"
base_url = "http://127.0.0.1:11434/v1"
wire_api = "responses"
"#,
            );

            let provider = codex_provider(
                "work-openai",
                "Work OpenAI",
                "new-key",
                "https://proxy.example.com/v1",
                "gpt-5.5",
            );
            let outputs = render_codex_at_home(&provider, home).expect("render codex");
            let auth: Value = serde_json::from_str(&rendered_content(&outputs, ".codex/auth.json"))
                .expect("parse auth");
            assert_eq!(
                auth.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
                Some("new-key")
            );
            assert_eq!(
                auth.pointer("/tokens/id_token").and_then(|v| v.as_str()),
                Some("login-token")
            );
            assert_eq!(
                auth.get("account_id").and_then(|v| v.as_str()),
                Some("acct_123")
            );

            let doc = rendered_content(&outputs, ".codex/config.toml")
                .parse::<toml_edit::DocumentMut>()
                .expect("parse toml");
            assert!(doc.get("preferred_auth_method").is_none());
            assert_eq!(
                doc.get("forced_login_method").and_then(|v| v.as_str()),
                Some("api")
            );
            assert_eq!(doc.get("model").and_then(|v| v.as_str()), Some("gpt-5.5"));
            assert_eq!(
                doc.get("model_provider").and_then(|v| v.as_str()),
                Some("onespace_work_openai")
            );
            assert!(doc
                .get("model_providers")
                .and_then(|v| v.as_table())
                .and_then(|table| table.get("ollama_lan"))
                .is_some());
            let onespace = doc
                .get("model_providers")
                .and_then(|v| v.as_table())
                .and_then(|table| table.get("onespace_work_openai"))
                .and_then(|v| v.as_table())
                .expect("onespace provider table");
            assert_eq!(
                onespace.get("base_url").and_then(|v| v.as_str()),
                Some("https://proxy.example.com/v1")
            );
            assert_eq!(
                onespace
                    .get("requires_openai_auth")
                    .and_then(|v| v.as_bool()),
                Some(true)
            );
        });
    }

    #[test]
    fn codex_projection_switches_model_provider_without_deleting_old_provider() {
        with_temp_dir("codex-projection-switch", |home| {
            let codex_dir = home.join(".codex");
            fs::create_dir_all(&codex_dir).expect("create codex dir");
            write_test_file(
                &codex_dir.join("config.toml"),
                r#"[model_providers.user_provider]
name = "User Provider"
base_url = "https://user.example.com/v1"
"#,
            );
            write_test_file(
                &codex_dir.join("auth.json"),
                r#"{"tokens":{"id_token":"login-token"}}"#,
            );

            let first = codex_provider(
                "provider-a",
                "Provider A",
                "key-a",
                "https://a.example.com/v1",
                "gpt-5.4",
            );
            let first_outputs = render_codex_at_home(&first, home).expect("render first");
            write_test_file(
                &codex_dir.join("config.toml"),
                &rendered_content(&first_outputs, ".codex/config.toml"),
            );
            write_test_file(
                &codex_dir.join("auth.json"),
                &rendered_content(&first_outputs, ".codex/auth.json"),
            );

            let second = codex_provider(
                "provider-b",
                "Provider B",
                "key-b",
                "https://b.example.com/v1",
                "gpt-5.5",
            );
            let second_outputs = render_codex_at_home(&second, home).expect("render second");
            let auth: Value =
                serde_json::from_str(&rendered_content(&second_outputs, ".codex/auth.json"))
                    .expect("parse auth");
            assert_eq!(
                auth.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
                Some("key-b")
            );
            assert_eq!(
                auth.pointer("/tokens/id_token").and_then(|v| v.as_str()),
                Some("login-token")
            );

            let doc = rendered_content(&second_outputs, ".codex/config.toml")
                .parse::<toml_edit::DocumentMut>()
                .expect("parse toml");
            let providers = doc
                .get("model_providers")
                .and_then(|v| v.as_table())
                .expect("model providers table");
            assert!(providers.get("user_provider").is_some());
            assert!(providers.get("onespace_provider_a").is_some());
            assert!(providers.get("onespace_provider_b").is_some());
            assert_eq!(
                doc.get("model_provider").and_then(|v| v.as_str()),
                Some("onespace_provider_b")
            );
            assert_eq!(doc.get("model").and_then(|v| v.as_str()), Some("gpt-5.5"));
        });
    }

    #[test]
    fn codex_unmanaged_reset_only_removes_onespace_provider_and_api_key() {
        with_temp_dir("codex-reset-unmanaged", |home| {
            let codex_dir = home.join(".codex");
            fs::create_dir_all(&codex_dir).expect("create codex dir");
            write_test_file(
                &codex_dir.join("auth.json"),
                r#"{
  "OPENAI_API_KEY": "key",
  "tokens": {"id_token": "login-token"}
}"#,
            );
            write_test_file(
                &codex_dir.join("config.toml"),
                r#"forced_login_method = "api"
model = "gpt-5.5"
model_provider = "onespace_provider_a"

[model_providers.user_provider]
name = "User Provider"
base_url = "https://user.example.com/v1"

[model_providers.onespace_provider_a]
name = "Provider A"
base_url = "https://a.example.com/v1"
wire_api = "responses"
"#,
            );

            let outputs = render_codex_reset_to_unmanaged_at_home(home).expect("render reset");
            let auth: Value = serde_json::from_str(&rendered_content(&outputs, ".codex/auth.json"))
                .expect("parse auth");
            assert!(auth.get("OPENAI_API_KEY").is_none());
            assert_eq!(
                auth.pointer("/tokens/id_token").and_then(|v| v.as_str()),
                Some("login-token")
            );

            let doc = rendered_content(&outputs, ".codex/config.toml")
                .parse::<toml_edit::DocumentMut>()
                .expect("parse toml");
            assert!(doc.get("forced_login_method").is_none());
            assert!(doc.get("model").is_none());
            assert!(doc.get("model_provider").is_none());
            let providers = doc
                .get("model_providers")
                .and_then(|v| v.as_table())
                .expect("model providers table");
            assert!(providers.get("user_provider").is_some());
            assert!(providers.get("onespace_provider_a").is_none());
        });
    }

    #[test]
    fn codex_system_import_reads_active_model_provider_table() {
        with_temp_dir("codex-system-import-model-provider", |home| {
            let codex_dir = home.join(".codex");
            fs::create_dir_all(&codex_dir).expect("create codex dir");
            write_test_file(
                &codex_dir.join("auth.json"),
                r#"{"OPENAI_API_KEY":"import-key"}"#,
            );
            write_test_file(
                &codex_dir.join("config.toml"),
                r#"forced_login_method = "api"
model = "gpt-5.5"
model_provider = "onespace_imported"

[model_providers.onespace_imported]
name = "Imported"
base_url = "https://import.example.com/v1"
wire_api = "responses"
"#,
            );

            let provider = read_system_provider_at_home("codex", home).expect("system provider");
            assert_eq!(provider.core.api_key, "import-key");
            assert_eq!(provider.core.model.as_deref(), Some("gpt-5.5"));
            assert_eq!(
                provider.core.base_url.as_deref(),
                Some("https://import.example.com/v1")
            );
            assert_eq!(
                provider
                    .tool_config
                    .get("codex_auth_mode")
                    .and_then(|v| v.as_str()),
                Some("api")
            );
            assert_eq!(
                provider
                    .tool_config
                    .get("wire_api")
                    .and_then(|v| v.as_str()),
                Some("responses")
            );
        });
    }

    #[test]
    fn claude_system_import_reads_supported_capabilities_and_effort() {
        with_temp_dir("claude-system-import-capabilities", |home| {
            let claude_dir = home.join(".claude");
            fs::create_dir_all(&claude_dir).expect("create claude dir");
            write_test_file(
                &claude_dir.join("settings.json"),
                r#"{
  "env": {
    "ANTHROPIC_API_KEY": "import-key",
    "ANTHROPIC_BASE_URL": "https://example.com",
    "ANTHROPIC_MODEL": "claude-sonnet-4-5[1m]",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-4-5[1m]",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "Sonnet",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES": "image,pdfs",
    "CLAUDE_CODE_EFFORT_LEVEL": "max"
  }
}"#,
            );

            let provider = read_system_provider_at_home("claude", home).expect("system provider");
            assert_eq!(provider.core.api_key, "import-key");
            assert_eq!(
                provider
                    .tool_config
                    .get("claude_reasoning_effort")
                    .and_then(|v| v.as_str()),
                Some("max")
            );

            let mappings: Vec<ClaudeModelMapping> = serde_json::from_value(
                provider
                    .tool_config
                    .get("claude_model_mappings")
                    .cloned()
                    .expect("claude model mappings"),
            )
            .expect("parse mappings");
            let sonnet = mappings
                .iter()
                .find(|mapping| mapping.family == "sonnet")
                .expect("sonnet mapping");
            assert_eq!(sonnet.upstream_model, "claude-sonnet-4-5");
            assert_eq!(sonnet.supports_1m, Some(true));
            assert_eq!(sonnet.display_name, "Sonnet");
            assert_eq!(
                sonnet.supported_capabilities.as_ref(),
                Some(&vec!["image".to_string(), "pdfs".to_string()])
            );
        });
    }

    #[test]
    fn claude_system_import_prefers_env_default_model_over_top_level_model() {
        with_temp_dir("claude-system-import-default-model-priority", |home| {
            let claude_dir = home.join(".claude");
            fs::create_dir_all(&claude_dir).expect("create claude dir");
            write_test_file(
                &claude_dir.join("settings.json"),
                r#"{
  "model": "top-level-model",
  "env": {
    "ANTHROPIC_API_KEY": "import-key",
    "ANTHROPIC_MODEL": "env-model"
  }
}"#,
            );

            let provider = read_system_provider_at_home("claude", home).expect("system provider");
            assert_eq!(provider.core.model.as_deref(), Some("env-model"));
            assert_eq!(
                provider
                    .tool_config
                    .get("claude_default_model")
                    .and_then(|v| v.as_str()),
                Some("env-model")
            );
        });
    }

    #[test]
    fn render_claude_to_dir_writes_supported_capabilities_and_selected_effort() {
        with_temp_dir("claude-render-capabilities", |home| {
            let outputs = render_claude_to_dir(
                &ProviderRecord {
                    core: ProviderCore {
                        id: "claude-custom".to_string(),
                        name: "Claude Custom".to_string(),
                        tool: "claude".to_string(),
                        api_key: "render-key".to_string(),
                        code: Some("claude-custom".to_string()),
                        base_url: Some("https://example.com".to_string()),
                        model: None,
                    },
                    runtime_policy: ProviderRuntimePolicy::default(),
                    favorite_at: None,
                    tool_config: serde_json::from_str(
                        r#"{
                            "claude_default_model": "claude-sonnet-4-5[1m]",
                            "claude_reasoning_effort": "auto",
                            "claude_model_mappings": [
                                {
                                    "family": "haiku",
                                    "display_name": "Haiku",
                                    "upstream_model": "claude-haiku-4-5",
                                    "supported_capabilities": ["prompt-cache"]
                                },
                                {
                                    "family": "sonnet",
                                    "display_name": "Sonnet",
                                    "upstream_model": "claude-sonnet-4-5",
                                    "supports_1m": true,
                                    "supported_capabilities": ["image", "pdfs"]
                                }
                            ]
                        }"#,
                    )
                    .unwrap(),
                    history: vec![],
                    extra: Map::new(),
                    is_enabled: Some(true),
                    provider_key: None,
                },
                &home.join(".claude"),
            )
            .expect("render claude");

            let rendered: Value =
                serde_json::from_str(&rendered_content(&outputs, ".claude/settings.json"))
                    .expect("parse rendered");
            let env = rendered["env"].as_object().expect("env");
            assert_eq!(
                rendered["model"],
                Value::String("claude-sonnet-4-5[1m]".to_string())
            );
            assert_eq!(
                env["CLAUDE_CODE_EFFORT_LEVEL"],
                Value::String("auto".to_string())
            );
            assert_eq!(
                env["ANTHROPIC_DEFAULT_HAIKU_MODEL_SUPPORTED_CAPABILITIES"],
                Value::String("prompt-cache".to_string())
            );
            assert_eq!(
                env["ANTHROPIC_DEFAULT_SONNET_MODEL"],
                Value::String("claude-sonnet-4-5[1m]".to_string())
            );
            assert_eq!(
                env["ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES"],
                Value::String("image,pdfs".to_string())
            );
        });
    }

    #[test]
    fn render_claude_to_dir_ignores_legacy_mapping_reasoning_effort() {
        with_temp_dir("claude-render-ignores-legacy-mapping-effort", |home| {
            let outputs = render_claude_to_dir(
                &ProviderRecord {
                    core: ProviderCore {
                        id: "claude-custom".to_string(),
                        name: "Claude Custom".to_string(),
                        tool: "claude".to_string(),
                        api_key: "render-key".to_string(),
                        code: Some("claude-custom".to_string()),
                        base_url: Some("https://example.com".to_string()),
                        model: None,
                    },
                    runtime_policy: ProviderRuntimePolicy::default(),
                    favorite_at: None,
                    tool_config: serde_json::from_str(
                        r#"{
                            "claude_default_model": "claude-sonnet-4-5[1m]",
                            "claude_reasoning_effort": "auto",
                            "claude_model_mappings": [
                                {
                                    "family": "sonnet",
                                    "display_name": "Sonnet",
                                    "upstream_model": "claude-sonnet-4-5",
                                    "supports_1m": true,
                                    "reasoning_effort": "xhigh"
                                }
                            ]
                        }"#,
                    )
                    .unwrap(),
                    history: vec![],
                    extra: Map::new(),
                    is_enabled: Some(true),
                    provider_key: None,
                },
                &home.join(".claude"),
            )
            .expect("render claude");

            let rendered: Value =
                serde_json::from_str(&rendered_content(&outputs, ".claude/settings.json"))
                    .expect("parse rendered");
            let env = rendered["env"].as_object().expect("env");
            assert_eq!(
                env["CLAUDE_CODE_EFFORT_LEVEL"],
                Value::String("auto".to_string())
            );
        });
    }

    #[test]
    fn render_claude_to_dir_removes_top_level_and_env_model_when_default_is_empty() {
        with_temp_dir("claude-render-removes-empty-default-model", |home| {
            let claude_dir = home.join(".claude");
            fs::create_dir_all(&claude_dir).expect("create claude dir");
            write_test_file(
                &claude_dir.join("settings.json"),
                r#"{
  "model": "old-model",
  "env": {
    "ANTHROPIC_API_KEY": "render-key",
    "ANTHROPIC_MODEL": "old-model"
  }
}"#,
            );

            let outputs = render_claude_to_dir(
                &ProviderRecord {
                    core: ProviderCore {
                        id: "claude-custom".to_string(),
                        name: "Claude Custom".to_string(),
                        tool: "claude".to_string(),
                        api_key: "render-key".to_string(),
                        code: Some("claude-custom".to_string()),
                        base_url: Some("https://example.com".to_string()),
                        model: None,
                    },
                    runtime_policy: ProviderRuntimePolicy::default(),
                    favorite_at: None,
                    tool_config: Map::new(),
                    history: vec![],
                    extra: Map::new(),
                    is_enabled: Some(true),
                    provider_key: None,
                },
                &claude_dir,
            )
            .expect("render claude");

            let rendered: Value =
                serde_json::from_str(&rendered_content(&outputs, ".claude/settings.json"))
                    .expect("parse rendered");
            assert!(rendered.get("model").is_none());
            assert!(
                rendered["env"]
                    .as_object()
                    .expect("env")
                    .get("ANTHROPIC_MODEL")
                    .is_none()
            );
        });
    }

    fn launcher_item(
        id: &str,
        pinned: bool,
        pin_order: u32,
        last_launched_at: Option<u64>,
    ) -> LauncherRecord {
        LauncherRecord {
            id: id.to_string(),
            name: format!("item-{}", id),
            item_type: "script".to_string(),
            target: "echo hello".to_string(),
            pinned,
            pin_order,
            launch_count: 0,
            last_launched_at,
            trusted: false,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn launcher_sort_prefers_pinned_then_recent() {
        let mut items = vec![
            launcher_item("a", false, 0, Some(100)),
            launcher_item("b", true, 1, Some(1)),
            launcher_item("c", true, 0, Some(50)),
            launcher_item("d", false, 0, Some(200)),
        ];
        sort_launcher_items(&mut items);
        let ids: Vec<String> = items.into_iter().map(|it| it.id).collect();
        assert_eq!(ids, vec!["c", "b", "d", "a"]);
    }

    #[test]
    fn launcher_merge_overwrites_same_id() {
        let mut existing = vec![
            launcher_item("a", false, 0, Some(10)),
            launcher_item("b", false, 0, Some(20)),
        ];
        let mut updated_a = launcher_item("a", true, 0, Some(30));
        updated_a.name = "updated".to_string();
        let new_c = launcher_item("c", false, 0, Some(40));
        merge_launcher_items(&mut existing, vec![updated_a.clone(), new_c.clone()]);
        assert_eq!(existing.len(), 3);
        assert!(existing.iter().any(|it| it.id == "c"));
        let a = existing
            .iter()
            .find(|it| it.id == "a")
            .expect("a should exist");
        assert_eq!(a.name, "updated");
        assert!(a.pinned);
    }

    #[test]
    fn launcher_import_input_defaults() {
        let now = 1000;
        let input = LauncherItemInput {
            id: None,
            name: "docs".to_string(),
            item_type: "url".to_string(),
            target: "https://example.com".to_string(),
            ..LauncherItemInput::default()
        };
        let parsed = launcher_record_from_import_input(input, now)
            .expect("parse launcher input should work");
        assert!(!parsed.id.is_empty());
        assert_eq!(parsed.item_type, "url");
        assert_eq!(parsed.created_at, now);
        assert_eq!(parsed.updated_at, now);
        assert!(parsed.trusted);
    }

    #[test]
    fn normalize_app_target_accepts_open_command() {
        let parsed = normalize_app_target("open -a \"Visual Studio Code\"")
            .expect("should parse open -a form");
        assert_eq!(parsed, "Visual Studio Code");
    }

    #[test]
    fn normalize_app_target_strips_smart_quotes() {
        let parsed = normalize_app_target("open -a “WPS”").expect("should strip smart quotes");
        assert_eq!(parsed, "WPS");
        let parsed2 = normalize_app_target("“微信").expect("should strip leading smart quote");
        assert_eq!(parsed2, "微信");
    }

    #[test]
    fn normalize_icon_candidate_name_adds_icns_extension() {
        assert_eq!(
            normalize_icon_candidate_name("AppIcon"),
            Some("AppIcon.icns".to_string())
        );
        assert_eq!(
            normalize_icon_candidate_name("Foo.icns"),
            Some("Foo.icns".to_string())
        );
    }

    #[test]
    fn extract_icon_candidates_from_plist_json_collects_expected_keys() {
        let plist = json!({
            "CFBundleIconFile": "MainIcon",
            "CFBundleIconName": "NamedIcon",
            "CFBundleIcons": {
                "CFBundlePrimaryIcon": {
                    "CFBundleIconFiles": ["SmallIcon", "LargeIcon"]
                }
            },
            "CFBundleIconFiles": ["FallbackIcon"]
        });

        let candidates = extract_icon_candidates_from_plist_json(&plist);
        assert!(candidates.iter().any(|it| it == "MainIcon.icns"));
        assert!(candidates.iter().any(|it| it == "NamedIcon.icns"));
        assert!(candidates.iter().any(|it| it == "LargeIcon.icns"));
        assert!(candidates.iter().any(|it| it == "FallbackIcon.icns"));
        assert!(candidates.iter().any(|it| it == "AppIcon.icns"));
    }

    #[test]
    fn sync_directory_bidirectional_exports_when_local_is_newer() {
        let root = make_temp_dir("sync-dir-export");
        let local = root.join("local");
        let shared = root.join("shared");
        fs::create_dir_all(&local).expect("create local");
        fs::create_dir_all(&shared).expect("create shared");

        let rel = Path::new("repo").join("skill.md");
        write_test_file(&shared.join(&rel), "shared-old");
        sleep(Duration::from_secs(1));
        write_test_file(&local.join(&rel), "local-new");

        let mut warnings = vec![];
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")
            .expect("sync should succeed");

        let synced = fs::read_to_string(shared.join(&rel)).expect("read shared");
        assert_eq!(synced, "local-new");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_directory_bidirectional_imports_when_shared_is_newer() {
        let root = make_temp_dir("sync-dir-import");
        let local = root.join("local");
        let shared = root.join("shared");
        fs::create_dir_all(&local).expect("create local");
        fs::create_dir_all(&shared).expect("create shared");

        let rel = Path::new("meta").join("index.json");
        write_test_file(&local.join(&rel), "local-old");
        sleep(Duration::from_secs(1));
        write_test_file(&shared.join(&rel), "shared-new");

        let mut warnings = vec![];
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")
            .expect("sync should succeed");

        let synced = fs::read_to_string(local.join(&rel)).expect("read local");
        assert_eq!(synced, "shared-new");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_directory_bidirectional_copies_single_side_files() {
        let root = make_temp_dir("sync-dir-single-side");
        let local = root.join("local");
        let shared = root.join("shared");
        fs::create_dir_all(&local).expect("create local");
        fs::create_dir_all(&shared).expect("create shared");

        let rel_local_only = Path::new("repository").join("local-only.txt");
        let rel_shared_only = Path::new("meta").join("shared-only.json");
        write_test_file(&local.join(&rel_local_only), "from-local");
        write_test_file(&shared.join(&rel_shared_only), "from-shared");

        let mut warnings = vec![];
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")
            .expect("sync should succeed");

        assert_eq!(
            fs::read_to_string(shared.join(&rel_local_only)).expect("read shared copy"),
            "from-local"
        );
        assert_eq!(
            fs::read_to_string(local.join(&rel_shared_only)).expect("read local copy"),
            "from-shared"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_directory_bidirectional_skips_export_when_icloud_placeholder_exists() {
        let root = make_temp_dir("sync-dir-icloud-placeholder");
        let local = root.join("local");
        let shared = root.join("shared");
        fs::create_dir_all(&local).expect("create local");
        fs::create_dir_all(&shared).expect("create shared");

        let rel = Path::new("repository").join("pending-skill.md");
        write_test_file(&local.join(&rel), "local-content");
        write_test_file(
            &shared.join("repository").join("pending-skill.md.icloud"),
            "",
        );

        let mut warnings = vec![];
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")
            .expect("sync should succeed");

        assert!(!shared.join(&rel).exists());
        assert!(warnings
            .iter()
            .any(|w| w.contains("shared file pending download")));
        assert!(warnings
            .iter()
            .any(|w| w.contains("skip exporting while shared file is pending download")));

        let _ = fs::remove_dir_all(&root);
    }

    fn history_entry(
        tool: &str,
        tool_session_id: &str,
        title: &str,
        working_dir: &str,
        model_name: Option<&str>,
        created_at_ms: i64,
        updated_at_ms: i64,
    ) -> ai_sessions::HistorySessionEntry {
        ai_sessions::HistorySessionEntry {
            tool: tool.to_string(),
            tool_session_id: tool_session_id.to_string(),
            title: title.to_string(),
            working_dir: working_dir.to_string(),
            model_name: model_name.map(|value| value.to_string()),
            created_at_ms,
            updated_at_ms,
        }
    }

    fn session_record(
        id: &str,
        tool: &str,
        working_dir: &str,
        created_at: u64,
        status: &str,
    ) -> SessionRecord {
        SessionRecord {
            id: id.to_string(),
            name: String::new(),
            working_dir: working_dir.to_string(),
            tool: tool.to_string(),
            tool_session_id: String::new(),
            model_name: None,
            name_source: "history".to_string(),
            runtime_mode: "shared".to_string(),
            runtime_profile_id: None,
            preset_id: None,
            created_at,
            last_used_at: created_at,
            status: status.to_string(),
            favorited_at: None,
            provider_id: None,
        }
    }

    #[test]
    fn cli_lookup_prefers_tool_session_id_over_record_id() {
        let mut state = SessionsState::default();

        let mut first = session_record("record-id", "codex", "/tmp/cli-lookup-one", 1, "active");
        first.tool_session_id = "ses_123".to_string();
        state.sessions.push(first);

        let mut second = session_record("ses_123", "claude", "/tmp/cli-lookup-two", 2, "active");
        second.tool_session_id = "claude_456".to_string();
        state.sessions.push(second);

        let matched = find_cli_session_in_state(&state, "ses_123").expect("session should match");

        assert_eq!(matched.tool, "codex");
        assert_eq!(matched.tool_session_id, "ses_123");
        assert_eq!(matched.working_dir, "/tmp/cli-lookup-one");
        assert_eq!(matched.id, "record-id");
    }

    #[test]
    fn cli_lookup_falls_back_to_record_id() {
        let mut state = SessionsState::default();
        let mut session =
            session_record("history-codex-1", "codex", "/tmp/cli-lookup", 1, "active");
        session.tool_session_id = "ses_999".to_string();
        state.sessions.push(session);

        let matched =
            find_cli_session_in_state(&state, "history-codex-1").expect("session should match");

        assert_eq!(matched.tool, "codex");
        assert_eq!(matched.tool_session_id, "ses_999");
        assert_eq!(matched.id, "history-codex-1");
    }

    #[test]
    fn create_flow_marks_session_active_when_launch_returns_real_session_id() {
        let working_dir = normalize_session_working_dir("/tmp/opencode-create-active");
        let mut session = session_record(
            "created",
            "opencode",
            &working_dir,
            1_700_000_000,
            "pending_bind",
        );

        apply_resolved_session_id_after_create(&mut session, Some(" ses_123 "), 1_700_000_111);

        assert_eq!(session.tool_session_id, "ses_123");
        assert_eq!(session.status, "active");
        assert_eq!(session.last_used_at, 1_700_000_111);
    }

    #[test]
    fn create_flow_keeps_session_pending_bind_when_launch_returns_no_session_id() {
        let working_dir = normalize_session_working_dir("/tmp/opencode-create-pending");
        let mut session =
            session_record("created", "opencode", &working_dir, 1_700_000_000, "active");
        session.tool_session_id = "stale-session-id".to_string();

        apply_resolved_session_id_after_create(&mut session, None, 1_700_000_222);

        assert!(session.tool_session_id.is_empty());
        assert_eq!(session.status, "pending_bind");
        assert_eq!(session.last_used_at, 1_700_000_222);
    }

    #[test]
    fn history_sync_binds_placeholder_session() {
        let working_dir = normalize_session_working_dir("/tmp/history-bind");
        let mut state = SessionsState {
            sessions: vec![session_record(
                "placeholder",
                "codex",
                &working_dir,
                1_700_000_000,
                "pending_bind",
            )],
            ..SessionsState::default()
        };

        let outcome = apply_history_entries_to_sessions_state(
            &mut state,
            "codex",
            vec![history_entry(
                "codex",
                "codex-session-1",
                "Imported Codex Title",
                &working_dir,
                Some("gpt-5.4"),
                1_700_000_001_000,
                1_700_000_005_000,
            )],
            1_700_000_010,
        );

        assert!(outcome.list_changed);
        assert_eq!(state.sessions.len(), 1);
        let session = &state.sessions[0];
        assert_eq!(session.tool_session_id, "codex-session-1");
        assert_eq!(session.name, "Imported Codex Title");
        assert_eq!(session.model_name.as_deref(), Some("gpt-5.4"));
        assert_eq!(session.status, "active");
    }

    #[test]
    fn history_sync_preserves_manual_name_but_updates_model() {
        let working_dir = normalize_session_working_dir("/tmp/history-manual");
        let mut session =
            session_record("existing", "claude", &working_dir, 1_700_000_000, "active");
        session.name = "Manual Title".to_string();
        session.name_source = "manual".to_string();
        session.tool_session_id = "claude-session-1".to_string();
        let mut state = SessionsState {
            sessions: vec![session],
            ..SessionsState::default()
        };

        let outcome = apply_history_entries_to_sessions_state(
            &mut state,
            "claude",
            vec![history_entry(
                "claude",
                "claude-session-1",
                "History Title",
                &working_dir,
                Some("qwen3.5-plus"),
                1_700_000_000_000,
                1_700_000_009_000,
            )],
            1_700_000_010,
        );

        assert!(outcome.list_changed);
        let session = &state.sessions[0];
        assert_eq!(session.name, "Manual Title");
        assert_eq!(session.model_name.as_deref(), Some("qwen3.5-plus"));
    }

    #[test]
    fn history_sync_preserves_existing_favorite_timestamp() {
        let working_dir = normalize_session_working_dir("/tmp/history-favorite");
        let mut session =
            session_record("existing", "codex", &working_dir, 1_700_000_000, "active");
        session.tool_session_id = "codex-session-1".to_string();
        session.favorited_at = Some(1_700_000_123);
        let mut state = SessionsState {
            sessions: vec![session],
            ..SessionsState::default()
        };

        let outcome = apply_history_entries_to_sessions_state(
            &mut state,
            "codex",
            vec![history_entry(
                "codex",
                "codex-session-1",
                "Updated Title",
                &working_dir,
                Some("gpt-5.5"),
                1_700_000_001_000,
                1_700_000_009_000,
            )],
            1_700_000_010,
        );

        assert!(outcome.list_changed);
        let session = &state.sessions[0];
        assert_eq!(session.favorited_at, Some(1_700_000_123));
        assert_eq!(session.name, "Updated Title");
    }

    #[test]
    fn history_sync_skips_tombstoned_sessions() {
        let working_dir = normalize_session_working_dir("/tmp/history-tombstone");
        let mut state = SessionsState::default();
        state
            .tombstones
            .insert(history_tombstone_key("gemini", "gemini-session-1").expect("tombstone key"));

        let outcome = apply_history_entries_to_sessions_state(
            &mut state,
            "gemini",
            vec![history_entry(
                "gemini",
                "gemini-session-1",
                "Should Stay Hidden",
                &working_dir,
                Some("gemini-3-pro-preview"),
                1_700_000_000_000,
                1_700_000_001_000,
            )],
            1_700_000_010,
        );

        assert!(!outcome.list_changed);
        assert!(state.sessions.is_empty());
    }

    #[test]
    fn normalize_sessions_state_marks_existing_backfills_with_baseline_parser_version() {
        let mut state = SessionsState::default();
        let codex = state
            .history_sync
            .tools
            .entry("codex".to_string())
            .or_insert_with(SessionsHistoryToolState::default);
        codex.full_backfill_done = true;
        codex.parser_version = 0;

        let changed = normalize_sessions_state(&mut state);

        assert!(changed);
        assert_eq!(
            state
                .history_sync
                .tools
                .get("codex")
                .map(|tool| tool.parser_version),
            Some(HISTORY_SYNC_BASE_PARSER_VERSION)
        );
    }

    #[test]
    fn history_sync_requires_full_backfill_when_codex_parser_version_is_stale() {
        let tool_state = SessionsHistoryToolState {
            full_backfill_done: true,
            parser_version: HISTORY_SYNC_BASE_PARSER_VERSION,
            last_seen_updated_at_ms: 1,
            last_completed_at: Some(1),
        };

        assert!(history_sync_requires_full_backfill(
            "codex",
            Some(&tool_state)
        ));
        assert!(!history_sync_requires_full_backfill(
            "claude",
            Some(&tool_state)
        ));
    }

    #[test]
    fn history_sync_requires_full_backfill_when_opencode_parser_version_is_stale() {
        let tool_state = SessionsHistoryToolState {
            full_backfill_done: true,
            parser_version: HISTORY_SYNC_BASE_PARSER_VERSION,
            last_seen_updated_at_ms: 1,
            last_completed_at: Some(1),
        };

        assert!(history_sync_requires_full_backfill(
            "opencode",
            Some(&tool_state)
        ));
        assert!(!history_sync_requires_full_backfill(
            "gemini",
            Some(&tool_state)
        ));
    }

    #[test]
    fn sort_sessions_favorited_first() {
        let mut sessions = vec![
            session_record("a", "claude", "/tmp", 100, "active"),
            session_record("b", "claude", "/tmp", 200, "active"),
        ];
        sessions[0].favorited_at = Some(150);

        sort_sessions_for_display(&mut sessions);
        assert_eq!(sessions[0].id, "a");
        assert_eq!(sessions[1].id, "b");
    }

    #[test]
    fn sort_sessions_multiple_favorites_by_favorited_at_desc() {
        let mut sessions = vec![
            session_record("a", "claude", "/tmp", 100, "active"),
            session_record("b", "claude", "/tmp", 200, "active"),
            session_record("c", "claude", "/tmp", 300, "active"),
        ];
        sessions[0].favorited_at = Some(150);
        sessions[2].favorited_at = Some(350);

        sort_sessions_for_display(&mut sessions);
        assert_eq!(sessions[0].id, "c");
        assert_eq!(sessions[1].id, "a");
        assert_eq!(sessions[2].id, "b");
    }

    #[test]
    fn sort_sessions_non_favoritized_by_last_used_desc() {
        let mut sessions = vec![
            session_record("a", "claude", "/tmp", 100, "active"),
            session_record("b", "claude", "/tmp", 200, "active"),
        ];

        sort_sessions_for_display(&mut sessions);
        assert_eq!(sessions[0].id, "b");
        assert_eq!(sessions[1].id, "a");
    }

    #[test]
    fn favorited_sessions_survive_history_window_filter() {
        let cutoff = session_history_cutoff_ts();
        let old_ts = cutoff.saturating_sub(86400 * 30); // 30 days ago
        let mut sessions = vec![session_record("old", "claude", "/tmp", old_ts, "active")];
        sessions[0].favorited_at = Some(old_ts);
        sessions[0].last_used_at = old_ts;

        let filtered = filter_sessions_by_history_window(sessions.iter());
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "old");
    }

    #[test]
    fn set_favorite_marks_session_with_timestamp() {
        let mut state = SessionsState::default();
        state
            .sessions
            .push(session_record("s1", "claude", "/tmp", 100, "active"));
        assert!(state.sessions[0].favorited_at.is_none());

        let result = set_session_favorite_impl(&mut state, "s1", true);
        assert!(result.is_ok());
        assert!(state.sessions[0].favorited_at.is_some());
        let ts = state.sessions[0].favorited_at.unwrap();
        assert!(ts >= 100);
    }

    #[test]
    fn unfavorite_clears_timestamp() {
        let mut state = SessionsState::default();
        state
            .sessions
            .push(session_record("s1", "claude", "/tmp", 100, "active"));
        state.sessions[0].favorited_at = Some(500);

        set_session_favorite_impl(&mut state, "s1", false).unwrap();
        assert!(state.sessions[0].favorited_at.is_none());
    }

    #[test]
    fn refavorite_keeps_original_timestamp() {
        let mut state = SessionsState::default();
        state
            .sessions
            .push(session_record("s1", "claude", "/tmp", 100, "active"));
        let first_ts = 500u64;
        state.sessions[0].favorited_at = Some(first_ts);

        set_session_favorite_impl(&mut state, "s1", true).unwrap();
        assert_eq!(state.sessions[0].favorited_at, Some(first_ts));
    }

    #[test]
    fn set_favorite_marks_service_provider_with_timestamp() {
        let mut state = ServiceProvidersState {
            active: HashMap::new(),
            providers: vec![ServiceProviderRecord {
                id: "p1".to_string(),
                name: "Provider 1".to_string(),
                tool: "codex".to_string(),
                api_key: "key".to_string(),
                favorite_at: None,
                ..ServiceProviderRecord::default()
            }],
        };

        let result = set_service_provider_favorite_impl(&mut state, "p1", true);
        assert!(result.is_ok());
        assert!(state.providers[0].favorite_at.is_some());
    }

    #[test]
    fn unset_favorite_clears_service_provider_timestamp() {
        let mut state = ServiceProvidersState {
            active: HashMap::new(),
            providers: vec![ServiceProviderRecord {
                id: "p1".to_string(),
                name: "Provider 1".to_string(),
                tool: "codex".to_string(),
                api_key: "key".to_string(),
                favorite_at: Some(123),
                ..ServiceProviderRecord::default()
            }],
        };

        set_service_provider_favorite_impl(&mut state, "p1", false).unwrap();
        assert_eq!(state.providers[0].favorite_at, None);
    }

    #[test]
    fn refavorite_service_provider_keeps_original_timestamp() {
        let mut state = ServiceProvidersState {
            active: HashMap::new(),
            providers: vec![ServiceProviderRecord {
                id: "p1".to_string(),
                name: "Provider 1".to_string(),
                tool: "codex".to_string(),
                api_key: "key".to_string(),
                favorite_at: Some(456),
                ..ServiceProviderRecord::default()
            }],
        };

        set_service_provider_favorite_impl(&mut state, "p1", true).unwrap();
        assert_eq!(state.providers[0].favorite_at, Some(456));
    }

    #[test]
    fn provider_conversion_chain_preserves_favorite_at() {
        let mut sp = ServiceProviderRecord {
            id: "p1".to_string(),
            name: "Provider 1".to_string(),
            tool: "claude".to_string(),
            api_key: "key".to_string(),
            favorite_at: Some(789),
            ..ServiceProviderRecord::default()
        };
        sp.tool_config
            .insert("remark".to_string(), Value::String("note".to_string()));

        let value = service_provider_to_value(&sp);
        assert_eq!(value.get("favorite_at").and_then(|v| v.as_u64()), Some(789));

        let from_value = service_provider_from_value(value.clone(), None);
        assert_eq!(from_value.favorite_at, Some(789));

        let legacy = service_provider_to_legacy(&sp);
        assert_eq!(
            legacy.get("favorite_at").and_then(|v| v.as_u64()),
            Some(789)
        );

        let provider = service_provider_to_provider_record(&sp);
        assert_eq!(provider.favorite_at, Some(789));

        let input = provider_input_from_value(&legacy).expect("provider input");
        assert_eq!(input.favorite_at, Some(789));

        let restored = provider_from_input(input, None);
        assert_eq!(restored.favorite_at, Some(789));
    }

    #[test]
    fn migrate_providers_to_service_providers_preserves_favorite_at() {
        let old = ProvidersState {
            active: HashMap::new(),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: "p1".to_string(),
                    name: "Claude".to_string(),
                    tool: "claude".to_string(),
                    api_key: "key".to_string(),
                    code: None,
                    base_url: None,
                    model: None,
                },
                runtime_policy: ProviderRuntimePolicy::default(),
                favorite_at: Some(321),
                tool_config: Map::new(),
                history: vec![],
                extra: Map::new(),
                is_enabled: None,
                provider_key: None,
            }],
        };

        let migrated = migrate_providers_to_service_providers(old);
        assert_eq!(migrated.providers[0].favorite_at, Some(321));
    }

    #[test]
    fn legacy_export_view_includes_favorite_at() {
        let state = ServiceProvidersState {
            active: HashMap::new(),
            providers: vec![ServiceProviderRecord {
                id: "p1".to_string(),
                name: "Provider 1".to_string(),
                tool: "codex".to_string(),
                api_key: "key".to_string(),
                favorite_at: Some(999),
                ..ServiceProviderRecord::default()
            }],
        };

        let legacy = service_providers_to_legacy_view(&state);
        assert_eq!(
            legacy.providers[0]
                .get("favorite_at")
                .and_then(|v| v.as_u64()),
            Some(999)
        );
    }

    #[test]
    fn set_favorite_unknown_session_returns_error() {
        let mut state = SessionsState::default();
        let result = set_session_favorite_impl(&mut state, "nonexistent", true);
        assert!(result.is_err());
    }

    #[test]
    fn set_favorite_persists_to_disk() {
        with_temp_dir("set-favorite-persists", |_| {
            let _ = config::get_app_dir().unwrap();
            let mut state = SessionsState::default();
            state
                .sessions
                .push(session_record("s1", "claude", "/tmp", 100, "active"));
            state
                .sessions
                .push(session_record("s2", "codex", "/tmp", 200, "active"));
            save_sessions_state(&state).unwrap();

            set_session_favorite_impl(&mut state, "s1", true).unwrap();
            save_sessions_state(&state).unwrap();

            let reloaded = load_sessions_state().unwrap();
            let s1 = reloaded.sessions.iter().find(|s| s.id == "s1").unwrap();
            assert!(s1.favorited_at.is_some());
            let s2 = reloaded.sessions.iter().find(|s| s.id == "s2").unwrap();
            assert!(s2.favorited_at.is_none());
        });
    }

    #[test]
    fn session_to_legacy_includes_favorited_at() {
        let mut state = SessionsState::default();
        state
            .sessions
            .push(session_record("s1", "claude", "/tmp", 100, "active"));

        // Set favorite and check session_to_legacy includes it.
        set_session_favorite_impl(&mut state, "s1", true).unwrap();
        let s1 = state.sessions.iter().find(|s| s.id == "s1").unwrap();
        let json = session_to_legacy(s1);
        assert!(
            json.get("favorited_at").is_some(),
            "favorited_at should be present in session_to_legacy output"
        );
        assert_eq!(json["favorited_at"].as_u64(), s1.favorited_at);

        // Unfavorite and check it's absent.
        set_session_favorite_impl(&mut state, "s1", false).unwrap();
        let s1 = state.sessions.iter().find(|s| s.id == "s1").unwrap();
        let json = session_to_legacy(s1);
        assert!(
            json.get("favorited_at").is_none(),
            "favorited_at should be absent after unfavorite"
        );
    }

    #[test]
    fn filter_and_sort_sessions_with_favorites() {
        let now = now_ts();
        let mut state = SessionsState::default();
        state
            .sessions
            .push(session_record("s1", "claude", "/tmp", now, "active"));
        state
            .sessions
            .push(session_record("s2", "codex", "/tmp", now, "active"));
        set_session_favorite_impl(&mut state, "s1", true).unwrap();

        // Simulate what sessions_list does: filter and sort.
        let filtered = filter_sessions_by_history_window(state.sessions.iter());

        assert_eq!(
            filtered.len(),
            2,
            "both sessions should be in filtered result"
        );

        // First session should be the favorited one (sorted first).
        assert_eq!(filtered[0].id, "s1");
        assert!(filtered[0].favorited_at.is_some());

        let json0 = session_to_legacy(&filtered[0]);
        assert!(
            json0.get("favorited_at").is_some(),
            "favorited session must include favorited_at in JSON"
        );

        // Second session is not favorited.
        assert_eq!(filtered[1].id, "s2");
        let json1 = session_to_legacy(&filtered[1]);
        assert!(
            json1.get("favorited_at").is_none(),
            "non-favorited session must not include favorited_at"
        );
    }

    // --- Permission mode validation tests ---

    #[test]
    fn permission_mode_missing_caller_defaults_ok() {
        // When caller does not pass permissionMode, and config is default → ok
        let config = super::ai_sessions::TerminalPermissionMode::Default;
        let result = super::validate_and_resolve_permission_mode(&config, None);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            super::ai_sessions::TerminalPermissionMode::Default
        );
    }

    #[test]
    fn permission_mode_config_full_access_requires_confirmation() {
        // Config is full_access, caller passes nothing → PERMISSION_CONFIRMATION_REQUIRED
        let config = super::ai_sessions::TerminalPermissionMode::FullAccess;
        let result = super::validate_and_resolve_permission_mode(&config, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.code == "PERMISSION_CONFIRMATION_REQUIRED");
    }

    #[test]
    fn permission_mode_full_access_confirmed_ok() {
        // Config full_access, caller confirms full_access → FullAccess
        let config = super::ai_sessions::TerminalPermissionMode::FullAccess;
        let result = super::validate_and_resolve_permission_mode(&config, Some("full_access"));
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            super::ai_sessions::TerminalPermissionMode::FullAccess
        );
    }

    #[test]
    fn permission_mode_full_access_config_default_override() {
        // Config full_access, caller chooses default → Default
        let config = super::ai_sessions::TerminalPermissionMode::FullAccess;
        let result = super::validate_and_resolve_permission_mode(&config, Some("default"));
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            super::ai_sessions::TerminalPermissionMode::Default
        );
    }

    #[test]
    fn permission_mode_config_default_rejects_elevation() {
        // Config default, caller tries to elevate to full_access → INVALID_PERMISSION_MODE
        let config = super::ai_sessions::TerminalPermissionMode::Default;
        let result = super::validate_and_resolve_permission_mode(&config, Some("full_access"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.code == "INVALID_PERMISSION_MODE");
    }

    #[test]
    fn permission_mode_invalid_caller_value_rejected() {
        // Caller passes a bogus value like "yolo" → INVALID_PERMISSION_MODE
        let config = super::ai_sessions::TerminalPermissionMode::FullAccess;
        let result = super::validate_and_resolve_permission_mode(&config, Some("yolo"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.code == "INVALID_PERMISSION_MODE");
    }

    #[test]
    fn session_provider_id_deserialize_old_json() {
        // Old session JSON without provider_id should deserialize successfully with provider_id = None
        let json = json!({
            "id": "test-session",
            "name": "Test",
            "working_dir": "/tmp",
            "tool": "claude",
            "tool_session_id": "ses_123",
            "created_at": 1000,
            "last_used_at": 1000,
            "status": "active"
        });
        let record: SessionRecord = serde_json::from_value(json).unwrap();
        assert!(record.provider_id.is_none());
    }

    #[test]
    fn session_provider_id_deserialize_new_json() {
        let json = json!({
            "id": "test-session",
            "name": "Test",
            "working_dir": "/tmp",
            "tool": "claude",
            "tool_session_id": "ses_123",
            "created_at": 1000,
            "last_used_at": 1000,
            "status": "active",
            "provider_id": "work-claude"
        });
        let record: SessionRecord = serde_json::from_value(json).unwrap();
        assert_eq!(record.provider_id, Some("work-claude".to_string()));
    }

    #[test]
    fn session_to_legacy_includes_provider_id() {
        let mut record = session_record("s1", "claude", "/tmp", 100, "active");
        record.provider_id = Some("work-claude".to_string());
        let json = session_to_legacy(&record);
        assert_eq!(
            json.get("provider_id").and_then(|v| v.as_str()),
            Some("work-claude")
        );
    }

    #[test]
    fn session_provider_id_none_in_legacy() {
        // session_record already sets provider_id: None
        let record = session_record("s1", "claude", "/tmp", 100, "active");
        let json = session_to_legacy(&record);
        assert_eq!(json.get("provider_id").and_then(|v| v.as_str()), None);
    }

    #[test]
    fn launch_claude_config_dir_with_provider_id() {
        with_temp_dir("launch-claude-config-dir-with-provider", |_| {
            let mut state = load_service_providers_state().unwrap();
            state.providers.push(ServiceProviderRecord {
                id: "work-claude".to_string(),
                name: "Work Claude".to_string(),
                tool: "claude".to_string(),
                icon: None,
                api_key: "sk-test".to_string(),
                base_url: None,
                model: None,
                claude_api_format: "anthropic_messages".to_string(),
                claude_connection_mode: "native_anthropic".to_string(),
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
                code: None,
                is_enabled: Some(true),
                provider_key: None,
                env_managed: Some(true),
                favorite_at: None,
                tool_config: Map::new(),
                history: vec![],
                extra: Map::new(),
                fetched_models: None,
            });
            save_service_providers_internal(&state).unwrap();

            let mut record = session_record("s1", "claude", "/tmp", 100, "active");
            record.provider_id = Some("work-claude".to_string());
            let options = super::launch_options_for_session(&record).unwrap();
            let env = options
                .env
                .expect("Claude with provider_id should have env");
            let dir = env
                .get("CLAUDE_CONFIG_DIR")
                .expect("Should have CLAUDE_CONFIG_DIR");
            assert!(dir.contains("claude_profiles"));
            assert!(dir.contains("work-claude"));
        });
    }

    #[test]
    fn launch_claude_config_dir_without_provider_id() {
        with_temp_dir("launch-claude-config-dir-without-provider", |_| {
            let record = session_record("s1", "claude", "/tmp", 100, "active");
            let options = super::launch_options_for_session(&record).unwrap();
            assert!(options.env.is_none());
        });
    }

    #[test]
    fn launch_claude_config_dir_non_claude_tool() {
        with_temp_dir("launch-claude-config-dir-non-claude", |_| {
            let mut record = session_record("s1", "codex", "/tmp", 100, "active");
            record.provider_id = Some("work-claude".to_string());
            let options = super::launch_options_for_session(&record).unwrap();
            assert!(options.env.is_none());
        });
    }

    #[test]
    fn migrate_providers_to_service_providers_basic() {
        let mut old_tool_config = Map::new();
        old_tool_config.insert(
            "claude_haiku_model".to_string(),
            Value::String("claude-haiku-latest".to_string()),
        );
        old_tool_config.insert(
            "claude_sonnet_model".to_string(),
            Value::String("claude-sonnet-latest".to_string()),
        );
        old_tool_config.insert(
            "claude_opus_model".to_string(),
            Value::String("claude-opus-latest".to_string()),
        );

        let mut active = HashMap::new();
        active.insert("claude".to_string(), "my-claude-id".to_string());

        let old = ProvidersState {
            active,
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: "my-claude-id".to_string(),
                    name: "My Claude".to_string(),
                    tool: "claude".to_string(),
                    api_key: "sk-test".to_string(),
                    base_url: Some("https://api.anthropic.com".to_string()),
                    model: Some("claude-sonnet-latest".to_string()),
                    code: None,
                },
                runtime_policy: Default::default(),
                favorite_at: None,
                tool_config: old_tool_config,
                history: vec![],
                extra: Map::new(),
                is_enabled: Some(true),
                provider_key: None,
            }],
        };

        let new = super::migrate_providers_to_service_providers(old);
        assert_eq!(new.providers.len(), 1);
        let sp = &new.providers[0];
        assert_eq!(sp.id, "my-claude-id");
        assert_eq!(sp.name, "My Claude");
        assert_eq!(sp.tool, "claude");
        assert_eq!(sp.claude_api_format, "anthropic_messages");
        assert_eq!(sp.claude_auth_env_key, "ANTHROPIC_API_KEY"); // non-empty api_key → ANTHROPIC_API_KEY
        assert_eq!(sp.claude_model_mappings.len(), 3);
        assert_eq!(sp.claude_model_mappings[0].family, "haiku");
        assert_eq!(
            sp.claude_model_mappings[0].upstream_model,
            "claude-haiku-latest"
        );
        assert_eq!(
            sp.claude_model_mappings[1].upstream_model,
            "claude-sonnet-latest"
        );
        assert_eq!(
            sp.claude_model_mappings[2].upstream_model,
            "claude-opus-latest"
        );
        assert_eq!(new.active.get("claude"), Some(&"my-claude-id".to_string()));
        assert!(sp.tool_config.get("claude_model_mappings").is_some());
        assert!(sp.tool_config.get("claude_haiku_model").is_none());
    }

    #[test]
    fn migrate_providers_non_claude_tool() {
        let old = ProvidersState {
            active: HashMap::new(),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: "codex-1".to_string(),
                    name: "My Codex".to_string(),
                    tool: "codex".to_string(),
                    api_key: "sk-codex".to_string(),
                    base_url: None,
                    model: Some("o3".to_string()),
                    code: None,
                },
                runtime_policy: Default::default(),
                favorite_at: None,
                tool_config: Map::new(),
                history: vec![],
                extra: Map::new(),
                is_enabled: None,
                provider_key: None,
            }],
        };

        let new = super::migrate_providers_to_service_providers(old);
        assert_eq!(new.providers.len(), 1);
        let sp = &new.providers[0];
        assert_eq!(sp.tool, "codex");
        assert!(sp.claude_model_mappings.is_empty());
        assert_eq!(sp.claude_auth_env_key, "ANTHROPIC_API_KEY"); // default
    }

    #[test]
    fn service_provider_to_provider_record_syncs_claude_api_fields_into_tool_config() {
        let service_provider = ServiceProviderRecord {
            id: "opencode-go".to_string(),
            name: "OpenCode Go".to_string(),
            tool: "claude".to_string(),
            icon: None,
            api_key: "sk-test".to_string(),
            base_url: Some("https://example.com".to_string()),
            model: Some("sonnet".to_string()),
            claude_api_format: "open_ai_chat".to_string(),
            claude_connection_mode: "native_anthropic".to_string(),
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

        let legacy = service_provider_to_provider_record(&service_provider);

        assert_eq!(
            legacy.tool_config.get("claude_api_format"),
            Some(&Value::String("open_ai_chat".to_string()))
        );
        assert_eq!(
            legacy.tool_config.get("claude_connection_mode"),
            Some(&Value::String("native_anthropic".to_string()))
        );
        assert_eq!(
            legacy.tool_config.get("claude_auth_env_key"),
            Some(&Value::String("ANTHROPIC_API_KEY".to_string()))
        );
    }

    #[test]
    fn migrate_legacy_claude_router_provider_preserves_openai_responses_format() {
        let mut tool_config = Map::new();
        tool_config.insert(
            "claude_connection_mode".to_string(),
            Value::String("protocol_router".to_string()),
        );
        tool_config.insert(
            "wire_api".to_string(),
            Value::String("responses".to_string()),
        );

        let old = ProvidersState {
            active: HashMap::new(),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: "router-claude".to_string(),
                    name: "Router Claude".to_string(),
                    tool: "claude".to_string(),
                    api_key: "sk-test".to_string(),
                    code: Some("opencode-go".to_string()),
                    base_url: Some("https://example.com/v1".to_string()),
                    model: Some("claude-sonnet-4".to_string()),
                },
                runtime_policy: ProviderRuntimePolicy::default(),
                favorite_at: None,
                tool_config,
                history: vec![],
                extra: Map::new(),
                is_enabled: Some(true),
                provider_key: None,
            }],
        };

        let migrated = migrate_providers_to_service_providers(old);
        let sp = &migrated.providers[0];
        assert_eq!(sp.claude_api_format, "open_ai_responses");
        assert_eq!(sp.claude_connection_mode, "protocol_router");
        assert_eq!(sp.protocol_router_wire_api, "open_ai_responses");
    }

    #[test]
    fn service_provider_from_value_infers_openai_responses_from_router_fields() {
        let value = json!({
            "id": "router-claude",
            "name": "Router Claude",
            "tool": "claude",
            "api_key": "sk-test",
            "base_url": "https://example.com/v1",
            "claude_connection_mode": "protocol_router",
            "protocol_router_wire_api": "open_ai_responses",
            "tool_config": {
                "wire_api": "responses"
            }
        });

        let record = service_provider_from_value(value, None);
        assert_eq!(record.claude_api_format, "open_ai_responses");
        assert_eq!(record.claude_connection_mode, "protocol_router");
        assert_eq!(record.protocol_router_wire_api, "open_ai_responses");
    }

    #[test]
    fn service_provider_from_value_prefers_top_level_claude_defaults_over_stale_tool_config() {
        let value = json!({
            "id": "work-alicode-plan",
            "name": "Work Alicode Plan",
            "tool": "claude",
            "api_key": "sk-test",
            "claude_default_model": "qwen3.7-plus",
            "claude_reasoning_effort": "xhigh",
            "tool_config": {
                "claude_default_model": "qwen3.6-plus",
                "claude_reasoning_effort": "high"
            }
        });

        let record = service_provider_from_value(value, None);
        assert_eq!(
            record
                .tool_config
                .get("claude_default_model")
                .and_then(|v| v.as_str()),
            Some("qwen3.7-plus")
        );
        assert_eq!(
            record
                .tool_config
                .get("claude_reasoning_effort")
                .and_then(|v| v.as_str()),
            Some("xhigh")
        );
        assert_eq!(record.model.as_deref(), Some("qwen3.7-plus"));
    }

    #[test]
    fn service_provider_from_value_clears_claude_model_when_default_is_empty() {
        let value = json!({
            "id": "work-empty-model",
            "name": "Work Empty Model",
            "tool": "claude",
            "api_key": "sk-test",
            "model": "legacy-model",
            "claude_default_model": "   ",
            "tool_config": {
                "claude_default_model": "legacy-model"
            }
        });

        let record = service_provider_from_value(value, None);
        assert_eq!(record.model, None);
        assert!(record.tool_config.get("claude_default_model").is_none());
    }

    #[test]
    fn normalize_service_provider_record_preserves_opencode_go_openai_responses() {
        let mut record = ServiceProviderRecord {
            id: "opencode-go".to_string(),
            name: "OpenCode Go".to_string(),
            tool: "claude".to_string(),
            icon: None,
            api_key: "sk-test".to_string(),
            base_url: Some("https://opencode.ai/zen/go/v1".to_string()),
            model: Some("claude-sonnet-4".to_string()),
            claude_api_format: "open_ai_responses".to_string(),
            claude_connection_mode: "protocol_router".to_string(),
            protocol_router_upstream_provider_id: None,
            protocol_router_wire_api: "open_ai_responses".to_string(),
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
            env_managed: Some(true),
            favorite_at: None,
            tool_config: Map::new(),
            history: vec![],
            extra: Map::new(),
            fetched_models: None,
        };

        normalize_service_provider_record(&mut record);

        assert_eq!(record.claude_api_format, "open_ai_responses");
        assert_eq!(record.protocol_router_wire_api, "open_ai_responses");
    }
}
