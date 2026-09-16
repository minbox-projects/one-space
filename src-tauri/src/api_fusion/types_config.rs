use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::api_fusion) const CONFIG_FILE: &str = "api_fusion.json";
pub(in crate::api_fusion) const DEFAULT_PORT: u16 = 17688;

pub(in crate::api_fusion) fn default_port() -> u16 {
    DEFAULT_PORT
}

pub(in crate::api_fusion) fn default_true() -> bool {
    true
}

pub(in crate::api_fusion) fn now_ts() -> u64 {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<UpstreamProtocol>,
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
pub struct FusionUpstreamProvider {
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

impl Default for FusionUpstreamProvider {
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

/// A local API key accepted by the API Fusion listener.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionKey {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: u64,
}

impl Default for FusionKey {
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

/// Persisted API Fusion configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub providers: Vec<FusionUpstreamProvider>,
    #[serde(default)]
    pub keys: Vec<FusionKey>,
    #[serde(default)]
    pub default_key_id: Option<String>,
    #[serde(default)]
    pub terminal_syncs: Vec<TerminalSyncRecord>,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            providers: Vec::new(),
            keys: Vec::new(),
            default_key_id: None,
            terminal_syncs: Vec::new(),
        }
    }
}

/// Runtime status summary exposed to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionStatus {
    pub running: bool,
    pub enabled: bool,
    pub port: u16,
    pub local_base_url: String,
    pub provider_count: usize,
    pub auto_disabled_count: usize,
    pub key_count: usize,
    pub default_key_id: Option<String>,
}
