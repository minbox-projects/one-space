use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AccountType {
    OAuth,
    ApiKey,
}

impl AccountType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::OAuth => "oauth",
            Self::ApiKey => "api_key",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UpstreamProtocol {
    Responses,
    ChatCompletions,
}

impl UpstreamProtocol {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat_completions",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GroupDto {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) sort_order: i64,
    pub(crate) is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AccountDto {
    pub(crate) id: String,
    pub(crate) stable_external_id: Option<String>,
    pub(crate) account_type: AccountType,
    pub(crate) name: String,
    pub(crate) group_id: String,
    pub(crate) sort_order: i64,
    pub(crate) note: String,
    pub(crate) enabled: bool,
    pub(crate) health_status: String,
    pub(crate) quota_threshold_override_percent: Option<u8>,
    pub(crate) base_url: Option<String>,
    pub(crate) auth_method: Option<String>,
    pub(crate) upstream_protocol: Option<UpstreamProtocol>,
    pub(crate) tags: Vec<String>,
    pub(crate) model_mappings: Vec<ModelMappingDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModelMappingDto {
    pub(crate) account_id: String,
    pub(crate) public_model_id: String,
    pub(crate) upstream_model_id: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct QuotaWindowDto {
    pub(crate) id: String,
    pub(crate) account_id: String,
    pub(crate) upstream_window_id: Option<String>,
    pub(crate) name: String,
    pub(crate) scope_type: QuotaScopeType,
    pub(crate) scope_value: Option<String>,
    pub(crate) used_percent: Option<f64>,
    pub(crate) remaining_percent: Option<f64>,
    pub(crate) resets_at: Option<String>,
    pub(crate) duration_seconds: Option<i64>,
    pub(crate) last_succeeded_at: Option<String>,
    pub(crate) is_stale: bool,
    pub(crate) raw_kind: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QuotaScopeType {
    Global,
    Model,
    Endpoint,
    Capability,
    Unknown,
}

impl QuotaScopeType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Model => "model",
            Self::Endpoint => "endpoint",
            Self::Capability => "capability",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PriceSnapshot {
    pub(crate) public_model_id: String,
    pub(crate) account_id: Option<String>,
    pub(crate) source: String,
    pub(crate) effective_at: String,
    pub(crate) input_per_million_usd: Option<String>,
    pub(crate) output_per_million_usd: Option<String>,
    pub(crate) cache_read_per_million_usd: Option<String>,
    pub(crate) cache_write_per_million_usd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DomainEvent {
    pub(crate) kind: String,
    pub(crate) entity_id: String,
    pub(crate) reason_code: Option<String>,
}
