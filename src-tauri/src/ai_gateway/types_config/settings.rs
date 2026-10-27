use serde::{Deserialize, Serialize};

use super::defaults::default_true;

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
