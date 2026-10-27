use super::aggregation::dedupe_history_sessions;
use super::cache::history_dependency;
use super::stats::history_scan_due;
use super::text::{trim_history_text, value_as_text};
use crate::ai_sessions::{
    candidate_home_dirs, canonicalize_to_string, collect_codex_session_files,
    parse_rfc3339_millis, shared_history_file_cache, HistorySessionEntry,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

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

        let index_dependency = history_dependency(&index_path);
        for (path, modified_ms) in collect_codex_session_files(&sessions_root, usize::MAX) {
            if !history_scan_due(&path, min_updated_at_ms) {
                continue;
            }
            let dependency = format!("{index_dependency}:{modified_ms}");
            let Some(session) = shared_history_file_cache().get_or_parse(
                &path,
                &dependency,
                || read_codex_history_session_file(&path, &titles, &updated_at_map, modified_ms),
            ) else {
                continue;
            };
            out.push((*session).clone());
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
        is_main_agent: true,
    })
}
