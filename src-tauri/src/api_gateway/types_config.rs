use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::api_gateway) const CONFIG_FILE: &str = "api_gateway.json";
/// Legacy config file kept for one-time read-only migration: if
/// `api_gateway.json` is absent but `api_fusion.json` exists, the stored
/// payload is read through the same decrypt path and rewritten to the new
/// file. Writes never touch the legacy file.
pub(in crate::api_gateway) const LEGACY_CONFIG_FILE: &str = "api_fusion.json";
pub(in crate::api_gateway) const DEFAULT_PORT: u16 = 17688;

/// Default request-log retention in days for configs written before the field existed.
pub const DEFAULT_USAGE_RETENTION_DAYS: u32 = 90;
/// Lower/upper bounds accepted by the retention-days setting.
pub const MIN_USAGE_RETENTION_DAYS: u32 = 1;
pub const MAX_USAGE_RETENTION_DAYS: u32 = 365;

pub(in crate::api_gateway) fn default_port() -> u16 {
    DEFAULT_PORT
}

pub(in crate::api_gateway) fn default_usage_retention_days() -> u32 {
    DEFAULT_USAGE_RETENTION_DAYS
}

pub(in crate::api_gateway) fn default_true() -> bool {
    true
}

pub(in crate::api_gateway) fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A single "local model name -> remote model name" mapping for an upstream provider.
///
/// `protocol` optionally pins this row to one endpoint family. An absent field or
/// JSON `null` means the row inherits the provider protocol, so existing configs
/// keep their behavior without migration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelMapping {
    pub local_model: String,
    pub upstream_model: String,
    /// Whether this row may serve requests. Rows in legacy configs without the
    /// field default to enabled; the value is always serialized so the user's
    /// per-mapping intent survives a round trip.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<UpstreamProtocol>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl ModelMapping {
    /// The protocol this row targets: its own declaration, else the provider's.
    pub fn effective_protocol(&self, provider_protocol: UpstreamProtocol) -> UpstreamProtocol {
        self.protocol.unwrap_or(provider_protocol)
    }
}

/// Upper bound on consecutive failures before a provider is automatically disabled.
pub const FAILURE_THRESHOLD: u32 = 3;

/// Which OpenAI-compatible endpoint family an upstream provider exposes.
///
/// The relay accepts `/chat/completions` and `/responses` from clients. A
/// provider's `protocol` is the default its mapping rows inherit; each row may
/// pin its own, so candidates are selected by a row's effective protocol and a
/// provider is not filtered by its own protocol alone.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamProtocol {
    #[default]
    ChatCompletions,
    Responses,
}

impl UpstreamProtocol {
    /// Canonical upstream path suffix for this protocol.
    pub fn endpoint_path(self) -> &'static str {
        match self {
            Self::ChatCompletions => "/chat/completions",
            Self::Responses => "/responses",
        }
    }
}

/// An upstream OpenAI-compatible provider used as a forwarding target.
///
/// `enabled` carries the user's intent while `auto_disabled` carries runtime health.
/// They are persisted independently and must never overwrite one another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayUpstreamProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub protocol: UpstreamProtocol,
    #[serde(default)]
    pub mappings: Vec<ModelMapping>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub auto_disabled: bool,
    #[serde(default)]
    pub disabled_reason: Option<String>,
    #[serde(default)]
    pub disabled_at: Option<u64>,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub last_error_at: Option<u64>,
}

impl Default for GatewayUpstreamProvider {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            default_model: None,
            protocol: UpstreamProtocol::ChatCompletions,
            mappings: Vec::new(),
            enabled: true,
            auto_disabled: false,
            disabled_reason: None,
            disabled_at: None,
            consecutive_failures: 0,
            last_error_at: None,
        }
    }
}

/// A local API key accepted by the API Gateway listener.
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

impl Default for GatewayKey {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            value: String::new(),
            enabled: true,
            created_at: now_ts(),
        }
    }
}

/// Ledger entry recording the last value written to a terminal provider record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalSyncRecord {
    pub provider_id: String,
    pub tool: String,
    pub synced_key_id: String,
    pub synced_base_url: String,
    pub synced_at: u64,
}

/// Pricing tier configuration during off-peak hours.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OffPeakPrice {
    pub start_time: String,
    pub end_time: String,
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
    #[serde(default)]
    pub output: f64,
}

/// Unit prices for one upstream model, in US dollars per million tokens.
///
/// Prices are matched against the upstream model name recorded at request time;
/// a missing row means the request is unpriced. Editing prices never rewrites
/// already-persisted usage records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelPrice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub upstream_model: String,
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_peak: Option<OffPeakPrice>,
}

/// Persisted API Gateway configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
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
    /// keeps older `api_gateway.json` files — and legacy `api_fusion.json`
    /// payloads migrated through the read-only compat path — readable without migration.
    #[serde(default = "default_usage_retention_days")]
    pub usage_retention_days: u32,
    /// User-maintained upstream-model price table; absent in older configs.
    #[serde(default)]
    pub model_prices: Vec<ModelPrice>,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            providers: Vec::new(),
            keys: Vec::new(),
            default_key_id: None,
            terminal_syncs: Vec::new(),
            usage_retention_days: DEFAULT_USAGE_RETENTION_DAYS,
            model_prices: Vec::new(),
        }
    }
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
