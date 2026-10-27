use super::aggregation::aggregate_tool_usage;
use super::antigravity::{collect_antigravity_usage_records, usage_home_dir};
use super::claude::collect_claude_usage_records;
use super::codex::collect_codex_usage_records;
use super::model::{ToolScan, UsageWindow, USAGE_TOOLS};
use super::opencode::collect_opencode_usage_records;
use super::stats::record_usage_collection_call;
use super::types::SessionUsageToolStats;
use super::windows::usage_window;
use crate::ai_sessions::tool_scan_cache;
use std::path::PathBuf;
use std::sync::Arc;

pub fn build_sessions_usage_tool_stats(tool: &str, days: u16) -> SessionUsageToolStats {
    let window = usage_window(days);
    build_sessions_usage_tool_stats_for_window(tool, &window, false)
}

pub(super) fn build_sessions_usage_tool_stats_for_window(
    tool: &str,
    window: &UsageWindow,
    include_model_breakdown: bool,
) -> SessionUsageToolStats {
    aggregate_tool_usage(
        tool,
        collect_usage_records_for_tool(tool, window, include_model_breakdown),
        window,
        include_model_breakdown,
    )
}

pub(super) fn normalize_usage_tool(tool: &str) -> Result<&'static str, String> {
    let normalized = tool.trim().to_ascii_lowercase();
    USAGE_TOOLS
        .iter()
        .copied()
        .find(|candidate| *candidate == normalized)
        .ok_or_else(|| format!("unsupported tool: {tool}"))
}

fn collect_usage_records_for_tool(
    tool: &str,
    window: &UsageWindow,
    include_model_breakdown: bool,
) -> Arc<ToolScan> {
    record_usage_collection_call();
    let Some(cache) = tool_scan_cache(tool) else {
        return Arc::new(ToolScan {
            source_status: "unavailable".to_string(),
            scanned_sessions: 0,
            transcript_calls: 0,
            records: Vec::new(),
            errors: vec![format!("unsupported tool: {tool}")],
        });
    };
    let root = usage_source_root(tool);
    cache.get_or_collect_keyed(&root, window.start_ms, window.end_ms, || match tool {
        "claude" => collect_claude_usage_records(window),
        "codex" => collect_codex_usage_records(window),
        "antigravity" => collect_antigravity_usage_records(window),
        "opencode" => collect_opencode_usage_records(window, include_model_breakdown),
        _ => ToolScan {
            source_status: "unavailable".to_string(),
            scanned_sessions: 0,
            transcript_calls: 0,
            records: Vec::new(),
            errors: vec![format!("unsupported tool: {tool}")],
        },
    })
}

/// Resolves the source root that a tool's collection depends on. Used only as a
/// cache discriminator so isolated HOMEs never share a cached collection.
fn usage_source_root(tool: &str) -> PathBuf {
    match tool {
        "antigravity" => usage_home_dir().unwrap_or_default(),
        _ => dirs::home_dir().unwrap_or_default(),
    }
}

pub(super) fn normalize_usage_days(days: Option<u16>) -> u16 {
    match days.unwrap_or(7) {
        7 => 7,
        15 => 15,
        30 => 30,
        _ => 7,
    }
}
