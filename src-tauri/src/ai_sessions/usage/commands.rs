use super::aggregation::aggregate_day_stats_from_tool_stats;
use super::model::USAGE_TOOLS;
use super::quota::antigravity_quota_cache;
use super::service::{
    build_sessions_usage_tool_stats, build_sessions_usage_tool_stats_for_window,
    normalize_usage_days, normalize_usage_tool,
};
use super::types::{SessionUsageDayStats, SessionUsageToolStats};
use super::windows::usage_day_window;
use crate::ai_sessions::{clear_source_file_caches, clear_tool_scan_caches};
use chrono::{Local, NaiveDate};

#[tauri::command]
pub fn sessions_usage_clear_cache() {
    clear_tool_scan_caches();
    clear_source_file_caches();
    let cache = antigravity_quota_cache();
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = None;
}

#[tauri::command]
pub fn sessions_usage_tool_stats(
    tool: String,
    days: Option<u16>,
) -> Result<SessionUsageToolStats, String> {
    let tool = normalize_usage_tool(&tool)?;
    let days = normalize_usage_days(days);
    Ok(build_sessions_usage_tool_stats(tool, days))
}

#[tauri::command]
pub fn sessions_usage_day_stats(date: String) -> Result<SessionUsageDayStats, String> {
    let parsed_date = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| format!("invalid date format: {date}, expected YYYY-MM-DD"))?;

    let today = Local::now().date_naive();
    if parsed_date > today {
        return Err("cannot query future dates".to_string());
    }

    let window = usage_day_window(parsed_date);
    let tool_stats = USAGE_TOOLS
        .iter()
        .map(|tool| build_sessions_usage_tool_stats_for_window(tool, &window, true))
        .collect::<Vec<_>>();

    Ok(aggregate_day_stats_from_tool_stats(date, &tool_stats))
}
