use serde::{Deserialize, Serialize};

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

/// Parsed `agy -p /usage --output-format json` envelope. Field names mirror the
/// CLI JSON contract so the frontend can render groups and buckets directly.
#[derive(Debug, Clone, Serialize)]
pub struct AntigravityQuotaSnapshot {
    pub groups: Vec<AntigravityQuotaGroup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AntigravityQuotaGroup {
    pub name: String,
    pub description: Option<String>,
    pub buckets: Vec<AntigravityQuotaBucket>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AntigravityQuotaBucket {
    pub id: String,
    pub name: String,
    pub window: String,
    pub remaining_fraction: f64,
    pub reset_time: String,
    pub description: Option<String>,
}
