#![allow(unused_imports)]

mod config;
mod defaults;
mod pool;
mod price;
mod protocol;
mod provider;
mod settings;
mod template;
mod terminal;

pub use config::{GatewayConfig, GatewayKey, GatewayStatus, GATEWAY_CONFIG_SCHEMA_VERSION};
pub use defaults::{
    validate_template_auto_refresh_minutes, DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES,
    DEFAULT_USAGE_RETENTION_DAYS, MAX_PROVIDER_WEIGHT, MAX_TEMPLATE_AUTO_REFRESH_MINUTES,
    MAX_USAGE_RETENTION_DAYS, MIN_PROVIDER_WEIGHT, MIN_TEMPLATE_AUTO_REFRESH_MINUTES,
    MIN_USAGE_RETENTION_DAYS,
};
pub use pool::{
    ModelPool, ModelPoolCandidate, PoolCandidateIdentity, PoolCandidateReasoning,
    PoolDefaultReasoningEffort, PoolReasoningAction, PoolReasoningBudget, PoolReasoningEffort,
    PoolReasoningMapping, PoolReasoningProfile,
};
pub use price::{ModelPrice, OffPeakPrice};
pub use protocol::{
    ErrorStrategy, QuotaStrategy, SyncStrategy, TransportStrategy, UpstreamProtocol,
};
pub use provider::{
    GatewayUpstreamProvider, KeyFailureKind, ModelMapping, UpstreamKey,
    AUTO_DISABLE_PROBE_COOLDOWN_SECS, FAILURE_THRESHOLD, KEY_PROBE_COOLDOWN_SECS,
};
pub use settings::{
    validate_gateway_port, validate_gateway_settings, GatewaySettings,
    DEFAULT_CONNECT_TIMEOUT_SECS, DEFAULT_CONTEXT_HOLD_TIMEOUT_SECS,
    DEFAULT_DAILY_RESET_OFFSET_HOURS, DEFAULT_FAILURE_THRESHOLD, DEFAULT_KEY_QUOTA_TTL_SECS,
    DEFAULT_MAX_RETRIES_PER_PROVIDER, DEFAULT_PROBE_COOLDOWN_SECS, DEFAULT_QUOTA_CACHE_TTL_SECS,
    DEFAULT_READ_TIMEOUT_SECS, DEFAULT_REQUEST_BUDGET_SECS, DEFAULT_RETRY_BASE_DELAY_MS,
    DEFAULT_SESSION_AFFINITY_TIMEOUT_SECS, MAX_ALLOWED_RETRIES_PER_PROVIDER,
    MAX_CONNECT_TIMEOUT_SECS, MAX_CONTEXT_HOLD_TIMEOUT_SECS, MAX_DAILY_RESET_OFFSET_HOURS,
    MAX_FAILURE_THRESHOLD, MAX_GATEWAY_PORT, MAX_KEY_QUOTA_TTL_SECS, MAX_PROBE_COOLDOWN_SECS,
    MAX_QUOTA_CACHE_TTL_SECS, MAX_READ_TIMEOUT_SECS, MAX_REQUEST_BUDGET_SECS,
    MAX_RETRY_BASE_DELAY_MS, MAX_SESSION_AFFINITY_TIMEOUT_SECS, MIN_CONNECT_TIMEOUT_SECS,
    MIN_DAILY_RESET_OFFSET_HOURS, MIN_GATEWAY_PORT, MIN_KEY_QUOTA_TTL_SECS, MIN_PROBE_COOLDOWN_SECS,
    MIN_QUOTA_CACHE_TTL_SECS, MIN_READ_TIMEOUT_SECS, MIN_REQUEST_BUDGET_SECS,
    MIN_RETRY_BASE_DELAY_MS,
};
pub use template::{ProviderTemplate, ProviderTemplateModel, ProviderTemplateState};
pub use terminal::{ClaudeModelSelection, TerminalSyncRecord};

pub(in crate::ai_gateway) use defaults::{
    default_port, default_provider_weight, default_template_auto_refresh_minutes, default_true,
    default_usage_retention_days, now_ts, normalize_template_auto_refresh_minutes, resolve_port,
    CONFIG_FILE, DEFAULT_PORT, DEV_DEFAULT_PORT,
};
pub(in crate::ai_gateway) use pool::{PoolNativeAction, PoolNativeReasoning};
pub(in crate::ai_gateway) use provider::KEY_QUOTA_MARK_TTL_SECS;
pub(in crate::ai_gateway) use settings::{
    default_bind_address, default_connect_timeout_secs, default_context_hold_timeout_secs,
    default_daily_reset_offset_hours, default_failure_threshold, default_key_quota_ttl_secs,
    default_max_retries_per_provider, default_probe_cooldown_secs, default_quota_cache_ttl_secs,
    default_read_timeout_secs, default_request_budget_secs, default_retry_base_delay_ms,
    default_session_affinity_timeout_secs,
};

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

