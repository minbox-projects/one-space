#![allow(unused_imports)]

mod aggregation;
mod antigravity;
mod claude;
mod codex;
mod commands;
mod json;
mod model;
mod opencode;
mod protobuf;
mod quota;
mod service;
mod stats;
#[cfg(test)]
mod test_support;
mod types;
mod windows;

pub use self::commands::*;
pub(crate) use self::quota::__cmd__sessions_antigravity_quota;
pub use self::quota::{
    antigravity_quota_cache_fresh, parse_antigravity_quota_envelope, sessions_antigravity_quota,
};
pub use self::service::build_sessions_usage_tool_stats;
pub use self::types::*;

pub(in crate::ai_sessions) use self::claude::parse_claude_usage_file;
pub(in crate::ai_sessions) use self::codex::parse_codex_usage_file;
pub(in crate::ai_sessions) use self::model::{ToolScan, UsageRecord, USAGE_TOOLS};
pub(in crate::ai_sessions) use self::opencode::{
    collect_opencode_usage_records_from_sources, parse_opencode_message_usage_dir,
};
pub(in crate::ai_sessions) use self::stats::record_usage_cache_hit;

#[cfg(test)]
pub use self::stats::{
    reset_usage_collection_stats, usage_collection_stats, UsageCollectionStats,
};
#[cfg(test)]
pub(in crate::ai_sessions) use self::test_support::{
    aggregate_day_stats_for_test, aggregate_usage_for_test, read_opencode_message_tokens_for_test,
    timestamp_days_ago, usage_file_may_overlap_window_for_test,
};
