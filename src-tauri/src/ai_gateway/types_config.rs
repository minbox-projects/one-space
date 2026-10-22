use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::ai_gateway) const CONFIG_FILE: &str = "ai_gateway.json";
pub(in crate::ai_gateway) const DEFAULT_PORT: u16 = 17688;
/// Dev (`debug_assertions`) default so `tauri dev` can run alongside the
/// installed release build. Release keeps `DEFAULT_PORT`.
pub(in crate::ai_gateway) const DEV_DEFAULT_PORT: u16 = 17689;

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

/// Current schema version of the persisted gateway configuration. A missing
/// `schema_version` field reads as `0` (legacy) and triggers the one-time
/// migration on the next configuration read; every write persists this value.
///
/// Version 2 replaces the provider-level single `api_key` with the ordered
/// `keys` pool; version 1 and older files are converted once by
/// [`super::migration::migrate_legacy_config`]. Version 3 carries the
/// `opencode-go` template identity and provider bindings, replacing the old
/// `opencode-zen` built-in through the same version-gated migration. Version 4
/// replaces a pool's single provider-specific `fallback` candidate with an
/// ordered list of gateway local model ids.
pub const GATEWAY_CONFIG_SCHEMA_VERSION: u32 = 4;

pub(in crate::ai_gateway) fn default_port() -> u16 {
    if cfg!(debug_assertions) {
        DEV_DEFAULT_PORT
    } else {
        DEFAULT_PORT
    }
}

/// Resolve the effective listening port for the current build profile.
///
/// `ai_gateway.json` is per build profile: the release build stores it under
/// `~/.config/onespace` and the debug (`tauri dev`) build under
/// `~/.config/onespace-dev`, and the stored port is not user editable. The
/// stored value may still originate from the other profile - a first-run seed
/// copy or legacy shared history - so the two canonical defaults are
/// translated per profile: `tauri dev` listens on `DEV_DEFAULT_PORT` while
/// release keeps `DEFAULT_PORT`, even when the other profile wrote its own
/// default into the file. A stored value that is neither canonical default is
/// a real custom port and is preserved; `0` falls back to the profile default.
pub(in crate::ai_gateway) fn resolve_port(stored: u16, is_dev: bool) -> u16 {
    match (stored, is_dev) {
        (0, true) | (DEFAULT_PORT, true) => DEV_DEFAULT_PORT,
        (0, false) | (DEV_DEFAULT_PORT, false) => DEFAULT_PORT,
        (other, _) => other,
    }
}

pub(in crate::ai_gateway) fn default_usage_retention_days() -> u32 {
    DEFAULT_USAGE_RETENTION_DAYS
}

pub(in crate::ai_gateway) fn default_template_auto_refresh_minutes() -> u32 {
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
pub(in crate::ai_gateway) fn normalize_template_auto_refresh_minutes(minutes: u32) -> u32 {
    if minutes == 0
        || (MIN_TEMPLATE_AUTO_REFRESH_MINUTES..=MAX_TEMPLATE_AUTO_REFRESH_MINUTES)
            .contains(&minutes)
    {
        minutes
    } else {
        DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES
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

pub(in crate::ai_gateway) fn default_provider_weight() -> u32 {
    1
}

pub(in crate::ai_gateway) fn default_true() -> bool {
    true
}

pub(in crate::ai_gateway) fn now_ts() -> u64 {
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
    /// in the provider editor and refreshed by a template sync from the template
    /// model's explicitly declared capabilities while the row still inherits them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_efforts: Vec<String>,
    /// Declared total context window in tokens. Absent means unspecified; an
    /// older configuration without the field reads as unknown rather than zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
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
            context_window: None,
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

/// Fixed cooldown before an auto-disabled mapping row becomes eligible for one
/// half-open probe attempt. Eligibility is evaluated against an explicit `now`,
/// so the 60-second boundary stays deterministic.
pub const AUTO_DISABLE_PROBE_COOLDOWN_SECS: u64 = 60;

/// Fixed cooldown before a quota-marked upstream key becomes eligible for one
/// half-open probe attempt. A dedicated constant keeps the key probe boundary
/// independent from the mapping-row cooldown even though both currently read 60
/// seconds; eligibility is evaluated against an explicit `now`.
pub const KEY_PROBE_COOLDOWN_SECS: u64 = 60;

/// Time-to-live after which a quota-marked upstream key becomes usable again
/// through normal list-order selection. Eligibility is inclusive at exactly
/// this many seconds and exclusive below it. Auth marks and quota marks without
/// a marking time never expire through this TTL.
pub(in crate::ai_gateway) const KEY_QUOTA_MARK_TTL_SECS: u64 = 1800;

/// Why an upstream key was auto-marked and taken out of the usable pool.
///
/// `Authentication` (401/403) is manual-only: it is never auto-probed and
/// recovers only when the user re-enables the key or edits its value.
/// `Quota` (a quota-exhausted 429) becomes probe-eligible after
/// [`KEY_PROBE_COOLDOWN_SECS`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyFailureKind {
    Authentication,
    Quota,
}

/// One entry of an upstream provider's ordered key pool.
///
/// `id` is a stable identity used to preserve runtime state across provider
/// upserts; `name` is a required, user-visible remark; `value` is the secret
/// upstream credential. `enabled` is the user's intent and is independent from
/// the runtime mark (`auto_marked`, `failure_kind`, `marked_at`, `reason`). The
/// list order is the persisted priority order. Every field is always
/// serialized, including `null` runtime fields and an empty array on the
/// provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpstreamKey {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// User intent: this key becomes eligible again at the next UTC+8 natural
    /// day boundary after a quota mark. Independent from the runtime mark and
    /// always serialized; a stored record without the field reads as `false`.
    #[serde(default)]
    pub daily_reset: bool,
    #[serde(default)]
    pub auto_marked: bool,
    #[serde(default)]
    pub failure_kind: Option<KeyFailureKind>,
    #[serde(default)]
    pub marked_at: Option<u64>,
    #[serde(default)]
    pub reason: Option<String>,
}

impl Default for UpstreamKey {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            value: String::new(),
            enabled: true,
            daily_reset: false,
            auto_marked: false,
            failure_kind: None,
            marked_at: None,
            reason: None,
        }
    }
}

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
    AnthropicMessages,
}

impl UpstreamProtocol {
    /// Canonical upstream path suffix for this protocol.
    pub fn endpoint_path(self) -> &'static str {
        match self {
            Self::ChatCompletions => "/chat/completions",
            Self::Responses => "/responses",
            Self::AnthropicMessages => "/messages",
        }
    }
}

/// Model-synchronization strategy declared by a provider template.
///
/// `generic` keeps today's model-list parsing and protocol-inheritance
/// fallback; `opencode_go` restricts a sync to model ids that the template
/// explicitly declares as Chat Completions or Responses (or that a future
/// source payload declares with a supported endpoint), never guessing a
/// protocol for undeclared or Messages-only ids.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncStrategy {
    #[default]
    Generic,
    OpencodeGo,
}

/// Quota/usage monitoring strategy declared by a provider template.
///
/// `none` performs no provider-specific monitor query, `commandcode` uses the
/// fixed CommandCode credits endpoint and `opencode_go` uses the fixed OpenCode
/// Go usage endpoint. The strategy is authoritative over any URL heuristic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum QuotaStrategy {
    #[serde(rename = "none")]
    #[default]
    NoQuota,
    #[serde(rename = "commandcode")]
    Commandcode,
    #[serde(rename = "opencode_go")]
    OpencodeGo,
}

/// Error-handling strategy declared by a provider template. Only the existing
/// generic classification exists in this release.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ErrorStrategy {
    #[default]
    Generic,
}

/// An upstream OpenAI-compatible provider used as a forwarding target.
///
/// `enabled` carries the user's intent; runtime health lives on each mapping
/// row (`ModelMapping`), so provider records persist no health state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayUpstreamProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    /// Ordered upstream credential pool. Always serialized, including an empty
    /// array; older files without the field migrate once into a one-entry pool.
    #[serde(default)]
    pub keys: Vec<UpstreamKey>,
    /// Transport-only credential selected for the current attempt. It is
    /// deliberately never serialized: the persisted record carries the pool in
    /// `keys`, and the legacy single field is converted by the version-gated
    /// migration in `migration.rs`. The forwarding layer reads this field, so
    /// every request slot sets it to the selected key's value before forwarding.
    #[serde(skip)]
    pub attempt_key: String,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub protocol: UpstreamProtocol,
    #[serde(default)]
    pub mappings: Vec<ModelMapping>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Template this provider was created from; `None` for manual providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    /// Upstream model names removed from this provider — by the manual model
    /// delete or by a template sync that dropped the model — so a later sync
    /// must not resurrect them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_models: Vec<String>,
    #[serde(default = "default_provider_weight")]
    pub weight: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Transport-only resolved model-pool candidate identity. A model-pool
    /// request clones its real provider into a one-candidate slot and attaches
    /// this identity so every attempt, retry and probe keeps the real provider
    /// and the original mapping health target while never persisting pool
    /// secrets. It is deliberately never serialized.
    #[serde(skip)]
    pub pool_candidate: Option<PoolCandidateIdentity>,
}

/// Transport-only identity of one prepared model-pool candidate.
///
/// It names the owning pool plus the real provider and the original mapping
/// `(local_model, upstream_model)` pair and effective protocol the candidate
/// references, so scheduler accounting, cooldown and mapping-health settlement
/// stay scoped to the exact source while the routing clone carries only the
/// provided candidate. Never serialized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolCandidateIdentity {
    pub pool_id: String,
    pub provider_id: String,
    pub local_model: String,
    pub upstream_model: String,
    pub protocol: UpstreamProtocol,
    /// The pool-only native reasoning control resolved for the current request
    /// (transport-only; never serialized). `None` when the request carries no
    /// pool reasoning policy for this candidate.
    pub(in crate::ai_gateway) reasoning: Option<PoolNativeReasoning>,
}

/// The concrete native action a pool candidate resolves to for one request.
///
/// Bounded to the shapes REQ-002 permits; the target protocol is carried on the
/// enclosing [`PoolNativeReasoning`] and shapes the wire control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ai_gateway) enum PoolNativeAction {
    /// A positive enum-shaped control carrying the native value for the
    /// candidate's declared profile.
    Enum {
        profile: PoolReasoningProfile,
        value: String,
    },
    /// An Anthropic manual-budget control.
    Budget { budget_tokens: u32 },
    /// An explicit disable/omission control.
    Disable { profile: PoolReasoningProfile },
    /// The `auto` policy: no control is injected and the provider default is
    /// retained.
    ProviderDefault,
}

/// One pool candidate's resolved native reasoning control for a request.
///
/// Built once per candidate so the upstream body and the recorded attempt
/// evidence are derived from the same constructed value; the allowlisted wire
/// controls are produced only by [`Self::wire_parameters`] and never carry
/// bodies, headers or credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ai_gateway) struct PoolNativeReasoning {
    /// Canonical resolved preference token: `low|medium|high|xhigh|none|auto`.
    pub(in crate::ai_gateway) resolved: String,
    /// The candidate's target upstream protocol, which shapes the wire control.
    pub(in crate::ai_gateway) protocol: UpstreamProtocol,
    pub(in crate::ai_gateway) action: PoolNativeAction,
}

impl PoolNativeReasoning {
    /// The `auto` provider-default control for one candidate.
    pub(in crate::ai_gateway) fn provider_default(protocol: UpstreamProtocol) -> Self {
        Self {
            resolved: "auto".to_string(),
            protocol,
            action: PoolNativeAction::ProviderDefault,
        }
    }

    /// Snapshot mode token for the attempt evidence.
    pub(in crate::ai_gateway) fn mode(&self) -> &'static str {
        match self.action {
            PoolNativeAction::Enum { .. } => "enum",
            PoolNativeAction::Budget { .. } => "budget",
            PoolNativeAction::Disable { .. } => "disable",
            PoolNativeAction::ProviderDefault => "provider_default",
        }
    }

    /// The allowlisted wire controls actually sent to the candidate, keyed by
    /// their top-level field so the caller can merge them into the constructed
    /// body and record them verbatim as evidence.
    pub(in crate::ai_gateway) fn wire_parameters(
        &self,
    ) -> serde_json::Map<String, serde_json::Value> {
        match &self.action {
            PoolNativeAction::ProviderDefault => serde_json::Map::new(),
            PoolNativeAction::Enum { profile, value } => match profile {
                PoolReasoningProfile::OpenaiEffort => match self.protocol {
                    UpstreamProtocol::Responses => {
                        wire_object(json!({ "reasoning": { "effort": value } }))
                    }
                    _ => wire_object(json!({ "reasoning_effort": value })),
                },
                PoolReasoningProfile::ChatThinkingSwitch => {
                    wire_object(json!({ "thinking": { "type": "enabled" } }))
                }
                PoolReasoningProfile::AnthropicEffortInherit => {
                    wire_object(json!({ "output_config": { "effort": value } }))
                }
                PoolReasoningProfile::AnthropicEffortAdaptive => wire_object(json!({
                    "output_config": { "effort": value },
                    "thinking": { "type": "adaptive" },
                })),
                PoolReasoningProfile::AnthropicManualBudget | PoolReasoningProfile::Unknown(_) => {
                    serde_json::Map::new()
                }
            },
            PoolNativeAction::Budget { budget_tokens } => wire_object(json!({
                "thinking": { "type": "enabled", "budget_tokens": budget_tokens },
            })),
            PoolNativeAction::Disable { profile } => match profile {
                PoolReasoningProfile::OpenaiEffort => match self.protocol {
                    UpstreamProtocol::Responses => {
                        wire_object(json!({ "reasoning": { "effort": "none" } }))
                    }
                    _ => wire_object(json!({ "reasoning_effort": "none" })),
                },
                _ => wire_object(json!({ "thinking": { "type": "disabled" } })),
            },
        }
    }
}

/// Wrap a `json!({..})` object literal into its map form.
fn wire_object(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    value
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new)
}

/// One public reasoning preference a model pool maps to a native action. The
/// five canonical values are the only ones a mapping row declares; `none` is the
/// explicit disable preference and is never a positive control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolReasoningEffort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    None,
}

impl PoolReasoningEffort {
    /// Canonical lowercase JSON token for this preference.
    pub(in crate::ai_gateway) fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
            Self::None => "none",
        }
    }
}

/// The pool-owned default policy. `auto` (the serde default, and the meaning of a
/// missing persisted value) omits every reasoning control and is never sent as an
/// effort string. [`Self::Unknown`] only ever round-trips a value written by
/// another build so an unrelated edit cannot silently rewrite it; it is always
/// rejected on a new save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolDefaultReasoningEffort {
    Auto,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    None,
    Unknown(String),
}

impl Default for PoolDefaultReasoningEffort {
    fn default() -> Self {
        Self::Auto
    }
}

impl PoolDefaultReasoningEffort {
    /// Canonical lowercase JSON token; a retained unknown value keeps its bytes.
    pub(in crate::ai_gateway) fn as_str(&self) -> &str {
        match self {
            Self::Auto => "auto",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
            Self::None => "none",
            Self::Unknown(raw) => raw.as_str(),
        }
    }
}

impl Serialize for PoolDefaultReasoningEffort {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PoolDefaultReasoningEffort {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "auto" => Self::Auto,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" => Self::Xhigh,
            "max" => Self::Max,
            "none" => Self::None,
            _ => Self::Unknown(raw),
        })
    }
}

/// The deliberately bounded pool candidate native control profile (REQ-002). An
/// unknown profile is retained for a tolerant read and rejected on a new save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolReasoningProfile {
    OpenaiEffort,
    AnthropicEffortInherit,
    AnthropicEffortAdaptive,
    AnthropicManualBudget,
    ChatThinkingSwitch,
    Unknown(String),
}

impl PoolReasoningProfile {
    /// Canonical lowercase JSON token for this profile.
    pub(in crate::ai_gateway) fn as_str(&self) -> &str {
        match self {
            Self::OpenaiEffort => "openai_effort",
            Self::AnthropicEffortInherit => "anthropic_effort_inherit",
            Self::AnthropicEffortAdaptive => "anthropic_effort_adaptive",
            Self::AnthropicManualBudget => "anthropic_manual_budget",
            Self::ChatThinkingSwitch => "chat_thinking_switch",
            Self::Unknown(raw) => raw.as_str(),
        }
    }
}

impl Serialize for PoolReasoningProfile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PoolReasoningProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "openai_effort" => Self::OpenaiEffort,
            "anthropic_effort_inherit" => Self::AnthropicEffortInherit,
            "anthropic_effort_adaptive" => Self::AnthropicEffortAdaptive,
            "anthropic_manual_budget" => Self::AnthropicManualBudget,
            "chat_thinking_switch" => Self::ChatThinkingSwitch,
            _ => Self::Unknown(raw),
        })
    }
}

/// One typed native action a public preference maps to. The serde tag is `kind`,
/// producing `{kind:"enum",value}`, `{kind:"budget",budget_tokens}` or
/// `{kind:"disable"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PoolReasoningAction {
    Enum { value: String },
    Budget { budget_tokens: u32 },
    Disable,
}

/// A candidate-declared inclusive native budget range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolReasoningBudget {
    pub min: u32,
    pub max: u32,
}

/// One public-preference-to-native-action mapping row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolReasoningMapping {
    pub effort: PoolReasoningEffort,
    pub action: PoolReasoningAction,
}

/// Pool-owned, candidate-local native reasoning declaration. Absence means the
/// candidate's capabilities are unknown and must never be inferred from source
/// metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolCandidateReasoning {
    pub profile: PoolReasoningProfile,
    /// Declared native enum value set for an enum-shaped profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// Declared native budget range for the manual-budget profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<PoolReasoningBudget>,
    /// Whether the candidate supports an explicit disable operation.
    #[serde(default)]
    pub supports_disable: bool,
    /// The public-to-native mapping matrix; a missing effort means unsupported.
    #[serde(default)]
    pub mappings: Vec<PoolReasoningMapping>,
}

/// One candidate of an operator-defined model pool.
///
/// A candidate is an explicit reference to one real provider mapping row: the
/// real provider id plus the original trimmed `(local_model, upstream_model)`
/// pair and effective protocol, with an integer weight from 1 to 100 defaulting
/// to 1. It never copies a source key or endpoint. Its optional `reasoning`
/// declaration is pool-local and additive under schema 3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelPoolCandidate {
    pub provider_id: String,
    pub local_model: String,
    pub upstream_model: String,
    pub protocol: UpstreamProtocol,
    #[serde(default = "default_provider_weight")]
    pub weight: u32,
    /// Pool-owned native reasoning declaration; absent means unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<PoolCandidateReasoning>,
}

impl Default for ModelPoolCandidate {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            local_model: String::new(),
            upstream_model: String::new(),
            protocol: UpstreamProtocol::ChatCompletions,
            weight: 1,
            reasoning: None,
        }
    }
}

/// An operator-defined model pool exposed as a virtual provider.
///
/// `id` is a stable identity, `name` is the trimmed display name, `model` is the
/// public model id clients request, `enabled` carries the operator's intent, and
/// `candidates` are the exact selected source references. A missing field in an
/// older configuration reads as an empty collection. Always serialized.
///
/// `default_reasoning_effort` is the pool-owned default policy: a canonical
/// preference or `auto`, where a missing persisted value reads as `auto`. It is
/// always serialized so a legacy pool persists as `auto` without inferring a
/// capability.
///
/// `fallback_models` is the ordered list of gateway local model ids used after
/// every regular candidate is exhausted. Each entry expands at request time to
/// the provider mappings serving that local model, so a fallback stores no
/// provider, upstream model, protocol, weight or fallback-local reasoning.
/// An absent or empty list means no configured fallback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelPool {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub candidates: Vec<ModelPoolCandidate>,
    #[serde(default)]
    pub default_reasoning_effort: PoolDefaultReasoningEffort,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_models: Vec<String>,
    /// Unified public context window limit in tokens. When set and > 0, overrides
    /// the conservative candidate aggregate advertised to clients and terminal sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
}

impl Default for ModelPool {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            model: String::new(),
            enabled: true,
            candidates: Vec::new(),
            default_reasoning_effort: PoolDefaultReasoningEffort::Auto,
            fallback_models: Vec::new(),
            context_window: None,
        }
    }
}

impl Default for GatewayUpstreamProvider {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            base_url: String::new(),
            keys: Vec::new(),
            attempt_key: String::new(),
            default_model: None,
            protocol: UpstreamProtocol::ChatCompletions,
            mappings: Vec::new(),
            enabled: true,
            template_id: None,
            ignored_models: Vec::new(),
            weight: 1,
            tags: Vec::new(),
            icon: None,
            pool_candidate: None,
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
/// `generic`), so a missing, `null` or unknown stored value never fails the
/// whole configuration while a future known value still round-trips.
fn normalize_persisted_template_strategy(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
) {
    let known = object.get(field).is_some_and(|value| match field {
        "sync_strategy" => serde_json::from_value::<SyncStrategy>(value.clone()).is_ok(),
        "quota_strategy" => serde_json::from_value::<QuotaStrategy>(value.clone()).is_ok(),
        "error_strategy" => serde_json::from_value::<ErrorStrategy>(value.clone()).is_ok(),
        _ => false,
    });
    if !known {
        object.remove(field);
    }
}

/// Tolerant read of a persisted [`ProviderTemplate`] nested in
/// [`ProviderTemplateState`]. The strict `ProviderTemplate` `Deserialize`
/// remains the new-save boundary; only the persisted wrapping state normalizes
/// the three closed strategy fields, so a configuration written by another
/// build with a not-yet-known strategy still loads as a whole.
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

/// The four explicit model choices an operator submits for a Claude terminal
/// sync: the default model plus the haiku/sonnet/opus family aliases.
///
/// Each value is a local mapping id (the id Claude Code sends), never a vendor
/// id guessed from a name, and the same id may fill several slots. The struct is
/// shared by the configure/sync command payload and the model-only
/// terminal-target snapshot; it never carries a credential value.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeModelSelection {
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub haiku: String,
    #[serde(default)]
    pub sonnet: String,
    #[serde(default)]
    pub opus: String,
}

impl ClaudeModelSelection {
    /// Trim every choice and return `None` when any of the four is blank, so a
    /// partial or empty selection is rejected before any Claude write.
    pub(in crate::ai_gateway) fn normalized(&self) -> Option<Self> {
        let trim = |value: &str| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        };
        Some(Self {
            default_model: trim(&self.default_model)?,
            haiku: trim(&self.haiku)?,
            sonnet: trim(&self.sonnet)?,
            opus: trim(&self.opus)?,
        })
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
        }
    }
}

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
    /// Global dynamic settings for the gateway (timeouts, retries, failure threshold,
    /// proxy, CORS, session affinity, etc.).
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

pub const MIN_GATEWAY_PORT: u16 = 1024;
pub const MAX_GATEWAY_PORT: u16 = 65535;

pub const DEFAULT_CONNECT_TIMEOUT_SECS: u32 = 10;
pub const MIN_CONNECT_TIMEOUT_SECS: u32 = 1;
pub const MAX_CONNECT_TIMEOUT_SECS: u32 = 60;

pub const DEFAULT_READ_TIMEOUT_SECS: u32 = 60;
pub const MIN_READ_TIMEOUT_SECS: u32 = 5;
pub const MAX_READ_TIMEOUT_SECS: u32 = 3600;

pub const DEFAULT_REQUEST_BUDGET_SECS: u32 = 120;
pub const MIN_REQUEST_BUDGET_SECS: u32 = 10;
pub const MAX_REQUEST_BUDGET_SECS: u32 = 7200;

pub const DEFAULT_MAX_RETRIES_PER_PROVIDER: u32 = 5;
pub const MAX_ALLOWED_RETRIES_PER_PROVIDER: u32 = 20;

pub const DEFAULT_RETRY_BASE_DELAY_MS: u32 = 2000;
pub const MIN_RETRY_BASE_DELAY_MS: u32 = 100;
pub const MAX_RETRY_BASE_DELAY_MS: u32 = 30000;

pub const DEFAULT_FAILURE_THRESHOLD: u32 = 3;
pub const MAX_FAILURE_THRESHOLD: u32 = 50;

pub const DEFAULT_PROBE_COOLDOWN_SECS: u64 = 60;
pub const MIN_PROBE_COOLDOWN_SECS: u64 = 5;
pub const MAX_PROBE_COOLDOWN_SECS: u64 = 7200;

pub const DEFAULT_KEY_QUOTA_TTL_SECS: u64 = 1800;
pub const MIN_KEY_QUOTA_TTL_SECS: u64 = 30;
pub const MAX_KEY_QUOTA_TTL_SECS: u64 = 86400;

pub const DEFAULT_SESSION_AFFINITY_TIMEOUT_SECS: u64 = 1800;
pub const MAX_SESSION_AFFINITY_TIMEOUT_SECS: u64 = 86400;

pub const DEFAULT_CONTEXT_HOLD_TIMEOUT_SECS: u64 = 1800;
pub const MAX_CONTEXT_HOLD_TIMEOUT_SECS: u64 = 86400;

pub const DEFAULT_DAILY_RESET_OFFSET_HOURS: i32 = 8;
pub const MIN_DAILY_RESET_OFFSET_HOURS: i32 = -12;
pub const MAX_DAILY_RESET_OFFSET_HOURS: i32 = 14;

pub const DEFAULT_QUOTA_CACHE_TTL_SECS: u64 = 300;
pub const MIN_QUOTA_CACHE_TTL_SECS: u64 = 10;
pub const MAX_QUOTA_CACHE_TTL_SECS: u64 = 3600;

pub(in crate::ai_gateway) fn default_bind_address() -> String {
    "127.0.0.1".to_string()
}
pub(in crate::ai_gateway) fn default_connect_timeout_secs() -> u32 {
    DEFAULT_CONNECT_TIMEOUT_SECS
}
pub(in crate::ai_gateway) fn default_read_timeout_secs() -> u32 {
    DEFAULT_READ_TIMEOUT_SECS
}
pub(in crate::ai_gateway) fn default_request_budget_secs() -> u32 {
    DEFAULT_REQUEST_BUDGET_SECS
}
pub(in crate::ai_gateway) fn default_max_retries_per_provider() -> u32 {
    DEFAULT_MAX_RETRIES_PER_PROVIDER
}
pub(in crate::ai_gateway) fn default_retry_base_delay_ms() -> u32 {
    DEFAULT_RETRY_BASE_DELAY_MS
}
pub(in crate::ai_gateway) fn default_failure_threshold() -> u32 {
    DEFAULT_FAILURE_THRESHOLD
}
pub(in crate::ai_gateway) fn default_probe_cooldown_secs() -> u64 {
    DEFAULT_PROBE_COOLDOWN_SECS
}
pub(in crate::ai_gateway) fn default_key_quota_ttl_secs() -> u64 {
    DEFAULT_KEY_QUOTA_TTL_SECS
}
pub(in crate::ai_gateway) fn default_session_affinity_timeout_secs() -> u64 {
    DEFAULT_SESSION_AFFINITY_TIMEOUT_SECS
}
pub(in crate::ai_gateway) fn default_context_hold_timeout_secs() -> u64 {
    DEFAULT_CONTEXT_HOLD_TIMEOUT_SECS
}
pub(in crate::ai_gateway) fn default_daily_reset_offset_hours() -> i32 {
    DEFAULT_DAILY_RESET_OFFSET_HOURS
}
pub(in crate::ai_gateway) fn default_quota_cache_ttl_secs() -> u64 {
    DEFAULT_QUOTA_CACHE_TTL_SECS
}

/// Dynamic operational settings for the AI Gateway.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewaySettings {
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    #[serde(default = "default_true")]
    pub enable_cors: bool,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default = "default_true")]
    pub require_api_key_for_lan: bool,

    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u32,
    #[serde(default = "default_read_timeout_secs")]
    pub read_timeout_secs: u32,
    #[serde(default = "default_request_budget_secs")]
    pub request_budget_secs: u32,
    #[serde(default = "default_max_retries_per_provider")]
    pub max_retries_per_provider: u32,
    #[serde(default = "default_retry_base_delay_ms")]
    pub retry_base_delay_ms: u32,

    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    #[serde(default = "default_probe_cooldown_secs")]
    pub probe_cooldown_secs: u64,
    #[serde(default = "default_key_quota_ttl_secs")]
    pub key_quota_ttl_secs: u64,

    #[serde(default = "default_session_affinity_timeout_secs")]
    pub session_affinity_timeout_secs: u64,
    #[serde(default = "default_context_hold_timeout_secs")]
    pub context_hold_timeout_secs: u64,
    #[serde(default)]
    pub custom_session_headers: Vec<String>,

    #[serde(default = "default_daily_reset_offset_hours")]
    pub daily_reset_offset_hours: i32,
    #[serde(default = "default_quota_cache_ttl_secs")]
    pub quota_cache_ttl_secs: u64,

    #[serde(default)]
    pub debug_logging: bool,
}

impl Default for GatewaySettings {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            enable_cors: true,
            proxy_url: None,
            require_api_key_for_lan: true,
            connect_timeout_secs: DEFAULT_CONNECT_TIMEOUT_SECS,
            read_timeout_secs: DEFAULT_READ_TIMEOUT_SECS,
            request_budget_secs: DEFAULT_REQUEST_BUDGET_SECS,
            max_retries_per_provider: DEFAULT_MAX_RETRIES_PER_PROVIDER,
            retry_base_delay_ms: DEFAULT_RETRY_BASE_DELAY_MS,
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            probe_cooldown_secs: DEFAULT_PROBE_COOLDOWN_SECS,
            key_quota_ttl_secs: DEFAULT_KEY_QUOTA_TTL_SECS,
            session_affinity_timeout_secs: DEFAULT_SESSION_AFFINITY_TIMEOUT_SECS,
            context_hold_timeout_secs: DEFAULT_CONTEXT_HOLD_TIMEOUT_SECS,
            custom_session_headers: Vec::new(),
            daily_reset_offset_hours: DEFAULT_DAILY_RESET_OFFSET_HOURS,
            quota_cache_ttl_secs: DEFAULT_QUOTA_CACHE_TTL_SECS,
            debug_logging: false,
        }
    }
}

pub fn validate_gateway_port(port: u16) -> Result<(), String> {
    if (MIN_GATEWAY_PORT..=MAX_GATEWAY_PORT).contains(&port) {
        Ok(())
    } else {
        Err(format!(
            "port must be between {MIN_GATEWAY_PORT} and {MAX_GATEWAY_PORT}, got {port}"
        ))
    }
}

pub fn validate_gateway_settings(settings: &GatewaySettings) -> Result<(), String> {
    let bind = settings.bind_address.trim();
    if bind.is_empty() {
        return Err("bind address cannot be empty".to_string());
    }
    if bind != "127.0.0.1"
        && bind != "0.0.0.0"
        && bind != "localhost"
        && bind.parse::<std::net::IpAddr>().is_err()
    {
        return Err(format!("invalid bind address '{bind}'"));
    }

    if let Some(ref proxy) = settings.proxy_url {
        let trimmed = proxy.trim();
        if !trimmed.is_empty()
            && !trimmed.starts_with("http://")
            && !trimmed.starts_with("https://")
            && !trimmed.starts_with("socks5://")
            && !trimmed.starts_with("socks5h://")
        {
            return Err("proxy URL must start with http://, https://, or socks5://".to_string());
        }
    }

    if !(MIN_CONNECT_TIMEOUT_SECS..=MAX_CONNECT_TIMEOUT_SECS).contains(&settings.connect_timeout_secs) {
        return Err(format!(
            "connect timeout must be between {MIN_CONNECT_TIMEOUT_SECS} and {MAX_CONNECT_TIMEOUT_SECS} seconds"
        ));
    }
    if !(MIN_READ_TIMEOUT_SECS..=MAX_READ_TIMEOUT_SECS).contains(&settings.read_timeout_secs) {
        return Err(format!(
            "read timeout must be between {MIN_READ_TIMEOUT_SECS} and {MAX_READ_TIMEOUT_SECS} seconds"
        ));
    }
    if !(MIN_REQUEST_BUDGET_SECS..=MAX_REQUEST_BUDGET_SECS).contains(&settings.request_budget_secs) {
        return Err(format!(
            "request budget must be between {MIN_REQUEST_BUDGET_SECS} and {MAX_REQUEST_BUDGET_SECS} seconds"
        ));
    }
    if settings.max_retries_per_provider > MAX_ALLOWED_RETRIES_PER_PROVIDER {
        return Err(format!(
            "max retries per provider cannot exceed {MAX_ALLOWED_RETRIES_PER_PROVIDER}"
        ));
    }
    if !(MIN_RETRY_BASE_DELAY_MS..=MAX_RETRY_BASE_DELAY_MS).contains(&settings.retry_base_delay_ms) {
        return Err(format!(
            "retry base delay must be between {MIN_RETRY_BASE_DELAY_MS} and {MAX_RETRY_BASE_DELAY_MS} ms"
        ));
    }
    if settings.failure_threshold > MAX_FAILURE_THRESHOLD {
        return Err(format!(
            "failure threshold cannot exceed {MAX_FAILURE_THRESHOLD}"
        ));
    }
    if !(MIN_PROBE_COOLDOWN_SECS..=MAX_PROBE_COOLDOWN_SECS).contains(&settings.probe_cooldown_secs) {
        return Err(format!(
            "probe cooldown must be between {MIN_PROBE_COOLDOWN_SECS} and {MAX_PROBE_COOLDOWN_SECS} seconds"
        ));
    }
    if !(MIN_KEY_QUOTA_TTL_SECS..=MAX_KEY_QUOTA_TTL_SECS).contains(&settings.key_quota_ttl_secs) {
        return Err(format!(
            "key quota TTL must be between {MIN_KEY_QUOTA_TTL_SECS} and {MAX_KEY_QUOTA_TTL_SECS} seconds"
        ));
    }
    if settings.session_affinity_timeout_secs > MAX_SESSION_AFFINITY_TIMEOUT_SECS {
        return Err(format!(
            "session affinity timeout cannot exceed {MAX_SESSION_AFFINITY_TIMEOUT_SECS} seconds"
        ));
    }
    if settings.context_hold_timeout_secs > MAX_CONTEXT_HOLD_TIMEOUT_SECS {
        return Err(format!(
            "context hold timeout cannot exceed {MAX_CONTEXT_HOLD_TIMEOUT_SECS} seconds"
        ));
    }
    if !(MIN_DAILY_RESET_OFFSET_HOURS..=MAX_DAILY_RESET_OFFSET_HOURS).contains(&settings.daily_reset_offset_hours) {
        return Err(format!(
            "daily reset offset hours must be between {MIN_DAILY_RESET_OFFSET_HOURS} and {MAX_DAILY_RESET_OFFSET_HOURS}"
        ));
    }
    if !(MIN_QUOTA_CACHE_TTL_SECS..=MAX_QUOTA_CACHE_TTL_SECS).contains(&settings.quota_cache_ttl_secs) {
        return Err(format!(
            "quota cache TTL must be between {MIN_QUOTA_CACHE_TTL_SECS} and {MAX_QUOTA_CACHE_TTL_SECS} seconds"
        ));
    }

    for header in &settings.custom_session_headers {
        let trimmed = header.trim();
        if trimmed.is_empty() || trimmed.contains(':') || trimmed.contains(' ') || !trimmed.is_ascii() {
            return Err(format!("invalid custom session header '{header}'"));
        }
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_gateway_port() {
        assert!(validate_gateway_port(1024).is_ok());
        assert!(validate_gateway_port(17688).is_ok());
        assert!(validate_gateway_port(65535).is_ok());

        assert!(validate_gateway_port(80).is_err());
        assert!(validate_gateway_port(1023).is_err());
    }

    #[test]
    fn test_validate_gateway_settings_default() {
        let default_settings = GatewaySettings::default();
        assert!(validate_gateway_settings(&default_settings).is_ok());
    }

    #[test]
    fn test_validate_gateway_settings_validation_rules() {
        let mut settings = GatewaySettings::default();

        // Bind address
        settings.bind_address = "invalid-address".to_string();
        assert!(validate_gateway_settings(&settings).is_err());
        settings.bind_address = "0.0.0.0".to_string();
        assert!(validate_gateway_settings(&settings).is_ok());
        settings.bind_address = "192.168.1.100".to_string();
        assert!(validate_gateway_settings(&settings).is_ok());

        // Proxy URL
        settings.proxy_url = Some("ftp://proxy.local".to_string());
        assert!(validate_gateway_settings(&settings).is_err());
        settings.proxy_url = Some("socks5://127.0.0.1:1080".to_string());
        assert!(validate_gateway_settings(&settings).is_ok());
        settings.proxy_url = Some("http://proxy.corporate.net:8080".to_string());
        assert!(validate_gateway_settings(&settings).is_ok());

        // Timeouts
        settings.connect_timeout_secs = 0;
        assert!(validate_gateway_settings(&settings).is_err());
        settings.connect_timeout_secs = 100;
        assert!(validate_gateway_settings(&settings).is_err());
        settings.connect_timeout_secs = 10;

        settings.read_timeout_secs = 4;
        assert!(validate_gateway_settings(&settings).is_err());
        settings.read_timeout_secs = 5000;
        assert!(validate_gateway_settings(&settings).is_err());
        settings.read_timeout_secs = 120;

        // Custom session headers
        settings.custom_session_headers = vec!["x-session-id".to_string(), "invalid header:name".to_string()];
        assert!(validate_gateway_settings(&settings).is_err());
        settings.custom_session_headers = vec!["x-session-id".to_string(), "x-trace-id".to_string()];
        assert!(validate_gateway_settings(&settings).is_ok());
    }

    #[test]
    fn test_gateway_config_backward_compatibility() {
        let legacy_json = r#"{
            "enabled": true,
            "port": 17688,
            "providers": [],
            "keys": [],
            "default_key_id": null,
            "terminal_syncs": []
        }"#;

        let config: GatewayConfig = serde_json::from_str(legacy_json).expect("deserialize legacy config");
        assert_eq!(config.settings, GatewaySettings::default());
        assert_eq!(config.settings.bind_address, "127.0.0.1");
        assert_eq!(config.settings.connect_timeout_secs, 10);
        assert_eq!(config.settings.read_timeout_secs, 60);
        assert_eq!(config.settings.request_budget_secs, 120);
        assert_eq!(config.settings.enable_cors, true);
    }
}
