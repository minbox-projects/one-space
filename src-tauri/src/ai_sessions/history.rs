#![allow(unused_imports)]

mod aggregation;
mod antigravity;
mod cache;
mod claude;
mod codex;
mod opencode;
mod stats;
mod text;

pub(in crate::ai_sessions) use self::aggregation::dedupe_history_sessions;
pub(in crate::ai_sessions) use self::antigravity::{
    antigravity_bindings_path, antigravity_brain_roots,
    antigravity_conversation_bindings_from_value, antigravity_conversations_roots,
    antigravity_entry_timestamp_ms, collect_antigravity_history_sessions,
    collect_antigravity_sessions_from_brain_root, find_antigravity_transcript,
    find_antigravity_transcript_for_conversation, read_antigravity_conversation_bindings,
    read_antigravity_history_file,
};
pub(in crate::ai_sessions) use self::claude::{
    collect_claude_history_sessions, read_claude_project_file,
};
pub(in crate::ai_sessions) use self::codex::{
    collect_codex_history_sessions, read_codex_history_session_file,
};
pub(in crate::ai_sessions) use self::opencode::{
    collect_opencode_history_sessions, collect_opencode_history_sessions_from_sources,
    read_opencode_model_name, read_opencode_project_worktree_map,
};
pub(in crate::ai_sessions) use self::stats::history_scan_due;
pub(in crate::ai_sessions) use self::text::{
    fallback_history_title, trim_history_text, value_as_text,
};

#[cfg(test)]
pub(in crate::ai_sessions) use self::opencode::read_opencode_history_file;
#[cfg(test)]
pub(in crate::ai_sessions) use self::stats::{
    history_sync_stats, reset_history_sync_stats, HistorySyncStats,
};
