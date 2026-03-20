use crate::get_data_dir;
use chrono::DateTime;
use rusqlite::Connection;
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HistorySessionEntry {
    pub tool: String,
    pub tool_session_id: String,
    pub title: String,
    pub working_dir: String,
    pub model_name: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub fn collect_history_sessions_for_tool(
    tool: &str,
    min_updated_at_ms: Option<i64>,
) -> Result<Vec<HistorySessionEntry>, String> {
    let normalized_tool = tool.trim().to_lowercase();
    let sessions = match normalized_tool.as_str() {
        "claude" => collect_claude_history_sessions(min_updated_at_ms),
        "codex" => collect_codex_history_sessions(min_updated_at_ms),
        "gemini" => collect_gemini_history_sessions(min_updated_at_ms),
        "opencode" => collect_opencode_history_sessions(min_updated_at_ms),
        other => return Err(format!("unsupported history tool: {}", other)),
    };
    Ok(sessions)
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

fn command_uses_resume_semantics(model_type: &str, command: &str) -> bool {
    let tokens = command
        .split_whitespace()
        .map(|token| token.trim_matches(|c| c == '"' || c == '\'').to_lowercase())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return false;
    }
    let has_flag = |flag: &str| {
        let equals_variant = format!("{flag}=");
        tokens
            .iter()
            .any(|token| token == flag || token.starts_with(&equals_variant))
    };
    match model_type {
        "claude" | "gemini" => has_flag("-r") || has_flag("--resume"),
        "codex" => tokens.iter().any(|token| token == "resume"),
        "opencode" => has_flag("-s") || has_flag("--session"),
        _ => false,
    }
}

fn validate_create_command(model_type: &str, command: &str) -> Result<(), String> {
    if command_uses_resume_semantics(model_type, command) {
        return Err(format!(
            "Configured create command for {} contains resume semantics",
            model_type
        ));
    }
    Ok(())
}

fn configured_create_command(model_type: &str, session_id: &str) -> Result<Option<String>, String> {
    let key = model_type.trim().to_lowercase();
    if key.is_empty() {
        return Ok(None);
    }
    let Some(cfg) = crate::config::get_config().ok() else {
        return Ok(None);
    };
    let Some(configured) = cfg
        .ai_model_launch_commands
        .as_ref()
        .and_then(|commands| commands.get(&key))
        .map(|cmd| cmd.trim().to_string())
        .filter(|cmd| !cmd.is_empty())
    else {
        return Ok(None);
    };
    let normalized = if key == "claude" && !configured.contains("{session_id}") {
        format!("{} --session-id {{session_id}}", configured)
    } else {
        configured
    };
    validate_create_command(&key, &normalized)?;
    Ok(Some(normalized.replace("{session_id}", session_id)))
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

fn build_create_command(model_type: &str, session_id: Option<&str>) -> Result<String, String> {
    if let Some(configured) = configured_create_command(model_type, session_id.unwrap_or(""))? {
        return Ok(configured);
    }
    match model_type.to_lowercase().as_str() {
        "claude" => {
            let Some(raw_id) = session_id else {
                return Err("Claude create requires session_id".to_string());
            };
            let create_id = raw_id.trim();
            if create_id.is_empty() {
                Err("Claude create requires session_id".to_string())
            } else {
                Ok(claude_new_command(create_id))
            }
        }
        "gemini" => Ok(gemini_new_command()),
        "opencode" => Ok(opencode_new_command()),
        "codex" => Ok(codex_new_command()),
        _ => Err("Unsupported model type for native session".to_string()),
    }
}

fn escape_applescript_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn clean_terminal_app_name(app_name: &str) -> String {
    let trimmed = app_name.trim();
    if trimmed.to_lowercase().ends_with(".app") {
        trimmed[..trimmed.len() - 4].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_terminal_app_key(app_name: &str) -> String {
    clean_terminal_app_name(app_name).to_lowercase()
}

fn resolve_terminal_app_name() -> String {
    let configured = crate::config::get_config()
        .ok()
        .and_then(|cfg| cfg.ai_terminal_app)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "终端".to_string());

    let cleaned = clean_terminal_app_name(&configured);
    let key = normalize_terminal_app_key(&cleaned);
    if key == "terminal" || cleaned == "终端" {
        "Terminal".to_string()
    } else {
        cleaned
    }
}

fn shell_single_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}

pub fn normalize_working_dir_for_terminal(working_dir: &str) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let raw = working_dir.trim();
    let candidate = if raw.is_empty() {
        home
    } else if raw == "~" {
        home
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else {
        let parsed = PathBuf::from(raw);
        if parsed.is_absolute() {
            parsed
        } else {
            home.join(parsed)
        }
    };

    fs::canonicalize(&candidate)
        .unwrap_or(candidate)
        .to_string_lossy()
        .to_string()
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

fn build_shell_command(
    resolved_working_dir: &str,
    command: &str,
    env: Option<&HashMap<String, String>>,
) -> String {
    if let Some(vars) = env.filter(|vars| !vars.is_empty()) {
        format!(
            "cd {} && env {} {}",
            shell_single_quote(resolved_working_dir),
            env_prefix(vars),
            command
        )
    } else {
        format!(
            "cd {} && {}",
            shell_single_quote(resolved_working_dir),
            command
        )
    }
}

fn build_standard_terminal_applescript(terminal_app: &str, shell_cmd: &str) -> String {
    let terminal_app = escape_applescript_string(terminal_app);
    format!(
        r#"tell application "{}"
            do script "{}"
            activate
        end tell"#,
        terminal_app,
        escape_applescript_string(&shell_cmd)
    )
}

fn build_ghostty_terminal_applescript(
    terminal_app: &str,
    resolved_working_dir: &str,
    shell_cmd: &str,
) -> String {
    let terminal_app = escape_applescript_string(terminal_app);
    let resolved_working_dir = escape_applescript_string(resolved_working_dir);
    let shell_cmd = escape_applescript_string(shell_cmd);
    format!(
        r#"tell application "{}"
            activate
            set launch_config to new surface configuration
            set initial working directory of launch_config to "{}"
            set initial input of launch_config to "{}" & linefeed
            new window with configuration launch_config
            activate
        end tell"#,
        terminal_app, resolved_working_dir, shell_cmd
    )
}

fn build_native_terminal_applescript(
    terminal_app: &str,
    resolved_working_dir: &str,
    shell_cmd: &str,
) -> String {
    if normalize_terminal_app_key(terminal_app) == "ghostty" {
        build_ghostty_terminal_applescript(terminal_app, resolved_working_dir, shell_cmd)
    } else {
        build_standard_terminal_applescript(terminal_app, shell_cmd)
    }
}

fn execute_applescript(script: &str) -> Result<(), String> {
    Command::new("osascript")
        .arg("-e")
        .arg(script)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn run_native_terminal_command_for_app_with_executor<F>(
    terminal_app: &str,
    working_dir: &str,
    command: &str,
    env: Option<&HashMap<String, String>>,
    execute_script: F,
) -> Result<(), String>
where
    F: FnOnce(String) -> Result<(), String>,
{
    let resolved_working_dir = normalize_working_dir_for_terminal(working_dir);
    let shell_cmd = build_shell_command(&resolved_working_dir, command, env);
    let script = build_native_terminal_applescript(terminal_app, &resolved_working_dir, &shell_cmd);
    execute_script(script)
}

fn run_native_terminal_command(
    working_dir: &str,
    command: &str,
    env: Option<&HashMap<String, String>>,
) -> Result<(), String> {
    let terminal_app = resolve_terminal_app_name();
    run_native_terminal_command_for_app_with_executor(
        &terminal_app,
        working_dir,
        command,
        env,
        |script| execute_applescript(&script),
    )
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
    launch_native_session_with_options(
        working_dir,
        model_type,
        session_id,
        &LaunchOptions::default(),
    )
}

pub fn launch_native_session_for_create_with_options(
    working_dir: &str,
    model_type: &str,
    requested_session_id: Option<&str>,
    options: &LaunchOptions,
) -> Result<Option<String>, String> {
    let launch_started_at_ms = now_epoch_millis();
    let seed_session_id = build_create_seed_session_id(model_type, requested_session_id);
    let command = build_create_command(model_type, seed_session_id.as_deref())?;
    run_native_terminal_command(working_dir, &command, options.env.as_ref())?;
    Ok(resolve_native_session_id_after_create(
        model_type,
        working_dir,
        seed_session_id.as_deref(),
        launch_started_at_ms,
        options.env.as_ref(),
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
    normalize_working_dir_for_terminal(path)
}

fn same_working_dir(left: &str, right: &str) -> bool {
    canonicalize_to_string(left) == canonicalize_to_string(right)
}

fn candidate_home_dirs(env: Option<&HashMap<String, String>>) -> Vec<PathBuf> {
    let mut homes = Vec::<PathBuf>::new();
    if let Some(env_home) = env
        .and_then(|vars| vars.get("HOME"))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        homes.push(PathBuf::from(env_home));
    }
    if let Some(system_home) = dirs::home_dir() {
        homes.push(system_home);
    }
    let mut deduped = Vec::<PathBuf>::new();
    let mut seen = HashSet::<String>::new();
    for home in homes {
        let key = fs::canonicalize(&home)
            .unwrap_or_else(|_| home.clone())
            .to_string_lossy()
            .to_string();
        if seen.insert(key) {
            deduped.push(home);
        }
    }
    deduped
}

fn build_create_seed_session_id(
    model_type: &str,
    requested_session_id: Option<&str>,
) -> Option<String> {
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
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    // Gemini and Opencode start slowly - allow more attempts (15 seconds)
    let max_attempts = if model_type.eq_ignore_ascii_case("gemini")
        || model_type.eq_ignore_ascii_case("opencode")
    {
        30
    } else {
        12
    };

    for attempt in 0..max_attempts {
        if let Some(id) = resolve_native_session_id_once(
            model_type,
            working_dir,
            seed_session_id,
            launch_started_at_ms,
            env,
        ) {
            return Some(id);
        }
        if attempt + 1 < max_attempts {
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    // For Gemini/Opencode, the session is already running even if we couldn't detect the ID
    // Return None to indicate "unbound" status rather than an error
    // The session will be bound later via pending_bind mechanism
    None
}

fn resolve_native_session_id_once(
    model_type: &str,
    working_dir: &str,
    seed_session_id: Option<&str>,
    launch_started_at_ms: i64,
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    match model_type.to_lowercase().as_str() {
        "claude" => resolve_claude_session_id(working_dir, launch_started_at_ms).or_else(|| {
            seed_session_id
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(String::from)
        }),
        "gemini" => resolve_gemini_session_id(working_dir, launch_started_at_ms),
        "codex" => resolve_codex_session_id(working_dir, launch_started_at_ms, env),
        "opencode" => resolve_opencode_session_id(working_dir, launch_started_at_ms),
        _ => None,
    }
}

fn resolve_codex_session_id_at_home(
    home: &Path,
    working_dir: &str,
    launch_started_at_ms: Option<i64>,
    max_scan: usize,
) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct CodexIndexEntry {
        id: String,
        #[serde(default)]
        updated_at: Option<String>,
    }

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
    if let Some(launch_started_at_ms) = launch_started_at_ms {
        entries.retain(|(_, updated_at_ms)| *updated_at_ms + 15_000 >= launch_started_at_ms);
    }
    if entries.is_empty() {
        return None;
    }

    let sessions_root = home.join(".codex").join("sessions");
    for (id, _) in entries.iter().take(max_scan) {
        if let Some(path) = find_codex_session_file_for_id(&sessions_root, id) {
            if let Some(cwd) = read_codex_session_cwd(&path) {
                if same_working_dir(&cwd, working_dir) {
                    return Some(id.clone());
                }
            }
        }
    }

    fallback_codex_session_id_by_scan(
        &sessions_root,
        working_dir,
        launch_started_at_ms,
        max_scan * 8,
    )
}

fn resolve_codex_session_id(
    working_dir: &str,
    launch_started_at_ms: i64,
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    for home in candidate_home_dirs(env) {
        if let Some(id) =
            resolve_codex_session_id_at_home(&home, working_dir, Some(launch_started_at_ms), 20)
        {
            return Some(id);
        }
    }
    None
}

fn resolve_codex_session_id_for_existing(
    working_dir: &str,
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    for home in candidate_home_dirs(env) {
        if let Some(id) = resolve_codex_session_id_at_home(&home, working_dir, None, 80) {
            return Some(id);
        }
    }
    None
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
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
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

fn collect_codex_session_files(root: &Path, limit: usize) -> Vec<(PathBuf, i64)> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::<(PathBuf, i64)>::new();
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
            let modified_ms = fs::metadata(&path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_epoch_millis)
                .unwrap_or(0);
            files.push((path, modified_ms));
        }
    }
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files.truncate(limit);
    files
}

fn read_codex_session_meta(path: &Path) -> Option<(String, String, i64)> {
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
    let payload = value.get("payload")?;
    let id = payload.get("id").and_then(|v| v.as_str())?.to_string();
    let cwd = payload.get("cwd").and_then(|v| v.as_str())?.to_string();
    let timestamp_ms = payload
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339_millis)
        .or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_epoch_millis)
        })?;
    Some((id, cwd, timestamp_ms))
}

fn fallback_codex_session_id_by_scan(
    sessions_root: &Path,
    working_dir: &str,
    launch_started_at_ms: Option<i64>,
    max_scan: usize,
) -> Option<String> {
    let normalized_working_dir = canonicalize_to_string(working_dir);
    let mut best: Option<(String, i64)> = None;
    for (path, _) in collect_codex_session_files(sessions_root, max_scan) {
        let Some((id, cwd, ts_ms)) = read_codex_session_meta(&path) else {
            continue;
        };
        if let Some(launch_started_at_ms) = launch_started_at_ms {
            if ts_ms + 15_000 < launch_started_at_ms {
                continue;
            }
        }
        if !same_working_dir(&cwd, &normalized_working_dir) {
            continue;
        }
        match &best {
            Some((_, best_ts_ms)) if *best_ts_ms >= ts_ms => {}
            _ => best = Some((id, ts_ms)),
        }
    }
    best.map(|(id, _)| id)
}

const GEMINI_BIND_WINDOW_MS: i64 = 15 * 60 * 1000;
const GEMINI_CREATE_GRACE_MS: i64 = 15_000;

#[derive(Debug, Clone)]
struct GeminiSessionCandidate {
    session_id: String,
    start_at_ms: i64,
    updated_at_ms: i64,
}

fn select_gemini_session_for_create(
    candidates: &[GeminiSessionCandidate],
    launch_started_at_ms: i64,
) -> Option<String> {
    let mut best_near_start: Option<(String, i64, i64)> = None;
    let mut best_recent_update: Option<(String, i64)> = None;

    for candidate in candidates {
        if candidate.updated_at_ms + GEMINI_CREATE_GRACE_MS < launch_started_at_ms {
            continue;
        }
        match &best_recent_update {
            Some((_, best_updated_at_ms)) if *best_updated_at_ms >= candidate.updated_at_ms => {}
            _ => {
                best_recent_update = Some((candidate.session_id.clone(), candidate.updated_at_ms));
            }
        }

        if candidate.start_at_ms + GEMINI_CREATE_GRACE_MS < launch_started_at_ms {
            continue;
        }
        let diff_ms = (candidate.start_at_ms - launch_started_at_ms).abs();
        match &best_near_start {
            Some((_, best_diff_ms, best_updated_at_ms))
                if *best_diff_ms < diff_ms
                    || (*best_diff_ms == diff_ms
                        && *best_updated_at_ms >= candidate.updated_at_ms) => {}
            _ => {
                best_near_start = Some((
                    candidate.session_id.clone(),
                    diff_ms,
                    candidate.updated_at_ms,
                ))
            }
        }
    }

    best_near_start
        .map(|(session_id, _, _)| session_id)
        .or_else(|| best_recent_update.map(|(session_id, _)| session_id))
}

fn select_gemini_session_for_existing(
    candidates: &[GeminiSessionCandidate],
    created_at_ms: Option<i64>,
) -> Option<String> {
    if let Some(created_at_ms) = created_at_ms {
        let mut best_near_start: Option<(String, i64, i64)> = None;

        for candidate in candidates {
            let start_diff_ms = (candidate.start_at_ms - created_at_ms).abs();
            if start_diff_ms <= GEMINI_BIND_WINDOW_MS {
                match &best_near_start {
                    Some((_, best_diff_ms, best_updated_at_ms))
                        if *best_diff_ms < start_diff_ms
                            || (*best_diff_ms == start_diff_ms
                                && *best_updated_at_ms >= candidate.updated_at_ms) => {}
                    _ => {
                        best_near_start = Some((
                            candidate.session_id.clone(),
                            start_diff_ms,
                            candidate.updated_at_ms,
                        ))
                    }
                }
            }
        }

        return best_near_start.map(|(session_id, _, _)| session_id);
    }

    candidates
        .iter()
        .max_by_key(|candidate| candidate.updated_at_ms)
        .map(|candidate| candidate.session_id.clone())
}

fn collect_gemini_session_candidates(
    working_dir: &str,
    exclude_ids: Option<&HashSet<String>>,
) -> Vec<GeminiSessionCandidate> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut candidates = Vec::<GeminiSessionCandidate>::new();

    for identifier in gemini_project_identifiers(working_dir) {
        let chats_dir = home
            .join(".gemini")
            .join("tmp")
            .join(identifier)
            .join("chats");
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
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !name.starts_with("session-") || !name.ends_with(".json") {
                continue;
            }
            let Some(candidate) = read_gemini_chat_file(&path) else {
                continue;
            };
            if exclude_ids
                .map(|ids| ids.contains(&candidate.session_id))
                .unwrap_or(false)
            {
                continue;
            }
            candidates.push(candidate);
        }
    }

    candidates
}

fn resolve_gemini_session_id(working_dir: &str, launch_started_at_ms: i64) -> Option<String> {
    let candidates = collect_gemini_session_candidates(working_dir, None);
    select_gemini_session_for_create(&candidates, launch_started_at_ms)
}

fn resolve_gemini_session_id_for_existing(
    working_dir: &str,
    created_at_ms: Option<i64>,
    exclude_ids: Option<&HashSet<String>>,
) -> Option<String> {
    let candidates = collect_gemini_session_candidates(working_dir, exclude_ids);
    select_gemini_session_for_existing(&candidates, created_at_ms)
}

fn resolve_gemini_session_id_for_pending_bind(
    working_dir: &str,
    created_at_ms: Option<i64>,
    exclude_ids: Option<&HashSet<String>>,
) -> Option<String> {
    let created_at_ms = created_at_ms?;
    let candidates = collect_gemini_session_candidates(working_dir, exclude_ids);
    select_gemini_session_for_create(&candidates, created_at_ms)
}

fn resolve_claude_session_id(working_dir: &str, launch_started_at_ms: i64) -> Option<String> {
    let home = dirs::home_dir()?;
    let history_path = home.join(".claude").join("history.jsonl");

    let content = fs::read_to_string(history_path).ok()?;

    let mut candidates: Vec<(String, i64, String)> = content
        .lines()
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            let session_id = value.get("sessionId")?.as_str()?.to_string();
            let timestamp = value.get("timestamp")?.as_i64()?;
            let project = value.get("project")?.as_str()?.to_string();
            Some((session_id, timestamp, project))
        })
        .collect();

    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let normalized_working_dir = canonicalize_to_string(working_dir);
    for (session_id, timestamp, project) in candidates.iter().take(100) {
        if *timestamp < launch_started_at_ms - 15000 || *timestamp > launch_started_at_ms + 15000 {
            continue;
        }
        let normalized_project = canonicalize_to_string(project);
        if same_working_dir(&normalized_project, &normalized_working_dir) {
            return Some(session_id.clone());
        }
    }

    None
}

fn resolve_claude_session_id_for_existing(
    working_dir: &str,
    created_at_ms: Option<i64>,
    exclude_ids: Option<&HashSet<String>>,
) -> Option<String> {
    let home = dirs::home_dir()?;
    let history_path = home.join(".claude").join("history.jsonl");

    let content = fs::read_to_string(history_path).ok()?;

    let mut candidates: Vec<(String, i64, String)> = content
        .lines()
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            let session_id = value.get("sessionId")?.as_str()?.to_string();
            let timestamp = value.get("timestamp")?.as_i64()?;
            let project = value.get("project")?.as_str()?.to_string();
            Some((session_id, timestamp, project))
        })
        .collect();

    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let normalized_working_dir = canonicalize_to_string(working_dir);
    for (session_id, timestamp, project) in candidates.iter().take(200) {
        if let Some(exclude) = exclude_ids {
            if exclude.contains(session_id) {
                continue;
            }
        }

        let normalized_project = canonicalize_to_string(project);
        if !same_working_dir(&normalized_project, &normalized_working_dir) {
            continue;
        }

        if let Some(created_at) = created_at_ms {
            if (*timestamp - created_at).abs() > 15000 {
                continue;
            }
        }

        return Some(session_id.clone());
    }

    None
}

fn gemini_project_identifiers(working_dir: &str) -> Vec<String> {
    let normalized_working_dir = canonicalize_to_string(working_dir);
    let mut identifiers = Vec::<String>::new();

    let mut check_dirs = Vec::new();
    let mut current = PathBuf::from(&normalized_working_dir);
    loop {
        check_dirs.push(current.to_string_lossy().to_string());
        if !current.pop() {
            break;
        }
    }

    let Some(home) = dirs::home_dir() else {
        return identifiers;
    };
    let projects_path = home.join(".gemini").join("projects.json");
    if let Ok(content) = fs::read_to_string(projects_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&content) {
            if let Some(projects) = value
                .get("projects")
                .and_then(|projects| projects.as_object())
            {
                for dir in &check_dirs {
                    if let Some(identifier) = projects.get(dir).and_then(|value| value.as_str()) {
                        identifiers.push(identifier.to_string());
                    }
                    for (project_path, identifier) in projects {
                        if same_working_dir(project_path, dir) {
                            if let Some(identifier) = identifier.as_str() {
                                identifiers.push(identifier.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(normalized_working_dir.as_bytes());
    identifiers.push(format!("{:x}", hasher.finalize()));

    // 也为所有的父目录计算后备的 hash
    for dir in &check_dirs {
        let mut h = Sha256::new();
        h.update(dir.as_bytes());
        identifiers.push(format!("{:x}", h.finalize()));
    }

    dedupe_strings(identifiers)
}

fn read_gemini_chat_file(path: &Path) -> Option<GeminiSessionCandidate> {
    let content = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;
    let session_id = value.get("sessionId").and_then(|v| v.as_str())?.to_string();
    let start_at_ms = value
        .get("startTime")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339_millis)
        .or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.created().ok())
                .map(system_time_to_epoch_millis)
        })
        .or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_epoch_millis)
        })?;
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
    Some(GeminiSessionCandidate {
        session_id,
        start_at_ms,
        updated_at_ms,
    })
}

#[derive(Debug, Clone)]
struct OpencodeStoragePaths {
    sessions_root: PathBuf,
    messages_root: PathBuf,
    projects_root: PathBuf,
}

fn candidate_opencode_storage_paths() -> Vec<OpencodeStoragePaths> {
    let mut roots = Vec::<PathBuf>::new();
    if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty())
    {
        roots.push(PathBuf::from(xdg_data_home).join("opencode"));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local").join("share").join("opencode"));
    }

    let mut out = Vec::<OpencodeStoragePaths>::new();
    let mut seen = HashSet::<String>::new();
    for root in roots {
        let storage_root = if root
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "storage")
            .unwrap_or(false)
        {
            root.clone()
        } else {
            root.join("storage")
        };
        let key = fs::canonicalize(&storage_root)
            .unwrap_or_else(|_| storage_root.clone())
            .to_string_lossy()
            .to_string();
        if !seen.insert(key) {
            continue;
        }
        let candidate = OpencodeStoragePaths {
            sessions_root: storage_root.join("session"),
            messages_root: storage_root.join("message"),
            projects_root: storage_root.join("project"),
        };
        if candidate.sessions_root.is_dir()
            || candidate.messages_root.is_dir()
            || candidate.projects_root.is_dir()
        {
            out.push(candidate);
        }
    }

    out
}

fn select_opencode_session_id_from_messages_root(
    messages_root: &Path,
    working_dir: &str,
    launch_started_at_ms: Option<i64>,
    max_scan: usize,
) -> Option<(String, i64)> {
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
    for (session_dir, _) in sessions.into_iter().take(max_scan) {
        let Some((session_id, created_at_ms, cwd)) = read_opencode_session_dir(&session_dir) else {
            continue;
        };
        if let Some(launch_started_at_ms) = launch_started_at_ms {
            if created_at_ms + 15_000 < launch_started_at_ms {
                continue;
            }
        }
        if let Some(cwd) = cwd {
            if same_working_dir(&cwd, &normalized_working_dir) {
                match &best {
                    Some((_, best_created_at_ms)) if *best_created_at_ms >= created_at_ms => {}
                    _ => best = Some((session_id, created_at_ms)),
                }
                continue;
            }
        }
        // Fallback: check if the session directory name matches the working directory
        // This handles cases where opencode creates session directories named after the project
        if let Some(session_dir_name) = session_dir.file_name().and_then(|n| n.to_str()) {
            let normalized_session_dir = canonicalize_to_string(session_dir_name);
            if same_working_dir(&normalized_session_dir, &normalized_working_dir) {
                match &best {
                    Some((_, best_created_at_ms)) if *best_created_at_ms >= created_at_ms => {}
                    _ => best = Some((session_id, created_at_ms)),
                }
            }
        }
    }

    best
}

fn resolve_opencode_session_id(working_dir: &str, launch_started_at_ms: i64) -> Option<String> {
    let mut best: Option<(String, i64)> = None;
    for storage_paths in candidate_opencode_storage_paths() {
        let Some((session_id, created_at_ms)) = select_opencode_session_id_from_messages_root(
            &storage_paths.messages_root,
            working_dir,
            Some(launch_started_at_ms),
            200,
        ) else {
            continue;
        };
        match &best {
            Some((_, best_created_at_ms)) if *best_created_at_ms >= created_at_ms => {}
            _ => best = Some((session_id, created_at_ms)),
        }
    }
    best.map(|(session_id, _)| session_id)
}

fn resolve_opencode_session_id_for_existing(working_dir: &str) -> Option<String> {
    let mut best: Option<(String, i64)> = None;
    for storage_paths in candidate_opencode_storage_paths() {
        let Some((session_id, created_at_ms)) = select_opencode_session_id_from_messages_root(
            &storage_paths.messages_root,
            working_dir,
            None,
            400,
        ) else {
            continue;
        };
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
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
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
            .or_else(|| {
                value
                    .get("updatedAt")
                    .and_then(|updated_at| updated_at.as_i64())
            })?;
        let cwd = value
            .get("path")
            .and_then(|path| path.get("cwd").or_else(|| path.get("root")))
            .and_then(|cwd| cwd.as_str())
            .map(|cwd| cwd.to_string())
            .or_else(|| {
                value
                    .get("directory")
                    .and_then(|dir| dir.as_str())
                    .map(|dir| dir.to_string())
            });
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
        if !storage_paths.messages_root.is_dir() {
            continue;
        }
        let _project_worktree_by_id =
            read_opencode_project_worktree_map(&storage_paths.projects_root);

        let Ok(entries) = fs::read_dir(&storage_paths.messages_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let session_dir = entry.path();
            if !session_dir.is_dir() {
                continue;
            }
            let session_id = session_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if !session_id.starts_with("ses_") {
                continue;
            }

            let mut message_files = Vec::<(PathBuf, i64)>::new();
            let Ok(msg_entries) = fs::read_dir(&session_dir) else {
                continue;
            };
            for msg_entry in msg_entries.flatten() {
                let path = msg_entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
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

            let mut session_title: Option<String> = None;
            let mut session_directory: Option<String> = None;
            let mut session_created_at_ms: Option<i64> = None;

            let session_diff_path = storage_paths
                .sessions_root
                .parent()
                .map(|p| p.join("session_diff").join(format!("{}.json", session_id)));
            if let Some(diff_path) = session_diff_path {
                if diff_path.is_file() {
                    if let Ok(content) = fs::read_to_string(&diff_path) {
                        if let Ok(value) = serde_json::from_str::<Value>(&content) {
                            if let Some(arr) = value.as_array() {
                                if !arr.is_empty() {
                                    if let Some(first) = arr.first() {
                                        if let Some(dir) =
                                            first.get("file").and_then(|v| v.as_str())
                                        {
                                            session_directory = Some(dir.to_string());
                                        }
                                    }
                                }
                            } else if let Some(dir) =
                                value.get("directory").and_then(|v| v.as_str())
                            {
                                session_directory = Some(dir.to_string());
                                session_title = value.get("title").and_then(value_as_text);
                                session_created_at_ms = value
                                    .get("time")
                                    .and_then(|t| t.get("created"))
                                    .and_then(|v| v.as_i64());
                            }
                        }
                    }
                }
            }

            let mut model_name: Option<String> = None;
            for (path, _modified_ms) in message_files.iter().take(20) {
                if session_directory.is_none() || session_created_at_ms.is_none() {
                    let Ok(content) = fs::read_to_string(path) else {
                        continue;
                    };
                    let Ok(value) = serde_json::from_str::<Value>(&content) else {
                        continue;
                    };
                    if session_directory.is_none() {
                        if let Some(cwd) = value
                            .get("path")
                            .and_then(|p| p.get("cwd").or_else(|| p.get("root")))
                            .and_then(|cwd| cwd.as_str())
                        {
                            session_directory = Some(cwd.to_string());
                        }
                    }
                    if session_created_at_ms.is_none() {
                        if let Some(created) = value
                            .get("time")
                            .and_then(|t| t.get("created"))
                            .and_then(|v| v.as_i64())
                        {
                            session_created_at_ms = Some(created);
                        }
                    }
                    if model_name.is_none() {
                        model_name = value
                            .get("modelID")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }
                if session_directory.is_some()
                    && session_created_at_ms.is_some()
                    && model_name.is_some()
                {
                    break;
                }
            }

            let Some(working_dir) = session_directory
                .as_deref()
                .map(canonicalize_to_string)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            let created_at_ms = session_created_at_ms.unwrap_or(0);
            let updated_at_ms = message_files
                .first()
                .map(|(_, ms)| *ms)
                .unwrap_or(created_at_ms);

            if let Some(min) = min_updated_at_ms {
                if updated_at_ms < min {
                    continue;
                }
            }

            let title = session_title
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| fallback_history_title("opencode", session_id));

            out.push(HistorySessionEntry {
                tool: "opencode".to_string(),
                tool_session_id: session_id.to_string(),
                title,
                working_dir,
                model_name,
                created_at_ms,
                updated_at_ms,
            });
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

pub fn resolve_native_session_id_for_existing(
    model_type: &str,
    working_dir: &str,
    env: Option<&HashMap<String, String>>,
    created_at_ms: Option<i64>,
    exclude_ids: Option<&HashSet<String>>,
    allow_pending_bind_fallback: bool,
) -> Option<String> {
    match model_type.to_lowercase().as_str() {
        "claude" => resolve_claude_session_id_for_existing(working_dir, created_at_ms, exclude_ids),
        "gemini" => {
            let strict =
                resolve_gemini_session_id_for_existing(working_dir, created_at_ms, exclude_ids);
            if strict.is_some() || !allow_pending_bind_fallback {
                strict
            } else {
                resolve_gemini_session_id_for_pending_bind(working_dir, created_at_ms, exclude_ids)
            }
        }
        "codex" => resolve_codex_session_id_for_existing(working_dir, env),
        "opencode" => resolve_opencode_session_id_for_existing(working_dir),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_native_terminal_applescript, clean_terminal_app_name, command_uses_resume_semantics,
        normalize_terminal_app_key, normalize_working_dir_for_terminal, read_claude_project_file,
        read_codex_history_session_file, read_gemini_history_file, read_opencode_history_file,
        run_native_terminal_command_for_app_with_executor, select_gemini_session_for_create,
        select_gemini_session_for_existing, validate_create_command, GeminiSessionCandidate,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn make_temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "onespace-ai-sessions-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn write_temp_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write temp file");
    }

    #[test]
    fn create_command_rejects_resume_flags() {
        assert!(command_uses_resume_semantics("gemini", "gemini -r latest"));
        assert!(command_uses_resume_semantics(
            "claude",
            "claude --resume abc"
        ));
        assert!(command_uses_resume_semantics("codex", "codex resume 123"));
        assert!(command_uses_resume_semantics(
            "opencode",
            "opencode --session 123"
        ));
    }

    #[test]
    fn create_command_allows_plain_create_invocation() {
        assert!(!command_uses_resume_semantics("gemini", "gemini"));
        assert!(!command_uses_resume_semantics(
            "codex",
            "codex --profile p1"
        ));
        assert!(validate_create_command("opencode", "opencode --profile dev").is_ok());
    }

    #[test]
    fn normalize_working_dir_handles_relative_and_home() {
        let dot = normalize_working_dir_for_terminal("./");
        assert!(dot.starts_with('/'));
        let home = normalize_working_dir_for_terminal("~");
        assert!(home.starts_with('/'));
    }

    #[test]
    fn terminal_app_name_normalization_handles_bundle_suffix() {
        assert_eq!(clean_terminal_app_name(" Ghostty.app "), "Ghostty");
        assert_eq!(normalize_terminal_app_key("GHOSTTY.app"), "ghostty");
    }

    #[test]
    fn native_terminal_applescript_uses_ghostty_window_launch() {
        let script = build_native_terminal_applescript(
            "Ghostty",
            "/tmp/ghostty-project",
            "codex resume 123",
        );
        assert!(script.contains("new surface configuration"));
        assert!(script.contains(
            "set initial working directory of launch_config to \"/tmp/ghostty-project\""
        ));
        assert!(script
            .contains("set initial input of launch_config to \"codex resume 123\" & linefeed"));
        assert!(script.contains("new window with configuration launch_config"));
        assert!(!script.contains("do script"));
    }

    #[test]
    fn native_terminal_applescript_keeps_do_script_for_terminal() {
        let script = build_native_terminal_applescript(
            "Terminal",
            "/tmp/default-project",
            "codex resume 123",
        );
        assert!(script.contains("do script \"codex resume 123\""));
        assert!(!script.contains("new surface configuration"));
    }

    #[test]
    fn native_terminal_runner_builds_ghostty_script_from_shared_entry() {
        let mut captured = String::new();
        run_native_terminal_command_for_app_with_executor(
            "Ghostty",
            "/tmp/ghostty-runner",
            "codex resume 123",
            None,
            |script| {
                captured = script;
                Ok(())
            },
        )
        .expect("capture ghostty script");

        assert!(captured.contains("new window with configuration launch_config"));
        assert!(captured.contains(
            "set initial input of launch_config to \"cd '/tmp/ghostty-runner' && codex resume 123\" & linefeed"
        ));
    }

    #[test]
    fn native_terminal_runner_builds_standard_terminal_script_from_shared_entry() {
        let mut captured = String::new();
        run_native_terminal_command_for_app_with_executor(
            "Terminal",
            "/tmp/terminal-runner",
            "codex resume 123",
            None,
            |script| {
                captured = script;
                Ok(())
            },
        )
        .expect("capture terminal script");

        assert!(captured.contains("do script \"cd '/tmp/terminal-runner' && codex resume 123\""));
        assert!(!captured.contains("new window with configuration launch_config"));
    }

    #[test]
    fn codex_history_parser_reads_title_model_and_working_dir() {
        let root = make_temp_dir("codex-history");
        let path = root.join("rollout-2026-03-03T09-19-17-session-1.jsonl");
        write_temp_file(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"session-1\",\"timestamp\":\"2026-03-03T01:19:17.343Z\",\"cwd\":\"/tmp/codex-project\"}}\n",
                "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.4\"}}\n"
            ),
        );

        let mut titles = HashMap::new();
        titles.insert("session-1".to_string(), "Codex Thread".to_string());
        let mut updated = HashMap::new();
        updated.insert("session-1".to_string(), 1_709_429_000_000_i64);

        let parsed =
            read_codex_history_session_file(&path, &titles, &updated, 1_709_428_000_000_i64)
                .expect("codex history entry");
        assert_eq!(parsed.title, "Codex Thread");
        assert_eq!(parsed.model_name.as_deref(), Some("gpt-5.4"));
        assert_eq!(
            parsed.working_dir,
            normalize_working_dir_for_terminal("/tmp/codex-project")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_history_parser_falls_back_to_first_user_message_when_thread_name_missing() {
        let root = make_temp_dir("codex-history-user-title");
        let path = root.join("rollout-2026-03-03T09-19-17-session-2.jsonl");
        write_temp_file(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"session-2\",\"timestamp\":\"2026-03-03T01:19:17.343Z\",\"cwd\":\"/tmp/codex-project\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"Name this project better\"}}\n",
                "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.4\"}}\n"
            ),
        );

        let parsed = read_codex_history_session_file(
            &path,
            &HashMap::new(),
            &HashMap::new(),
            1_709_428_000_000_i64,
        )
        .expect("codex history entry");
        assert_eq!(parsed.title, "Name this project better");
        assert_eq!(parsed.tool_session_id, "session-2");

        let path_without_title = root.join("rollout-2026-03-03T09-19-17-session-3.jsonl");
        write_temp_file(
            &path_without_title,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"session-3\",\"timestamp\":\"2026-03-03T01:19:17.343Z\",\"cwd\":\"/tmp/codex-project\"}}\n",
                "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.4\"}}\n"
            ),
        );

        let parsed_without_title = read_codex_history_session_file(
            &path_without_title,
            &HashMap::new(),
            &HashMap::new(),
            1_709_428_000_000_i64,
        )
        .expect("codex history entry without title");
        assert_eq!(parsed_without_title.title, "session-3");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_history_parser_prefers_last_prompt_and_reads_model() {
        let root = make_temp_dir("claude-history");
        let path = root.join("session-1.jsonl");
        write_temp_file(
            &path,
            concat!(
                "{\"type\":\"user\",\"sessionId\":\"session-1\",\"cwd\":\"/tmp/claude-project\",\"message\":{\"content\":\"first user prompt\"},\"timestamp\":\"2026-03-10T05:09:58.846Z\"}\n",
                "{\"type\":\"assistant\",\"sessionId\":\"session-1\",\"cwd\":\"/tmp/claude-project\",\"message\":{\"model\":\"qwen3.5-plus\"},\"timestamp\":\"2026-03-10T05:10:07.255Z\"}\n",
                "{\"type\":\"last-prompt\",\"sessionId\":\"session-1\",\"lastPrompt\":\"final prompt title\"}\n"
            ),
        );

        let parsed = read_claude_project_file(&path, None).expect("claude history entry");
        assert_eq!(parsed.title, "final prompt title");
        assert_eq!(parsed.model_name.as_deref(), Some("qwen3.5-plus"));
        assert_eq!(
            parsed.working_dir,
            normalize_working_dir_for_terminal("/tmp/claude-project")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gemini_history_parser_reads_first_user_title_and_model() {
        let root = make_temp_dir("gemini-history");
        let path = root.join("session-gemini.json");
        write_temp_file(
            &path,
            r#"{
  "sessionId": "gemini-session-1",
  "projectHash": "project-1",
  "startTime": "2026-01-09T01:40:36.999Z",
  "lastUpdated": "2026-01-09T02:33:05.005Z",
  "messages": [
    { "type": "user", "content": "Gemini first prompt" },
    { "type": "gemini", "content": "Assistant reply", "model": "gemini-3-pro-preview" }
  ]
}"#,
        );
        let mut project_map = HashMap::new();
        project_map.insert(
            "project-1".to_string(),
            normalize_working_dir_for_terminal("/tmp/gemini-project"),
        );

        let parsed = read_gemini_history_file(&path, &project_map).expect("gemini history entry");
        assert_eq!(parsed.title, "Gemini first prompt");
        assert_eq!(parsed.model_name.as_deref(), Some("gemini-3-pro-preview"));
        assert_eq!(
            parsed.working_dir,
            normalize_working_dir_for_terminal("/tmp/gemini-project")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn opencode_history_parser_reads_title_and_message_model() {
        let root = make_temp_dir("opencode-history");
        let session_path = root.join("storage/session/project-1/session-1.json");
        let messages_root = root.join("storage/message");
        let project_worktree_by_id = HashMap::new();
        write_temp_file(
            &session_path,
            r#"{
  "id": "ses_123",
  "directory": "/tmp/opencode-project",
  "title": "OpenCode Session Title",
  "time": { "created": 1770800496647, "updated": 1770800790445 }
}"#,
        );
        write_temp_file(
            &messages_root.join("ses_123/msg_1.json"),
            r#"{
  "role": "assistant",
  "modelID": "gpt-5-codex"
}"#,
        );

        let parsed =
            read_opencode_history_file(&session_path, &messages_root, &project_worktree_by_id)
                .expect("opencode history");
        assert_eq!(parsed.title, "OpenCode Session Title");
        assert_eq!(parsed.model_name.as_deref(), Some("gpt-5-codex"));
        assert_eq!(
            parsed.working_dir,
            normalize_working_dir_for_terminal("/tmp/opencode-project")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn opencode_history_parser_falls_back_to_project_worktree_when_directory_missing() {
        let root = make_temp_dir("opencode-history-project-fallback");
        let session_path = root.join("storage/session/project-1/session-1.json");
        let messages_root = root.join("storage/message");
        let mut project_worktree_by_id = HashMap::new();
        project_worktree_by_id.insert(
            "project-1".to_string(),
            normalize_working_dir_for_terminal("/tmp/opencode-project-from-project"),
        );
        write_temp_file(
            &session_path,
            r#"{
  "id": "ses_456",
  "projectID": "project-1",
  "slug": "steady-signal",
  "time": { "created": 1770800496647, "updated": 1770800790445 }
}"#,
        );
        write_temp_file(
            &messages_root.join("ses_456/msg_1.json"),
            r#"{
  "role": "assistant",
  "modelID": "claude-opus-4-6"
}"#,
        );

        let parsed =
            read_opencode_history_file(&session_path, &messages_root, &project_worktree_by_id)
                .expect("opencode history with project fallback");
        assert_eq!(parsed.title, "steady-signal");
        assert_eq!(parsed.model_name.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(
            parsed.working_dir,
            normalize_working_dir_for_terminal("/tmp/opencode-project-from-project")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gemini_existing_binding_does_not_fallback_to_latest_when_created_time_present() {
        let created_at_ms = 1_700_000_000_000_i64;
        let candidates = vec![
            GeminiSessionCandidate {
                session_id: "older-but-updated".to_string(),
                start_at_ms: created_at_ms - 3_600_000,
                updated_at_ms: created_at_ms + 10_000,
            },
            GeminiSessionCandidate {
                session_id: "latest".to_string(),
                start_at_ms: created_at_ms - 7_200_000,
                updated_at_ms: created_at_ms + 20_000,
            },
        ];
        let selected = select_gemini_session_for_existing(&candidates, Some(created_at_ms));
        assert!(selected.is_none());
    }

    #[test]
    fn gemini_existing_binding_prefers_start_time_over_recent_updates() {
        let created_at_ms = 1_700_000_000_000_i64;
        let candidates = vec![
            GeminiSessionCandidate {
                session_id: "target".to_string(),
                start_at_ms: created_at_ms + 2_000,
                updated_at_ms: created_at_ms + 15_000,
            },
            GeminiSessionCandidate {
                session_id: "distractor".to_string(),
                start_at_ms: created_at_ms - 7_200_000,
                updated_at_ms: created_at_ms + 30_000,
            },
        ];
        let selected = select_gemini_session_for_existing(&candidates, Some(created_at_ms));
        assert_eq!(selected.as_deref(), Some("target"));
    }

    #[test]
    fn gemini_create_binding_prefers_nearest_start_time() {
        let launch_started_at_ms = 1_700_000_000_000_i64;
        let candidates = vec![
            GeminiSessionCandidate {
                session_id: "new".to_string(),
                start_at_ms: launch_started_at_ms + 1_000,
                updated_at_ms: launch_started_at_ms + 2_000,
            },
            GeminiSessionCandidate {
                session_id: "old-resumed".to_string(),
                start_at_ms: launch_started_at_ms - 3_600_000,
                updated_at_ms: launch_started_at_ms + 3_000,
            },
        ];
        let selected = select_gemini_session_for_create(&candidates, launch_started_at_ms);
        assert_eq!(selected.as_deref(), Some("new"));
    }

    #[test]
    fn gemini_create_binding_falls_back_to_recent_update_when_no_near_start() {
        let launch_started_at_ms = 1_700_000_000_000_i64;
        let candidates = vec![
            GeminiSessionCandidate {
                session_id: "old-resumed".to_string(),
                start_at_ms: launch_started_at_ms - 86_400_000,
                updated_at_ms: launch_started_at_ms + 2_000,
            },
            GeminiSessionCandidate {
                session_id: "stale".to_string(),
                start_at_ms: launch_started_at_ms - 172_800_000,
                updated_at_ms: launch_started_at_ms - 1_000,
            },
        ];
        let selected = select_gemini_session_for_create(&candidates, launch_started_at_ms);
        assert_eq!(selected.as_deref(), Some("old-resumed"));
    }

    #[test]
    #[ignore = "local environment smoke test"]
    fn test_local_gemini_binding() {
        let working_dir = "/Users/yuqiyu/AiHistorys/one-space/onespace-app";
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        use std::collections::HashSet;
        let exclude = HashSet::new();

        let candidates = super::collect_gemini_session_candidates(working_dir, Some(&exclude));
        println!("Found {} candidates for {}", candidates.len(), working_dir);
        for c in &candidates {
            println!(
                " - ID: {}, start: {}, updated: {}",
                c.session_id, c.start_at_ms, c.updated_at_ms
            );
        }

        let bind_time = now - 60000;
        let res = super::resolve_gemini_session_id_for_pending_bind(
            working_dir,
            Some(bind_time),
            Some(&exclude),
        );
        println!("Selected for pending bind (1m ago): {:?}", res);
    }

    #[test]
    #[ignore = "local environment smoke test"]
    fn test_local_claude_binding() {
        let working_dir = "/Users/yuqiyu/AiHistorys/one-space/onespace-app";
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let res = super::resolve_claude_session_id(working_dir, now);
        println!("Resolved claude session (now): {:?}", res);

        // 使用实际的历史记录时间戳测试
        let test_timestamp = 1773388541848_i64; // 最近一条记录的时间
        let res_past = super::resolve_claude_session_id("/Users/yuqiyu/AiHistorys", test_timestamp);
        println!("Resolved claude session (historical): {:?}", res_past);
        assert!(res_past.is_some(), "Should find historical session");
    }
}
