use super::aggregation::{aggregate_day_stats_from_tool_stats, aggregate_tool_usage};
use super::json::usage_file_may_overlap_window;
use super::model::{ToolScan, UsageRecord, UsageWindow};
use super::opencode::read_opencode_message_tokens_from_db;
use super::types::{SessionUsageDayStats, SessionUsageToolStats};
use super::windows::usage_window;
use chrono::{Duration, Local, TimeZone};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
pub(in crate::ai_sessions) fn aggregate_usage_for_test(
    tool: &str,
    days: u16,
    records: Vec<UsageRecord>,
) -> SessionUsageToolStats {
    aggregate_tool_usage(
        tool,
        Arc::new(ToolScan {
            source_status: "available".to_string(),
            scanned_sessions: 1,
            transcript_calls: 0,
            records,
            errors: Vec::new(),
        }),
        &usage_window(days),
        true,
    )
}

#[cfg(test)]
pub(in crate::ai_sessions) fn aggregate_day_stats_for_test(
    date: String,
    tool_stats: &[SessionUsageToolStats],
) -> SessionUsageDayStats {
    aggregate_day_stats_from_tool_stats(date, tool_stats)
}

#[cfg(test)]
pub(in crate::ai_sessions) fn read_opencode_message_tokens_for_test(
    conn: &Connection,
    start_ms: i64,
    end_ms: i64,
) -> Vec<UsageRecord> {
    let date = Local::now().date_naive();
    read_opencode_message_tokens_from_db(
        conn,
        Path::new(":memory:"),
        &UsageWindow {
            days: 1,
            start_date: date,
            end_date: date,
            start_ms,
            end_ms,
        },
    )
    .records
}

#[cfg(test)]
pub(in crate::ai_sessions) fn usage_file_may_overlap_window_for_test(
    modified_ms: i64,
    window_start_ms: i64,
) -> bool {
    usage_file_may_overlap_window(modified_ms, window_start_ms)
}

#[cfg(test)]
pub(in crate::ai_sessions) fn timestamp_days_ago(days_ago: i64) -> i64 {
    let date = Local::now().date_naive() - Duration::days(days_ago);
    Local
        .from_local_datetime(&date.and_hms_opt(12, 0, 0).expect("valid test time"))
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or(0)
        })
}
