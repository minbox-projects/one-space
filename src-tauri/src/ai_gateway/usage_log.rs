//! Usage metering and request logs for the AI gateway.
//!
//! Records live in a dedicated SQLite file (never the encrypted
//! `ai_gateway.json`) and deliberately contain no request/response bodies,
//! headers or credentials. The store base path is injectable so tests point at
//! a temp directory and never touch real application data.

mod clock;
mod errors;
mod pricing;
mod range;
mod records;
mod schema;
mod stats;
mod store;
mod tokens;

pub use pricing::{compute_cost_at_time_with_breakdown, match_price_for_provider};
#[cfg(test)]
pub use pricing::{compute_cost, compute_cost_at_time, is_off_peak};

#[allow(unused_imports)]
pub use records::{
    validate_retention_days, NativeReasoningSnapshot, UsageBucketRow, UsageCostBreakdown,
    UsageLogGroupRow, UsageLogRecord, UsageLogsPage, UsageMetrics, UsageModelRow,
    UsageProviderModelRow, UsageProviderRow, UsageProviderStatsRow, UsageResult, UsageStats,
    UsageTokens, UnpricedUsageItem,
};

pub use stats::USAGE_LOG_PAGE_SIZE;

pub(in crate::ai_gateway) use clock::now_millis;
pub(in crate::ai_gateway) use errors::{extract_upstream_error_text, sanitize_error_text};
#[allow(unused_imports)]
pub(in crate::ai_gateway) use range::{
    resolve_range, resolve_range_selector, ResolvedRange, TimeRange,
};
pub(in crate::ai_gateway) use records::{
    normalize_retention_days, LogFilter, UsageAccounting, UsageLogEntry,
};
#[allow(unused_imports)]
pub(in crate::ai_gateway) use schema::CANONICAL_USAGE_SEMANTICS;
pub(in crate::ai_gateway) use stats::{ELIGIBLE_ROW_SQL, UNPRICED_ROW_SQL};
pub(in crate::ai_gateway) use store::{UsageLogStore, USAGE_DB_FILE};
pub(in crate::ai_gateway) use tokens::{
    canonical_usage_from_value, parse_usage_from_response, CanonicalUsage, SseUsageAccumulator,
};

#[cfg(test)]
pub(in crate::ai_gateway) use clock::{clear_usage_clock_for_test, set_usage_clock_for_test};
#[cfg(test)]
pub(in crate::ai_gateway) use schema::{
    clear_usage_log_insert_failure, inject_usage_log_insert_failure_at,
    reset_usage_log_write_stats, usage_log_write_stats, UsageLogWriteStats,
};
#[cfg(test)]
pub(in crate::ai_gateway) use tokens::usage_tokens_from_value;
