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

#[derive(Debug, Serialize, Clone)]
pub struct ProviderHistoryEntry {
    pub ts: u64,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

fn provider_history_ts_from_value(value: Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64().map(|raw| {
            if raw > 10_000_000_000 {
                raw / 1000
            } else {
                raw
            }
        }),
        Value::String(raw) => raw.parse::<u64>().ok().map(|parsed| {
            if parsed > 10_000_000_000 {
                parsed / 1000
            } else {
                parsed
            }
        }),
        _ => None,
    }
}

impl<'de> Deserialize<'de> for ProviderHistoryEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut obj = Map::<String, Value>::deserialize(deserializer)?;
        let ts = obj
            .remove("ts")
            .or_else(|| obj.remove("timestamp"))
            .and_then(provider_history_ts_from_value)
            .ok_or_else(|| {
                serde::de::Error::custom("history timestamp must be number or string")
            })?;
        let action = obj
            .remove("action")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "update".to_string());
        let snapshot = obj.remove("snapshot");
        let content = obj
            .remove("content")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let summary = obj
            .remove("summary")
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        Ok(ProviderHistoryEntry {
            ts,
            action,
            snapshot,
            content,
            summary,
        })
    }
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
