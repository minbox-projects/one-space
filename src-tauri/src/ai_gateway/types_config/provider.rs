use serde::{Deserialize, Serialize};

use super::defaults::{default_provider_weight, default_true};
use super::pool::PoolCandidateIdentity;
use super::protocol::{TransportStrategy, UpstreamProtocol};

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
    /// Transport resolved from this provider's bound template. It is a runtime
    /// cache published by configuration normalization and is deliberately never
    /// serialized: the persisted record only carries the template binding, and
    /// an absent or broken binding resolves to direct.
    #[serde(skip)]
    pub transport: TransportStrategy,
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
            transport: TransportStrategy::Direct,
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
