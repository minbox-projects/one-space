use super::{
    normalize_runtime_mode, normalize_session_name_source, SessionRecord, SessionsHistoryToolState,
    SessionsState, HISTORY_SYNC_BASE_PARSER_VERSION, HISTORY_SYNC_TOOLS,
};
use std::collections::BTreeSet;
use std::fs::{self};
use std::path::PathBuf;

pub(in crate::app_store) fn session_install_scope_and_root(
    session: &SessionRecord,
) -> (String, Option<String>) {
    if normalize_runtime_mode(Some(&session.runtime_mode)) != "strict" {
        return ("global".to_string(), None);
    }
    let raw = session.working_dir.trim();
    if raw.is_empty() {
        return ("project".to_string(), None);
    }
    let root = fs::canonicalize(PathBuf::from(raw))
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| Some(raw.to_string()));
    ("project".to_string(), root)
}

pub(in crate::app_store) fn normalize_sessions_state(state: &mut SessionsState) -> bool {
    let mut changed = false;
    for session in &mut state.sessions {
        let normalized_name_source = normalize_session_name_source(&session.name_source);
        if session.name_source != normalized_name_source {
            session.name_source = normalized_name_source;
            changed = true;
        }
        let normalized_model_name = session
            .model_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if session.model_name != normalized_model_name {
            session.model_name = normalized_model_name;
            changed = true;
        }
        if session.runtime_mode.trim().is_empty() {
            session.runtime_mode = "shared".to_string();
            changed = true;
        }
        if normalize_runtime_mode(Some(&session.runtime_mode)) != session.runtime_mode {
            session.runtime_mode = normalize_runtime_mode(Some(&session.runtime_mode));
            changed = true;
        }
        if session.runtime_mode == "shared" && session.runtime_profile_id.is_some() {
            session.runtime_profile_id = None;
            changed = true;
        }
    }

    let mut normalized_tombstones = BTreeSet::new();
    for tombstone in state.tombstones.iter() {
        let trimmed = tombstone.trim();
        if trimmed.is_empty() {
            changed = true;
            continue;
        }
        normalized_tombstones.insert(trimmed.to_string());
    }
    if normalized_tombstones != state.tombstones {
        state.tombstones = normalized_tombstones;
        changed = true;
    }

    for tool in HISTORY_SYNC_TOOLS {
        let entry = state
            .history_sync
            .tools
            .entry(tool.to_string())
            .or_insert_with(SessionsHistoryToolState::default);
        if entry.parser_version == 0 && entry.full_backfill_done {
            entry.parser_version = HISTORY_SYNC_BASE_PARSER_VERSION;
            changed = true;
        }
    }
    changed
}
