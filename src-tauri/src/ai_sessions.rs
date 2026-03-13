use crate::get_data_dir;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiSession {
    pub id: String,
    pub name: String,
    pub working_dir: String,
    pub model_type: String,
    pub tool_session_id: String,
    pub created_at: u64,
}

fn get_sessions_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join("ai_sessions.json"))
}

fn codex_resume_command(session_id: &str) -> String {
    format!("codex resume {}", shell_single_quote(session_id))
}

fn gemini_resume_command(session_id: &str) -> String {
    format!("gemini -r {}", shell_single_quote(session_id))
}

fn claude_resume_command(session_id: &str) -> String {
    format!("claude -r {}", shell_single_quote(session_id))
}

fn codex_new_command() -> String {
    "codex".to_string()
}

fn gemini_new_command() -> String {
    "gemini".to_string()
}

fn claude_new_command(session_id: &str) -> String {
    format!("claude --session-id {}", shell_single_quote(session_id))
}

fn opencode_new_command() -> String {
    "opencode".to_string()
}

fn configured_create_command(model_type: &str, session_id: &str) -> Option<String> {
    let key = model_type.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    let cfg = crate::config::get_config().ok()?;
    let configured = cfg
        .ai_model_launch_commands
        .as_ref()
        .and_then(|commands| commands.get(&key))
        .map(|cmd| cmd.trim().to_string())
        .filter(|cmd| !cmd.is_empty())?;
    let normalized = if key == "claude" && !configured.contains("{session_id}") {
        format!("{} --session-id {{session_id}}", configured)
    } else {
        configured
    };
    Some(normalized.replace("{session_id}", session_id))
}

pub fn get_ai_sessions() -> Result<Vec<AiSession>, String> {
    let path = get_sessions_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut sessions: Vec<AiSession> = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(sessions)
}

#[allow(dead_code)]
pub fn save_ai_session(session: AiSession) -> Result<(), String> {
    let mut sessions = get_ai_sessions()?;
    if let Some(pos) = sessions.iter().position(|s| s.id == session.id) {
        sessions[pos] = session;
    } else {
        sessions.push(session);
    }

    let path = get_sessions_path()?;
    let content = serde_json::to_string_pretty(&sessions).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(())
}

#[allow(dead_code)]
pub fn delete_ai_session(id: String) -> Result<(), String> {
    let mut sessions = get_ai_sessions()?;
    sessions.retain(|s| s.id != id);

    let path = get_sessions_path()?;
    let content = serde_json::to_string_pretty(&sessions).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(())
}

fn build_resume_command(model_type: &str, session_id: &str) -> Option<String> {
    let resume_id = session_id.trim();
    if resume_id.is_empty() {
        return None;
    }
    match model_type.to_lowercase().as_str() {
        "claude" => Some(claude_resume_command(resume_id)),
        "gemini" => Some(gemini_resume_command(resume_id)),
        "opencode" => Some(format!("opencode -s {}", shell_single_quote(resume_id))),
        "codex" => Some(codex_resume_command(resume_id)),
        _ => None,
    }
}

fn build_create_command(model_type: &str, session_id: Option<&str>) -> Option<String> {
    if let Some(configured) = configured_create_command(model_type, session_id.unwrap_or("")) {
        return Some(configured);
    }
    match model_type.to_lowercase().as_str() {
        "claude" => {
            let create_id = session_id?.trim();
            if create_id.is_empty() {
                None
            } else {
                Some(claude_new_command(create_id))
            }
        }
        "gemini" => Some(gemini_new_command()),
        "opencode" => Some(opencode_new_command()),
        "codex" => Some(codex_new_command()),
        _ => None,
    }
}

fn escape_applescript_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn resolve_terminal_app_name() -> String {
    let configured = crate::config::get_config()
        .ok()
        .and_then(|cfg| cfg.ai_terminal_app)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "终端".to_string());

    if configured == "终端" {
        "Terminal".to_string()
    } else {
        configured
    }
}

fn shell_single_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}

fn env_prefix(env: &HashMap<String, String>) -> String {
    let mut pairs = env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
        .into_iter()
        .map(|(k, v)| format!("{}={}", k, shell_single_quote(&v)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn run_native_terminal_command(
    working_dir: &str,
    command: &str,
    env: Option<&HashMap<String, String>>,
) -> Result<(), String> {
    let shell_cmd = if let Some(vars) = env.filter(|vars| !vars.is_empty()) {
        format!(
            "cd {} && env {} {}",
            shell_single_quote(working_dir),
            env_prefix(vars),
            command
        )
    } else {
        format!("cd {} && {}", shell_single_quote(working_dir), command)
    };

    let terminal_app = escape_applescript_string(&resolve_terminal_app_name());
    let script = format!(
        r#"tell application "{}"
            activate
            do script "{}"
        end tell"#,
        terminal_app,
        escape_applescript_string(&shell_cmd)
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    pub env: Option<HashMap<String, String>>,
}

pub fn launch_native_session_with_options(
    working_dir: &str,
    model_type: &str,
    session_id: &str,
    options: &LaunchOptions,
) -> Result<(), String> {
    let command = build_resume_command(model_type, session_id)
        .ok_or_else(|| "Unsupported model type for native session".to_string())?;
    run_native_terminal_command(working_dir, &command, options.env.as_ref())
}

#[allow(dead_code)]
pub fn launch_native_session(
    working_dir: &str,
    model_type: &str,
    session_id: &str,
) -> Result<(), String> {
    launch_native_session_with_options(working_dir, model_type, session_id, &LaunchOptions::default())
}

pub fn launch_native_session_for_create_with_options(
    working_dir: &str,
    model_type: &str,
    requested_session_id: Option<&str>,
    options: &LaunchOptions,
) -> Result<Option<String>, String> {
    let launch_started_at_ms = now_epoch_millis();
    let seed_session_id = build_create_seed_session_id(model_type, requested_session_id);
    let command = build_create_command(model_type, seed_session_id.as_deref())
        .ok_or_else(|| "Unsupported model type for native session".to_string())?;
    run_native_terminal_command(working_dir, &command, options.env.as_ref())?;
    Ok(resolve_native_session_id_after_create(
        model_type,
        working_dir,
        seed_session_id.as_deref(),
        launch_started_at_ms,
    ))
}

pub fn launch_native_session_for_create(
    working_dir: &str,
    model_type: &str,
    requested_session_id: Option<&str>,
) -> Result<Option<String>, String> {
    launch_native_session_for_create_with_options(
        working_dir,
        model_type,
        requested_session_id,
        &LaunchOptions::default(),
    )
}

#[allow(dead_code)]
pub fn create_native_session(
    name: String,
    working_dir: String,
    model_type: String,
    tool_session_id: String,
) -> Result<AiSession, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    let session = AiSession {
        id,
        name,
        working_dir: working_dir.clone(),
        model_type: model_type.clone(),
        tool_session_id: tool_session_id.clone(),
        created_at,
    };

    save_ai_session(session.clone())?;

    let _ = launch_native_session_for_create(&working_dir, &model_type, Some(&tool_session_id))?;

    Ok(session)
}

fn now_epoch_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn system_time_to_epoch_millis(ts: SystemTime) -> i64 {
    ts.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_rfc3339_millis(input: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(input)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

fn canonicalize_to_string(path: &str) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .to_string()
}

fn same_working_dir(left: &str, right: &str) -> bool {
    canonicalize_to_string(left) == canonicalize_to_string(right)
}

fn build_create_seed_session_id(model_type: &str, requested_session_id: Option<&str>) -> Option<String> {
    if model_type.eq_ignore_ascii_case("claude") {
        let requested = requested_session_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| id.to_string());
        return Some(requested.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()));
    }
    None
}

fn resolve_native_session_id_after_create(
    model_type: &str,
    working_dir: &str,
    seed_session_id: Option<&str>,
    launch_started_at_ms: i64,
) -> Option<String> {
    let max_attempts = 12;
    for attempt in 0..max_attempts {
        if let Some(id) =
            resolve_native_session_id_once(model_type, working_dir, seed_session_id, launch_started_at_ms)
        {
            return Some(id);
        }
        if attempt + 1 < max_attempts {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    None
}

fn resolve_native_session_id_once(
    model_type: &str,
    working_dir: &str,
    seed_session_id: Option<&str>,
    launch_started_at_ms: i64,
) -> Option<String> {
    match model_type.to_lowercase().as_str() {
        "claude" => seed_session_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| id.to_string()),
        "gemini" => resolve_gemini_session_id(working_dir, launch_started_at_ms),
        "codex" => resolve_codex_session_id(working_dir, launch_started_at_ms),
        "opencode" => resolve_opencode_session_id(working_dir, launch_started_at_ms),
        _ => None,
    }
}

fn resolve_codex_session_id(working_dir: &str, launch_started_at_ms: i64) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct CodexIndexEntry {
        id: String,
        #[serde(default)]
        updated_at: Option<String>,
    }

    let home = dirs::home_dir()?;
    let index_path = home.join(".codex").join("session_index.jsonl");
    let content = fs::read_to_string(index_path).ok()?;

    let mut entries: Vec<(String, i64)> = content
        .lines()
        .filter_map(|line| serde_json::from_str::<CodexIndexEntry>(line).ok())
        .map(|entry| {
            let updated = entry
                .updated_at
                .as_deref()
                .and_then(parse_rfc3339_millis)
                .unwrap_or(0);
            (entry.id, updated)
        })
        .collect();

    entries.sort_by(|a, b| b.1.cmp(&a.1));
    entries.retain(|(_, updated_at_ms)| *updated_at_ms + 15_000 >= launch_started_at_ms);
    if entries.is_empty() {
        return None;
    }

    let sessions_root = home.join(".codex").join("sessions");
    for (id, _) in entries.iter().take(20) {
        if let Some(path) = find_codex_session_file_for_id(&sessions_root, id) {
            if let Some(cwd) = read_codex_session_cwd(&path) {
                if same_working_dir(&cwd, working_dir) {
                    return Some(id.clone());
                }
            }
        }
    }

    entries.first().map(|entry| entry.0.clone())
}

fn find_codex_session_file_for_id(root: &Path, session_id: &str) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }
    let mut stack = vec![root.to_path_buf()];
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
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
            if name.contains(session_id) && name.ends_with(".jsonl") {
                return Some(path);
            }
        }
    }
    None
}

fn read_codex_session_cwd(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    let line_len = reader.read_line(&mut first_line).ok()?;
    if line_len == 0 {
        return None;
    }
    let value: Value = serde_json::from_str(first_line.trim()).ok()?;
    if value.get("type").and_then(|v| v.as_str()) != Some("session_meta") {
        return None;
    }
    value
        .get("payload")
        .and_then(|payload| payload.get("cwd"))
        .and_then(|cwd| cwd.as_str())
        .map(|cwd| cwd.to_string())
}

fn resolve_gemini_session_id(working_dir: &str, launch_started_at_ms: i64) -> Option<String> {
    let home = dirs::home_dir()?;
    let mut best: Option<(String, i64)> = None;

    for identifier in gemini_project_identifiers(working_dir) {
        let chats_dir = home.join(".gemini").join("tmp").join(identifier).join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(chats_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
            if !name.starts_with("session-") || !name.ends_with(".json") {
                continue;
            }
            let Some((session_id, updated_at_ms)) = read_gemini_chat_file(&path) else {
                continue;
            };
            if updated_at_ms + 15_000 < launch_started_at_ms {
                continue;
            }
            match &best {
                Some((_, best_updated_at_ms)) if *best_updated_at_ms >= updated_at_ms => {}
                _ => best = Some((session_id, updated_at_ms)),
            }
        }
    }

    best.map(|(session_id, _)| session_id)
}

fn gemini_project_identifiers(working_dir: &str) -> Vec<String> {
    let normalized_working_dir = canonicalize_to_string(working_dir);
    let mut identifiers = Vec::<String>::new();

    let Some(home) = dirs::home_dir() else {
        return identifiers;
    };
    let projects_path = home.join(".gemini").join("projects.json");
    if let Ok(content) = fs::read_to_string(projects_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&content) {
            if let Some(projects) = value.get("projects").and_then(|projects| projects.as_object()) {
                if let Some(identifier) = projects
                    .get(&normalized_working_dir)
                    .and_then(|value| value.as_str())
                {
                    identifiers.push(identifier.to_string());
                }
                for (project_path, identifier) in projects {
                    if same_working_dir(project_path, &normalized_working_dir) {
                        if let Some(identifier) = identifier.as_str() {
                            identifiers.push(identifier.to_string());
                        }
                    }
                }
            }
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(normalized_working_dir.as_bytes());
    identifiers.push(format!("{:x}", hasher.finalize()));

    dedupe_strings(identifiers)
}

fn read_gemini_chat_file(path: &Path) -> Option<(String, i64)> {
    let content = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;
    let session_id = value.get("sessionId").and_then(|v| v.as_str())?.to_string();
    let updated_at_ms = value
        .get("lastUpdated")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339_millis)
        .or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_epoch_millis)
        })?;
    Some((session_id, updated_at_ms))
}

fn resolve_opencode_session_id(working_dir: &str, launch_started_at_ms: i64) -> Option<String> {
    let home = dirs::home_dir()?;
    let messages_root = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("storage")
        .join("message");
    if !messages_root.is_dir() {
        return None;
    }

    let normalized_working_dir = canonicalize_to_string(working_dir);
    let mut sessions = Vec::<(PathBuf, i64)>::new();
    let Ok(entries) = fs::read_dir(messages_root) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let modified_ms = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(system_time_to_epoch_millis)
            .unwrap_or(0);
        sessions.push((path, modified_ms));
    }
    sessions.sort_by(|a, b| b.1.cmp(&a.1));

    let mut best: Option<(String, i64)> = None;
    for (session_dir, _) in sessions.into_iter().take(200) {
        let Some((session_id, created_at_ms, cwd)) = read_opencode_session_dir(&session_dir) else {
            continue;
        };
        if created_at_ms + 15_000 < launch_started_at_ms {
            continue;
        }
        let Some(cwd) = cwd else {
            continue;
        };
        if !same_working_dir(&cwd, &normalized_working_dir) {
            continue;
        }
        match &best {
            Some((_, best_created_at_ms)) if *best_created_at_ms >= created_at_ms => {}
            _ => best = Some((session_id, created_at_ms)),
        }
    }
    best.map(|(session_id, _)| session_id)
}

fn read_opencode_session_dir(session_dir: &Path) -> Option<(String, i64, Option<String>)> {
    let mut message_files = Vec::<(PathBuf, i64)>::new();
    let Ok(entries) = fs::read_dir(session_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
        if !name.starts_with("msg_") || !name.ends_with(".json") {
            continue;
        }
        let modified_ms = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(system_time_to_epoch_millis)
            .unwrap_or(0);
        message_files.push((path, modified_ms));
    }
    message_files.sort_by(|a, b| b.1.cmp(&a.1));

    let fallback_session_id = session_dir
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())?;

    let mut best: Option<(String, i64, Option<String>)> = None;
    for (path, _) in message_files.into_iter().take(10) {
        let content = fs::read_to_string(path).ok()?;
        let value: Value = serde_json::from_str(&content).ok()?;
        let session_id = value
            .get("sessionID")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .unwrap_or_else(|| fallback_session_id.clone());
        let created_at_ms = value
            .get("time")
            .and_then(|time| time.get("created"))
            .and_then(|created| created.as_i64())
            .or_else(|| value.get("updatedAt").and_then(|updated_at| updated_at.as_i64()))?;
        let cwd = value
            .get("path")
            .and_then(|path| path.get("cwd").or_else(|| path.get("root")))
            .and_then(|cwd| cwd.as_str())
            .map(|cwd| cwd.to_string());
        match &best {
            Some((_, best_created_at_ms, best_cwd))
                if *best_created_at_ms > created_at_ms
                    || (*best_created_at_ms == created_at_ms && best_cwd.is_some()) => {}
            _ => best = Some((session_id, created_at_ms, cwd)),
        }
    }
    best
}

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
