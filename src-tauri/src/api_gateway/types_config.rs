use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::api_gateway) const CONFIG_FILE: &str = "api_gateway.json";
/// Legacy file names from the `api_fusion` era, kept ONLY for deletion.
/// These files are never read, copied or opened — see `cleanup_legacy_files`.
pub(in crate::api_gateway) const LEGACY_CONFIG_FILE_NAME: &str = "api_fusion.json";
pub(in crate::api_gateway) const LEGACY_USAGE_DB_FILE_NAME: &str = "api_fusion_usage.db";
pub(in crate::api_gateway) const DEFAULT_PORT: u16 = 17688;
/// Dev (`debug_assertions`) default so `tauri dev` can run alongside the
/// installed release build. Release keeps `DEFAULT_PORT`.
pub(in crate::api_gateway) const DEV_DEFAULT_PORT: u16 = 17689;

/// Default request-log retention in days for configs written before the field existed.
pub const DEFAULT_USAGE_RETENTION_DAYS: u32 = 90;
/// Lower/upper bounds accepted by the retention-days setting.
pub const MIN_USAGE_RETENTION_DAYS: u32 = 1;
pub const MAX_USAGE_RETENTION_DAYS: u32 = 365;

/// Default interval in minutes between automatic provider-template refreshes
/// for configs written before the field existed.
pub const DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES: u32 = 60;
/// Lower/upper bounds accepted by the template auto-refresh interval setting;
/// `0` is the separate disabled value.
pub const MIN_TEMPLATE_AUTO_REFRESH_MINUTES: u32 = 10;
pub const MAX_TEMPLATE_AUTO_REFRESH_MINUTES: u32 = 1440;

pub const MIN_PROVIDER_WEIGHT: u32 = 1;
pub const MAX_PROVIDER_WEIGHT: u32 = 100;

pub(in crate::api_gateway) fn default_port() -> u16 {
    if cfg!(debug_assertions) {
        DEV_DEFAULT_PORT
    } else {
        DEFAULT_PORT
    }
}

/// Resolve the effective listening port for the current build profile.
///
/// `api_gateway.json` is shared by `tauri dev` (debug) and the installed
/// release build, and the stored port is not user editable. The two canonical
/// defaults are translated per profile so `tauri dev` listens on
/// `DEV_DEFAULT_PORT` while release keeps `DEFAULT_PORT`, even after the other
/// profile wrote its own default into the shared file. A stored value that is
/// neither canonical default is a real custom port and is preserved; `0` falls
/// back to the profile default.
pub(in crate::api_gateway) fn resolve_port(stored: u16, is_dev: bool) -> u16 {
    match (stored, is_dev) {
        (0, true) | (DEFAULT_PORT, true) => DEV_DEFAULT_PORT,
        (0, false) | (DEV_DEFAULT_PORT, false) => DEFAULT_PORT,
        (other, _) => other,
    }
}

pub(in crate::api_gateway) fn default_usage_retention_days() -> u32 {
    DEFAULT_USAGE_RETENTION_DAYS
}

pub(in crate::api_gateway) fn default_template_auto_refresh_minutes() -> u32 {
    DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES
}

/// Validate a user-provided template auto-refresh interval: only `0` (disabled)
/// or 10-1440 minutes are accepted, and anything else is rejected with an
/// actionable error and never persisted.
pub fn validate_template_auto_refresh_minutes(minutes: i64) -> Result<u32, String> {
    if minutes == 0
        || (MIN_TEMPLATE_AUTO_REFRESH_MINUTES as i64..=MAX_TEMPLATE_AUTO_REFRESH_MINUTES as i64)
            .contains(&minutes)
    {
        Ok(minutes as u32)
    } else {
        Err(format!(
            "template auto refresh minutes must be 0 (disabled) or between {MIN_TEMPLATE_AUTO_REFRESH_MINUTES} and {MAX_TEMPLATE_AUTO_REFRESH_MINUTES}, got {minutes}"
        ))
    }
}

/// Normalize a persisted template auto-refresh interval on read: `0` (disabled)
/// and in-range 10-1440 values are kept, while any other value written by an
/// older or corrupted config falls back to the default.
pub(in crate::api_gateway) fn normalize_template_auto_refresh_minutes(minutes: u32) -> u32 {
    if minutes == 0
        || (MIN_TEMPLATE_AUTO_REFRESH_MINUTES..=MAX_TEMPLATE_AUTO_REFRESH_MINUTES).contains(&minutes)
    {
        minutes
    } else {
        DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES
    }
}

pub(in crate::api_gateway) fn default_provider_weight() -> u32 {
    1
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
    /// Whether this row may serve requests. Rows in older configs without the
    /// field default to enabled; the value is always serialized so the user's
    /// per-mapping intent survives a round trip.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<UpstreamProtocol>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Ordered reasoning-effort levels this mapping advertises; maintained manually
    /// in the provider editor. Templates and template syncs neither carry nor write it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_efforts: Vec<String>,
    /// Runtime health: whether the row was automatically disabled after failures.
    /// Independent of `enabled` (the user's intent) and always serialized; an older
    /// config without the field reads as healthy.
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

impl Default for ModelMapping {
    fn default() -> Self {
        Self {
            local_model: String::new(),
            upstream_model: String::new(),
            enabled: true,
            protocol: None,
            display_name: None,
            reasoning_efforts: Vec::new(),
            auto_disabled: false,
            disabled_reason: None,
            disabled_at: None,
            consecutive_failures: 0,
            last_error_at: None,
        }
    }
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
    /// Template this provider was created from; `None` for manual providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    /// Upstream model names the user explicitly removed so a template sync
    /// must not resurrect them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_models: Vec<String>,
    #[serde(default = "default_provider_weight")]
    pub weight: u32,
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
            template_id: None,
            ignored_models: Vec::new(),
            weight: 1,
        }
    }
}

fn is_zero_f64(val: &f64) -> bool {
    *val == 0.0
}

/// One model entry of a provider template.
///
/// `protocol` is optional and inherits the template protocol when absent.
/// `display_name` is the official model name shown by the gateway. `enabled`
/// carries the operator's local intent and is always persisted; a model stored
/// without the flag reads as enabled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderTemplateModel {
    pub upstream_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<UpstreamProtocol>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub input: f64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub cache_read: f64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub cache_write: f64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub output: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub off_peaks: Vec<OffPeakPrice>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_efforts: Vec<String>,
}

impl Default for ProviderTemplateModel {
    fn default() -> Self {
        Self {
            upstream_model: String::new(),
            local_model: None,
            display_name: None,
            protocol: None,
            enabled: true,
            input: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 0.0,
            off_peaks: Vec::new(),
            reasoning_efforts: Vec::new(),
        }
    }
}

/// A built-in provider template shipped with the app.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderTemplate {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub protocol: UpstreamProtocol,
    #[serde(default)]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_url: Option<String>,
    #[serde(default)]
    pub models: Vec<ProviderTemplateModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Persisted template state: the last parsed snapshot plus sync metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderTemplateState {
    pub template_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<ProviderTemplate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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

/// Normalize an off-peak weekday set: drop values above `6`, deduplicate and
/// sort ascending; an empty result collapses to `None` (meaning every day).
fn normalize_days(days: Option<Vec<u8>>) -> Option<Vec<u8>> {
    let mut days: Vec<u8> = days
        .unwrap_or_default()
        .into_iter()
        .filter(|day| *day <= 6)
        .collect();
    days.sort_unstable();
    days.dedup();
    if days.is_empty() {
        None
    } else {
        Some(days)
    }
}

fn days_are_absent(days: &Option<Vec<u8>>) -> bool {
    normalize_days(days.clone()).is_none()
}

fn serialize_days<S>(days: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    normalize_days(days.clone())
        .unwrap_or_default()
        .serialize(serializer)
}

fn deserialize_days<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<Vec<i64>>::deserialize(deserializer)?;
    let days = raw.map(|values| {
        values
            .into_iter()
            .filter_map(|value| {
                if (0..=6).contains(&value) {
                    Some(value as u8)
                } else {
                    None
                }
            })
            .collect::<Vec<u8>>()
    });
    Ok(normalize_days(days))
}

/// Pricing tier configuration during off-peak hours.
///
/// `days` optionally scopes the window to UTC+8 weekdays (`0` Sunday .. `6`
/// Saturday). Absent, empty or all-invalid means every day; the set is
/// normalized (deduplicated, sorted ascending, out-of-range dropped) on both
/// read and write so an old configuration without the field keeps its behavior.
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
    #[serde(
        default,
        skip_serializing_if = "days_are_absent",
        serialize_with = "serialize_days",
        deserialize_with = "deserialize_days"
    )]
    pub days: Option<Vec<u8>>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub off_peaks: Vec<OffPeakPrice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_peak: Option<OffPeakPrice>,
}

impl ModelPrice {
    /// Return the active off-peak configurations.
    ///
    /// If `off_peaks` contains entries, returns a slice to them; otherwise, falls back
    /// to `off_peak` for backwards compatibility with older single-value configurations.
    pub fn effective_off_peaks(&self) -> &[OffPeakPrice] {
        if !self.off_peaks.is_empty() {
            &self.off_peaks
        } else if let Some(ref op) = self.off_peak {
            std::slice::from_ref(op)
        } else {
            &[]
        }
    }
}

impl Default for ModelPrice {
    fn default() -> Self {
        Self {
            provider_id: None,
            upstream_model: String::new(),
            input: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 0.0,
            off_peaks: Vec::new(),
            off_peak: None,
        }
    }
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
    /// keeps older `api_gateway.json` files readable without migration.
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
    /// `api_gateway.json` files readable without migration and the value is
    /// always serialized.
    #[serde(default = "default_template_auto_refresh_minutes")]
    pub template_auto_refresh_minutes: u32,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
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
