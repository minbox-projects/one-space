use serde::{Deserialize, Serialize};

use super::defaults::{
    default_port, default_template_auto_refresh_minutes, default_true,
    default_usage_retention_days, normalize_template_auto_refresh_minutes,
    DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES, DEFAULT_USAGE_RETENTION_DAYS,
};
use super::pool::ModelPool;
use super::price::ModelPrice;
use super::provider::GatewayUpstreamProvider;
use super::settings::GatewaySettings;
use super::template::ProviderTemplateState;
use super::terminal::TerminalSyncRecord;

/// Current schema version of the persisted gateway configuration. A missing
/// `schema_version` field reads as `0` (legacy) and triggers the one-time
/// migration on the next configuration read; every write persists this value.
///
/// Version 2 replaces the provider-level single `api_key` with the ordered
/// `keys` pool; version 1 and older files are converted once by
/// [`crate::ai_gateway::migration::migrate_legacy_config`]. Version 3 carries the
/// `opencode-go` template identity and provider bindings, replacing the old
/// `opencode-zen` built-in through the same version-gated migration. Version 4
/// replaces a pool's single provider-specific `fallback` candidate with an
/// ordered list of gateway local model ids. Version 5 adds the template-declared
/// sidecar transport and binds the matching untemplated provider to the built-in
/// OpenCode Zen Free template. Version 6 removes the gateway's obsolete
/// independent upstream proxy URL so the application-wide network proxy is the
/// single routing source.
pub const GATEWAY_CONFIG_SCHEMA_VERSION: u32 = 6;

/// Persisted AI Gateway configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Persisted schema version; a missing field reads as `0` (legacy) and the
    /// read path migrates once to [`GATEWAY_CONFIG_SCHEMA_VERSION`].
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub providers: Vec<GatewayUpstreamProvider>,
    #[serde(default)]
    pub keys: Vec<GatewayKey>,
    #[serde(default)]
    pub default_key_id: Option<String>,
    #[serde(default)]
    pub terminal_syncs: Vec<TerminalSyncRecord>,
    /// Request-log retention in days (1-365, default 90). `#[serde(default)]`
    /// keeps older `ai_gateway.json` files readable without migration.
    #[serde(default = "default_usage_retention_days")]
    pub usage_retention_days: u32,
    /// User-maintained upstream-model price table; absent in older configs.
    #[serde(default)]
    pub model_prices: Vec<ModelPrice>,
    /// Persisted provider-template state (last snapshot plus sync metadata).
    #[serde(default)]
    pub provider_templates: Vec<ProviderTemplateState>,
    /// Deleted template IDs; absent in older configs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted_template_ids: Vec<String>,
    /// Interval in minutes between automatic provider-template refreshes; `0`
    /// disables the schedule. `#[serde(default = ...)]` keeps older
    /// `ai_gateway.json` files readable without migration and the value is
    /// always serialized.
    #[serde(
        default = "default_template_auto_refresh_minutes",
        deserialize_with = "deserialize_template_auto_refresh_minutes"
    )]
    pub template_auto_refresh_minutes: u32,
    /// Operator-defined model pools. Always serialized with a `serde(default)`
    /// empty collection, so a configuration written before pools existed loads
    /// unchanged and every later write keeps the field present.
    #[serde(default)]
    pub model_pools: Vec<ModelPool>,
    /// Aliases that were previously owned by a model pool (renamed or deleted)
    /// and must never be acquired by an ordinary provider mapping or served as
    /// an ordinary route. Always serialized with a `serde(default)` empty list.
    #[serde(default)]
    pub model_pool_reserved_models: Vec<String>,
    /// Global dynamic settings for the gateway (timeouts, retries, failure
    /// threshold, CORS, session affinity, etc.).
    #[serde(default)]
    pub settings: GatewaySettings,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            schema_version: GATEWAY_CONFIG_SCHEMA_VERSION,
            enabled: false,
            port: default_port(),
            providers: Vec::new(),
            keys: Vec::new(),
            default_key_id: None,
            terminal_syncs: Vec::new(),
            usage_retention_days: DEFAULT_USAGE_RETENTION_DAYS,
            model_prices: Vec::new(),
            provider_templates: Vec::new(),
            deleted_template_ids: Vec::new(),
            template_auto_refresh_minutes: DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES,
            model_pools: Vec::new(),
            model_pool_reserved_models: Vec::new(),
            settings: GatewaySettings::default(),
        }
    }
}

/// Read a persisted template auto-refresh interval tolerantly: values that are
/// not a plain non-negative JSON integer (null, strings, fractions, negatives
/// or numbers beyond `u32`) read as the default instead of failing the whole
/// config, while integers are normalized like any other stored value.
fn deserialize_template_auto_refresh_minutes<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(raw
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .map(normalize_template_auto_refresh_minutes)
        .unwrap_or(DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES))
}

/// A local API key accepted by the AI Gateway listener.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayKey {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: u64,
}

/// Runtime status summary exposed to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStatus {
    pub running: bool,
    pub enabled: bool,
    pub port: u16,
    pub local_base_url: String,
    pub provider_count: usize,
    pub auto_disabled_count: usize,
    pub key_count: usize,
    pub default_key_id: Option<String>,
}
