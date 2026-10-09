mod commands;
mod forwarding;
mod go_usage;
mod migration;
mod quota;
mod runtime_http;
mod selection;
mod storage;
mod templates;
#[cfg(test)]
mod tests;
mod types_config;
mod usage_log;
mod usage_store;

mod auto_refresh;

pub use commands::*;
pub use quota::ai_gateway_provider_quota;
pub(crate) use quota::__cmd__ai_gateway_provider_quota;
pub use go_usage::ai_gateway_provider_go_usage;
pub(crate) use go_usage::__cmd__ai_gateway_provider_go_usage;
pub use auto_refresh::TemplateAutoRefreshStatus;
pub use types_config::*;
pub use usage_log::*;

/// Tauri event emitted after a configuration write that flipped any mapping
/// row's `auto_disabled` in either direction (REQ-005/AC-008). Consumed by the
/// AI Gateway page listener; the name is the cross-stack contract.
pub(in crate::ai_gateway) const AI_GATEWAY_CONFIG_UPDATED_EVENT: &str =
    "ai-gateway-config-update";

/// Tauri event emitted when a key transitions into the authentication-failed
/// mark, carrying a [`GatewayKeyAuthFailedPayload`] (REQ-005/AC-007, AC-010).
/// Consumed by the AI Gateway toast listener; the name is the cross-stack
/// contract and never carries a key value.
pub(in crate::ai_gateway) const AI_GATEWAY_KEY_AUTH_FAILED_EVENT: &str =
    "ai-gateway-key-auth-failed";

/// Tauri event emitted whenever the per-template automatic-refresh failure set
/// changes (a failure recorded or cleared), carrying a
/// [`TemplateAutoRefreshStatus`] snapshot `{ failures: [...] }`. The name is the
/// cross-stack contract consumed by the AI Gateway auto-refresh adapter.
pub(in crate::ai_gateway) const AI_GATEWAY_TEMPLATE_AUTO_REFRESH_UPDATED_EVENT: &str =
    "ai-gateway-template-auto-refresh-updated";

/// Payload of [`AI_GATEWAY_KEY_AUTH_FAILED_EVENT`]: one authentication-failed
/// transition. It names the provider and key and carries the sanitized reason
/// and marking time, but never the key value (REQ-005). Serialized snake_case,
/// with no skipped fields, so the cross-stack JSON shape is stable.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(in crate::ai_gateway) struct GatewayKeyAuthFailedPayload {
    pub provider_id: String,
    pub provider_name: String,
    pub key_id: String,
    pub key_name: String,
    pub reason: String,
    pub marked_at: u64,
}
