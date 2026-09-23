mod commands;
mod forwarding;
mod runtime_http;
mod selection;
mod storage;
mod templates;
#[cfg(test)]
mod tests;
mod types_config;
mod usage_log;

pub use commands::*;
pub use templates::*;
pub use types_config::*;
pub use usage_log::*;

/// Tauri event emitted after a configuration write that flipped any mapping
/// row's `auto_disabled` in either direction (REQ-005/AC-008). Consumed by the
/// API Gateway page listener; the name is the cross-stack contract.
pub(in crate::api_gateway) const API_GATEWAY_CONFIG_UPDATED_EVENT: &str =
    "api-gateway-config-update";
