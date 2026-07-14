use super::{
    candidate_home_dirs, candidate_opencode_storage_paths, collect_codex_session_files,
    parse_rfc3339_millis, system_time_to_epoch_millis,
};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE_TOOLS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SessionUsageSummary {
    pub total_tokens: u64,
    pub calls: u64,
    pub sessions: u64,
    pub cache_hit_rate: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionUsageDaily {
    pub date: String,
    pub total_tokens: u64,
    pub calls: u64,
    pub sessions: u64,
    pub cache_hit_rate: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionUsagePeakDay {
    pub date: String,
    pub total_tokens: u64,
    pub calls: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionUsageToolStats {
    pub tool: String,
    pub source_status: String,
    pub summary: SessionUsageSummary,
    pub daily: Vec<SessionUsageDaily>,
    pub peak_day: Option<SessionUsagePeakDay>,
    pub scanned_sessions: u64,
    pub scanned_calls: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionUsageStatsResponse {
    pub days: u16,
    pub tools: Vec<SessionUsageToolStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionUsageDayBreakdown {
    pub tool: String,
    pub total_tokens: u64,
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionUsageDayStats {
    pub date: String,
    pub total_tokens: u64,
    pub calls: u64,
    pub sessions: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub breakdown: Vec<SessionUsageDayBreakdown>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ai_sessions) struct UsageRecord {
    pub session_id: String,
    pub timestamp_ms: i64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone)]
struct UsageWindow {
    days: u16,
    start_date: NaiveDate,
    end_date: NaiveDate,
    start_ms: i64,
    end_ms: i64,
}

#[derive(Debug, Clone)]
struct ToolScan {
    source_status: String,
    scanned_sessions: u64,
    records: Vec<UsageRecord>,
    errors: Vec<String>,
}

#[derive(Debug, Default)]
struct UsageBucket {
    total_tokens: u64,
    calls: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_tokens: u64,
    cache_read_tokens: u64,
    sessions: HashSet<String>,
}

#[tauri::command]
pub fn sessions_usage_stats(days: Option<u16>) -> Result<SessionUsageStatsResponse, String> {
    let days = normalize_usage_days(days);
    Ok(build_sessions_usage_stats(days))
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
        .map(|tool| build_sessions_usage_tool_stats_for_window(tool, &window))
        .collect::<Vec<_>>();

    Ok(aggregate_day_stats_from_tool_stats(date, &tool_stats))
}

pub fn build_sessions_usage_stats(days: u16) -> SessionUsageStatsResponse {
    let window = usage_window(days);
    let tools = USAGE_TOOLS
        .iter()
        .map(|tool| build_sessions_usage_tool_stats_for_window(tool, &window))
        .collect();
    SessionUsageStatsResponse { days, tools }
}

pub fn build_sessions_usage_tool_stats(tool: &str, days: u16) -> SessionUsageToolStats {
    let window = usage_window(days);
    build_sessions_usage_tool_stats_for_window(tool, &window)
}

fn build_sessions_usage_tool_stats_for_window(
    tool: &str,
    window: &UsageWindow,
) -> SessionUsageToolStats {
    aggregate_tool_usage(tool, collect_usage_records_for_tool(tool), window)
}

fn normalize_usage_tool(tool: &str) -> Result<&'static str, String> {
    let normalized = tool.trim().to_ascii_lowercase();
    USAGE_TOOLS
        .iter()
        .copied()
        .find(|candidate| *candidate == normalized)
        .ok_or_else(|| format!("unsupported tool: {tool}"))
}

fn collect_usage_records_for_tool(tool: &str) -> ToolScan {
    match tool {
        "claude" => collect_claude_usage_records(),
        "codex" => collect_codex_usage_records(),
        "gemini" => collect_gemini_usage_records(),
        "opencode" => collect_opencode_usage_records(),
        _ => ToolScan {
            source_status: "unavailable".to_string(),
            scanned_sessions: 0,
            records: Vec::new(),
            errors: vec![format!("unsupported tool: {tool}")],
        },
    }
}

fn normalize_usage_days(days: Option<u16>) -> u16 {
    match days.unwrap_or(7) {
        7 => 7,
        15 => 15,
        30 => 30,
        _ => 7,
    }
}

fn usage_window(days: u16) -> UsageWindow {
    let end_date = Local::now().date_naive();
    let start_date = end_date - Duration::days(days.saturating_sub(1) as i64);
    let start_ms = Local
        .from_local_datetime(&start_date.and_hms_opt(0, 0, 0).expect("valid start"))
        .single()
        .or_else(|| {
            Local
                .from_local_datetime(&start_date.and_hms_opt(1, 0, 0).expect("valid start"))
                .single()
        })
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    let next_date = end_date + Duration::days(1);
    let end_ms = Local
        .from_local_datetime(&next_date.and_hms_opt(0, 0, 0).expect("valid end"))
        .single()
        .or_else(|| {
            Local
                .from_local_datetime(&next_date.and_hms_opt(1, 0, 0).expect("valid end"))
                .single()
        })
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(i64::MAX);
    UsageWindow {
        days,
        start_date,
        end_date,
        start_ms,
        end_ms,
    }
}

fn usage_day_window(date: NaiveDate) -> UsageWindow {
    let start_ms = Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("valid start"))
        .single()
        .or_else(|| {
            Local
                .from_local_datetime(&date.and_hms_opt(1, 0, 0).expect("valid start"))
                .single()
        })
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    let next_date = date + Duration::days(1);
    let end_ms = Local
        .from_local_datetime(&next_date.and_hms_opt(0, 0, 0).expect("valid end"))
        .single()
        .or_else(|| {
            Local
                .from_local_datetime(&next_date.and_hms_opt(1, 0, 0).expect("valid end"))
                .single()
        })
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(i64::MAX);
    UsageWindow {
        days: 1,
        start_date: date,
        end_date: date,
        start_ms,
        end_ms,
    }
}

fn aggregate_day_stats_from_tool_stats(
    date: String,
    tool_stats: &[SessionUsageToolStats],
) -> SessionUsageDayStats {
    let mut total_tokens = 0u64;
    let mut total_calls = 0u64;
    let mut total_sessions = 0u64;
    let mut total_input = 0u64;
    let mut total_output = 0u64;
    let mut total_cache = 0u64;
    let mut breakdown = Vec::new();

    for tool_stat in tool_stats {
        let day = tool_stat
            .daily
            .iter()
            .find(|item| item.date == date)
            .cloned()
            .unwrap_or(SessionUsageDaily {
                date: date.clone(),
                total_tokens: 0,
                calls: 0,
                sessions: 0,
                cache_hit_rate: 0,
                input_tokens: 0,
                output_tokens: 0,
                cache_tokens: 0,
            });

        total_tokens = total_tokens.saturating_add(day.total_tokens);
        total_calls = total_calls.saturating_add(day.calls);
        total_sessions = total_sessions.saturating_add(day.sessions);
        total_input = total_input.saturating_add(day.input_tokens);
        total_output = total_output.saturating_add(day.output_tokens);
        total_cache = total_cache.saturating_add(day.cache_tokens);

        breakdown.push(SessionUsageDayBreakdown {
            tool: tool_stat.tool.clone(),
            total_tokens: day.total_tokens,
            calls: day.calls,
            input_tokens: day.input_tokens,
            output_tokens: day.output_tokens,
            cache_tokens: day.cache_tokens,
        });
    }

    SessionUsageDayStats {
        date,
        total_tokens,
        calls: total_calls,
        sessions: total_sessions,
        input_tokens: total_input,
        output_tokens: total_output,
        cache_tokens: total_cache,
        breakdown,
    }
}

fn aggregate_tool_usage(tool: &str, scan: ToolScan, window: &UsageWindow) -> SessionUsageToolStats {
    let mut by_date = HashMap::<String, UsageBucket>::new();
    let mut scanned_calls = 0_u64;
    for record in scan.records {
        if record.timestamp_ms < window.start_ms || record.timestamp_ms >= window.end_ms {
            continue;
        }
        let Some(date) = local_date_key(record.timestamp_ms) else {
            continue;
        };
        scanned_calls += 1;
        let bucket = by_date.entry(date).or_default();
        bucket.total_tokens = bucket.total_tokens.saturating_add(record.total_tokens);
        bucket.calls = bucket.calls.saturating_add(1);
        bucket.input_tokens = bucket.input_tokens.saturating_add(record.input_tokens);
        bucket.output_tokens = bucket.output_tokens.saturating_add(record.output_tokens);
        bucket.cache_tokens = bucket.cache_tokens.saturating_add(record.cache_tokens);
        bucket.cache_read_tokens = bucket
            .cache_read_tokens
            .saturating_add(record.cache_read_tokens);
        bucket.sessions.insert(record.session_id);
    }

    let mut summary_sessions = HashSet::<String>::new();
    let mut summary = SessionUsageSummary::default();
    let mut daily = Vec::new();
    for offset in 0..window.days {
        let date = window.start_date + Duration::days(offset as i64);
        if date > window.end_date {
            continue;
        }
        let key = date.format("%Y-%m-%d").to_string();
        let bucket = by_date.remove(&key).unwrap_or_default();
        summary.total_tokens = summary.total_tokens.saturating_add(bucket.total_tokens);
        summary.calls = summary.calls.saturating_add(bucket.calls);
        summary.input_tokens = summary.input_tokens.saturating_add(bucket.input_tokens);
        summary.output_tokens = summary.output_tokens.saturating_add(bucket.output_tokens);
        summary.cache_tokens = summary.cache_tokens.saturating_add(bucket.cache_tokens);
        for session_id in &bucket.sessions {
            summary_sessions.insert(session_id.clone());
        }
        daily.push(SessionUsageDaily {
            date: key,
            total_tokens: bucket.total_tokens,
            calls: bucket.calls,
            sessions: bucket.sessions.len() as u64,
            cache_hit_rate: cache_hit_rate_percent(
                tool,
                bucket.cache_read_tokens,
                bucket.input_tokens,
            ),
            input_tokens: bucket.input_tokens,
            output_tokens: bucket.output_tokens,
            cache_tokens: bucket.cache_tokens,
        });
    }
    summary.sessions = summary_sessions.len() as u64;
    // 排除未使用的日期，计算每日缓存命中率的平均值
    let used_rates: Vec<u64> = daily
        .iter()
        .filter(|day| day.calls > 0)
        .map(|day| day.cache_hit_rate)
        .collect();
    summary.cache_hit_rate = if used_rates.is_empty() {
        0
    } else {
        used_rates.iter().sum::<u64>() / used_rates.len() as u64
    };

    let peak_day = daily
        .iter()
        .filter(|day| day.total_tokens > 0 || day.calls > 0)
        .max_by(|left, right| {
            left.total_tokens
                .cmp(&right.total_tokens)
                .then_with(|| left.calls.cmp(&right.calls))
                .then_with(|| right.date.cmp(&left.date))
        })
        .map(|day| SessionUsagePeakDay {
            date: day.date.clone(),
            total_tokens: day.total_tokens,
            calls: day.calls,
        });

    SessionUsageToolStats {
        tool: tool.to_string(),
        source_status: if scan.source_status == "available" && scan.scanned_sessions == 0 {
            "empty".to_string()
        } else {
            scan.source_status
        },
        summary,
        daily,
        peak_day,
        scanned_sessions: scan.scanned_sessions,
        scanned_calls,
        errors: scan.errors,
    }
}

fn local_date_key(timestamp_ms: i64) -> Option<String> {
    DateTime::from_timestamp_millis(timestamp_ms).map(|dt| {
        dt.with_timezone(&Local)
            .date_naive()
            .format("%Y-%m-%d")
            .to_string()
    })
}

fn cache_hit_rate_percent(tool: &str, cache_read_tokens: u64, input_tokens: u64) -> u64 {
    // Codex reports cached input as part of input_tokens; other sources report it separately.
    let total = if tool == "codex" {
        input_tokens
    } else {
        input_tokens.saturating_add(cache_read_tokens)
    };
    if total == 0 {
        return 0;
    }
    cache_read_tokens.saturating_mul(100) / total
}

fn total_or_sum(total: u64, input: u64, output: u64, cache: u64) -> u64 {
    if total > 0 {
        total
    } else {
        input.saturating_add(output).saturating_add(cache)
    }
}

fn json_u64(value: Option<&Value>) -> u64 {
    match value {
        Some(Value::Number(number)) => number.as_u64().unwrap_or_else(|| {
            number
                .as_i64()
                .filter(|value| *value > 0)
                .map(|value| value as u64)
                .unwrap_or(0)
        }),
        Some(Value::String(text)) => text.trim().parse::<u64>().unwrap_or(0),
        _ => 0,
    }
}

fn file_stem_session_id(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn modified_ms(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_to_epoch_millis)
        .unwrap_or(0)
}

fn collect_claude_usage_records() -> ToolScan {
    let Some(home) = dirs::home_dir() else {
        return unavailable_scan();
    };
    let projects_root = home.join(".claude").join("projects");
    if !projects_root.is_dir() {
        return unavailable_scan();
    }
    let mut scan = ToolScan {
        source_status: "available".to_string(),
        scanned_sessions: 0,
        records: Vec::new(),
        errors: Vec::new(),
    };
    for path in json_files_recursive(&projects_root, "jsonl") {
        scan.scanned_sessions += 1;
        match parse_claude_usage_file(&path) {
            Ok(records) => scan.records.extend(records),
            Err(error) => scan.errors.push(format!("{}: {error}", path.display())),
        }
    }
    scan
}

pub(in crate::ai_sessions) fn parse_claude_usage_file(
    path: &Path,
) -> Result<Vec<UsageRecord>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let fallback_session_id = file_stem_session_id(path);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if value.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let Some(usage) = value
            .get("message")
            .and_then(|message| message.get("usage"))
        else {
            continue;
        };
        let cache_read = json_u64(usage.get("cache_read_input_tokens"));
        let cache_create = json_u64(usage.get("cache_creation_input_tokens"));
        let input = json_u64(usage.get("input_tokens"));
        let output = json_u64(usage.get("output_tokens"));
        let cache = cache_read.saturating_add(cache_create);
        let total = total_or_sum(json_u64(usage.get("total_tokens")), input, output, cache);
        if input == 0 && output == 0 && cache == 0 && total == 0 {
            continue;
        }
        let timestamp_ms = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_rfc3339_millis)
            .unwrap_or_else(|| modified_ms(path));
        out.push(UsageRecord {
            session_id: value
                .get("sessionId")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&fallback_session_id)
                .to_string(),
            timestamp_ms,
            input_tokens: input,
            output_tokens: output,
            cache_tokens: cache,
            cache_read_tokens: cache_read,
            total_tokens: total,
        });
    }
    Ok(out)
}

fn collect_codex_usage_records() -> ToolScan {
    let mut scan = ToolScan {
        source_status: "unavailable".to_string(),
        scanned_sessions: 0,
        records: Vec::new(),
        errors: Vec::new(),
    };
    for home in candidate_home_dirs(None) {
        for root in [
            home.join(".codex").join("sessions"),
            home.join(".codex").join("archived_sessions"),
        ] {
            if !root.is_dir() {
                continue;
            }
            scan.source_status = "available".to_string();
            for (path, _) in collect_codex_session_files(&root, usize::MAX) {
                scan.scanned_sessions += 1;
                match parse_codex_usage_file(&path) {
                    Ok(records) => scan.records.extend(records),
                    Err(error) => scan.errors.push(format!("{}: {error}", path.display())),
                }
            }
        }
    }
    scan
}

pub(in crate::ai_sessions) fn parse_codex_usage_file(
    path: &Path,
) -> Result<Vec<UsageRecord>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let fallback_session_id = file_stem_session_id(path);
    let mut session_id = String::new();
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if value.get("type").and_then(|v| v.as_str()) == Some("session_meta") {
            if session_id.is_empty() {
                session_id = value
                    .get("payload")
                    .and_then(|payload| payload.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
            }
            continue;
        }
        if value.get("type").and_then(|v| v.as_str()) != Some("event_msg") {
            continue;
        }
        let Some(payload) = value.get("payload") else {
            continue;
        };
        if payload.get("type").and_then(|v| v.as_str()) != Some("token_count") {
            continue;
        }
        let Some(usage) = payload
            .get("info")
            .and_then(|info| info.get("last_token_usage"))
        else {
            continue;
        };
        let cache_read = json_u64(usage.get("cached_input_tokens"));
        let input = json_u64(usage.get("input_tokens"));
        let output = json_u64(usage.get("output_tokens"));
        let cache = cache_read;
        let total = total_or_sum(json_u64(usage.get("total_tokens")), input, output, cache);
        if input == 0 && output == 0 && cache == 0 && total == 0 {
            continue;
        }
        let timestamp_ms = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .or_else(|| payload.get("timestamp").and_then(|v| v.as_str()))
            .and_then(parse_rfc3339_millis)
            .unwrap_or_else(|| modified_ms(path));
        out.push(UsageRecord {
            session_id: if session_id.is_empty() {
                fallback_session_id.clone()
            } else {
                session_id.clone()
            },
            timestamp_ms,
            input_tokens: input,
            output_tokens: output,
            cache_tokens: cache,
            cache_read_tokens: cache_read,
            total_tokens: total,
        });
    }
    Ok(out)
}

fn collect_gemini_usage_records() -> ToolScan {
    let Some(home) = dirs::home_dir() else {
        return unavailable_scan();
    };
    let tmp_root = home.join(".gemini").join("tmp");
    if !tmp_root.is_dir() {
        return unavailable_scan();
    }
    let mut scan = ToolScan {
        source_status: "available".to_string(),
        scanned_sessions: 0,
        records: Vec::new(),
        errors: Vec::new(),
    };
    for path in gemini_session_files(&tmp_root) {
        scan.scanned_sessions += 1;
        match parse_gemini_usage_file(&path) {
            Ok(records) => scan.records.extend(records),
            Err(error) => scan.errors.push(format!("{}: {error}", path.display())),
        }
    }
    scan
}

pub(in crate::ai_sessions) fn parse_gemini_usage_file(
    path: &Path,
) -> Result<Vec<UsageRecord>, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let session_id = value
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
        })
        .to_string();
    let fallback_ts = value
        .get("lastUpdated")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("startTime").and_then(|v| v.as_str()))
        .and_then(parse_rfc3339_millis)
        .unwrap_or_else(|| modified_ms(path));
    let mut out = Vec::new();
    if let Some(messages) = value
        .get("messages")
        .and_then(|messages| messages.as_array())
    {
        for message in messages {
            let Some(tokens) = message.get("tokens") else {
                continue;
            };
            let input = json_u64(tokens.get("input"));
            let output = json_u64(tokens.get("output"));
            let cache =
                json_u64(tokens.get("cached")).saturating_add(json_u64(tokens.get("cache")));
            let total = total_or_sum(json_u64(tokens.get("total")), input, output, cache);
            if input == 0 && output == 0 && cache == 0 && total == 0 {
                continue;
            }
            let timestamp_ms = message
                .get("timestamp")
                .and_then(|v| v.as_str())
                .or_else(|| message.get("time").and_then(|v| v.as_str()))
                .and_then(parse_rfc3339_millis)
                .unwrap_or(fallback_ts);
            out.push(UsageRecord {
                session_id: session_id.clone(),
                timestamp_ms,
                input_tokens: input,
                output_tokens: output,
                cache_tokens: cache,
                cache_read_tokens: 0,
                total_tokens: total,
            });
        }
    }
    Ok(out)
}

fn collect_opencode_usage_records() -> ToolScan {
    if let Some(scan) = collect_opencode_usage_from_db() {
        return scan;
    }
    let mut scan = ToolScan {
        source_status: "unavailable".to_string(),
        scanned_sessions: 0,
        records: Vec::new(),
        errors: Vec::new(),
    };
    for storage_paths in candidate_opencode_storage_paths() {
        if !storage_paths.sessions_root.is_dir() && !storage_paths.messages_root.is_dir() {
            continue;
        }
        scan.source_status = "available".to_string();
        for session_id in opencode_json_session_ids(&storage_paths.sessions_root) {
            scan.scanned_sessions += 1;
            let messages_dir = storage_paths.messages_root.join(&session_id);
            match parse_opencode_message_usage_dir(&messages_dir, &session_id) {
                Ok(records) => scan.records.extend(records),
                Err(error) => scan
                    .errors
                    .push(format!("{}: {error}", messages_dir.display())),
            }
        }
    }
    scan
}

fn collect_opencode_usage_from_db() -> Option<ToolScan> {
    let db_path = dirs::home_dir()?
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    if !db_path.exists() {
        return None;
    }
    let conn = match Connection::open(&db_path) {
        Ok(conn) => conn,
        Err(error) => {
            return Some(ToolScan {
                source_status: "error".to_string(),
                scanned_sessions: 0,
                records: Vec::new(),
                errors: vec![format!("{}: {error}", db_path.display())],
            });
        }
    };
    if let Some(scan) = read_opencode_session_summary_tokens(&conn, &db_path) {
        return Some(scan);
    }
    Some(read_opencode_message_tokens_from_db(&conn, &db_path))
}

fn read_opencode_session_summary_tokens(conn: &Connection, db_path: &Path) -> Option<ToolScan> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, time_updated,
                   COALESCE(tokens_input, 0),
                   COALESCE(tokens_output, 0),
                   COALESCE(tokens_cache_read, 0),
                   COALESCE(tokens_cache_write, 0)
            FROM session
            WHERE time_archived IS NULL
            "#,
        )
        .ok()?;
    let mut scan = ToolScan {
        source_status: "available".to_string(),
        scanned_sessions: 0,
        records: Vec::new(),
        errors: Vec::new(),
    };
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, u64>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, u64>(5)?,
            ))
        })
        .ok()?;
    for row in rows {
        match row {
            Ok((session_id, updated_at, input, output, cache_read, cache_write)) => {
                scan.scanned_sessions += 1;
                let cache = cache_read.saturating_add(cache_write);
                let total = input.saturating_add(output).saturating_add(cache);
                if total == 0 {
                    continue;
                }
                scan.records.push(UsageRecord {
                    session_id,
                    timestamp_ms: updated_at,
                    input_tokens: input,
                    output_tokens: output,
                    cache_tokens: cache,
                    cache_read_tokens: cache_read,
                    total_tokens: total,
                });
            }
            Err(error) => scan.errors.push(format!("{}: {error}", db_path.display())),
        }
    }
    Some(scan)
}

fn read_opencode_message_tokens_from_db(conn: &Connection, db_path: &Path) -> ToolScan {
    let mut scan = ToolScan {
        source_status: "available".to_string(),
        scanned_sessions: 0,
        records: Vec::new(),
        errors: Vec::new(),
    };
    let session_ids = match read_opencode_db_session_ids(conn) {
        Ok(ids) => ids,
        Err(error) => {
            scan.source_status = "error".to_string();
            scan.errors.push(format!("{}: {error}", db_path.display()));
            return scan;
        }
    };
    scan.scanned_sessions = session_ids.len() as u64;
    let mut stmt = match conn.prepare(
        r#"
        SELECT session_id, time_created, data
        FROM message
        WHERE session_id IN (SELECT id FROM session WHERE time_archived IS NULL)
        ORDER BY time_created ASC
        "#,
    ) {
        Ok(stmt) => stmt,
        Err(error) => {
            scan.source_status = "error".to_string();
            scan.errors.push(format!("{}: {error}", db_path.display()));
            return scan;
        }
    };
    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) {
        Ok(rows) => rows,
        Err(error) => {
            scan.source_status = "error".to_string();
            scan.errors.push(format!("{}: {error}", db_path.display()));
            return scan;
        }
    };
    for row in rows {
        match row {
            Ok((session_id, timestamp_ms, data)) => match parse_opencode_tokens_value(
                serde_json::from_str::<Value>(&data)
                    .ok()
                    .as_ref()
                    .and_then(|value| value.get("tokens")),
            ) {
                Some((input, output, cache, cache_read, total)) => scan.records.push(UsageRecord {
                    session_id,
                    timestamp_ms,
                    input_tokens: input,
                    output_tokens: output,
                    cache_tokens: cache,
                    cache_read_tokens: cache_read,
                    total_tokens: total,
                }),
                None => {}
            },
            Err(error) => scan.errors.push(format!("{}: {error}", db_path.display())),
        }
    }
    scan
}

fn read_opencode_db_session_ids(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT id FROM session WHERE time_archived IS NULL")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub(in crate::ai_sessions) fn parse_opencode_message_usage_dir(
    messages_dir: &Path,
    session_id: &str,
) -> Result<Vec<UsageRecord>, String> {
    if !messages_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for path in json_files_recursive(messages_dir, "json") {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        let Some((input, output, cache, cache_read, total)) = parse_opencode_tokens_value(
            value
                .get("tokens")
                .or_else(|| value.get("data").and_then(|data| data.get("tokens"))),
        ) else {
            continue;
        };
        let timestamp_ms = value
            .get("time")
            .and_then(|time| time.get("created"))
            .and_then(|v| v.as_i64())
            .or_else(|| value.get("time_created").and_then(|v| v.as_i64()))
            .unwrap_or_else(|| modified_ms(&path));
        out.push(UsageRecord {
            session_id: session_id.to_string(),
            timestamp_ms,
            input_tokens: input,
            output_tokens: output,
            cache_tokens: cache,
            cache_read_tokens: cache_read,
            total_tokens: total,
        });
    }
    Ok(out)
}

fn parse_opencode_tokens_value(tokens: Option<&Value>) -> Option<(u64, u64, u64, u64, u64)> {
    let tokens = tokens?;
    let input = json_u64(tokens.get("input")).saturating_add(json_u64(tokens.get("input_tokens")));
    let output =
        json_u64(tokens.get("output")).saturating_add(json_u64(tokens.get("output_tokens")));
    let cache_read = json_u64(tokens.get("cache_read"))
        .saturating_add(json_u64(tokens.get("cached")))
        .saturating_add(json_u64(tokens.get("cache_read_tokens")));
    let cache_write = json_u64(tokens.get("cache_write"))
        .saturating_add(json_u64(tokens.get("cache_write_tokens")))
        .saturating_add(json_u64(tokens.get("cache")));
    let cache = cache_read.saturating_add(cache_write);
    let total = total_or_sum(
        json_u64(tokens.get("total")).saturating_add(json_u64(tokens.get("total_tokens"))),
        input,
        output,
        cache,
    );
    if input == 0 && output == 0 && cache == 0 && total == 0 {
        None
    } else {
        Some((input, output, cache, cache_read, total))
    }
}

fn unavailable_scan() -> ToolScan {
    ToolScan {
        source_status: "unavailable".to_string(),
        scanned_sessions: 0,
        records: Vec::new(),
        errors: Vec::new(),
    }
}

fn json_files_recursive(root: &Path, suffix: &str) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(suffix))
                .unwrap_or(false)
            {
                out.push(path);
            }
        }
    }
    out
}

fn gemini_session_files(root: &Path) -> Vec<PathBuf> {
    json_files_recursive(root, ".json")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("session-") && name.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect()
}

fn opencode_json_session_ids(sessions_root: &Path) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in json_files_recursive(sessions_root, ".json") {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let Some(id) = value
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if seen.insert(id.to_string()) {
            out.push(id.to_string());
        }
    }
    out
}

#[cfg(test)]
pub(in crate::ai_sessions) fn aggregate_usage_for_test(
    tool: &str,
    days: u16,
    records: Vec<UsageRecord>,
) -> SessionUsageToolStats {
    aggregate_tool_usage(
        tool,
        ToolScan {
            source_status: "available".to_string(),
            scanned_sessions: 1,
            records,
            errors: Vec::new(),
        },
        &usage_window(days),
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
