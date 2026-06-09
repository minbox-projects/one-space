fn dedupe_strings(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::new();
    for item in items {
        if item.is_empty() || !seen.insert(item.clone()) {
            continue;
        }
        out.push(item);
    }
    out
}

fn trim_history_text(input: &str) -> Option<String> {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim();
    if trimmed.is_empty() {
        return None;
    }
    let clipped: String = trimmed.chars().take(140).collect();
    Some(clipped)
}

fn history_scan_due(path: &Path, min_updated_at_ms: Option<i64>) -> bool {
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

fn value_as_text(value: &Value) -> Option<String> {
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

fn fallback_history_title(tool: &str, session_id: &str) -> String {
    let suffix: String = session_id.chars().take(8).collect();
    format!("{} {}", tool.to_uppercase(), suffix)
}

fn collect_codex_history_sessions(min_updated_at_ms: Option<i64>) -> Vec<HistorySessionEntry> {
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

fn read_codex_history_session_file(
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

fn collect_claude_history_sessions(min_updated_at_ms: Option<i64>) -> Vec<HistorySessionEntry> {
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

fn read_claude_project_file(
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

fn collect_gemini_history_sessions(min_updated_at_ms: Option<i64>) -> Vec<HistorySessionEntry> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let tmp_root = home.join(".gemini").join("tmp");
    if !tmp_root.is_dir() {
        return Vec::new();
    }

    let project_map = gemini_identifier_path_map();
    let mut out = Vec::new();
    let mut stack = vec![tmp_root];
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
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !name.starts_with("session-") || !name.ends_with(".json") {
                continue;
            }
            if !history_scan_due(&path, min_updated_at_ms) {
                continue;
            }
            let Some(session) = read_gemini_history_file(&path, &project_map) else {
                continue;
            };
            out.push(session);
        }
    }

    dedupe_history_sessions(out)
}

fn gemini_identifier_path_map() -> HashMap<String, String> {
    let Some(home) = dirs::home_dir() else {
        return HashMap::new();
    };
    let projects_path = home.join(".gemini").join("projects.json");
    let Ok(content) = fs::read_to_string(projects_path) else {
        return HashMap::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return HashMap::new();
    };
    let Some(projects) = value
        .get("projects")
        .and_then(|projects| projects.as_object())
    else {
        return HashMap::new();
    };

    let mut out = HashMap::new();
    for (path, identifier) in projects {
        let Some(identifier) = identifier.as_str() else {
            continue;
        };
        let normalized_path = canonicalize_to_string(path);
        if normalized_path.is_empty() {
            continue;
        }
        out.insert(identifier.to_string(), normalized_path.clone());
        let mut hasher = Sha256::new();
        hasher.update(normalized_path.as_bytes());
        out.insert(format!("{:x}", hasher.finalize()), normalized_path);
    }
    out
}

fn read_gemini_history_file(
    path: &Path,
    project_map: &HashMap<String, String>,
) -> Option<HistorySessionEntry> {
    let content = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;
    let session_id = value
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())?;
    let project_hash = value
        .get("projectHash")
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    let dir_key = path
        .parent()
        .and_then(|parent| parent.parent())
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let working_dir = project_map
        .get(&project_hash)
        .or_else(|| project_map.get(&dir_key))
        .cloned()
        .unwrap_or_default();
    if working_dir.is_empty() {
        return None;
    }

    let created_at_ms = value
        .get("startTime")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339_millis)
        .unwrap_or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.created().ok())
                .map(system_time_to_epoch_millis)
                .unwrap_or(0)
        });
    let updated_at_ms = value
        .get("lastUpdated")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339_millis)
        .unwrap_or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_epoch_millis)
                .unwrap_or(created_at_ms)
        });

    let mut title = None::<String>;
    let mut model_name = None::<String>;
    if let Some(messages) = value
        .get("messages")
        .and_then(|messages| messages.as_array())
    {
        for message in messages {
            let msg_type = message.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if title.is_none() && msg_type.eq_ignore_ascii_case("user") {
                title = message.get("content").and_then(value_as_text);
            }
            if !msg_type.eq_ignore_ascii_case("user") {
                model_name = message
                    .get("model")
                    .and_then(|v| v.as_str())
                    .and_then(trim_history_text)
                    .or(model_name);
            }
        }
    }

    Some(HistorySessionEntry {
        tool: "gemini".to_string(),
        tool_session_id: session_id.clone(),
        title: title.unwrap_or_else(|| fallback_history_title("gemini", &session_id)),
        working_dir,
        model_name,
        created_at_ms,
        updated_at_ms,
    })
}

fn collect_opencode_history_sessions(min_updated_at_ms: Option<i64>) -> Vec<HistorySessionEntry> {
    let mut out = Vec::new();

    // Try to read from SQLite database first (opencode 1.2+)
    if let Some(sessions) = collect_opencode_sessions_from_db(min_updated_at_ms) {
        return sessions;
    }

    // Fallback to file-based storage (opencode 1.1.x)
    for storage_paths in candidate_opencode_storage_paths() {
        if !storage_paths.sessions_root.is_dir() {
            continue;
        }
        let project_worktree_by_id =
            read_opencode_project_worktree_map(&storage_paths.projects_root);

        let mut stack = vec![storage_paths.sessions_root.clone()];
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
                let Some(parsed) = read_opencode_history_file(
                    &path,
                    &storage_paths.messages_root,
                    &project_worktree_by_id,
                ) else {
                    continue;
                };
                if let Some(min) = min_updated_at_ms {
                    if parsed.updated_at_ms < min {
                        continue;
                    }
                }
                out.push(parsed);
            }
        }
    }

    dedupe_history_sessions(out)
}

fn collect_opencode_sessions_from_db(
    min_updated_at_ms: Option<i64>,
) -> Option<Vec<HistorySessionEntry>> {
    let db_path = dirs::home_dir()?
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");

    if !db_path.exists() {
        return None;
    }

    let conn = Connection::open(&db_path).ok()?;

    let mut stmt = conn
        .prepare(
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
        )
        .ok()?;

    let mut out = Vec::new();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .ok()?;

    for row_result in rows.flatten() {
        let (session_id, title, directory, time_created, time_updated, model_id) = row_result;

        // Filter by min_updated_at_ms if specified
        if let Some(min) = min_updated_at_ms {
            if time_updated < min {
                continue;
            }
        }

        let working_dir = canonicalize_to_string(&directory);
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

    Some(dedupe_history_sessions(out))
}

fn read_opencode_project_worktree_map(projects_root: &Path) -> HashMap<String, String> {
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

fn read_opencode_history_file(
    path: &Path,
    messages_root: &Path,
    project_worktree_by_id: &HashMap<String, String>,
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
        .map(canonicalize_to_string)
        .filter(|value| !value.is_empty())
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

fn read_opencode_model_name(messages_dir: PathBuf) -> Option<String> {
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

fn dedupe_history_sessions(items: Vec<HistorySessionEntry>) -> Vec<HistorySessionEntry> {
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
