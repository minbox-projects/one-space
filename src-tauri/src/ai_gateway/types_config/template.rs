use serde::{Deserialize, Serialize};

use super::defaults::default_true;
use super::price::{is_zero_f64, OffPeakPrice};
use super::protocol::{
    ErrorStrategy, QuotaStrategy, SyncStrategy, TransportStrategy, UpstreamProtocol,
};

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
    /// Declared total context window in tokens. Absent means unspecified; an
    /// older configuration without the field reads as unknown rather than zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
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
            context_window: None,
        }
    }
}

/// A built-in provider template shipped with the app.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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
    /// Model-synchronization strategy; a missing value on an older template
    /// reads as `generic`.
    #[serde(default)]
    pub sync_strategy: SyncStrategy,
    /// Quota/usage monitoring strategy; a missing value reads as `none`.
    #[serde(default)]
    pub quota_strategy: QuotaStrategy,
    /// Error-handling strategy; a missing value reads as `generic`.
    #[serde(default)]
    pub error_strategy: ErrorStrategy,
    /// Upstream transport strategy; a missing value reads as `direct`. The
    /// declared strategy is authoritative over URL, name or icon heuristics.
    #[serde(default)]
    pub transport_strategy: TransportStrategy,
    #[serde(default)]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_url: Option<String>,
    #[serde(default)]
    pub models: Vec<ProviderTemplateModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Drop one closed strategy field from a persisted template object unless it
/// holds a value this build recognizes. Removing it lets the field's
/// `#[serde(default)]` supply the documented default (`generic`/`none`/
/// `generic`/`direct`), so a missing, `null` or unknown stored value never
/// fails the whole configuration while a future known value still round-trips.
fn normalize_persisted_template_strategy(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
) {
    let known = object.get(field).is_some_and(|value| match field {
        "sync_strategy" => serde_json::from_value::<SyncStrategy>(value.clone()).is_ok(),
        "quota_strategy" => serde_json::from_value::<QuotaStrategy>(value.clone()).is_ok(),
        "error_strategy" => serde_json::from_value::<ErrorStrategy>(value.clone()).is_ok(),
        "transport_strategy" => serde_json::from_value::<TransportStrategy>(value.clone()).is_ok(),
        _ => false,
    });
    if !known {
        object.remove(field);
    }
}

/// Tolerant read of a persisted [`ProviderTemplate`] nested in
/// [`ProviderTemplateState`]. The strict `ProviderTemplate` `Deserialize`
/// remains the new-save boundary; only the persisted wrapping state normalizes
/// the closed strategy fields, so a configuration written by another build with
/// a not-yet-known strategy still loads as a whole.
fn deserialize_persisted_provider_template<'de, D>(
    deserializer: D,
) -> Result<Option<ProviderTemplate>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(mut value) = raw else {
        return Ok(None);
    };
    if let serde_json::Value::Object(object) = &mut value {
        normalize_persisted_template_strategy(object, "sync_strategy");
        normalize_persisted_template_strategy(object, "quota_strategy");
        normalize_persisted_template_strategy(object, "error_strategy");
        normalize_persisted_template_strategy(object, "transport_strategy");
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(|error| <D::Error as serde::de::Error>::custom(error))
}

/// Persisted template state: the last parsed snapshot plus sync metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderTemplateState {
    pub template_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_persisted_provider_template"
    )]
    pub template: Option<ProviderTemplate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}
