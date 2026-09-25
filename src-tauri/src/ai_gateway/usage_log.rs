//! Usage metering and request logs for the AI gateway.
//!
//! Records live in a dedicated SQLite file (never the encrypted
//! `ai_gateway.json`) and deliberately contain no request/response bodies,
//! headers or credentials. The store base path is injectable so tests point at
//! a temp directory and never touch real application data.

use super::{
    default_true, ModelPrice, DEFAULT_USAGE_RETENTION_DAYS, MAX_USAGE_RETENTION_DAYS,
    MIN_USAGE_RETENTION_DAYS,
};
use chrono::{Datelike, Timelike};
use rusqlite::{params_from_iter, Connection, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// File name of the dedicated usage-log database under `get_app_dir()`.
pub(in crate::ai_gateway) const USAGE_DB_FILE: &str = "ai_gateway_usage.db";
/// Fixed page size for the ungrouped request-log list.
pub const USAGE_LOG_PAGE_SIZE: u32 = 50;
/// Milliseconds in one UTC day.
const DAY_MS: i64 = 86_400_000;
/// Milliseconds of the fixed UTC+8 offset (the product's reporting timezone).
const UTC8_OFFSET_MS: i64 = 8 * 3_600_000;
/// Maximum stored length of one sanitized upstream error message (REQ-004).
const MAX_ERROR_MESSAGE_CHARS: usize = 4096;
/// Marker appended when an error message had to be truncated.
const ERROR_TRUNCATION_MARKER: char = '…';
/// Credential-shaped runs shorter than this are left readable.
const MIN_MASKED_CREDENTIAL_CHARS: usize = 8;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS usage_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ms INTEGER NOT NULL,
    local_model TEXT NOT NULL,
    upstream_model TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    provider_name TEXT NOT NULL,
    result TEXT NOT NULL,
    status INTEGER NOT NULL,
    input_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,
    amount REAL,
    duration_ms INTEGER NOT NULL,
    error_message TEXT,
    terminal INTEGER NOT NULL DEFAULT 1,
    reasoning_effort TEXT
);
CREATE INDEX IF NOT EXISTS idx_usage_logs_timestamp ON usage_logs(timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_usage_logs_local_model ON usage_logs(local_model);
";

pub(in crate::ai_gateway) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn utc8_offset() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt((UTC8_OFFSET_MS / 1000) as i32)
        .expect("UTC+8 is a valid fixed offset")
}

// ---------------------------------------------------------------------------
// Pure computation and parsing
// ---------------------------------------------------------------------------

/// Token counts captured from one upstream response.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageTokens {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

impl UsageTokens {
    pub fn total(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
            .saturating_add(self.output_tokens)
    }
}

/// Terminal outcome recorded for one forwarded request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageResult {
    Success,
    Failure,
}

impl UsageResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }

    pub(in crate::ai_gateway) fn parse(value: &str) -> Option<Self> {
        match value {
            "success" => Some(Self::Success),
            "failure" => Some(Self::Failure),
            _ => None,
        }
    }
}

/// One persisted request-log row. Contains no bodies, headers or credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageLogRecord {
    pub timestamp_ms: i64,
    pub local_model: String,
    pub upstream_model: String,
    pub provider_id: String,
    pub provider_name: String,
    pub result: UsageResult,
    pub status: u16,
    pub input_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    /// `None` when the upstream model had no matching price row at record time.
    pub amount: Option<f64>,
    pub duration_ms: u64,
    /// Sanitized, bounded and redacted upstream error text; `None` for a
    /// success or a failure without readable error information (REQ-003/REQ-004).
    #[serde(default)]
    pub error_message: Option<String>,
    /// Whether this row is the request's single terminal row; non-terminal rows
    /// are the completed attempts that a later row superseded (REQ-001).
    #[serde(default = "default_true")]
    pub terminal: bool,
    /// Reasoning effort level (e.g. "low", "medium", "high") requested by client;
    /// `None` when not specified.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

/// Validate a user-provided retention value: only 1-365 days are accepted.
pub fn validate_retention_days(days: i64) -> Result<u32, String> {
    if (MIN_USAGE_RETENTION_DAYS as i64..=MAX_USAGE_RETENTION_DAYS as i64).contains(&days) {
        Ok(days as u32)
    } else {
        Err(format!(
            "usage retention days must be between {MIN_USAGE_RETENTION_DAYS} and {MAX_USAGE_RETENTION_DAYS}, got {days}"
        ))
    }
}

/// Clamp a persisted retention value, falling back to the default for
/// out-of-range values written by older or corrupted configs.
pub(in crate::ai_gateway) fn normalize_retention_days(days: u32) -> u32 {
    if (MIN_USAGE_RETENTION_DAYS..=MAX_USAGE_RETENTION_DAYS).contains(&days) {
        days
    } else {
        DEFAULT_USAGE_RETENTION_DAYS
    }
}

/// Cost in US dollars for the four token tiers, prices are per million tokens.
pub fn compute_cost(price: &ModelPrice, tokens: &UsageTokens) -> f64 {
    (price.input * tokens.input_tokens as f64
        + price.cache_read * tokens.cache_read_tokens as f64
        + price.cache_write * tokens.cache_write_tokens as f64
        + price.output * tokens.output_tokens as f64)
        / 1_000_000.0
}

/// Check whether a timestamp_ms falls into the off-peak time window [start_time, end_time) in UTC+8.
///
/// `start_time` and `end_time` are strings in "HH:mm" 24-hour format (e.g. "00:30", "08:30").
/// - When start_time < end_time: [start_time, end_time) within the same UTC+8 day.
/// - When start_time > end_time: overnight window, [start_time, 24:00) or [00:00, end_time).
/// - When start_time == end_time: zero duration window (evaluates to false).
///
/// Test-only weekday-less compatibility entry point; production pricing goes
/// through [`is_off_peak_with_days`].
#[cfg(test)]
pub fn is_off_peak(timestamp_ms: i64, start_time: &str, end_time: &str) -> bool {
    is_off_peak_with_days(timestamp_ms, start_time, end_time, None)
}

/// Days-aware variant of [`is_off_peak`]. `days` is the optional UTC+8 weekday
/// set (`0` Sunday .. `6` Saturday) the window applies to; `None`, an empty set
/// or a set with no value in `0..=6` means every day.
fn is_off_peak_with_days(
    timestamp_ms: i64,
    start_time: &str,
    end_time: &str,
    days: Option<&[u8]>,
) -> bool {
    let dt = match chrono::DateTime::from_timestamp_millis(timestamp_ms) {
        Some(dt) => dt.with_timezone(&utc8_offset()),
        None => return false,
    };
    if !matches_off_peak_days(days, dt.weekday().num_days_from_sunday()) {
        return false;
    }
    let current_minute = dt.hour() * 60 + dt.minute();

    let parse_minute = |s: &str| -> Option<u32> {
        let mut parts = s.trim().split(':');
        let h: u32 = parts.next()?.parse().ok()?;
        let m: u32 = parts.next()?.parse().ok()?;
        if h < 24 && m < 60 {
            Some(h * 60 + m)
        } else {
            None
        }
    };

    let (start_min, end_min) = match (parse_minute(start_time), parse_minute(end_time)) {
        (Some(s), Some(e)) => (s, e),
        _ => return false,
    };

    if start_min == end_min {
        false
    } else if start_min < end_min {
        current_minute >= start_min && current_minute < end_min
    } else {
        current_minute >= start_min || current_minute < end_min
    }
}

/// Whether the request's UTC+8 weekday is included by an optional weekday set.
/// Out-of-range values are ignored; an empty/all-invalid set means every day.
fn matches_off_peak_days(days: Option<&[u8]>, weekday: u32) -> bool {
    let Some(days) = days else {
        return true;
    };
    let mut has_valid = false;
    for day in days {
        if *day > 6 {
            continue;
        }
        has_valid = true;
        if *day as u32 == weekday {
            return true;
        }
    }
    !has_valid
}

/// Cost in US dollars for the four token tiers.
///
/// If off-peak pricing configurations are present and `timestamp_ms` falls within an off-peak
/// window in UTC+8, the first matching off-peak pricing tier is used; otherwise, the standard pricing tier is used.
pub fn compute_cost_at_time(
    price: &ModelPrice,
    tokens: &UsageTokens,
    timestamp_ms: i64,
) -> f64 {
    for window in &price.off_peaks {
        if is_off_peak_with_days(
            timestamp_ms,
            &window.start_time,
            &window.end_time,
            window.days.as_deref(),
        ) {
            return (window.input * tokens.input_tokens as f64
                + window.cache_read * tokens.cache_read_tokens as f64
                + window.cache_write * tokens.cache_write_tokens as f64
                + window.output * tokens.output_tokens as f64)
                / 1_000_000.0;
        }
    }

    compute_cost(price, tokens)
}

/// Match a price row scoped to the forwarded provider and upstream model.
///
/// Matching is exact and case-sensitive; a row that belongs to another provider
/// or carries no provider id never prices a request.
pub fn match_price_for_provider<'a>(
    provider_id: &str,
    upstream_model: &str,
    prices: &'a [ModelPrice],
) -> Option<&'a ModelPrice> {
    prices.iter().find(|price| {
        price.provider_id.as_deref() == Some(provider_id)
            && price.upstream_model == upstream_model
    })
}

fn token_number(value: Option<&Value>) -> u64 {
    match value {
        Some(Value::Number(number)) => number
            .as_u64()
            .or_else(|| number.as_i64().map(|value| value.max(0) as u64))
            .or_else(|| number.as_f64().map(|value| value.max(0.0) as u64))
            .unwrap_or(0),
        Some(Value::String(text)) => text.trim().parse::<u64>().unwrap_or(0),
        _ => 0,
    }
}

/// A nested cache field when its key is present with a non-null value.
/// A present field that parses to zero stays `Some(0)` so shape detection can
/// distinguish "present" from "absent"; `null` counts as absent.
fn nested_token(usage: &Value, object: &str, key: &str) -> Option<u64> {
    usage
        .get(object)
        .and_then(|details| details.get(key))
        .filter(|value| !value.is_null())
        .map(|value| token_number(Some(value)))
}

fn top_field_present(usage: &Value, key: &str) -> bool {
    usage.get(key).is_some_and(|value| !value.is_null())
}

/// Canonical normalization result: mutually exclusive tiers plus whether the
/// source shape was a valid decomposition.
///
/// Invalid shapes (nested cache read conflicting with top-level
/// `cache_read_input_tokens`, or nested cache components exceeding the
/// reported inclusive input) fall back to the reported input with no cache
/// tiers so billing stays conservative (no double counting, no negative)
/// while the store layer keeps the row cache-statistics-ineligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_gateway) struct CanonicalUsage {
    pub tokens: UsageTokens,
    pub valid: bool,
}

/// Persisted usage classification of one new request-log row (REQ-004).
///
/// `present` means the upstream response carried a `usage` object at all;
/// `valid` means its decomposition into the four exclusive tiers succeeded.
/// Rows written by [`UsageLogStore::append`] / [`UsageLogStore::append_batch`]
/// always carry the canonical contract, while the accounting-aware insert path
/// persists the parser's real classification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ai_gateway) struct UsageAccounting {
    pub present: bool,
    pub valid: bool,
}

impl UsageAccounting {
    /// The classification written by the request-log contract path.
    #[cfg(test)]
    pub(in crate::ai_gateway) const CANONICAL: Self = Self {
        present: true,
        valid: true,
    };
}

/// Map an upstream `usage` object to the four exclusive token tiers (REQ-001).
///
/// Missing fields become 0; the caller decides whether usage was present at all.
///
/// Shape table:
/// - OpenAI Chat/Responses report the inclusive input total (`input_tokens`
///   wins over `prompt_tokens`) with the cached subset nested under
///   `prompt_tokens_details` / `input_tokens_details`. The stored ordinary
///   tier is the checked remainder `reported - read - write`; each tier is
///   billed exactly once by `total()` and `compute_cost`.
/// - Anthropic-style split has no nested cache details: the top-level input
///   is already ordinary and the flat `cache_read_input_tokens` /
///   `cache_creation_input_tokens` tiers are independent.
/// - Mixed compatible write (nested cache read present, no nested cache
///   write, top-level creation present) follows OpenAI inclusive semantics
///   with the top-level creation value as the deducted cache-write fallback.
/// - Nested cache write wins over the top-level creation fallback.
/// - `output_tokens` wins over `completion_tokens`.
pub(in crate::ai_gateway) fn canonical_usage_from_value(usage: &Value) -> CanonicalUsage {
    let reported = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .map(|value| token_number(Some(value)))
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .map(|value| token_number(Some(value)))
        .unwrap_or(0);
    let nested_read = nested_token(usage, "prompt_tokens_details", "cached_tokens")
        .or_else(|| nested_token(usage, "input_tokens_details", "cached_tokens"));
    let nested_write = nested_token(usage, "prompt_tokens_details", "cache_write_tokens")
        .or_else(|| nested_token(usage, "input_tokens_details", "cache_write_tokens"));
    let has_nested = nested_read.is_some() || nested_write.is_some();
    let has_top_read = top_field_present(usage, "cache_read_input_tokens");
    let top_read = token_number(usage.get("cache_read_input_tokens"));
    let top_creation = token_number(usage.get("cache_creation_input_tokens"));

    // Conflicting shape: nested cache read coexists with the top-level
    // cache-read tier. Never merge; fall back with no cache tiers.
    if nested_read.is_some() && has_top_read {
        return CanonicalUsage {
            tokens: UsageTokens {
                input_tokens: reported,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens,
            },
            valid: false,
        };
    }

    if has_nested {
        let cache_read_tokens = nested_read.unwrap_or(0);
        // Nested cache write wins; otherwise the top-level creation value is
        // the compatible fallback deducted from the inclusive total.
        let cache_write_tokens = nested_write.unwrap_or(top_creation);
        match reported
            .checked_sub(cache_read_tokens)
            .and_then(|rest| rest.checked_sub(cache_write_tokens))
        {
            Some(input_tokens) => CanonicalUsage {
                tokens: UsageTokens {
                    input_tokens,
                    cache_read_tokens,
                    cache_write_tokens,
                    output_tokens,
                },
                valid: true,
            },
            // Illegal decomposition: nested components exceed reported input.
            // Never produce negative ordinary input via saturating subtraction.
            None => CanonicalUsage {
                tokens: UsageTokens {
                    input_tokens: reported,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    output_tokens,
                },
                valid: false,
            },
        }
    } else {
        // Anthropic-style split: the reported input is already ordinary.
        CanonicalUsage {
            tokens: UsageTokens {
                input_tokens: reported,
                cache_read_tokens: top_read,
                cache_write_tokens: top_creation,
                output_tokens,
            },
            valid: true,
        }
    }
}

#[cfg(test)]
pub(in crate::ai_gateway) fn usage_tokens_from_value(usage: &Value) -> UsageTokens {
    canonical_usage_from_value(usage).tokens
}

/// Parse `usage` out of a complete (buffered) upstream JSON response, keeping
/// the canonical validity classification so the store can persist it.
pub(in crate::ai_gateway) fn parse_usage_from_response(body: &[u8]) -> Option<CanonicalUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let usage = value.get("usage")?;
    if usage.is_object() {
        Some(canonical_usage_from_value(usage))
    } else {
        None
    }
}

/// Extract the upstream's own error information from a failure body (REQ-003).
///
/// A standard JSON envelope yields its non-empty `error.message`; any other body
/// yields a readable summary (lossy decode, markup removed, consecutive
/// whitespace collapsed, trimmed). Bodies without readable text yield `None`.
pub(in crate::ai_gateway) fn extract_upstream_error_text(body: &[u8]) -> Option<String> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        let message = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|message| !message.is_empty());
        if let Some(message) = message {
            return Some(message.to_string());
        }
    }
    let decoded = String::from_utf8_lossy(body);
    let summary = strip_html_markup(&decoded)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if summary.is_empty() {
        None
    } else {
        Some(summary)
    }
}

/// Drop every `<...>` markup span so only the readable text remains. An
/// unmatched `<` is kept literally instead of swallowing the rest of the body.
fn strip_html_markup(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        result.push_str(&rest[..start]);
        match rest[start..].find('>') {
            Some(end) => rest = &rest[start + end + 1..],
            None => {
                result.push_str(&rest[start..]);
                return result;
            }
        }
    }
    result.push_str(rest);
    result
}

/// Redact, bound and normalize one extracted upstream error message (REQ-004).
///
/// The provider key is replaced verbatim with `[redacted]`, `Bearer` and `sk-`
/// credential shapes are masked, and text longer than
/// [`MAX_ERROR_MESSAGE_CHARS`] is truncated on a Unicode scalar boundary with a
/// trailing `…`. Text that stays empty is stored as no message (`None`).
pub(in crate::ai_gateway) fn sanitize_error_text(text: &str, api_key: &str) -> Option<String> {
    let redacted = if api_key.is_empty() {
        text.to_string()
    } else {
        text.replace(api_key, "[redacted]")
    };
    let masked = mask_credential_shapes(&redacted);
    let trimmed = masked.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= MAX_ERROR_MESSAGE_CHARS {
        return Some(trimmed.to_string());
    }
    let mut bounded: String = trimmed.chars().take(MAX_ERROR_MESSAGE_CHARS - 1).collect();
    bounded.push(ERROR_TRUNCATION_MARKER);
    Some(bounded)
}

/// Mask `sk-` prefixed tokens and `Bearer <token>` credentials so no complete
/// token can reach the log. Only ASCII credential characters join a token, so
/// multi-byte text can never be split by the masking.
fn mask_credential_shapes(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut masked = String::with_capacity(text.len());
    let mut copied = 0;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let mut replacement_start = cursor;
        let mut replacement_end = cursor;
        if bytes[cursor..].starts_with(b"sk-") {
            replacement_end = credential_run_end(bytes, cursor + 3).unwrap_or(cursor);
        } else if starts_with_ignore_ascii_case(bytes, cursor, b"bearer") {
            let mut token_start = cursor + b"bearer".len();
            while token_start < bytes.len() && bytes[token_start].is_ascii_whitespace() {
                token_start += 1;
            }
            if token_start > cursor + b"bearer".len() {
                if let Some(end) = credential_run_end(bytes, token_start) {
                    replacement_start = token_start;
                    replacement_end = end;
                }
            }
        }
        if replacement_end > replacement_start {
            masked.push_str(&text[copied..replacement_start]);
            masked.push_str("[redacted]");
            copied = replacement_end;
            cursor = replacement_end;
        } else {
            cursor += 1;
        }
    }
    masked.push_str(&text[copied..]);
    masked
}

/// End of the credential-character run starting at `start`, when it is long
/// enough to be a token rather than an ordinary word.
fn credential_run_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut end = start;
    while end < bytes.len() && is_credential_byte(bytes[end]) {
        end += 1;
    }
    if end - start >= MIN_MASKED_CREDENTIAL_CHARS {
        Some(end)
    } else {
        None
    }
}

fn is_credential_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

fn starts_with_ignore_ascii_case(bytes: &[u8], start: usize, word: &[u8]) -> bool {
    bytes.len() - start >= word.len()
        && bytes[start..start + word.len()]
            .iter()
            .zip(word)
            .all(|(byte, expected)| byte.eq_ignore_ascii_case(expected))
}

/// Read-only SSE accumulator that extracts the last valid `usage` object from a
/// forwarded stream without touching the bytes written to the caller.
///
/// Both locations are recognized: Chat Completions carries `usage` at the event
/// top level, while Responses carries it nested under the completed response
/// (`response.completed` -> `response.usage`). Only a JSON object replaces the
/// last accumulated object, so a later `null`, string or number payload can
/// never overwrite it.
#[derive(Default)]
pub(in crate::ai_gateway) struct SseUsageAccumulator {
    buffer: String,
    usage: Option<CanonicalUsage>,
}

impl SseUsageAccumulator {
    pub(in crate::ai_gateway) fn feed(&mut self, chunk: &[u8]) {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));
        while let Some(position) = self.buffer.find('\n') {
            let line: String = self.buffer.drain(..=position).collect();
            self.consume_line(line.trim_end_matches(['\r', '\n']));
        }
        // Guard against a pathological stream that never emits a newline: the
        // usage chunk arrives at the end, so only a bounded tail can matter.
        if self.buffer.len() > 256 * 1024 {
            self.buffer.clear();
        }
    }

    fn consume_line(&mut self, line: &str) {
        let Some(data) = line.strip_prefix("data:") else {
            return;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        // Chat Completions reports usage at the event top level; Responses
        // reports it nested under the response. A non-object payload (`null`,
        // string, number) never replaces the last valid object.
        let usage = value
            .get("usage")
            .or_else(|| value.get("response").and_then(|response| response.get("usage")));
        if let Some(usage) = usage {
            if usage.is_object() {
                self.usage = Some(canonical_usage_from_value(usage));
            }
        }
    }

    pub(in crate::ai_gateway) fn usage(&self) -> Option<UsageTokens> {
        self.usage.map(|canonical| canonical.tokens)
    }

    /// The last valid usage object with its canonical validity classification.
    pub(in crate::ai_gateway) fn canonical_usage(&self) -> Option<CanonicalUsage> {
        self.usage
    }
}

// ---------------------------------------------------------------------------
// UTC+8 range resolution
// ---------------------------------------------------------------------------

/// Half-open millisecond range `[start, end)`; `None` means unbounded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ai_gateway) struct TimeRange {
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

/// UTC+8 midnight (epoch milliseconds) of the natural day containing `now_ms`.
fn utc8_day_start_ms(now_ms: i64) -> i64 {
    (now_ms + UTC8_OFFSET_MS).div_euclid(DAY_MS) * DAY_MS - UTC8_OFFSET_MS
}

/// Resolve a quick-range `days` selector to UTC+8 day boundaries.
///
/// `None` means "all"; `Some(1)` means today; `Some(n)` covers today plus the
/// previous `n - 1` natural days, with the day boundary fixed at UTC+8 midnight.
pub(in crate::ai_gateway) fn resolve_range(days: Option<i64>, now_ms: i64) -> TimeRange {
    let Some(days) = days.filter(|days| *days >= 1) else {
        return TimeRange::default();
    };
    let today_start_ms = utc8_day_start_ms(now_ms);
    TimeRange {
        start_ms: Some(today_start_ms - (days - 1) * DAY_MS),
        end_ms: Some(today_start_ms + DAY_MS),
    }
}

/// A resolved range selector together with the bucket granularity its window
/// implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_gateway) struct ResolvedRange {
    pub range: TimeRange,
    /// `true` when the window spans exactly one UTC+8 natural day, which is the
    /// condition for hourly buckets.
    pub hourly: bool,
}

/// Resolve a range selector to a UTC+8 window and its bucket granularity.
///
/// The selector is trimmed first:
/// - `None`, an empty string or `"all"` is the unbounded all-time window;
/// - `"today"` / `"yesterday"` are one UTC+8 natural day (hourly buckets);
/// - `"7d"` / `"15d"` / `"30d"` reuse [`resolve_range`], so their windows stay
///   byte-identical to the previous `days = 7/15/30` behavior.
///
/// Any other selector is rejected with the supported vocabulary; it never
/// silently falls back to all-time or today.
pub(in crate::ai_gateway) fn resolve_range_selector(
    selector: Option<&str>,
    now_ms: i64,
) -> Result<ResolvedRange, String> {
    let selector = selector.map(str::trim).unwrap_or("");
    let range = match selector {
        "" | "all" => TimeRange::default(),
        "today" => resolve_range(Some(1), now_ms),
        "yesterday" => {
            let today_start_ms = utc8_day_start_ms(now_ms);
            TimeRange {
                start_ms: Some(today_start_ms - DAY_MS),
                end_ms: Some(today_start_ms),
            }
        }
        "7d" => resolve_range(Some(7), now_ms),
        "15d" => resolve_range(Some(15), now_ms),
        "30d" => resolve_range(Some(30), now_ms),
        other => {
            return Err(format!(
                "unsupported range '{other}': expected one of 'today', 'yesterday', '7d', '15d', '30d' or 'all'"
            ))
        }
    };
    let hourly = matches!(
        (range.start_ms, range.end_ms),
        (Some(start), Some(end)) if end - start == DAY_MS
    );
    Ok(ResolvedRange { range, hourly })
}

fn day_index_label(day_index: i64) -> String {
    let utc_ms = day_index * DAY_MS - UTC8_OFFSET_MS;
    chrono::DateTime::from_timestamp_millis(utc_ms)
        .map(|instant| {
            instant
                .with_timezone(&utc8_offset())
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// Aggregated metrics shared by the stats cards, buckets and grouping rows.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct UsageMetrics {
    pub request_count: u32,
    pub input_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub amount: f64,
    pub unpriced_count: u32,
    /// Sum of `cache_read_tokens` over cache-eligible rows (REQ-002).
    pub cache_hit_tokens: u64,
    /// Sum of `ordinary + cache_read + cache_write` over cache-eligible rows.
    pub cache_eligible_tokens: u64,
    /// Token-weighted `cache_hit_tokens / cache_eligible_tokens` as a 0..=100
    /// percentage; `None` when no eligible positive denominator exists.
    pub cache_hit_rate_percent: Option<f64>,
    /// Number of cache-eligible rows in this aggregation.
    pub cache_rate_eligible_count: u32,
    /// Number of in-range successful terminal rows, eligible or not.
    pub successful_request_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageBucketRow {
    pub label: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageProviderRow {
    pub provider_id: String,
    pub provider_name: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageModelRow {
    pub local_model: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
    pub providers: Vec<UsageProviderRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnpricedUsageItem {
    pub provider_id: String,
    pub provider_name: String,
    pub local_model: String,
    pub upstream_model: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageStats {
    /// `"hour"` for a single-day range, `"day"` for multi-day or all.
    pub granularity: String,
    #[serde(flatten)]
    pub totals: UsageMetrics,
    pub buckets: Vec<UsageBucketRow>,
    pub models: Vec<UsageModelRow>,
    #[serde(default)]
    pub unpriced_items: Vec<UnpricedUsageItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageLogGroupRow {
    pub group: String,
    pub request_count: u32,
    pub error_count: u32,
    pub last_request_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageLogsPage {
    pub page: u32,
    pub page_size: u32,
    pub total: u32,
    pub total_pages: u32,
    pub group_by: Option<String>,
    #[serde(default)]
    pub records: Vec<UsageLogRecord>,
    #[serde(default)]
    pub groups: Vec<UsageLogGroupRow>,
    /// Distinct, non-empty in-range `local_model` values, independent of the
    /// current page and of the active model filter. Empty for grouped responses.
    #[serde(default)]
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ai_gateway) struct LogFilter {
    pub status: Option<UsageResult>,
    pub model: Option<String>,
}

/// SQL predicate selecting a cache-eligible row (REQ-002/REQ-004): an in-range
/// successful terminal row with a present, valid canonical decomposition and a
/// positive cache denominator. Totals, buckets, models and providers all reuse
/// this one predicate so no level can drift from another.
const ELIGIBLE_ROW_SQL: &str = "result = 'success' \
    AND usage_semantics = 'canonical_v1' \
    AND usage_present = 1 \
    AND cache_accounting_valid = 1 \
    AND (input_tokens + cache_read_tokens + cache_write_tokens) > 0";

/// The shared metric projection, in [`metrics_from_row`] order. The final four
/// aggregates are the additive cache fields; the percentage is derived from the
/// two sums in Rust so every level uses the same token weighting.
fn metric_columns() -> String {
    format!(
        "COUNT(*), \
         COALESCE(SUM(input_tokens), 0), \
         COALESCE(SUM(cache_read_tokens), 0), \
         COALESCE(SUM(cache_write_tokens), 0), \
         COALESCE(SUM(output_tokens), 0), \
         COALESCE(SUM(total_tokens), 0), \
         COALESCE(SUM(amount), 0.0), \
         COALESCE(SUM(CASE WHEN amount IS NULL AND upstream_model <> '' AND (result = 'success' OR total_tokens > 0) THEN 1 ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN result = 'success' THEN 1 ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN {ELIGIBLE_ROW_SQL} THEN cache_read_tokens ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN {ELIGIBLE_ROW_SQL} THEN input_tokens + cache_read_tokens + cache_write_tokens ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN {ELIGIBLE_ROW_SQL} THEN 1 ELSE 0 END), 0)"
    )
}

/// Column list and placeholders shared by every usage-log insert path. New rows
/// always carry the canonical semantics marker; presence and accounting
/// validity vary per row.
const INSERT_SQL: &str = "
INSERT INTO usage_logs (
    timestamp_ms, local_model, upstream_model, provider_id, provider_name,
    result, status, input_tokens, cache_read_tokens, cache_write_tokens,
    output_tokens, total_tokens, amount, duration_ms, error_message, terminal,
    reasoning_effort, usage_semantics, usage_present, cache_accounting_valid
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
";

/// Semantics marker persisted for every newly written row (REQ-004).
const CANONICAL_USAGE_SEMANTICS: &str = "canonical_v1";

/// Column list of every record-producing `SELECT`, in [`record_from_row`] order.
const RECORD_COLUMNS: &str = "timestamp_ms, local_model, upstream_model, provider_id, provider_name, result, status, input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, total_tokens, amount, duration_ms, error_message, terminal, reasoning_effort";

fn metrics_from_row(row: &Row<'_>, offset: usize) -> rusqlite::Result<UsageMetrics> {
    let cache_hit_tokens = row.get::<_, i64>(offset + 9)? as u64;
    let cache_eligible_tokens = row.get::<_, i64>(offset + 10)? as u64;
    let cache_hit_rate_percent = if cache_eligible_tokens > 0 {
        Some(cache_hit_tokens as f64 * 100.0 / cache_eligible_tokens as f64)
    } else {
        None
    };
    Ok(UsageMetrics {
        request_count: row.get::<_, i64>(offset)? as u32,
        input_tokens: row.get::<_, i64>(offset + 1)? as u64,
        cache_read_tokens: row.get::<_, i64>(offset + 2)? as u64,
        cache_write_tokens: row.get::<_, i64>(offset + 3)? as u64,
        output_tokens: row.get::<_, i64>(offset + 4)? as u64,
        total_tokens: row.get::<_, i64>(offset + 5)? as u64,
        amount: row.get::<_, f64>(offset + 6)?,
        unpriced_count: row.get::<_, i64>(offset + 7)? as u32,
        successful_request_count: row.get::<_, i64>(offset + 8)? as u32,
        cache_hit_tokens,
        cache_eligible_tokens,
        cache_hit_rate_percent,
        cache_rate_eligible_count: row.get::<_, i64>(offset + 11)? as u32,
    })
}

fn record_from_row(row: &Row<'_>) -> rusqlite::Result<UsageLogRecord> {
    let result: String = row.get(5)?;
    Ok(UsageLogRecord {
        timestamp_ms: row.get(0)?,
        local_model: row.get(1)?,
        upstream_model: row.get(2)?,
        provider_id: row.get(3)?,
        provider_name: row.get(4)?,
        result: UsageResult::parse(&result).unwrap_or(UsageResult::Failure),
        status: row.get::<_, i64>(6)? as u16,
        input_tokens: row.get::<_, i64>(7)? as u64,
        cache_read_tokens: row.get::<_, i64>(8)? as u64,
        cache_write_tokens: row.get::<_, i64>(9)? as u64,
        output_tokens: row.get::<_, i64>(10)? as u64,
        total_tokens: row.get::<_, i64>(11)? as u64,
        amount: row.get::<_, Option<f64>>(12)?,
        duration_ms: row.get::<_, i64>(13)? as u64,
        error_message: row.get::<_, Option<String>>(14)?,
        terminal: row.get::<_, i64>(15)? != 0,
        reasoning_effort: row.get::<_, Option<String>>(16)?,
    })
}

/// Build the `WHERE` clause shared by every user-visible log query.
///
/// `terminal_only` adds the statistics/grouping filter; the ungrouped list keeps
/// every row that survives the filter.
fn bind(
    range: &TimeRange,
    filter: &LogFilter,
    terminal_only: bool,
) -> (String, Vec<rusqlite::types::Value>) {
    let mut clauses: Vec<&str> = Vec::new();
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(start) = range.start_ms {
        clauses.push("timestamp_ms >= ?");
        params.push(start.into());
    }
    if let Some(end) = range.end_ms {
        clauses.push("timestamp_ms < ?");
        params.push(end.into());
    }
    if terminal_only {
        clauses.push("terminal = 1");
    }
    if let Some(status) = filter.status {
        clauses.push("result = ?");
        params.push(status.as_str().to_string().into());
    }
    if let Some(model) = filter.model.as_deref().filter(|model| !model.is_empty()) {
        clauses.push("local_model = ?");
        params.push(model.to_string().into());
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    (where_sql, params)
}

/// Run one additive `ALTER TABLE ... ADD COLUMN`, treating a concurrent
/// `duplicate column name` failure as success: two openers can both observe the
/// column as missing and race the same ALTER, and the first to commit wins. Every
/// other error is returned unchanged.
fn add_usage_log_column(connection: &Connection, sql: &str) -> Result<(), String> {
    match connection.execute(sql, []) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().to_ascii_lowercase().contains("duplicate column name") => {
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

/// Add the attempt/terminal columns to a database written before this change
/// (REQ-005). The `PRAGMA table_info` inspection makes the migration idempotent:
/// present columns are left alone, existing rows default to terminal with no
/// message. A concurrent opener may still win the race between the inspection
/// and an `ALTER TABLE`, so a `duplicate column name` failure from any additive
/// statement is accepted as success; any other failure is returned so the caller
/// swallows it like any other log-write error and a later open retries.
fn migrate_usage_logs(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(usage_logs)")
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    let has_column = |name: &str| columns.iter().any(|column| column == name);
    if !has_column("error_message") {
        add_usage_log_column(connection, "ALTER TABLE usage_logs ADD COLUMN error_message TEXT")?;
    }
    if !has_column("terminal") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN terminal INTEGER NOT NULL DEFAULT 1",
        )?;
    }
    if !has_column("reasoning_effort") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN reasoning_effort TEXT",
        )?;
    }
    // Additive usage-accounting columns (REQ-004). Pre-existing rows default to
    // legacy semantics with no present/valid usage and stay excluded from the
    // new cache numerator/denominator; original tokens, totals, amounts and log
    // fields are never rewritten.
    if !has_column("usage_semantics") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN usage_semantics TEXT NOT NULL DEFAULT 'legacy'",
        )?;
    }
    if !has_column("usage_present") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN usage_present INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    if !has_column("cache_accounting_valid") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN cache_accounting_valid INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    Ok(())
}

/// One persistable row: its request-log record plus the real usage
/// classification captured while parsing the upstream response (REQ-004).
#[derive(Debug, Clone, PartialEq)]
pub(in crate::ai_gateway) struct UsageLogEntry {
    pub record: UsageLogRecord,
    pub accounting: UsageAccounting,
}

/// Bind one record and its classification into a prepared [`INSERT_SQL`].
fn insert_record(
    statement: &mut rusqlite::Statement<'_>,
    record: &UsageLogRecord,
    accounting: UsageAccounting,
) -> Result<(), String> {
    statement
        .execute(rusqlite::params![
            record.timestamp_ms,
            record.local_model,
            record.upstream_model,
            record.provider_id,
            record.provider_name,
            record.result.as_str(),
            record.status as i64,
            record.input_tokens as i64,
            record.cache_read_tokens as i64,
            record.cache_write_tokens as i64,
            record.output_tokens as i64,
            record.total_tokens as i64,
            record.amount,
            record.duration_ms as i64,
            record.error_message,
            record.terminal,
            record.reasoning_effort,
            CANONICAL_USAGE_SEMANTICS,
            accounting.present as i64,
            accounting.valid as i64,
        ])
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Permanently delete rows older than the retention window (REQ-009).
fn apply_retention(connection: &Connection, retention_days: u32) {
    let cutoff = now_millis() - normalize_retention_days(retention_days) as i64 * DAY_MS;
    let _ = connection.execute("DELETE FROM usage_logs WHERE timestamp_ms < ?", [cutoff]);
}

/// SQLite-backed usage-log storage bound to one explicit database path.
#[derive(Debug, Clone)]
pub(in crate::ai_gateway) struct UsageLogStore {
    path: PathBuf,
}

impl UsageLogStore {
    /// Injected base path, used by tests to point at a temp directory.
    pub(in crate::ai_gateway) fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Default store under `get_app_dir()/ai_gateway_usage.db`.
    pub(in crate::ai_gateway) fn default_store() -> Result<Self, String> {
        let dir = crate::config::get_app_dir()?;
        let path = dir.join(USAGE_DB_FILE);
        super::migration::migrate_legacy_files();
        Ok(Self::at(path))
    }

    fn open(&self) -> Result<Connection, String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let connection = Connection::open(&self.path).map_err(|error| error.to_string())?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|error| error.to_string())?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|error| error.to_string())?;
        migrate_usage_logs(&connection)?;
        // The version-gated one-time cleanup lives in the permanent migration
        // module; a later open finds the marker advanced and deletes nothing.
        super::migration::migrate_usage_database(&connection)?;
        Ok(connection)
    }

    /// Insert one record with the canonical request-log classification
    /// (`canonical_v1`, usage present and accounting valid), then permanently
    /// delete records older than the retention window (REQ-004/REQ-009).
    #[cfg(test)]
    pub(in crate::ai_gateway) fn append(
        &self,
        record: &UsageLogRecord,
        retention_days: u32,
    ) -> Result<(), String> {
        self.append_with_accounting(record, UsageAccounting::CANONICAL, retention_days)
    }

    /// Insert one record carrying the parser's real usage classification. Used
    /// by the request runtime so missing or invalid usage is persisted as such.
    pub(in crate::ai_gateway) fn append_with_accounting(
        &self,
        record: &UsageLogRecord,
        accounting: UsageAccounting,
        retention_days: u32,
    ) -> Result<(), String> {
        let connection = self.open()?;
        {
            let mut statement = connection
                .prepare(INSERT_SQL)
                .map_err(|error| error.to_string())?;
            insert_record(&mut statement, record, accounting)?;
        }
        apply_retention(&connection, retention_days);
        Ok(())
    }

    /// Insert every record of one request through a single connection, in slice
    /// order, then apply the same retention cleanup as [`Self::append`]
    /// (REQ-005). Each record carries the canonical request-log classification.
    /// An empty slice writes nothing.
    #[cfg(test)]
    pub(in crate::ai_gateway) fn append_batch(
        &self,
        records: &[UsageLogRecord],
        retention_days: u32,
    ) -> Result<(), String> {
        let entries: Vec<UsageLogEntry> = records
            .iter()
            .cloned()
            .map(|record| UsageLogEntry {
                record,
                accounting: UsageAccounting::CANONICAL,
            })
            .collect();
        self.append_batch_with_accounting(&entries, retention_days)
    }

    /// Insert every entry of one request through a single connection, in slice
    /// order, then apply the same retention cleanup as [`Self::append`]
    /// (REQ-005). An empty slice writes nothing.
    pub(in crate::ai_gateway) fn append_batch_with_accounting(
        &self,
        entries: &[UsageLogEntry],
        retention_days: u32,
    ) -> Result<(), String> {
        let connection = self.open()?;
        {
            let mut statement = connection
                .prepare(INSERT_SQL)
                .map_err(|error| error.to_string())?;
            for entry in entries {
                insert_record(&mut statement, &entry.record, entry.accounting)?;
            }
        }
        apply_retention(&connection, retention_days);
        Ok(())
    }

    /// Test-only inspection helper: total row count.
    #[cfg(test)]
    pub(in crate::ai_gateway) fn count(&self) -> Result<u32, String> {
        let connection = self.open()?;
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM usage_logs", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        Ok(count as u32)
    }

    /// Test-only inspection helper: every row, newest first.
    #[cfg(test)]
    pub(in crate::ai_gateway) fn all_records(&self) -> Result<Vec<UsageLogRecord>, String> {
        let connection = self.open()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {RECORD_COLUMNS}
                 FROM usage_logs ORDER BY timestamp_ms DESC, id DESC"
            ))
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], record_from_row)
            .map_err(|error| error.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())
    }

    /// Ungrouped page of records, newest first, 50 per page. `page` is clamped
    /// to the valid range so switching ranges never lands on a blank page.
    pub(in crate::ai_gateway) fn query_logs(
        &self,
        range: &TimeRange,
        filter: &LogFilter,
        page: u32,
    ) -> Result<UsageLogsPage, String> {
        let connection = self.open()?;
        let (where_sql, params) = bind(range, filter, false);
        let total: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM usage_logs{where_sql}"),
                params_from_iter(params.iter()),
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let total = total as u32;
        let total_pages = total.div_ceil(USAGE_LOG_PAGE_SIZE).max(1);
        let page = page.max(1).min(total_pages);
        let offset = (page - 1) * USAGE_LOG_PAGE_SIZE;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {RECORD_COLUMNS}
                 FROM usage_logs{where_sql}
                 ORDER BY timestamp_ms DESC, id DESC
                 LIMIT ? OFFSET ?"
            ))
            .map_err(|error| error.to_string())?;
        let mut page_params = params;
        page_params.push((USAGE_LOG_PAGE_SIZE as i64).into());
        page_params.push((offset as i64).into());
        let records = statement
            .query_map(params_from_iter(page_params.iter()), record_from_row)
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        // The model facet spans the whole range and ignores the model filter, so
        // the filter selector can offer in-range models absent from this page.
        let facet_filter = LogFilter {
            status: filter.status,
            model: None,
        };
        let (facet_where_sql, facet_params) = bind(range, &facet_filter, false);
        let facet_where = if facet_where_sql.is_empty() {
            " WHERE local_model <> ''".to_string()
        } else {
            format!("{facet_where_sql} AND local_model <> ''")
        };
        let mut facet_statement = connection
            .prepare(&format!(
                "SELECT DISTINCT local_model FROM usage_logs{facet_where} ORDER BY local_model ASC"
            ))
            .map_err(|error| error.to_string())?;
        let models = facet_statement
            .query_map(params_from_iter(facet_params.iter()), |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        Ok(UsageLogsPage {
            page,
            page_size: USAGE_LOG_PAGE_SIZE,
            total,
            total_pages,
            group_by: None,
            records,
            groups: Vec::new(),
            models,
        })
    }

    /// Grouped rows by `"model"` or `"day"` (UTC+8). `error_count` counts only
    /// `failure` rows.
    pub(in crate::ai_gateway) fn group_logs(
        &self,
        range: &TimeRange,
        filter: &LogFilter,
        group_by: &str,
    ) -> Result<Vec<UsageLogGroupRow>, String> {
        let connection = self.open()?;
        let (where_sql, params) = bind(range, filter, true);
        let mut groups = Vec::new();
        if group_by == "day" {
            let sql = format!(
                "SELECT ((timestamp_ms + 28800000) / 86400000) AS day_index,
                        COUNT(*),
                        COALESCE(SUM(CASE WHEN result = 'failure' THEN 1 ELSE 0 END), 0),
                        COALESCE(MAX(timestamp_ms), 0)
                 FROM usage_logs{where_sql}
                 GROUP BY day_index ORDER BY day_index ASC"
            );
            let mut statement = connection.prepare(&sql).map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params_from_iter(params.iter()), |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })
                .map_err(|error| error.to_string())?;
            for row in rows {
                let (index, request_count, error_count, last_request_at_ms) =
                    row.map_err(|error| error.to_string())?;
                groups.push(UsageLogGroupRow {
                    group: day_index_label(index),
                    request_count: request_count as u32,
                    error_count: error_count as u32,
                    last_request_at_ms,
                });
            }
        } else {
            let sql = format!(
                "SELECT local_model,
                        COUNT(*),
                        COALESCE(SUM(CASE WHEN result = 'failure' THEN 1 ELSE 0 END), 0),
                        COALESCE(MAX(timestamp_ms), 0)
                 FROM usage_logs{where_sql}
                 GROUP BY local_model ORDER BY MAX(timestamp_ms) DESC, local_model ASC"
            );
            let mut statement = connection.prepare(&sql).map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params_from_iter(params.iter()), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })
                .map_err(|error| error.to_string())?;
            for row in rows {
                let (group, request_count, error_count, last_request_at_ms) =
                    row.map_err(|error| error.to_string())?;
                groups.push(UsageLogGroupRow {
                    group,
                    request_count: request_count as u32,
                    error_count: error_count as u32,
                    last_request_at_ms,
                });
            }
        }
        Ok(groups)
    }

    pub(in crate::ai_gateway) fn usage_stats(
        &self,
        range: &TimeRange,
        hour_buckets: bool,
    ) -> Result<UsageStats, String> {
        let connection = self.open()?;
        // Every aggregate below shares this one terminal-only filter.
        let (stats_where, params) = bind(range, &LogFilter::default(), true);
        // A failed attempt with no candidate provider (`provider_id = ''`) still
        // counts toward totals/models, but must not leak a blank provider row.
        let provider_where = format!("{stats_where} AND provider_id <> ''");
        let totals: UsageMetrics = connection
            .query_row(
                &format!("SELECT {} FROM usage_logs{stats_where}", metric_columns()),
                params_from_iter(params.iter()),
                |row| metrics_from_row(row, 0),
            )
            .map_err(|error| error.to_string())?;

        let bucket_sql = if hour_buckets {
            "((timestamp_ms + 28800000) / 3600000) % 24"
        } else {
            "((timestamp_ms + 28800000) / 86400000)"
        };
        let mut statement = connection
            .prepare(&format!(
                "SELECT {bucket_sql} AS bucket_key, {}
                 FROM usage_logs{stats_where}
                 GROUP BY bucket_key ORDER BY bucket_key ASC",
                metric_columns()
            ))
            .map_err(|error| error.to_string())?;
        let mut buckets = Vec::new();
        for row in statement
            .query_map(params_from_iter(params.iter()), |row| {
                let key = row.get::<_, i64>(0)?;
                Ok((key, metrics_from_row(row, 1)?))
            })
            .map_err(|error| error.to_string())?
        {
            let (key, metrics) = row.map_err(|error| error.to_string())?;
            let label = if hour_buckets {
                format!("{key:02}:00")
            } else {
                day_index_label(key)
            };
            buckets.push(UsageBucketRow { label, metrics });
        }

        let mut model_statement = connection
            .prepare(&format!(
                "SELECT local_model, {}
                 FROM usage_logs{stats_where}
                 GROUP BY local_model ORDER BY COALESCE(SUM(total_tokens), 0) DESC, local_model ASC",
                metric_columns()
            ))
            .map_err(|error| error.to_string())?;
        let model_rows = model_statement
            .query_map(params_from_iter(params.iter()), |row| {
                let model = row.get::<_, String>(0)?;
                Ok((model, metrics_from_row(row, 1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;

        let mut provider_statement = connection
            .prepare(&format!(
                "SELECT local_model, provider_id, provider_name, {}
                 FROM usage_logs{provider_where}
                 GROUP BY local_model, provider_id, provider_name
                 ORDER BY local_model ASC, provider_name ASC",
                metric_columns()
            ))
            .map_err(|error| error.to_string())?;
        let provider_rows = provider_statement
            .query_map(params_from_iter(params.iter()), |row| {
                let model = row.get::<_, String>(0)?;
                let provider_id = row.get::<_, String>(1)?;
                let provider_name = row.get::<_, String>(2)?;
                Ok((model, provider_id, provider_name, metrics_from_row(row, 3)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;

        let mut models = Vec::new();
        for (local_model, metrics) in model_rows {
            let providers = provider_rows
                .iter()
                .filter(|(model, _, _, _)| model == &local_model)
                .map(|(_, provider_id, provider_name, metrics)| UsageProviderRow {
                    provider_id: provider_id.clone(),
                    provider_name: provider_name.clone(),
                    metrics: *metrics,
                })
                .collect();
            models.push(UsageModelRow {
                local_model,
                metrics,
                providers,
            });
        }

        let mut unpriced_items = Vec::new();
        if totals.unpriced_count > 0 {
            let mut unpriced_statement = connection
                .prepare(&format!(
                     "SELECT provider_id, provider_name, local_model, upstream_model, COUNT(*)
                      FROM usage_logs{stats_where} AND amount IS NULL AND upstream_model <> '' AND (result = 'success' OR total_tokens > 0)
                     GROUP BY provider_id, provider_name, local_model, upstream_model
                     ORDER BY COUNT(*) DESC, provider_name ASC, local_model ASC"
                ))
                .map_err(|error| error.to_string())?;
            for row in unpriced_statement
                .query_map(params_from_iter(params.iter()), |row| {
                    Ok(UnpricedUsageItem {
                        provider_id: row.get(0)?,
                        provider_name: row.get(1)?,
                        local_model: row.get(2)?,
                        upstream_model: row.get(3)?,
                        count: row.get::<_, i64>(4)? as u32,
                    })
                })
                .map_err(|error| error.to_string())?
            {
                unpriced_items.push(row.map_err(|error| error.to_string())?);
            }
        }

        Ok(UsageStats {
            granularity: if hour_buckets { "hour" } else { "day" }.to_string(),
            totals,
            buckets,
            models,
            unpriced_items,
        })
    }
}
