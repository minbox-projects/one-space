use crate::app_store::SessionRecord;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::app_store) const SCHEMA_VERSION: u32 = 1;
pub(in crate::app_store) const OUTBOX_DEDUP_WINDOW_SECS: u64 = 3;
pub(in crate::app_store) const MANAGED_TOOLS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];
pub(in crate::app_store) const HISTORY_SYNC_TOOLS: [&str; 4] =
    ["claude", "codex", "gemini", "opencode"];
pub(in crate::app_store) const HISTORY_SYNC_BASE_PARSER_VERSION: u32 = 1;
pub(in crate::app_store) const CODEX_HISTORY_TITLE_PARSER_VERSION: u32 = 2;
pub(in crate::app_store) const OPENCODE_HISTORY_PROJECT_FALLBACK_VERSION: u32 = 2;
pub(in crate::app_store) const HISTORY_BIND_WINDOW_SECS: u64 = 15 * 60;
pub(in crate::app_store) const LAUNCHER_EXPORT_VERSION: u32 = 1;
pub(in crate::app_store) const PROVIDERS_EXPORT_VERSION: u32 = 1;
pub(in crate::app_store) const PROVIDER_HISTORY_LIMIT: usize = 5;
pub(in crate::app_store) const LAUNCHER_TYPES: [&str; 5] =
    ["app", "script", "url", "folder", "internal"];
pub(in crate::app_store) static SESSION_CREATE_LOCKS: OnceLock<Mutex<HashSet<String>>> =
    OnceLock::new();
pub(in crate::app_store) static SESSIONS_HISTORY_SYNC_RUNNING: AtomicBool = AtomicBool::new(false);
pub(in crate::app_store) static SESSIONS_STATE_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiMeta {
    pub schema_version: u32,
    pub revision: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiOk<T> {
    pub ok: bool,
    pub data: T,
    pub meta: ApiMeta,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiErr {
    pub ok: bool,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DashboardCounts {
    pub launcher: usize,
    pub workspaces: usize,
    pub sessions: usize,
    pub ssh: usize,
    pub snippets: usize,
    pub bookmarks: usize,
    pub notes: usize,
    pub ai_news: usize,
    pub environments: usize,
    pub skills: usize,
    pub subagents: usize,
    pub mcp_servers: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_type: Option<String>,
}

pub(in crate::app_store) fn api_ok<T: Serialize>(
    data: T,
    meta: ApiMeta,
) -> Result<ApiOk<T>, ApiErr> {
    Ok(ApiOk {
        ok: true,
        data,
        meta,
    })
}

pub(in crate::app_store) fn api_error(code: &str, message: impl Into<String>) -> ApiErr {
    ApiErr {
        ok: false,
        code: code.to_string(),
        message: message.into(),
        details: None,
    }
}

pub(in crate::app_store) fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(in crate::app_store) fn default_session_name_source() -> String {
    "manual".to_string()
}

pub(in crate::app_store) fn required_history_parser_version(tool: &str) -> u32 {
    if tool.eq_ignore_ascii_case("codex") {
        CODEX_HISTORY_TITLE_PARSER_VERSION
    } else if tool.eq_ignore_ascii_case("opencode") {
        OPENCODE_HISTORY_PROJECT_FALLBACK_VERSION
    } else {
        HISTORY_SYNC_BASE_PARSER_VERSION
    }
}

pub(in crate::app_store) fn normalize_session_name_source(input: &str) -> String {
    let value = input.trim().to_lowercase();
    if value == "history" {
        "history".to_string()
    } else {
        "manual".to_string()
    }
}

pub(in crate::app_store) fn sessions_history_days() -> u64 {
    crate::config::get_storage_config()
        .ok()
        .and_then(|cfg| cfg.ai_sessions_history_days)
        .unwrap_or(30)
}

pub(in crate::app_store) fn session_history_cutoff_ts() -> u64 {
    let history_days = sessions_history_days();
    let now = now_ts();
    now.saturating_sub(history_days * 24 * 60 * 60)
}

pub(in crate::app_store) fn filter_sessions_by_history_window<'a>(
    sessions: impl Iterator<Item = &'a SessionRecord>,
) -> Vec<SessionRecord> {
    let cutoff_ts = session_history_cutoff_ts();
    let mut filtered = sessions
        .filter(|session| {
            // Favorited sessions are always kept regardless of history window.
            if session.favorited_at.is_some() {
                true
            } else {
                session.last_used_at >= cutoff_ts || session.created_at >= cutoff_ts
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    sort_sessions_for_display(&mut filtered);
    filtered
}

/// Sort sessions for display: favorited first (by favorited_at desc),
/// then non-favorited by last_used_at/created_at desc, with name/id tiebreak.
pub(in crate::app_store) fn sort_sessions_for_display(sessions: &mut Vec<SessionRecord>) {
    // Pre-compute lowercase names to avoid repeated allocations in comparator.
    let mut keyed: Vec<_> = sessions
        .drain(..)
        .map(|s| {
            let lower = s.name.to_lowercase();
            (
                s.favorited_at.is_some(),
                s.favorited_at,
                s.last_used_at,
                s.created_at,
                lower,
                s.id.clone(),
                s,
            )
        })
        .collect();

    keyed.sort_by(|a, b| {
        let (a_fav, _, a_used, a_created, a_lower, a_id, _) = a;
        let (b_fav, _, b_used, b_created, b_lower, b_id, _) = b;

        match (a_fav, b_fav) {
            (true, true) => {
                b.1.cmp(&a.1)
                    .then_with(|| b_used.cmp(a_used))
                    .then_with(|| b_created.cmp(a_created))
                    .then_with(|| a_lower.cmp(b_lower))
                    .then_with(|| a_id.cmp(b_id))
            }
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => b_used
                .cmp(a_used)
                .then_with(|| b_created.cmp(a_created))
                .then_with(|| a_lower.cmp(b_lower))
                .then_with(|| a_id.cmp(b_id)),
        }
    });

    sessions.extend(keyed.into_iter().map(|t| t.6));
}

pub(in crate::app_store) fn session_create_locks() -> &'static Mutex<HashSet<String>> {
    SESSION_CREATE_LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(in crate::app_store) fn sessions_state_write_lock() -> &'static Mutex<()> {
    SESSIONS_STATE_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

pub(in crate::app_store) fn lock_sessions_state_write(
) -> Result<std::sync::MutexGuard<'static, ()>, String> {
    sessions_state_write_lock()
        .lock()
        .map_err(|_| "sessions state write lock poisoned".to_string())
}

pub(in crate::app_store) fn acquire_session_create_lock(
    key: String,
) -> Result<Option<String>, String> {
    let mut locks = session_create_locks()
        .lock()
        .map_err(|_| "session create lock poisoned".to_string())?;
    if locks.contains(&key) {
        return Ok(None);
    }
    locks.insert(key.clone());
    Ok(Some(key))
}

pub(in crate::app_store) fn release_session_create_lock(key: &str) {
    if let Ok(mut locks) = session_create_locks().lock() {
        locks.remove(key);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SchemaMeta {
    pub schema_version: u32,
    pub created_at: u64,
    pub last_migrated_at: u64,
    pub revision: u64,
}

impl Default for SchemaMeta {
    fn default() -> Self {
        let now = now_ts();
        Self {
            schema_version: SCHEMA_VERSION,
            created_at: now,
            last_migrated_at: now,
            revision: 1,
        }
    }
}
