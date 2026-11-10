//! Unified session inventory aggregation.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use super::types::{
    ExternalOpenCodeSessionInfo, HostedSessionInfo, InlineKeyboardButton, InlineKeyboardMarkup,
    OpenCodeTodoInfo,
};

/// Maximum number of session entries rendered in one overview message.
pub const SESSION_OVERVIEW_MAX_ENTRIES: usize = 8;
/// Maximum number of characters rendered on one overview line.
pub const SESSION_OVERVIEW_MAX_LINE_CHARS: usize = 88;
/// Maximum number of characters rendered in one overview message.
pub const SESSION_OVERVIEW_MAX_CHARS: usize = 3900;
/// Maximum byte length of one session button identifier.
const SESSION_CALLBACK_IDENTIFIER_MAX_BYTES: usize = 48;

/// Marker for an external or hosted session whose real ID cannot be read.
pub const SESSION_ID_UNAVAILABLE: &str = "unavailable";
/// Marker for a running plan execution that has not emitted its session ID yet.
pub const SESSION_ID_CAPTURING: &str = "capturing";
/// Marker for a Telegram conversation without an assigned session.
pub const SESSION_ID_UNASSIGNED: &str = "unassigned";

/// Kind of a unified session inventory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEntryKind {
    PlanExecution,
    Hosted,
    TelegramConversation,
    Terminal,
}

impl SessionEntryKind {
    fn precedence(self) -> u8 {
        match self {
            SessionEntryKind::PlanExecution => 0,
            SessionEntryKind::Hosted => 1,
            SessionEntryKind::TelegramConversation => 2,
            SessionEntryKind::Terminal => 3,
        }
    }
}

/// One deduplicated session shown by the unified inventory.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionEntry {
    pub primary_kind: SessionEntryKind,
    pub kinds: Vec<SessionEntryKind>,
    pub session_id: Option<String>,
    pub session_id_marker: String,
    pub plan_id: Option<String>,
    pub pid: Option<u32>,
    pub project_name: String,
    pub status_label: String,
    pub is_busy: bool,
}

/// Current or last Telegram natural-language conversation session.
#[derive(Debug, Clone, PartialEq)]
pub struct ConversationSessionState {
    pub session_id: Option<String>,
    pub project_name: Option<String>,
    pub is_busy: bool,
}

/// Single active Telegram plan-execution session.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanExecutionState {
    pub plan_id: String,
    pub session_id: Option<String>,
    pub project_name: String,
    pub is_busy: bool,
}

/// Progress detail read from the OpenCode database for one session ID.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionProgressDetail {
    pub current_step: Option<u32>,
    pub last_tool: Option<String>,
    pub last_tool_status: Option<String>,
    pub last_tool_input_preview: Option<String>,
    pub todos: Vec<OpenCodeTodoInfo>,
}

/// Result of resolving a `/sessions <identifier>` argument.
#[derive(Debug, Clone, PartialEq)]
pub enum IdentifierResolution {
    Session(SessionEntry),
    Plan {
        plan_id: String,
        entry: Option<SessionEntry>,
    },
    Ambiguous(Vec<String>),
    Unknown,
}

/// Session ID and execution status for a plan progress detail.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanExecutionSummary {
    pub session_id_marker: String,
    pub execution_status: String,
    pub is_executing: bool,
}

/// Live sources aggregated per inventory request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionInventorySources {
    pub external: Vec<ExternalOpenCodeSessionInfo>,
    pub hosted: Vec<HostedSessionInfo>,
    pub conversation: Option<ConversationSessionState>,
    pub plan_execution: Option<PlanExecutionState>,
}

/// Aggregate all sources into deduplicated, deterministically sorted entries.
pub fn build_session_entries(sources: &SessionInventorySources) -> Vec<SessionEntry> {
    let mut entries: Vec<SessionEntry> = Vec::new();

    if let Some(plan) = &sources.plan_execution {
        let entry = SessionEntry {
            primary_kind: SessionEntryKind::PlanExecution,
            kinds: vec![SessionEntryKind::PlanExecution],
            session_id: plan.session_id.clone(),
            session_id_marker: String::new(),
            plan_id: Some(plan.plan_id.clone()),
            pid: None,
            project_name: plan.project_name.clone(),
            status_label: "Running".to_string(),
            is_busy: plan.is_busy,
        };
        merge_entry(
            &mut entries,
            entry,
            Some(plan.plan_id.as_str()),
            plan.session_id.as_deref(),
        );
    }

    for hosted in &sources.hosted {
        let entry = SessionEntry {
            primary_kind: SessionEntryKind::Hosted,
            kinds: vec![SessionEntryKind::Hosted],
            session_id: Some(hosted.session_id.clone()),
            session_id_marker: String::new(),
            plan_id: None,
            pid: Some(hosted.pid),
            project_name: hosted.project_name.clone(),
            status_label: "Hosted".to_string(),
            is_busy: false,
        };
        merge_entry(&mut entries, entry, None, Some(hosted.session_id.as_str()));
    }

    if let Some(conversation) = &sources.conversation {
        let entry = SessionEntry {
            primary_kind: SessionEntryKind::TelegramConversation,
            kinds: vec![SessionEntryKind::TelegramConversation],
            session_id: conversation.session_id.clone(),
            session_id_marker: String::new(),
            plan_id: None,
            pid: None,
            project_name: conversation.project_name.clone().unwrap_or_default(),
            status_label: if conversation.is_busy { "Busy" } else { "Idle" }.to_string(),
            is_busy: conversation.is_busy,
        };
        merge_entry(
            &mut entries,
            entry,
            None,
            conversation.session_id.as_deref(),
        );
    }

    for external in &sources.external {
        let entry = SessionEntry {
            primary_kind: SessionEntryKind::Terminal,
            kinds: vec![SessionEntryKind::Terminal],
            session_id: external.session_id.clone(),
            session_id_marker: String::new(),
            plan_id: None,
            pid: Some(external.pid),
            project_name: external.project_name.clone(),
            status_label: if external.is_busy { "Busy" } else { "Idle" }.to_string(),
            is_busy: external.is_busy,
        };
        merge_entry(&mut entries, entry, None, external.session_id.as_deref());
    }

    for entry in &mut entries {
        entry.primary_kind = highest_precedence(&entry.kinds);
        entry.session_id_marker = session_marker(entry);
        entry.status_label = status_label(entry.primary_kind, entry.is_busy);
    }

    entries.sort_by(|a, b| {
        a.primary_kind
            .precedence()
            .cmp(&b.primary_kind.precedence())
            .then_with(|| b.is_busy.cmp(&a.is_busy))
            .then_with(|| a.project_name.cmp(&b.project_name))
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    entries
}

/// A session record is a subagent when it has a non-empty parent or its title
/// carries the existing subagent marker.
pub fn is_subagent_session(title: Option<&str>, parent_id: Option<&str>) -> bool {
    if let Some(parent) = parent_id {
        if !parent.trim().is_empty() {
            return true;
        }
    }
    title.map_or(false, |value| {
        value.contains("(@") && value.contains("subagent)")
    })
}

fn merge_entry(
    entries: &mut Vec<SessionEntry>,
    entry: SessionEntry,
    plan_key: Option<&str>,
    session_key: Option<&str>,
) {
    let plan_key = plan_key.filter(|value| !value.is_empty());
    let session_key = session_key.filter(|value| !value.is_empty());

    let target = entries.iter_mut().find(|existing| {
        session_key.map_or(false, |key| existing.session_id.as_deref() == Some(key))
            || plan_key.map_or(false, |key| existing.plan_id.as_deref() == Some(key))
    });

    let Some(existing) = target else {
        entries.push(entry);
        return;
    };

    for kind in &entry.kinds {
        if !existing.kinds.contains(kind) {
            existing.kinds.push(*kind);
        }
    }
    if existing.session_id.is_none() {
        existing.session_id = entry.session_id;
    }
    if existing.plan_id.is_none() {
        existing.plan_id = entry.plan_id;
    }
    if existing.pid.is_none() {
        existing.pid = entry.pid;
    }
    if existing.project_name.is_empty() {
        existing.project_name = entry.project_name;
    }
    existing.is_busy = existing.is_busy || entry.is_busy;
}

fn highest_precedence(kinds: &[SessionEntryKind]) -> SessionEntryKind {
    kinds
        .iter()
        .copied()
        .min_by_key(|kind| kind.precedence())
        .unwrap_or(SessionEntryKind::Terminal)
}

fn status_label(primary_kind: SessionEntryKind, is_busy: bool) -> String {
    match primary_kind {
        SessionEntryKind::PlanExecution => "Running".to_string(),
        SessionEntryKind::Hosted => {
            if is_busy {
                "Busy".to_string()
            } else {
                "Hosted".to_string()
            }
        }
        SessionEntryKind::TelegramConversation | SessionEntryKind::Terminal => {
            if is_busy {
                "Busy".to_string()
            } else {
                "Idle".to_string()
            }
        }
    }
}

fn session_marker(entry: &SessionEntry) -> String {
    if let Some(session_id) = &entry.session_id {
        return session_id.clone();
    }
    match entry.primary_kind {
        SessionEntryKind::PlanExecution => SESSION_ID_CAPTURING.to_string(),
        SessionEntryKind::TelegramConversation => SESSION_ID_UNASSIGNED.to_string(),
        SessionEntryKind::Hosted | SessionEntryKind::Terminal => SESSION_ID_UNAVAILABLE.to_string(),
    }
}

/// Render a grouped, bounded overview of the supplied session entries.
pub fn format_sessions_overview(
    entries: &[SessionEntry],
) -> (String, Option<InlineKeyboardMarkup>) {
    if entries.is_empty() {
        return (
            "*OneSpace* ╎ 会话概览\n\n\
            > ⚪ 当前没有活跃会话。\n\n\
            发送 `/sessions` 刷新状态；可使用 `/plans` 浏览方案，或用 `/host <id>`、`/prompt` 操作活跃目标。".to_string(),
            None,
        );
    }

    let rendered = entries.len().min(SESSION_OVERVIEW_MAX_ENTRIES);
    let omitted = &entries[rendered..];

    let mut lines = vec!["*OneSpace* ╎ 会话概览".to_string()];
    let mut buttons: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for kind in [
        SessionEntryKind::PlanExecution,
        SessionEntryKind::Hosted,
        SessionEntryKind::TelegramConversation,
        SessionEntryKind::Terminal,
    ] {
        let group: Vec<&SessionEntry> = entries[..rendered]
            .iter()
            .filter(|entry| entry.primary_kind == kind)
            .collect();
        if group.is_empty() {
            continue;
        }
        lines.push(String::new());
        lines.push(format!("*{}*", group_label(kind)));
        for entry in group {
            for line in format_entry_lines(entry) {
                lines.push(clamp_line(&line));
            }
            if let Some(identifier) = session_callback_identifier(entry) {
                buttons.push(vec![InlineKeyboardButton::callback(
                    button_label(entry),
                    format!("open_session:{identifier}"),
                )]);
            }
        }
    }

    if !omitted.is_empty() {
        lines.push(String::new());
        lines.push(clamp_line(&format!(
            "还有 {} 个会话未展示：",
            omitted.len()
        )));
        for entry in omitted {
            let identifier = entry
                .session_id
                .as_deref()
                .unwrap_or(&entry.session_id_marker);
            lines.push(clamp_line(identifier));
        }
    }

    let text = clamp_chars(lines.join("\n"), SESSION_OVERVIEW_MAX_CHARS);
    buttons.push(vec![InlineKeyboardButton::callback(
        "🔄 刷新会话",
        "open_sessions",
    )]);
    (text, Some(InlineKeyboardMarkup::new(buttons)))
}

fn group_label(kind: SessionEntryKind) -> &'static str {
    match kind {
        SessionEntryKind::PlanExecution => "Plan Execution",
        SessionEntryKind::Hosted => "Hosted",
        SessionEntryKind::TelegramConversation => "Conversation",
        SessionEntryKind::Terminal => "Terminal",
    }
}

fn format_entry_lines(entry: &SessionEntry) -> [String; 2] {
    let status_line = format!("> {} · {}", entry.project_name, entry.status_label);
    let mut identifier_line = format!("> {}", entry.session_id_marker);
    if let Some(pid) = entry.pid {
        identifier_line.push_str(&format!(" · PID: {pid}"));
    }
    if let Some(plan_id) = &entry.plan_id {
        identifier_line.push_str(&format!(" · Plan: {plan_id}"));
    }
    [status_line, identifier_line]
}

/// Pick the first available callback identifier, skipping over-long values.
fn session_callback_identifier(entry: &SessionEntry) -> Option<String> {
    let identifier = entry
        .session_id
        .clone()
        .or_else(|| entry.pid.map(|pid| pid.to_string()))
        .or_else(|| entry.plan_id.clone())?;
    if identifier.len() > SESSION_CALLBACK_IDENTIFIER_MAX_BYTES {
        return None;
    }
    Some(identifier)
}

fn button_label(entry: &SessionEntry) -> String {
    let icon = match entry.primary_kind {
        SessionEntryKind::PlanExecution => "🚀 ",
        SessionEntryKind::Hosted => "🤖 ",
        SessionEntryKind::TelegramConversation => "💬 ",
        SessionEntryKind::Terminal => "💻 ",
    };
    let project = clamp_chars(entry.project_name.clone(), 24);
    format!("{icon}{project} · {}", entry.status_label)
}

fn clamp_line(line: &str) -> String {
    clamp_chars(line.to_string(), SESSION_OVERVIEW_MAX_LINE_CHARS)
}

fn clamp_chars(mut text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text;
    }
    let keep = max_chars.saturating_sub(1);
    let byte_index = text
        .char_indices()
        .nth(keep)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    text.truncate(byte_index);
    text.push('…');
    text
}

/// Read step, latest-tool and todo progress for one exact session ID.
///
/// Missing database files, tables or session rows degrade to `None` instead of
/// panicking.
pub fn lookup_session_progress(session_id: &str, db_path: &Path) -> Option<SessionProgressDetail> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .ok()?;

    // Count both v1 `step-start` and v2 `step_start` part types.
    let step_count_sql = "SELECT count(*) FROM part WHERE session_id = ?1 \
         AND (json_extract(data, '$.type') = 'step-start' \
              OR json_extract(data, '$.type') = 'step_start')";
    let mut stmt = conn.prepare(step_count_sql).ok()?;
    let step_count = stmt
        .query_row([session_id], |row| row.get::<_, i64>(0))
        .ok()?;
    drop(stmt);

    let mut detail = SessionProgressDetail {
        current_step: (step_count > 0).then_some(step_count as u32),
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        todos: Vec::new(),
    };

    // Latest tool part provides the tool name, status and input preview.
    let parts_sql =
        "SELECT data FROM part WHERE session_id = ?1 ORDER BY time_created DESC LIMIT 20";
    if let Ok(mut stmt) = conn.prepare(parts_sql) {
        if let Ok(rows) = stmt.query_map([session_id], |row| row.get::<_, String>(0)) {
            for data_str in rows.flatten() {
                let Ok(v) = serde_json::from_str::<Value>(&data_str) else {
                    continue;
                };
                if v.get("type").and_then(|t| t.as_str()) == Some("tool") {
                    detail.last_tool = v
                        .get("tool")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());
                    detail.last_tool_status = v
                        .get("state")
                        .and_then(|s| s.get("status"))
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string());
                    detail.last_tool_input_preview =
                        super::opencode_inspector::extract_tool_input_preview(&v);
                    break;
                }
            }
        }
    }

    let todos_sql = "SELECT position, status, content FROM todo WHERE session_id = ?1 \
         ORDER BY position ASC LIMIT 10";
    if let Ok(mut stmt) = conn.prepare(todos_sql) {
        if let Ok(rows) = stmt.query_map([session_id], |row| {
            Ok(OpenCodeTodoInfo {
                position: row.get::<_, i32>(0)?,
                status: row.get::<_, String>(1)?,
                content: row.get::<_, String>(2)?,
            })
        }) {
            detail.todos = rows.flatten().collect();
        }
    }

    if detail.current_step.is_none() && detail.last_tool.is_none() && detail.todos.is_empty() {
        return None;
    }

    Some(detail)
}

/// Resolve a session ID, numeric PID or plan ID against inventory entries.
pub fn resolve_session_identifier(entries: &[SessionEntry], raw: &str) -> IdentifierResolution {
    let identifier = raw.trim();
    if identifier.is_empty() {
        return IdentifierResolution::Unknown;
    }

    let by_session: Vec<SessionEntry> = entries
        .iter()
        .filter(|entry| entry.session_id.as_deref() == Some(identifier))
        .cloned()
        .collect();
    match by_session.len() {
        0 => {}
        1 => return IdentifierResolution::Session(by_session.into_iter().next().unwrap()),
        _ => {
            return IdentifierResolution::Ambiguous(
                by_session.iter().map(candidate_session_id).collect(),
            );
        }
    }

    if identifier.bytes().all(|byte| byte.is_ascii_digit()) {
        let Some(pid) = identifier.parse::<u32>().ok() else {
            return IdentifierResolution::Unknown;
        };
        let by_pid: Vec<SessionEntry> = entries
            .iter()
            .filter(|entry| entry.pid == Some(pid))
            .cloned()
            .collect();
        return match by_pid.len() {
            0 => IdentifierResolution::Unknown,
            1 => IdentifierResolution::Session(by_pid.into_iter().next().unwrap()),
            _ => IdentifierResolution::Ambiguous(by_pid.iter().map(candidate_session_id).collect()),
        };
    }

    let entry = entries
        .iter()
        .find(|entry| entry.plan_id.as_deref() == Some(identifier))
        .cloned();
    IdentifierResolution::Plan {
        plan_id: identifier.to_string(),
        entry,
    }
}

fn candidate_session_id(entry: &SessionEntry) -> String {
    entry
        .session_id
        .clone()
        .unwrap_or_else(|| entry.session_id_marker.clone())
}

/// Summarize whether a plan is actively executing and its captured session ID.
pub fn plan_execution_summary(
    plan_id: &str,
    execution: Option<&PlanExecutionState>,
) -> PlanExecutionSummary {
    match execution.filter(|state| state.plan_id == plan_id) {
        Some(state) => PlanExecutionSummary {
            session_id_marker: state
                .session_id
                .clone()
                .unwrap_or_else(|| SESSION_ID_CAPTURING.to_string()),
            execution_status: "Running".to_string(),
            is_executing: true,
        },
        None => PlanExecutionSummary {
            session_id_marker: SESSION_ID_UNAVAILABLE.to_string(),
            execution_status: "Not executing".to_string(),
            is_executing: false,
        },
    }
}
