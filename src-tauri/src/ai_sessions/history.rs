use super::{
    candidate_home_dirs, candidate_opencode_storage_paths, canonicalize_to_string,
    collect_codex_session_files, parse_rfc3339_millis, system_time_to_epoch_millis,
    HistorySessionEntry,
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub(in crate::ai_sessions) fn trim_history_text(input: &str) -> Option<String> {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim();
    if trimmed.is_empty() {
        return None;
    }
    let clipped: String = trimmed.chars().take(140).collect();
    Some(clipped)
}

pub(in crate::ai_sessions) fn history_scan_due(
    path: &Path,
    min_updated_at_ms: Option<i64>,
) -> bool {
    let Some(min_updated_at_ms) = min_updated_at_ms else {
        return true;
    };
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_to_epoch_millis)
        .map(|modified_at_ms| modified_at_ms + 2_000 >= min_updated_at_ms)
        .unwrap_or(true)
}

pub(in crate::ai_sessions) fn value_as_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return trim_history_text(text);
    }
    if let Some(array) = value.as_array() {
        let parts = array
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|text| text.as_str())
                    .or_else(|| item.get("content").and_then(|content| content.as_str()))
                    .and_then(trim_history_text)
            })
            .collect::<Vec<_>>();
        if !parts.is_empty() {
            return trim_history_text(&parts.join(" "));
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(text) = object.get("text").and_then(|text| text.as_str()) {
            return trim_history_text(text);
        }
        if let Some(text) = object.get("content").and_then(|text| text.as_str()) {
            return trim_history_text(text);
        }
    }
    None
}

pub(in crate::ai_sessions) fn fallback_history_title(tool: &str, session_id: &str) -> String {
    let suffix: String = session_id.chars().take(8).collect();
    format!("{} {}", tool.to_uppercase(), suffix)
}

pub(in crate::ai_sessions) fn collect_codex_history_sessions(
    min_updated_at_ms: Option<i64>,
) -> Vec<HistorySessionEntry> {
    let mut out = Vec::new();

    for home in candidate_home_dirs(None) {
        let index_path = home.join(".codex").join("session_index.jsonl");
        let sessions_root = home.join(".codex").join("sessions");
        if !index_path.exists() || !sessions_root.is_dir() {
            continue;
        }

        #[derive(Debug, Deserialize)]
        struct CodexIndexEntry {
            id: String,
            #[serde(default)]
            thread_name: Option<String>,
            #[serde(default)]
            updated_at: Option<String>,
        }

        let mut titles = HashMap::<String, String>::new();
        let mut updated_at_map = HashMap::<String, i64>::new();
        if let Ok(content) = fs::read_to_string(&index_path) {
            for line in content.lines() {
                let Ok(entry) = serde_json::from_str::<CodexIndexEntry>(line) else {
                    continue;
                };
                if let Some(title) = entry.thread_name.as_deref().and_then(trim_history_text) {
                    titles.insert(entry.id.clone(), title);
                }
                if let Some(updated_at_ms) =
                    entry.updated_at.as_deref().and_then(parse_rfc3339_millis)
                {
                    updated_at_map.insert(entry.id, updated_at_ms);
                }
            }
        }

        for (path, modified_ms) in collect_codex_session_files(&sessions_root, usize::MAX) {
            if !history_scan_due(&path, min_updated_at_ms) {
                continue;
            }
            let Some(session) =
                read_codex_history_session_file(&path, &titles, &updated_at_map, modified_ms)
            else {
                continue;
            };
            out.push(session);
        }
    }

    dedupe_history_sessions(out)
}

pub(in crate::ai_sessions) fn read_codex_history_session_file(
    path: &Path,
    titles: &HashMap<String, String>,
    updated_at_map: &HashMap<String, i64>,
    modified_ms: i64,
) -> Option<HistorySessionEntry> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut session_id = String::new();
    let mut working_dir = String::new();
    let mut created_at_ms = 0_i64;
    let mut model_name = None::<String>;
    let mut first_user_title = None::<String>;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match value.get("type").and_then(|v| v.as_str()) {
            Some("session_meta") => {
                let Some(payload) = value.get("payload") else {
                    continue;
                };
                if session_id.is_empty() {
                    session_id = payload
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                }
                if working_dir.is_empty() {
                    working_dir = payload
                        .get("cwd")
                        .and_then(|v| v.as_str())
                        .map(canonicalize_to_string)
                        .unwrap_or_default();
                }
                if created_at_ms == 0 {
                    created_at_ms = payload
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .and_then(parse_rfc3339_millis)
                        .unwrap_or(0);
                }
            }
            Some("turn_context") => {
                model_name = value
                    .get("payload")
                    .and_then(|payload| payload.get("model"))
                    .and_then(|v| v.as_str())
                    .and_then(trim_history_text);
            }
            Some("event_msg") => {
                if first_user_title.is_some() {
                    continue;
                }
                let Some(payload) = value.get("payload") else {
                    continue;
                };
                if payload.get("type").and_then(|v| v.as_str()) != Some("user_message") {
                    continue;
                }
                first_user_title = payload.get("message").and_then(value_as_text);
            }
            _ => {}
        }
    }

    if session_id.is_empty() || working_dir.is_empty() {
        return None;
    }

    let updated_at_ms = updated_at_map
        .get(&session_id)
        .copied()
        .unwrap_or(modified_ms.max(created_at_ms));
    let title = titles
        .get(&session_id)
        .cloned()
        .or(first_user_title)
        .unwrap_or_else(|| session_id.clone());

    Some(HistorySessionEntry {
        tool: "codex".to_string(),
        tool_session_id: session_id.clone(),
        title,
        working_dir,
        model_name,
        created_at_ms: if created_at_ms > 0 {
            created_at_ms
        } else {
            updated_at_ms
        },
        updated_at_ms,
    })
}

pub(in crate::ai_sessions) fn collect_claude_history_sessions(
    min_updated_at_ms: Option<i64>,
) -> Vec<HistorySessionEntry> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let projects_root = home.join(".claude").join("projects");
    if !projects_root.is_dir() {
        return Vec::new();
    }

    let mut fallback_by_session = HashMap::<String, (String, String)>::new();
    let history_path = home.join(".claude").join("history.jsonl");
    if let Ok(content) = fs::read_to_string(history_path) {
        for line in content.lines() {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(session_id) = value.get("sessionId").and_then(|v| v.as_str()) else {
                continue;
            };
            let cwd = value
                .get("project")
                .and_then(|v| v.as_str())
                .map(canonicalize_to_string)
                .unwrap_or_default();
            let title = value
                .get("display")
                .and_then(value_as_text)
                .unwrap_or_default();
            if cwd.is_empty() && title.is_empty() {
                continue;
            }
            fallback_by_session.insert(session_id.to_string(), (cwd, title));
        }
    }

    let mut stack = vec![projects_root];
    let mut out = Vec::new();
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".jsonl"))
                .unwrap_or(false)
            {
                continue;
            }
            if !history_scan_due(&path, min_updated_at_ms) {
                continue;
            }
            let Some(session) = read_claude_project_file(
                &path,
                fallback_by_session.get(
                    path.file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or_default(),
                ),
            ) else {
                continue;
            };
            out.push(session);
        }
    }

    dedupe_history_sessions(out)
}

pub(in crate::ai_sessions) fn read_claude_project_file(
    path: &Path,
    fallback: Option<&(String, String)>,
) -> Option<HistorySessionEntry> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut session_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut working_dir = String::new();
    let mut created_at_ms = 0_i64;
    let mut updated_at_ms = 0_i64;
    let mut first_user_title = None::<String>;
    let mut last_prompt_title = None::<String>;
    let mut model_name = None::<String>;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if session_id.is_empty() {
            session_id = value
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
        }
        if working_dir.is_empty() {
            working_dir = value
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(canonicalize_to_string)
                .unwrap_or_default();
        }
        if let Some(ts_ms) = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_rfc3339_millis)
        {
            if created_at_ms == 0 || ts_ms < created_at_ms {
                created_at_ms = ts_ms;
            }
            if ts_ms > updated_at_ms {
                updated_at_ms = ts_ms;
            }
        }
        match value.get("type").and_then(|v| v.as_str()) {
            Some("user") => {
                if first_user_title.is_none() {
                    first_user_title = value
                        .get("message")
                        .and_then(|message| message.get("content"))
                        .and_then(value_as_text);
                }
            }
            Some("assistant") => {
                model_name = value
                    .get("message")
                    .and_then(|message| message.get("model"))
                    .and_then(|v| v.as_str())
                    .and_then(trim_history_text);
            }
            Some("last-prompt") => {
                last_prompt_title = value.get("lastPrompt").and_then(value_as_text);
            }
            _ => {}
        }
    }

    if let Some((fallback_dir, _)) = fallback {
        if working_dir.is_empty() && !fallback_dir.is_empty() {
            working_dir = canonicalize_to_string(fallback_dir);
        }
    }
    if updated_at_ms == 0 {
        updated_at_ms = fs::metadata(path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(system_time_to_epoch_millis)
            .unwrap_or(created_at_ms);
    }
    if created_at_ms == 0 {
        created_at_ms = updated_at_ms;
    }
    if session_id.is_empty() || working_dir.is_empty() {
        return None;
    }

    let title = last_prompt_title
        .or(first_user_title)
        .or_else(|| fallback.and_then(|(_, title)| trim_history_text(title)))
        .unwrap_or_else(|| fallback_history_title("claude", &session_id));

    Some(HistorySessionEntry {
        tool: "claude".to_string(),
        tool_session_id: session_id,
        title,
        working_dir,
        model_name,
        created_at_ms,
        updated_at_ms,
    })
}

pub(in crate::ai_sessions) fn antigravity_brain_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".gemini").join("antigravity-cli").join("brain"),
        home.join(".gemini").join("antigravity").join("brain"),
    ]
}

pub(in crate::ai_sessions) fn antigravity_bindings_path(home: &Path) -> PathBuf {
    home.join(".gemini")
        .join("antigravity-cli")
        .join("cache")
        .join("last_conversations.json")
}

pub(in crate::ai_sessions) fn read_antigravity_conversation_bindings(
    home: &Path,
) -> HashMap<String, String> {
    let path = antigravity_bindings_path(home);
    let Ok(content) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return HashMap::new();
    };
    antigravity_conversation_bindings_from_value(&value)
}

/// Parse `cache/last_conversations.json` into a `conversation-id -> workspace` map.
///
/// Accepts the flat `{ "<workspace>": "<conversation-id>" }` form and a
/// `conversations` container that holds either objects with explicit fields or a
/// map keyed by conversation id.
pub(in crate::ai_sessions) fn antigravity_conversation_bindings_from_value(
    value: &Value,
) -> HashMap<String, String> {
    let mut out = HashMap::<String, String>::new();
    if let Some(conversations) = value.get("conversations") {
        match conversations {
            Value::Array(items) => {
                for item in items {
                    collect_antigravity_binding_object(item, None, &mut out);
                }
            }
            Value::Object(map) => {
                for (key, item) in map {
                    if let Some(workspace) = item.as_str() {
                        insert_antigravity_binding(&mut out, Some(key.as_str()), Some(workspace));
                    } else {
                        collect_antigravity_binding_object(item, Some(key.as_str()), &mut out);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(object) = value.as_object() {
        for (key, entry) in object {
            if key == "conversations" {
                continue;
            }
            if let Some(conversation_id) = entry.as_str() {
                // Flat form: key is the workspace, value is the conversation id.
                insert_antigravity_binding(&mut out, Some(conversation_id), Some(key.as_str()));
            } else {
                collect_antigravity_binding_object(entry, Some(key.as_str()), &mut out);
            }
        }
    }
    out
}

fn collect_antigravity_binding_object(
    value: &Value,
    fallback_conversation_id: Option<&str>,
    out: &mut HashMap<String, String>,
) {
    let conversation_id = antigravity_string_field(
        value,
        &[
            "conversationId",
            "conversation_id",
            "conversation",
            "id",
        ],
    )
    .or_else(|| fallback_conversation_id.map(str::to_string));
    let workspace = antigravity_string_field(
        value,
        &[
            "workspace",
            "cwd",
            "workingDir",
            "working_dir",
            "directory",
            "path",
            "project",
        ],
    );
    insert_antigravity_binding(out, conversation_id.as_deref(), workspace.as_deref());
}

fn insert_antigravity_binding(
    out: &mut HashMap<String, String>,
    conversation_id: Option<&str>,
    workspace: Option<&str>,
) {
    let Some(conversation_id) = conversation_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let Some(workspace) = workspace.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let normalized = canonicalize_to_string(workspace);
    if normalized.is_empty() {
        return;
    }
    out.insert(conversation_id.to_string(), normalized);
}

fn antigravity_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
    })
}

pub(in crate::ai_sessions) fn collect_antigravity_history_sessions(
    min_updated_at_ms: Option<i64>,
) -> Vec<HistorySessionEntry> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let bindings = read_antigravity_conversation_bindings(&home);
    if bindings.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for brain_root in antigravity_brain_roots(&home) {
        out.extend(collect_antigravity_sessions_from_brain_root(
            &brain_root,
            &bindings,
            min_updated_at_ms,
        ));
    }
    dedupe_history_sessions(out)
}

pub(in crate::ai_sessions) fn collect_antigravity_sessions_from_brain_root(
    brain_root: &Path,
    bindings: &HashMap<String, String>,
    min_updated_at_ms: Option<i64>,
) -> Vec<HistorySessionEntry> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(brain_root) else {
        return out;
    };
    for entry in entries.flatten() {
        let conversation_dir = entry.path();
        if !conversation_dir.is_dir() {
            continue;
        }
        let Some(conversation_id) = conversation_dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let Some(working_dir) = bindings.get(conversation_id) else {
            continue;
        };
        let Some(transcript) = find_antigravity_transcript(&conversation_dir) else {
            continue;
        };
        if !history_scan_due(&transcript, min_updated_at_ms) {
            continue;
        }
        if let Some(session) =
            read_antigravity_history_file(&transcript, conversation_id, working_dir)
        {
            out.push(session);
        }
    }
    out
}

pub(in crate::ai_sessions) fn find_antigravity_transcript(
    conversation_dir: &Path,
) -> Option<PathBuf> {
    if !conversation_dir.is_dir() {
        return None;
    }
    let mut stack = vec![conversation_dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == "transcript_full.jsonl")
                .unwrap_or(false)
            {
                return Some(path);
            }
        }
    }
    None
}

pub(in crate::ai_sessions) fn find_antigravity_transcript_for_conversation(
    home: &Path,
    conversation_id: &str,
) -> Option<PathBuf> {
    for brain_root in antigravity_brain_roots(home) {
        if let Some(path) = find_antigravity_transcript(&brain_root.join(conversation_id)) {
            return Some(path);
        }
    }
    None
}

pub(in crate::ai_sessions) fn read_antigravity_history_file(
    path: &Path,
    conversation_id: &str,
    working_dir: &str,
) -> Option<HistorySessionEntry> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut title = None::<String>;
    let mut model_name = None::<String>;
    let mut first_ts_ms = 0_i64;
    let mut last_ts_ms = 0_i64;
    let mut parsed_any = false;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        parsed_any = true;
        if let Some(ts_ms) = antigravity_entry_timestamp_ms(&value) {
            if first_ts_ms == 0 || ts_ms < first_ts_ms {
                first_ts_ms = ts_ms;
            }
            if ts_ms > last_ts_ms {
                last_ts_ms = ts_ms;
            }
        }
        if title.is_none() && antigravity_entry_is_user_input(&value) {
            title = antigravity_entry_text(&value);
        }
        if model_name.is_none() {
            model_name = antigravity_entry_model(&value);
        }
    }

    if !parsed_any {
        return None;
    }
    let working_dir = working_dir.trim();
    if working_dir.is_empty() {
        return None;
    }
    let normalized_working_dir = canonicalize_to_string(working_dir);
    if normalized_working_dir.is_empty() {
        return None;
    }

    let modified_at_ms = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_to_epoch_millis)
        .unwrap_or(0);
    let created_at_ms = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.created().ok())
        .map(system_time_to_epoch_millis)
        .unwrap_or(modified_at_ms);
    let start_ms = if first_ts_ms > 0 {
        first_ts_ms
    } else {
        created_at_ms
    };
    let updated_at_ms = if last_ts_ms > 0 {
        last_ts_ms
    } else {
        modified_at_ms.max(start_ms)
    };

    Some(HistorySessionEntry {
        tool: "antigravity".to_string(),
        tool_session_id: conversation_id.to_string(),
        title: title.unwrap_or_else(|| fallback_history_title("antigravity", conversation_id)),
        working_dir: normalized_working_dir,
        model_name,
        created_at_ms: start_ms,
        updated_at_ms,
    })
}

fn antigravity_entry_is_user_input(value: &Value) -> bool {
    value
        .get("type")
        .and_then(|kind| kind.as_str())
        .map(|kind| kind.eq_ignore_ascii_case("USER_INPUT"))
        .unwrap_or(false)
}

fn antigravity_entry_text(value: &Value) -> Option<String> {
    for key in ["content", "text", "message", "input", "prompt"] {
        if let Some(text) = value.get(key).and_then(value_as_text) {
            return Some(text);
        }
    }
    value.get("USER_INPUT").and_then(value_as_text)
}

pub(in crate::ai_sessions) fn antigravity_entry_timestamp_ms(value: &Value) -> Option<i64> {
    for key in [
        "timestamp",
        "createdAt",
        "created_at",
        "time",
        "updatedAt",
        "updated_at",
    ] {
        if let Some(ts_ms) = value.get(key).and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_str().and_then(parse_rfc3339_millis))
        }) {
            return Some(ts_ms);
        }
    }
    None
}

fn antigravity_entry_model(value: &Value) -> Option<String> {
    for key in ["model", "modelName", "model_name"] {
        if let Some(model) = value
            .get(key)
            .and_then(|item| item.as_str())
            .and_then(trim_history_text)
        {
            return Some(model);
        }
    }
    None
}

pub(in crate::ai_sessions) fn collect_opencode_history_sessions(
    min_updated_at_ms: Option<i64>,
) -> Vec<HistorySessionEntry> {
    let db_path = dirs::home_dir()
        .map(|home| {
            home.join(".local")
                .join("share")
                .join("opencode")
                .join("opencode.db")
        })
        .unwrap_or_default();
    let storage_roots = candidate_opencode_storage_paths()
        .into_iter()
        .filter_map(|paths| paths.sessions_root.parent().map(Path::to_path_buf))
        .collect::<Vec<_>>();

    let mut sessions =
        collect_opencode_history_sessions_from_sources(&db_path, &storage_roots, min_updated_at_ms);
    for session in &mut sessions {
        session.working_dir = canonicalize_to_string(&session.working_dir);
    }
    sessions
}

pub(in crate::ai_sessions) fn collect_opencode_history_sessions_from_sources(
    db_path: &Path,
    storage_roots: &[PathBuf],
    min_updated_at_ms: Option<i64>,
) -> Vec<HistorySessionEntry> {
    let mut by_session_id = HashMap::<String, HistorySessionEntry>::new();
    let mut higher_priority_ids = HashSet::<String>::new();

    if db_path.is_file() {
        if let Ok(conn) = Connection::open(db_path) {
            let v2_ids = collect_opencode_session_ids(&conn, "SELECT id FROM session_v2");
            let v2_sessions = collect_opencode_sessions_from_db_query(
                &conn,
                r#"
                SELECT s.id, s.title, s.directory, s.time_created, s.time_updated,
                        (SELECT COALESCE(
                                   json_extract(m.data, '$.data.model.id'),
                                   json_extract(m.data, '$.data.modelID'),
                                   json_extract(m.data, '$.model.id'),
                                   json_extract(m.data, '$.modelID')
                               )
                        FROM session_message m
                        WHERE m.session_id = s.id
                        ORDER BY m.time_created DESC
                        LIMIT 1) as model_id
                FROM session_v2 s
                WHERE s.time_archived IS NULL
                ORDER BY s.time_updated DESC
                "#,
                min_updated_at_ms,
            );
            insert_opencode_sessions_by_priority(&mut by_session_id, v2_sessions);
            higher_priority_ids.extend(v2_ids);

            let v1_ids = collect_opencode_session_ids(&conn, "SELECT id FROM session");
            let v1_sessions = collect_opencode_sessions_from_db_query(
                &conn,
                r#"
                SELECT s.id, s.title, s.directory, s.time_created, s.time_updated,
                       (SELECT json_extract(m.data, '$.modelID')
                        FROM message m
                        WHERE m.session_id = s.id
                        ORDER BY m.time_created DESC
                        LIMIT 1) as model_id
                FROM session s
                WHERE s.time_archived IS NULL
                ORDER BY s.time_updated DESC
                "#,
                min_updated_at_ms,
            );
            insert_opencode_sessions_by_priority(
                &mut by_session_id,
                v1_sessions
                    .into_iter()
                    .filter(|session| !higher_priority_ids.contains(&session.tool_session_id))
                    .collect(),
            );
            higher_priority_ids.extend(v1_ids);
        }
    }

    let mut json_sessions = Vec::new();
    for storage_root in storage_roots {
        json_sessions.extend(collect_opencode_sessions_from_storage_root(storage_root));
    }
    insert_opencode_sessions_by_priority(
        &mut by_session_id,
        dedupe_history_sessions(json_sessions)
            .into_iter()
            .filter(|session| !higher_priority_ids.contains(&session.tool_session_id))
            .filter(|session| {
                min_updated_at_ms
                    .map(|min| session.updated_at_ms >= min)
                    .unwrap_or(true)
            })
            .collect(),
    );

    dedupe_history_sessions(by_session_id.into_values().collect())
}

fn collect_opencode_session_ids(conn: &Connection, query: &str) -> HashSet<String> {
    let Ok(mut stmt) = conn.prepare(query) else {
        return HashSet::new();
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) else {
        return HashSet::new();
    };

    rows.flatten()
        .map(|session_id| session_id.trim().to_string())
        .filter(|session_id| !session_id.is_empty())
        .collect()
}

fn collect_opencode_sessions_from_db_query(
    conn: &Connection,
    query: &str,
    min_updated_at_ms: Option<i64>,
) -> Vec<HistorySessionEntry> {
    let Ok(mut stmt) = conn.prepare(query) else {
        return Vec::new();
    };

    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    }) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for row_result in rows.flatten() {
        let (session_id, title, directory, time_created, time_updated, model_id) = row_result;
        if let Some(min) = min_updated_at_ms {
            if time_updated < min {
                continue;
            }
        }

        let session_id = session_id.trim().to_string();
        if session_id.is_empty() {
            continue;
        }
        let working_dir = directory.trim().to_string();
        if working_dir.is_empty() {
            continue;
        }

        out.push(HistorySessionEntry {
            tool: "opencode".to_string(),
            tool_session_id: session_id,
            title: title.trim().to_string(),
            working_dir,
            model_name: model_id.filter(|m| !m.trim().is_empty()),
            created_at_ms: time_created,
            updated_at_ms: time_updated,
        });
    }

    out
}

fn collect_opencode_sessions_from_storage_root(storage_root: &Path) -> Vec<HistorySessionEntry> {
    let sessions_root = storage_root.join("session");
    if !sessions_root.is_dir() {
        return Vec::new();
    }
    let messages_root = storage_root.join("message");
    let project_worktree_by_id = read_opencode_project_worktree_map(&storage_root.join("project"));
    let mut out = Vec::new();
    let mut stack = vec![sessions_root];

    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".json"))
                .unwrap_or(false)
            {
                continue;
            }
            let Some(parsed) = read_opencode_history_file_from_source(
                &path,
                &messages_root,
                &project_worktree_by_id,
                false,
            ) else {
                continue;
            };
            out.push(parsed);
        }
    }

    out
}

fn insert_opencode_sessions_by_priority(
    by_session_id: &mut HashMap<String, HistorySessionEntry>,
    sessions: Vec<HistorySessionEntry>,
) {
    for mut session in sessions {
        let session_id = session.tool_session_id.trim().to_string();
        if session_id.is_empty() {
            continue;
        }
        session.tool_session_id = session_id.clone();
        by_session_id.entry(session_id).or_insert(session);
    }
}

pub(in crate::ai_sessions) fn read_opencode_project_worktree_map(
    projects_root: &Path,
) -> HashMap<String, String> {
    let mut out = HashMap::<String, String>::new();
    if !projects_root.is_dir() {
        return out;
    }

    let mut stack = vec![projects_root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".json"))
                .unwrap_or(false)
            {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(&content) else {
                continue;
            };
            let Some(project_id) = value
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Some(worktree) = value
                .get("worktree")
                .and_then(|v| v.as_str())
                .map(canonicalize_to_string)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            out.insert(project_id.to_string(), worktree);
        }
    }

    out
}

#[cfg(test)]
pub(in crate::ai_sessions) fn read_opencode_history_file(
    path: &Path,
    messages_root: &Path,
    project_worktree_by_id: &HashMap<String, String>,
) -> Option<HistorySessionEntry> {
    read_opencode_history_file_from_source(path, messages_root, project_worktree_by_id, true)
}

fn read_opencode_history_file_from_source(
    path: &Path,
    messages_root: &Path,
    project_worktree_by_id: &HashMap<String, String>,
    normalize_directory: bool,
) -> Option<HistorySessionEntry> {
    let content = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;
    let session_id = value
        .get("id")
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())?;
    let project_id = value
        .get("projectID")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let working_dir = value
        .get("directory")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if normalize_directory {
                canonicalize_to_string(value)
            } else {
                value.to_string()
            }
        })
        .or_else(|| {
            project_id.and_then(|project_id| project_worktree_by_id.get(project_id).cloned())
        })
        .unwrap_or_default();
    if working_dir.is_empty() {
        return None;
    }
    let modified_at_ms = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_to_epoch_millis)
        .unwrap_or(0);
    let created_at_ms = value
        .get("time")
        .and_then(|time| time.get("created"))
        .and_then(|v| v.as_i64())
        .unwrap_or(modified_at_ms);
    let updated_at_ms = value
        .get("time")
        .and_then(|time| time.get("updated"))
        .and_then(|v| v.as_i64())
        .unwrap_or(modified_at_ms.max(created_at_ms));
    let title = value
        .get("title")
        .and_then(value_as_text)
        .or_else(|| {
            value
                .get("slug")
                .and_then(|v| v.as_str())
                .and_then(trim_history_text)
        })
        .unwrap_or_else(|| fallback_history_title("opencode", &session_id));
    let model_name = read_opencode_model_name(messages_root.join(&session_id));

    Some(HistorySessionEntry {
        tool: "opencode".to_string(),
        tool_session_id: session_id,
        title,
        working_dir,
        model_name,
        created_at_ms,
        updated_at_ms,
    })
}

pub(in crate::ai_sessions) fn read_opencode_model_name(messages_dir: PathBuf) -> Option<String> {
    if !messages_dir.is_dir() {
        return None;
    }
    let mut files = Vec::<(PathBuf, i64)>::new();
    let Ok(entries) = fs::read_dir(messages_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let modified_at_ms = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(system_time_to_epoch_millis)
            .unwrap_or(0);
        files.push((path, modified_at_ms));
    }
    files.sort_by(|a, b| b.1.cmp(&a.1));

    for (path, _) in files.into_iter().take(20) {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let role = value.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if !role.eq_ignore_ascii_case("assistant") {
            continue;
        }
        if let Some(model_name) = value
            .get("modelID")
            .and_then(|v| v.as_str())
            .and_then(trim_history_text)
        {
            return Some(model_name);
        }
    }
    None
}

pub(in crate::ai_sessions) fn dedupe_history_sessions(
    items: Vec<HistorySessionEntry>,
) -> Vec<HistorySessionEntry> {
    let mut by_key = HashMap::<(String, String), HistorySessionEntry>::new();
    for item in items {
        let key = (item.tool.clone(), item.tool_session_id.clone());
        match by_key.get(&key) {
            Some(existing) if existing.updated_at_ms >= item.updated_at_ms => {}
            _ => {
                by_key.insert(key, item);
            }
        }
    }
    let mut out = by_key.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.updated_at_ms
            .cmp(&a.updated_at_ms)
            .then_with(|| a.tool.cmp(&b.tool))
    });
    out
}
