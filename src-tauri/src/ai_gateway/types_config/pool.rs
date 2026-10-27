use serde::{Deserialize, Serialize};
use serde_json::json;

use super::defaults::{default_provider_weight, default_true};
use super::protocol::UpstreamProtocol;

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
