use chrono::NaiveDate;

pub(in crate::ai_sessions) const USAGE_TOOLS: [&str; 4] =
    ["claude", "codex", "antigravity", "opencode"];

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
pub(super) struct UsageWindow {
    pub(super) days: u16,
    pub(super) start_date: NaiveDate,
    pub(super) end_date: NaiveDate,
    pub(super) start_ms: i64,
    pub(super) end_ms: i64,
}

#[derive(Debug, Default)]
pub(in crate::ai_sessions) struct ToolScan {
    pub(in crate::ai_sessions) source_status: String,
    pub(in crate::ai_sessions) scanned_sessions: u64,
    /// In-window `USER_INPUT` rows counted from Antigravity brain transcripts.
    /// They carry no tokens and never become `UsageRecord`s; only the call
    /// count surfaces, so summary/daily stay untouched.
    pub(in crate::ai_sessions) transcript_calls: u64,
    pub(in crate::ai_sessions) records: Vec<UsageRecord>,
    pub(in crate::ai_sessions) errors: Vec<String>,
}

pub(super) fn unavailable_scan() -> ToolScan {
    ToolScan {
        source_status: "unavailable".to_string(),
        scanned_sessions: 0,
        transcript_calls: 0,
        records: Vec::new(),
        errors: Vec::new(),
    }
}
