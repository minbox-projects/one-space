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
