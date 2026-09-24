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

pub use commands::*;
pub use quota::{
    ai_gateway_provider_quota, ProviderQuota, QuotaCredits, QuotaWindowLimits, QuotaWindow,
};
pub(crate) use quota::__cmd__ai_gateway_provider_quota;
pub use go_usage::{ai_gateway_provider_go_usage, GoUsage, GoUsageWindow, ProviderGoUsage};
pub(crate) use go_usage::__cmd__ai_gateway_provider_go_usage;
pub use templates::*;
pub use types_config::*;
pub use usage_log::*;

/// Tauri event emitted after a configuration write that flipped any mapping
/// row's `auto_disabled` in either direction (REQ-005/AC-008). Consumed by the
/// AI Gateway page listener; the name is the cross-stack contract.
pub(in crate::ai_gateway) const AI_GATEWAY_CONFIG_UPDATED_EVENT: &str =
    "ai-gateway-config-update";
