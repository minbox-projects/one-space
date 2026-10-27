use crate::ai_gateway::provider_usage::{
    resolve_explicit_day, ProviderUsageHistory, ProviderUsageIdentity,
};
use crate::ai_gateway::storage::{modify_config, read_config};
use crate::ai_gateway::usage_log::{
    normalize_retention_days, now_millis, resolve_range_selector, validate_retention_days,
    LogFilter, TimeRange, UsageLogStore, UsageLogsPage, UsageStats, USAGE_LOG_PAGE_SIZE,
};
use crate::ai_gateway::UsageResult;

// ---------------------------------------------------------------------------
// Usage statistics, request logs and retention
// ---------------------------------------------------------------------------

/// Aggregated cards, UTC+8 buckets and per-model/provider detail for `range`.
/// `None`, an empty string and `"all"` mean all time; `"today"` and
/// `"yesterday"` bucket by hour; `"7d"`/`"15d"`/`"30d"` bucket by day. Unknown
/// selectors are rejected with the supported vocabulary.
#[tauri::command]
pub fn ai_gateway_usage_stats(range: Option<String>) -> Result<UsageStats, String> {
    let resolved = resolve_range_selector(range.as_deref(), now_millis())?;
    UsageLogStore::default_store()?.usage_stats(&resolved.range, resolved.hourly)
}

/// One page (50 rows, newest first) of request logs, or grouped rows when
/// `group_by` is `"model"`, `"provider"` or `"day"`. Range resolution, grouping,
/// filtering and pagination all happen here in the backend. The two trailing
/// optional selectors focus an exact provider ID and/or a strict `YYYY-MM-DD`
/// UTC+8 day: an explicit day owns a half-open window that takes precedence over
/// the quick range, while the recorded ID/day boundary composes by AND with the
/// ordinary name/status/model filters and with grouping. An explicitly blank ID
/// or day is an actionable error and an unknown nonblank ID is empty. Callers
/// that omit both keep their existing six-argument quick-range behavior.
#[tauri::command]
pub fn ai_gateway_request_logs(
    range: Option<String>,
    group_by: Option<String>,
    status: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    page: Option<u32>,
    provider_id: Option<String>,
    day: Option<String>,
) -> Result<UsageLogsPage, String> {
    // An explicitly blank provider id is rejected; a nonblank id is kept
    // verbatim so adjacent identities ("p" and "p ") never collapse.
    if let Some(value) = provider_id.as_deref() {
        if value.trim().is_empty() {
            return Err("a provider id must be nonblank".to_string());
        }
    }
    // Resolve the explicit day first: when present it owns a strict UTC+8
    // half-open window and the quick range is ignored, even an unsupported
    // selector. With no day the named range keeps its ordinary meaning.
    let range = match day.as_deref() {
        Some(raw) => resolve_explicit_day(Some(raw), TimeRange::default())?,
        None => resolve_range_selector(range.as_deref(), now_millis())?.range,
    };
    let filter = LogFilter {
        status: status.as_deref().and_then(UsageResult::parse),
        model: model.filter(|value| !value.trim().is_empty()),
        provider: provider.filter(|value| !value.trim().is_empty()),
    };
    let store = UsageLogStore::default_store()?;
    let group = group_by.as_deref().unwrap_or("none");
    if group.is_empty() || group == "none" {
        return match provider_id.as_deref() {
            Some(provider_id) => {
                store.query_provider_logs(&range, &filter, provider_id, page.unwrap_or(1))
            }
            None => store.query_logs(&range, &filter, page.unwrap_or(1)),
        };
    }
    if !matches!(group, "model" | "provider" | "day") {
        return Err(format!(
            "unsupported group_by '{group}': expected 'none', 'model', 'provider' or 'day'"
        ));
    }
    let groups = match provider_id.as_deref() {
        Some(provider_id) => store.group_logs_with_provider(&range, &filter, provider_id, group)?,
        None => store.group_logs(&range, &filter, group)?,
    };
    Ok(UsageLogsPage {
        page: 1,
        page_size: USAGE_LOG_PAGE_SIZE,
        total: groups.len() as u32,
        total_pages: 1,
        group_by: Some(group.to_string()),
        records: Vec::new(),
        groups,
        models: Vec::new(),
        providers: Vec::new(),
    })
}

/// Provider identities for the history selector: the union of configured and
/// historical providers keyed by exact provider ID. The current configured name
/// wins, a deleted identity keeps its latest recorded name, and an unused
/// configured provider is selectable without a fabricated first-use date.
#[tauri::command]
pub fn ai_gateway_usage_providers() -> Result<Vec<ProviderUsageIdentity>, String> {
    let configured: Vec<(String, String)> = read_config()?
        .providers
        .iter()
        .map(|provider| (provider.id.clone(), provider.name.clone()))
        .collect();
    UsageLogStore::default_store()?.provider_usage_identities(&configured)
}

/// One provider's complete UTC+8 daily calendar, coverage classification and
/// cumulative statistics through the current day.
#[tauri::command]
pub fn ai_gateway_provider_usage_history(
    provider_id: String,
) -> Result<ProviderUsageHistory, String> {
    if provider_id.trim().is_empty() {
        return Err("a provider id must be nonblank".to_string());
    }
    UsageLogStore::default_store()?.provider_usage_history(&provider_id, now_millis())
}

#[tauri::command]
pub fn ai_gateway_usage_retention_get() -> Result<u32, String> {
    Ok(normalize_retention_days(read_config()?.usage_retention_days))
}

/// Replace only the retention days; invalid values (not 1-365) are rejected
/// with an actionable error and are never persisted.
#[tauri::command]
pub fn ai_gateway_usage_retention_save(days: i64) -> Result<u32, String> {
    let validated = validate_retention_days(days)?;
    modify_config(|config| {
        config.usage_retention_days = validated;
        Ok((true, ()))
    })?;
    Ok(read_config()?.usage_retention_days)
}
