use super::aggregation::dedupe_history_sessions;
use super::stats::history_scan_due;
use super::text::{fallback_history_title, trim_history_text, value_as_text};
use crate::ai_sessions::{
    canonicalize_to_string, parse_rfc3339_millis, shared_history_file_cache,
    system_time_to_epoch_millis, HistorySessionEntry,
};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub(in crate::ai_sessions) fn antigravity_brain_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".gemini").join("antigravity-cli").join("brain"),
        home.join(".gemini").join("antigravity").join("brain"),
    ]
}

pub(in crate::ai_sessions) fn antigravity_conversations_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".gemini").join("antigravity-cli").join("conversations"),
        home.join(".gemini").join("antigravity").join("conversations"),
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
        let dependency = format!("{conversation_id}\u{1}{working_dir}");
        if let Some(session) = shared_history_file_cache().get_or_parse(
            &transcript,
            &dependency,
            || read_antigravity_history_file(&transcript, conversation_id, working_dir),
        ) {
            out.push((*session).clone());
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
    let mut fallback = None;
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
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name == "transcript_full.jsonl" {
                    return Some(path);
                }
                if name == "transcript.jsonl" && fallback.is_none() {
                    fallback = Some(path);
                }
            }
        }
    }
    fallback
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
        is_main_agent: true,
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
