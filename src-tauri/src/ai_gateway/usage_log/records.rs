//! Persisted usage-log record types and aggregation results.

use super::super::{
    default_true, UpstreamProtocol, DEFAULT_USAGE_RETENTION_DAYS, MAX_USAGE_RETENTION_DAYS,
    MIN_USAGE_RETENTION_DAYS,
};
use serde::{Deserialize, Serialize};

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

/// Effective USD-per-million rates and unrounded USD fees frozen for one row.
/// The total remains exclusively in [`UsageLogRecord::amount`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct UsageCostBreakdown {
    pub input_price: f64,
    pub output_price: f64,
    pub cache_read_price: f64,
    pub cache_write_price: f64,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_read_cost: f64,
    pub cache_write_cost: f64,
}

/// Pool-only native reasoning evidence recorded for one attempt.
///
/// `protocol` is the attempt's actual upstream protocol, `mode` is one of
/// `enum | budget | disable | provider_default`, and `parameters` carries only
/// the allowlisted wire controls actually sent. It never contains request or
/// response bodies, headers or credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NativeReasoningSnapshot {
    pub protocol: UpstreamProtocol,
    pub mode: String,
    pub parameters: serde_json::Map<String, serde_json::Value>,
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
    /// Record-time pricing details; absent for legacy or unmatched-price rows.
    #[serde(default)]
    pub cost_breakdown: Option<UsageCostBreakdown>,
    pub duration_ms: u64,
    /// Sanitized, bounded and redacted upstream error text; `None` for a
    /// success or a failure without readable error information.
    #[serde(default)]
    pub error_message: Option<String>,
    /// Whether this row is the request's single terminal row; non-terminal rows
    /// are the completed attempts that a later row superseded.
    #[serde(default = "default_true")]
    pub terminal: bool,
    /// Reasoning effort level (e.g. "low", "medium", "high") requested by client;
    /// `None` when not specified.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Measured first meaningful upstream output latency in milliseconds,
    /// unrounded. `None` for
    /// non-streaming, synthetic no-upstream, no-output and historical rows; a
    /// measured zero offset stays `Some(0.0)`.
    #[serde(default)]
    pub first_token_ms: Option<f64>,
    /// Average output tokens per second over the unrounded whole-attempt window.
    /// The stored column is an availability marker only: a
    /// measured read reprojects it from the row's own `output_tokens` and
    /// positive `duration_ms` (`record_from_row`). `None` when unavailable; an
    /// explicit valid zero count over a positive attempt is `Some(0.0)`.
    #[serde(default)]
    pub avg_tps: Option<f64>,
    /// Pool-only resolved preference (`low|medium|high|xhigh|none|auto`);
    /// `None` for ordinary, historical and synthetic rows without a pool policy.
    #[serde(default)]
    pub resolved_reasoning_effort: Option<String>,
    /// Pool-only native-control snapshot actually sent upstream;
    /// `None` for ordinary/historical rows and for a synthetic no-attempt row.
    #[serde(default)]
    pub native_reasoning: Option<NativeReasoningSnapshot>,
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

/// Persisted usage classification of one new request-log row.
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
    /// Sum of `cache_read_tokens` over cache-eligible rows.
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
pub struct UsageProviderModelRow {
    pub local_model: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageProviderStatsRow {
    pub provider_id: String,
    pub provider_name: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
    pub models: Vec<UsageProviderModelRow>,
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
    pub providers: Vec<UsageProviderStatsRow>,
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
    /// Distinct, non-empty in-range `provider_name` values, independent of the
    /// current page and of the active provider filter. Empty for grouped responses.
    #[serde(default)]
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ai_gateway) struct LogFilter {
    pub status: Option<UsageResult>,
    pub model: Option<String>,
    pub provider: Option<String>,
}

/// One persistable row: its request-log record plus the real usage
/// classification captured while parsing the upstream response.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::ai_gateway) struct UsageLogEntry {
    pub record: UsageLogRecord,
    pub accounting: UsageAccounting,
}
