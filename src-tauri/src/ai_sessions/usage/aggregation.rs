use super::model::{ToolScan, UsageRecord, UsageWindow};
use super::types::{
    SessionUsageDaily, SessionUsageDayBreakdown, SessionUsageDayStats, SessionUsageModelStats,
    SessionUsagePeakDay, SessionUsageSummary, SessionUsageToolStats,
};
use chrono::{DateTime, Duration, Local};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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

pub(super) fn aggregate_day_stats_from_tool_stats(
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

pub(super) fn aggregate_tool_usage(
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

    scanned_calls = scanned_calls.saturating_add(scan.transcript_calls);

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
        source_status: if scan.source_status == "available" && scan.scanned_sessions == 0 {
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
