use super::aggregation::dedupe_history_sessions;
use super::cache::history_dependency;
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
use std::path::Path;

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
    if let Ok(content) = fs::read_to_string(&history_path) {
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
    let history_dependency = history_dependency(&history_path);

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
            let Some(session) = shared_history_file_cache().get_or_parse(
                &path,
                &history_dependency,
                || {
                    read_claude_project_file(
                        &path,
                        fallback_by_session.get(
                            path.file_stem()
                                .and_then(|stem| stem.to_str())
                                .unwrap_or_default(),
                        ),
                    )
                },
            ) else {
                continue;
            };
            out.push((*session).clone());
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
        is_main_agent: true,
    })
}
