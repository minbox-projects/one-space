use super::{
    candidate_home_dirs, candidate_opencode_storage_paths, collect_codex_session_files,
    parse_rfc3339_millis, system_time_to_epoch_millis,
};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration as StdDuration, Instant};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE_TOOLS: [&str; 4] = ["claude", "codex", "antigravity", "opencode"];
const USAGE_SCAN_CACHE_TTL: StdDuration = StdDuration::from_secs(30);

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
    #[serde(skip)]
    pub models: Vec<SessionUsageModelStats>,
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
    pub cache_hit_rate: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub models: Vec<SessionUsageModelStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionUsageModelStats {
    pub model: String,
    pub total_tokens: u64,
    pub calls: u64,
    pub sessions: u64,
    pub cache_hit_rate: u64,
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
    pub model: Option<String>,
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

#[derive(Debug, Default)]
pub(in crate::ai_sessions) struct ToolScan {
    pub(in crate::ai_sessions) source_status: String,
    pub(in crate::ai_sessions) scanned_sessions: u64,
    pub(in crate::ai_sessions) records: Vec<UsageRecord>,
    pub(in crate::ai_sessions) errors: Vec<String>,
}

#[derive(Debug)]
struct CachedToolScan {
    collected_at: Instant,
    start_ms: i64,
    end_ms: i64,
    scan: Arc<ToolScan>,
}

#[derive(Debug, Default)]
pub(in crate::ai_sessions) struct ToolScanCache {
    entry: Mutex<Option<CachedToolScan>>,
}

impl ToolScanCache {
    pub(in crate::ai_sessions) fn get_or_collect<F>(
        &self,
        start_ms: i64,
        end_ms: i64,
        collect: F,
    ) -> Arc<ToolScan>
    where
        F: FnOnce() -> ToolScan,
    {
        let mut entry = self
            .entry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = entry.as_ref() {
            if cached.collected_at.elapsed() < USAGE_SCAN_CACHE_TTL
                && cached.start_ms <= start_ms
                && cached.end_ms >= end_ms
            {
                return cached.scan.clone();
            }
        }

        let scan = Arc::new(collect());
        *entry = Some(CachedToolScan {
            collected_at: Instant::now(),
            start_ms,
            end_ms,
            scan: Arc::clone(&scan),
        });
        scan
    }

    pub(in crate::ai_sessions) fn clear(&self) {
        let mut entry = self
            .entry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *entry = None;
    }
}

#[derive(Debug)]
struct UsageScanCaches {
    by_tool: HashMap<&'static str, ToolScanCache>,
}

impl UsageScanCaches {
    fn for_tool(&self, tool: &str) -> Option<&ToolScanCache> {
        self.by_tool.get(tool)
    }

    fn clear(&self) {
        for cache in self.by_tool.values() {
            cache.clear();
        }
    }
}

impl Default for UsageScanCaches {
    fn default() -> Self {
        Self {
            by_tool: USAGE_TOOLS
                .iter()
                .copied()
                .filter(|tool| *tool != "opencode" && *tool != "antigravity")
                .map(|tool| (tool, ToolScanCache::default()))
                .collect(),
        }
    }
}

fn usage_scan_caches() -> &'static UsageScanCaches {
    static CACHES: OnceLock<UsageScanCaches> = OnceLock::new();
    CACHES.get_or_init(UsageScanCaches::default)
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

fn add_record_to_bucket(bucket: &mut UsageBucket, record: &UsageRecord) {
    bucket.total_tokens = bucket.total_tokens.saturating_add(record.total_tokens);
    bucket.calls = bucket.calls.saturating_add(1);
    bucket.input_tokens = bucket.input_tokens.saturating_add(record.input_tokens);
    bucket.output_tokens = bucket.output_tokens.saturating_add(record.output_tokens);
    bucket.cache_tokens = bucket.cache_tokens.saturating_add(record.cache_tokens);
    bucket.cache_read_tokens = bucket
        .cache_read_tokens
        .saturating_add(record.cache_read_tokens);
    bucket.sessions.insert(record.session_id.clone());
}

#[tauri::command]
pub fn sessions_usage_stats(days: Option<u16>) -> Result<SessionUsageStatsResponse, String> {
    let days = normalize_usage_days(days);
    Ok(build_sessions_usage_stats(days))
}

#[tauri::command]
pub fn sessions_usage_clear_cache() {
    usage_scan_caches().clear();
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

pub fn build_sessions_usage_stats(days: u16) -> SessionUsageStatsResponse {
    let window = usage_window(days);
    let tools = USAGE_TOOLS
        .iter()
        .map(|tool| build_sessions_usage_tool_stats_for_window(tool, &window, false))
        .collect();
    SessionUsageStatsResponse { days, tools }
}

pub fn build_sessions_usage_tool_stats(tool: &str, days: u16) -> SessionUsageToolStats {
    let window = usage_window(days);
    build_sessions_usage_tool_stats_for_window(tool, &window, false)
}

fn build_sessions_usage_tool_stats_for_window(
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

fn normalize_usage_tool(tool: &str) -> Result<&'static str, String> {
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
    if tool == "opencode" {
        return Arc::new(collect_opencode_usage_records(
            window,
            include_model_breakdown,
        ));
    }
    if tool == "antigravity" {
        return Arc::new(collect_antigravity_usage_records(window));
    }
    let Some(cache) = usage_scan_caches().for_tool(tool) else {
        return Arc::new(ToolScan {
            source_status: "unavailable".to_string(),
            scanned_sessions: 0,
            records: Vec::new(),
            errors: vec![format!("unsupported tool: {tool}")],
        });
    };
    cache.get_or_collect(window.start_ms, window.end_ms, || match tool {
        "claude" => collect_claude_usage_records(window),
        "codex" => collect_codex_usage_records(window),
        _ => ToolScan {
            source_status: "unavailable".to_string(),
            scanned_sessions: 0,
            records: Vec::new(),
            errors: vec![format!("unsupported tool: {tool}")],
        },
    })
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
            cache_hit_rate: day.cache_hit_rate,
            input_tokens: day.input_tokens,
            output_tokens: day.output_tokens,
            cache_tokens: day.cache_tokens,
            models: tool_stat.models.clone(),
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

fn aggregate_tool_usage(
    tool: &str,
    scan: Arc<ToolScan>,
    window: &UsageWindow,
    include_model_breakdown: bool,
) -> SessionUsageToolStats {
    let mut by_date = HashMap::<String, UsageBucket>::new();
    let mut by_model = HashMap::<String, UsageBucket>::new();
    let mut scanned_calls = 0_u64;
    for record in &scan.records {
        if record.timestamp_ms < window.start_ms || record.timestamp_ms >= window.end_ms {
            continue;
        }
        let Some(date) = local_date_key(record.timestamp_ms) else {
            continue;
        };
        scanned_calls += 1;
        let bucket = by_date.entry(date).or_default();
        add_record_to_bucket(bucket, record);
        if include_model_breakdown {
            let model = record
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("unknown")
                .to_string();
            let model_bucket = by_model.entry(model).or_default();
            add_record_to_bucket(model_bucket, record);
        }
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

    let mut models = by_model
        .into_iter()
        .map(|(model, bucket)| SessionUsageModelStats {
            model,
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
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| {
        right
            .total_tokens
            .cmp(&left.total_tokens)
            .then_with(|| right.calls.cmp(&left.calls))
            .then_with(|| left.model.cmp(&right.model))
    });

    SessionUsageToolStats {
        tool: tool.to_string(),
        source_status: if scan.source_status == "available" && scan.records.is_empty() {
            "empty".to_string()
        } else {
            scan.source_status.clone()
        },
        summary,
        daily,
        peak_day,
        scanned_sessions: scan.scanned_sessions,
        scanned_calls,
        errors: scan.errors.clone(),
        models,
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

fn json_nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn opencode_model_name(value: &Value) -> Option<String> {
    let model = value
        .get("modelID")
        .or_else(|| value.get("model"))
        .or_else(|| value.get("data").and_then(|data| data.get("modelID")))?;
    json_nonempty_string(Some(model)).or_else(|| {
        model
            .get("id")
            .and_then(|id| json_nonempty_string(Some(id)))
    })
}

fn opencode_v2_model_name(value: &Value) -> Option<String> {
    value
        .get("model")
        .and_then(|model| model.get("id"))
        .or_else(|| {
            value
                .get("data")
                .and_then(|data| data.get("model"))
                .and_then(|model| model.get("id"))
        })
        .and_then(|id| json_nonempty_string(Some(id)))
        .or_else(|| json_nonempty_string(value.get("modelID")))
        .or_else(|| {
            value
                .get("data")
                .and_then(|data| json_nonempty_string(data.get("modelID")))
        })
        .or_else(|| {
            value
                .get("model")
                .and_then(|model| json_nonempty_string(Some(model)))
        })
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

fn usage_file_may_overlap_window(modified_ms: i64, window_start_ms: i64) -> bool {
    // Unknown metadata must fall back to parsing so optimization never drops usage.
    modified_ms == 0 || modified_ms >= window_start_ms
}

fn collect_claude_usage_records(window: &UsageWindow) -> ToolScan {
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
        if !usage_file_may_overlap_window(modified_ms(&path), window.start_ms) {
            continue;
        }
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
            model: value
                .get("message")
                .and_then(|message| json_nonempty_string(message.get("model"))),
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

fn collect_codex_usage_records(window: &UsageWindow) -> ToolScan {
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
            for (path, modified_ms) in collect_codex_session_files(&root, usize::MAX) {
                if !usage_file_may_overlap_window(modified_ms, window.start_ms) {
                    break;
                }
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
    let mut current_model = None;
    let mut records_pending_model: Vec<usize> = Vec::new();
    let mut out: Vec<UsageRecord> = Vec::new();
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
            if current_model.is_none() {
                current_model = value
                    .get("payload")
                    .and_then(|payload| json_nonempty_string(payload.get("model")));
            }
            continue;
        }
        if value.get("type").and_then(|v| v.as_str()) == Some("turn_context") {
            let next_model = value
                .get("payload")
                .and_then(|payload| json_nonempty_string(payload.get("model")));
            if current_model.is_none() {
                if let Some(model) = next_model.as_ref() {
                    for index in records_pending_model.drain(..) {
                        out[index].model = Some(model.clone());
                    }
                }
            }
            current_model = next_model.or(current_model);
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
        // Codex reports cached input as part of input_tokens (see
        // cache_hit_rate_percent), so the no-total fallback must not add the
        // cache tier on top of the input tier.
        let total = match json_u64(usage.get("total_tokens")) {
            0 => input.saturating_add(output),
            total => total,
        };
        if input == 0 && output == 0 && cache == 0 && total == 0 {
            continue;
        }
        let timestamp_ms = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .or_else(|| payload.get("timestamp").and_then(|v| v.as_str()))
            .and_then(parse_rfc3339_millis)
            .unwrap_or_else(|| modified_ms(path));
        let model = payload
            .get("info")
            .and_then(|info| json_nonempty_string(info.get("model")))
            .or_else(|| current_model.clone());
        if model.is_none() {
            records_pending_model.push(out.len());
        }
        out.push(UsageRecord {
            session_id: if session_id.is_empty() {
                fallback_session_id.clone()
            } else {
                session_id.clone()
            },
            model,
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

fn collect_antigravity_usage_records(window: &UsageWindow) -> ToolScan {
    let Some(home) = usage_home_dir() else {
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
    let Ok(rollouts) = fs::read_dir(&tmp_root) else {
        return unavailable_scan();
    };
    for rollout in rollouts.flatten() {
        let chats_dir = rollout.path().join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        for path in antigravity_session_files(&chats_dir) {
            if !usage_file_may_overlap_window(modified_ms(&path), window.start_ms) {
                continue;
            }
            scan.scanned_sessions += 1;
            let parsed =
                if path.extension().and_then(|extension| extension.to_str()) == Some("jsonl") {
                    parse_antigravity_jsonl_usage_file(&path)
                } else {
                    parse_antigravity_json_usage_file(&path)
                };
            match parsed {
                Ok(records) => scan.records.extend(records),
                Err(error) => scan.errors.push(format!("{}: {error}", path.display())),
            }
        }
    }
    scan
}

/// Resolves HOME for usage scans so tests can redirect it via the thread-local
/// override without mutating the process environment.
fn usage_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        if let Some(home) = crate::config::test_home::test_home_override() {
            return Some(home);
        }
    }
    dirs::home_dir()
}

fn antigravity_session_files(chats_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(chats_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("session-") && (name.ends_with(".json") || name.ends_with(".jsonl")) {
            out.push(path);
        }
    }
    out
}

fn json_millis(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            parse_rfc3339_millis(text).or_else(|| text.parse::<i64>().ok())
        }
        _ => None,
    }
}

fn parse_antigravity_json_usage_file(path: &Path) -> Result<Vec<UsageRecord>, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_str(&content).map_err(|error| error.to_string())?;
    let session_id = json_nonempty_string(value.get("sessionId"))
        .unwrap_or_else(|| file_stem_session_id(path));
    let file_model = json_nonempty_string(value.get("model"))
        .or_else(|| json_nonempty_string(value.get("modelName")));
    let mut out = Vec::new();
    let Some(messages) = value.get("messages").and_then(Value::as_array) else {
        return Ok(out);
    };
    for message in messages {
        let Some(tokens) = message.get("tokens") else { continue; };
        let input = json_u64(tokens.get("input"));
        let output = json_u64(tokens.get("output"));
        let cached = json_u64(tokens.get("cached"));
        let total = total_or_sum(json_u64(tokens.get("total")), input, output, cached);
        if input == 0 && output == 0 && cached == 0 && total == 0 {
            continue;
        }
        let timestamp_ms = json_millis(message.get("timestamp"))
            .or_else(|| json_millis(message.get("time")))
            .or_else(|| json_millis(value.get("lastUpdated")))
            .or_else(|| json_millis(value.get("startTime")))
            .unwrap_or_else(|| modified_ms(path));
        out.push(UsageRecord {
            session_id: session_id.clone(),
            model: json_nonempty_string(message.get("model"))
                .or_else(|| json_nonempty_string(message.get("modelName")))
                .or_else(|| file_model.clone()),
            timestamp_ms,
            input_tokens: input,
            output_tokens: output,
            cache_tokens: cached,
            cache_read_tokens: 0,
            total_tokens: total,
        });
    }
    Ok(out)
}

fn parse_antigravity_jsonl_usage_file(path: &Path) -> Result<Vec<UsageRecord>, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let reader = BufReader::new(file);
    let fallback_session_id = file_stem_session_id(path);
    let mut session_id = String::new();
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|error| error.to_string())?;
        if value.get("$set").is_some() {
            continue;
        }
        if session_id.is_empty() {
            if let Some(id) = json_nonempty_string(value.get("sessionId")) {
                session_id = id;
            }
        }
        let Some(tokens) = value.get("tokens") else {
            continue;
        };
        let input = json_u64(tokens.get("input"));
        let output = json_u64(tokens.get("output"));
        let cached = json_u64(tokens.get("cached"));
        let total = total_or_sum(json_u64(tokens.get("total")), input, output, cached);
        if input == 0 && output == 0 && cached == 0 && total == 0 {
            continue;
        }
        let timestamp_ms = json_millis(value.get("timestamp"))
            .or_else(|| json_millis(value.get("time")))
            .unwrap_or_else(|| modified_ms(path));
        out.push(UsageRecord {
            session_id: if session_id.is_empty() {
                fallback_session_id.clone()
            } else {
                session_id.clone()
            },
            model: json_nonempty_string(value.get("model"))
                .or_else(|| json_nonempty_string(value.get("modelName"))),
            timestamp_ms,
            input_tokens: input,
            output_tokens: output,
            cache_tokens: cached,
            cache_read_tokens: 0,
            total_tokens: total,
        });
    }
    Ok(out)
}

fn collect_opencode_usage_records(
    window: &UsageWindow,
    _include_model_breakdown: bool,
) -> ToolScan {
    let db_path = dirs::home_dir()
        .map(|home| {
            home.join(".local")
                .join("share")
                .join("opencode")
                .join("opencode.db")
        })
        .unwrap_or_default();
    let storage_roots = candidate_opencode_storage_paths()
        .into_iter()
        .filter_map(|paths| paths.sessions_root.parent().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    collect_opencode_usage_records_from_sources(
        &db_path,
        &storage_roots,
        window.start_ms,
        window.end_ms,
    )
}

#[derive(Debug, Default)]
struct OpenCodeUsageSource {
    available: bool,
    session_ids: HashSet<String>,
    blocking_session_ids: HashSet<String>,
    records: Vec<UsageRecord>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum OpenCodeDbUsageSchema {
    V2,
    V1,
}

impl OpenCodeDbUsageSchema {
    fn session_table(self) -> &'static str {
        match self {
            Self::V2 => "session_v2",
            Self::V1 => "session",
        }
    }

    fn message_table(self) -> &'static str {
        match self {
            Self::V2 => "session_message",
            Self::V1 => "message",
        }
    }
}

pub(in crate::ai_sessions) fn collect_opencode_usage_records_from_sources(
    db_path: &Path,
    storage_roots: &[PathBuf],
    start_ms: i64,
    end_ms: i64,
) -> ToolScan {
    let mut sources = Vec::<OpenCodeUsageSource>::new();
    let mut source_errors = Vec::<String>::new();

    if db_path.is_file() {
        match Connection::open(db_path) {
            Ok(conn) => {
                let mut found_supported_schema = false;
                for schema in [OpenCodeDbUsageSchema::V2, OpenCodeDbUsageSchema::V1] {
                    match opencode_db_schema_presence(&conn, schema) {
                        Ok((false, false)) => {}
                        Ok((true, true)) => {
                            found_supported_schema = true;
                            match read_opencode_usage_source_from_db(
                                &conn, db_path, schema, start_ms, end_ms,
                            ) {
                                Ok(source) => sources.push(source),
                                Err(error) => source_errors.push(error),
                            }
                        }
                        Ok((has_session, has_message)) => {
                            found_supported_schema = true;
                            source_errors.push(format!(
                                "{}: incomplete OpenCode usage schema ({}={}, {}={})",
                                db_path.display(),
                                schema.session_table(),
                                has_session,
                                schema.message_table(),
                                has_message
                            ));
                        }
                        Err(error) => {
                            found_supported_schema = true;
                            source_errors.push(format!("{}: {error}", db_path.display()));
                        }
                    }
                }
                if !found_supported_schema {
                    source_errors.push(format!(
                        "{}: no supported OpenCode usage tables",
                        db_path.display()
                    ));
                }
            }
            Err(error) => source_errors.push(format!("{}: {error}", db_path.display())),
        }
    }

    for storage_root in storage_roots {
        let source = read_opencode_usage_source_from_storage_root(storage_root, start_ms, end_ms);
        if source.available || !source.errors.is_empty() {
            sources.push(source);
        }
    }

    let mut available = false;
    let mut claimed_session_ids = HashSet::<String>::new();
    let mut blocked_session_ids = HashSet::<String>::new();
    let mut records = Vec::<UsageRecord>::new();
    let mut errors = source_errors;
    for source in sources {
        errors.extend(source.errors);
        if !source.available {
            continue;
        }
        available = true;
        let newly_claimed = source
            .session_ids
            .into_iter()
            .filter(|session_id| !blocked_session_ids.contains(session_id))
            .filter(|session_id| claimed_session_ids.insert(session_id.clone()))
            .collect::<HashSet<_>>();
        records.extend(
            source
                .records
                .into_iter()
                .filter(|record| newly_claimed.contains(&record.session_id)),
        );
        blocked_session_ids.extend(source.blocking_session_ids);
    }

    ToolScan {
        source_status: if available {
            "available"
        } else if errors.is_empty() {
            "unavailable"
        } else {
            "error"
        }
        .to_string(),
        scanned_sessions: claimed_session_ids.len() as u64,
        records,
        errors,
    }
}

fn opencode_db_schema_presence(
    conn: &Connection,
    schema: OpenCodeDbUsageSchema,
) -> Result<(bool, bool), rusqlite::Error> {
    Ok((
        sqlite_table_exists(conn, schema.session_table())?,
        sqlite_table_exists(conn, schema.message_table())?,
    ))
}

fn sqlite_table_exists(conn: &Connection, table: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )
}

fn read_trimmed_session_ids(
    conn: &Connection,
    query: &str,
    db_path: &Path,
    errors: &mut Vec<String>,
) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare(query)
        .map_err(|error| format!("{}: {error}", db_path.display()))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("{}: {error}", db_path.display()))?;

    let mut out = HashSet::new();
    for row in rows {
        match row {
            Ok(session_id) if !session_id.trim().is_empty() => {
                out.insert(session_id.trim().to_string());
            }
            Ok(_) => {}
            Err(error) => errors.push(format!("{}: {error}", db_path.display())),
        }
    }
    Ok(out)
}

fn read_opencode_usage_source_from_db(
    conn: &Connection,
    db_path: &Path,
    schema: OpenCodeDbUsageSchema,
    start_ms: i64,
    end_ms: i64,
) -> Result<OpenCodeUsageSource, String> {
    let session_query = match schema {
        OpenCodeDbUsageSchema::V2 => "SELECT id FROM session_v2 WHERE time_archived IS NULL",
        OpenCodeDbUsageSchema::V1 => "SELECT id FROM session WHERE time_archived IS NULL",
    };
    let blocking_session_query = match schema {
        OpenCodeDbUsageSchema::V2 => "SELECT id FROM session_v2",
        OpenCodeDbUsageSchema::V1 => "SELECT id FROM session",
    };
    let message_query = match schema {
        OpenCodeDbUsageSchema::V2 => {
            r#"
            SELECT session_id, time_created, data
            FROM session_message
            WHERE time_created >= ?1
              AND time_created < ?2
            ORDER BY time_created ASC
            "#
        }
        OpenCodeDbUsageSchema::V1 => {
            r#"
            SELECT session_id, time_created, data
            FROM message
            WHERE time_created >= ?1
              AND time_created < ?2
            ORDER BY time_created ASC
            "#
        }
    };

    let mut source = OpenCodeUsageSource {
        available: true,
        ..OpenCodeUsageSource::default()
    };
    source.session_ids =
        read_trimmed_session_ids(conn, session_query, db_path, &mut source.errors)?;
    source.blocking_session_ids =
        read_trimmed_session_ids(conn, blocking_session_query, db_path, &mut source.errors)?;

    let mut message_stmt = conn
        .prepare(message_query)
        .map_err(|error| format!("{}: {error}", db_path.display()))?;
    let rows = message_stmt
        .query_map(params![start_ms, end_ms], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("{}: {error}", db_path.display()))?;
    for row in rows {
        match row {
            Ok((session_id, timestamp_ms, data)) => {
                let session_id = session_id.trim();
                if session_id.is_empty() || !source.session_ids.contains(session_id) {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&data) else {
                    source.errors.push(format!(
                        "{}: invalid OpenCode usage JSON for session {}",
                        db_path.display(),
                        session_id
                    ));
                    continue;
                };
                let tokens = value
                    .get("tokens")
                    .or_else(|| value.get("data").and_then(|data| data.get("tokens")));
                if let Some((input, output, cache, cache_read, total)) =
                    parse_opencode_tokens_value(tokens)
                {
                    source.records.push(UsageRecord {
                        session_id: session_id.to_string(),
                        model: match schema {
                            OpenCodeDbUsageSchema::V2 => opencode_v2_model_name(&value),
                            OpenCodeDbUsageSchema::V1 => opencode_model_name(&value),
                        },
                        timestamp_ms,
                        input_tokens: input,
                        output_tokens: output,
                        cache_tokens: cache,
                        cache_read_tokens: cache_read,
                        total_tokens: total,
                    });
                }
            }
            Err(error) => source
                .errors
                .push(format!("{}: {error}", db_path.display())),
        }
    }
    Ok(source)
}

fn read_opencode_usage_source_from_storage_root(
    storage_root: &Path,
    start_ms: i64,
    end_ms: i64,
) -> OpenCodeUsageSource {
    let sessions_root = storage_root.join("session");
    let messages_root = storage_root.join("message");
    if !sessions_root.is_dir() && !messages_root.is_dir() {
        return OpenCodeUsageSource::default();
    }
    let mut source = OpenCodeUsageSource {
        available: true,
        ..OpenCodeUsageSource::default()
    };
    source.session_ids = opencode_json_session_ids(&sessions_root)
        .into_iter()
        .collect();
    source.blocking_session_ids = source.session_ids.clone();
    for session_id in &source.session_ids {
        let messages_dir = messages_root.join(session_id);
        let (records, errors) = parse_opencode_message_usage_dir(&messages_dir, session_id);
        source.records.extend(
            records
                .into_iter()
                .filter(|record| record.timestamp_ms >= start_ms && record.timestamp_ms < end_ms),
        );
        source.errors.extend(errors);
    }
    source
}

#[cfg(test)]
fn read_opencode_message_tokens_from_db(
    conn: &Connection,
    db_path: &Path,
    window: &UsageWindow,
) -> ToolScan {
    match read_opencode_usage_source_from_db(
        conn,
        db_path,
        OpenCodeDbUsageSchema::V1,
        window.start_ms,
        window.end_ms,
    ) {
        Ok(source) => ToolScan {
            source_status: "available".to_string(),
            scanned_sessions: source.session_ids.len() as u64,
            records: source.records,
            errors: source.errors,
        },
        Err(error) => ToolScan {
            source_status: "error".to_string(),
            scanned_sessions: 0,
            records: Vec::new(),
            errors: vec![error],
        },
    }
}

pub(in crate::ai_sessions) fn parse_opencode_message_usage_dir(
    messages_dir: &Path,
    session_id: &str,
) -> (Vec<UsageRecord>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    if !messages_dir.is_dir() {
        return (out, errors);
    }
    for path in json_files_recursive(messages_dir, "json") {
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let value: Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(error) => {
                errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
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
            model: opencode_model_name(&value),
            timestamp_ms,
            input_tokens: input,
            output_tokens: output,
            cache_tokens: cache,
            cache_read_tokens: cache_read,
            total_tokens: total,
        });
    }
    (out, errors)
}

fn parse_opencode_tokens_value(tokens: Option<&Value>) -> Option<(u64, u64, u64, u64, u64)> {
    let tokens = tokens?;
    let input = json_u64(tokens.get("input")).saturating_add(json_u64(tokens.get("input_tokens")));
    let output =
        json_u64(tokens.get("output")).saturating_add(json_u64(tokens.get("output_tokens")));
    let cache_read = json_u64(tokens.get("cache_read"))
        .saturating_add(json_u64(tokens.get("cached")))
        .saturating_add(json_u64(tokens.get("cache_read_tokens")))
        .saturating_add(json_u64(
            tokens.get("cache").and_then(|cache| cache.get("read")),
        ));
    let cache_write = json_u64(tokens.get("cache_write"))
        .saturating_add(json_u64(tokens.get("cache_write_tokens")))
        .saturating_add(json_u64(tokens.get("cache")))
        .saturating_add(json_u64(
            tokens.get("cache").and_then(|cache| cache.get("write")),
        ));
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
        Arc::new(ToolScan {
            source_status: "available".to_string(),
            scanned_sessions: 1,
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
