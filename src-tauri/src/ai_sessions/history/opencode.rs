use super::aggregation::dedupe_history_sessions;
use super::text::{fallback_history_title, trim_history_text, value_as_text};
use crate::ai_sessions::{
    candidate_opencode_storage_paths, canonicalize_to_string, system_time_to_epoch_millis,
    HistorySessionEntry,
};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

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
                        LIMIT 1) as model_id,
                        s.parent_id
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
                        LIMIT 1) as model_id,
                       s.parent_id
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
            row.get::<_, Option<String>>(6).unwrap_or(None),
        ))
    }) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for row_result in rows.flatten() {
        let (session_id, title, directory, time_created, time_updated, model_id, parent_id) = row_result;
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

        let has_parent = parent_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .is_some();
        let title_trimmed = title.trim().to_string();
        let title_lower = title_trimmed.to_lowercase();
        let is_main_agent = !has_parent && !title_lower.contains("(@") && !title_lower.contains("subagent");

        out.push(HistorySessionEntry {
            tool: "opencode".to_string(),
            tool_session_id: session_id,
            title: title_trimmed,
            working_dir,
            model_name: model_id.filter(|m| !m.trim().is_empty()),
            created_at_ms: time_created,
            updated_at_ms: time_updated,
            is_main_agent,
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
    let parent_id = value
        .get("parentID")
        .or_else(|| value.get("parentId"))
        .or_else(|| value.get("parent_id"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let title_lower = title.to_lowercase();
    let is_main_agent = parent_id.is_none() && !title_lower.contains("(@") && !title_lower.contains("subagent");

    Some(HistorySessionEntry {
        tool: "opencode".to_string(),
        tool_session_id: session_id,
        title,
        working_dir,
        model_name,
        created_at_ms,
        updated_at_ms,
        is_main_agent,
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
