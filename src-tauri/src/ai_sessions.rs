use crate::get_data_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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

fn codex_resume_or_new_command(session_id: &str) -> String {
    format!("codex resume {} 2>/dev/null || codex", session_id)
}

fn gemini_resume_or_new_command(session_id: &str) -> String {
    format!(
        "gemini -r {} 2>/dev/null || gemini -r latest 2>/dev/null || gemini",
        session_id
    )
}

fn claude_resume_or_new_command(session_id: &str) -> String {
    format!(
        "claude -r {} 2>/dev/null || claude --session-id {}",
        session_id, session_id
    )
}

pub fn get_ai_sessions() -> Result<Vec<AiSession>, String> {
    let path = get_sessions_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut sessions: Vec<AiSession> = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    // Sort by created_at in descending order (newest first)
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
    match model_type.to_lowercase().as_str() {
        "claude" => Some(claude_resume_or_new_command(session_id)),
        "gemini" => Some(gemini_resume_or_new_command(session_id)),
        "opencode" => Some(format!("opencode -s {}", session_id)),
        "codex" => Some(codex_resume_or_new_command(session_id)),
        _ => None,
    }
}

fn build_create_command(model_type: &str, session_id: &str) -> Option<String> {
    match model_type.to_lowercase().as_str() {
        "claude" => Some(claude_resume_or_new_command(session_id)),
        "gemini" => Some("gemini".to_string()),
        "opencode" => Some("opencode".to_string()),
        "codex" => Some(codex_resume_or_new_command(session_id)),
        _ => None,
    }
}

fn escape_applescript_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_shell_single_quoted(input: &str) -> String {
    input.replace('\'', "'\"'\"'")
}

fn sanitize_session_id_for_filename(session_id: &str) -> String {
    let mut out = String::with_capacity(session_id.len());
    for ch in session_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

fn wrap_with_transcript_capture(session_id: &str, command: &str) -> Result<String, String> {
    let logs_dir = get_data_dir()?.join("data").join("sessions").join("logs");
    fs::create_dir_all(&logs_dir).map_err(|e| e.to_string())?;
    let safe_id = sanitize_session_id_for_filename(session_id);
    let log_path = logs_dir.join(format!("{}.log", safe_id));
    let log_path_escaped = escape_shell_single_quoted(&log_path.to_string_lossy());
    let command_escaped = escape_shell_single_quoted(command);
    Ok(format!(
        "script -q '{}' /bin/zsh -lc '{}'",
        log_path_escaped, command_escaped
    ))
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

fn run_native_terminal_command(working_dir: &str, command: &str) -> Result<(), String> {
    let terminal_app = escape_applescript_string(&resolve_terminal_app_name());
    let working_dir_escaped = escape_shell_single_quoted(working_dir);
    let shell_line = format!("cd '{}' && {}", working_dir_escaped, command);
    let applescript_line = escape_applescript_string(&shell_line);
    let script = format!(
        r#"tell application "{}"
            activate
            do script "{}"
        end tell"#,
        terminal_app, applescript_line
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn launch_native_session(
    working_dir: &str,
    model_type: &str,
    session_id: &str,
) -> Result<(), String> {
    let base_command = build_resume_command(model_type, session_id)
        .ok_or_else(|| "Unsupported model type for native session".to_string())?;
    let command = wrap_with_transcript_capture(session_id, &base_command).unwrap_or_else(|e| {
        eprintln!("Failed to enable transcript capture for session {}: {}", session_id, e);
        base_command
    });
    run_native_terminal_command(working_dir, &command)
}

pub fn launch_native_session_for_create(
    working_dir: &str,
    model_type: &str,
    session_id: &str,
) -> Result<(), String> {
    let base_command = build_create_command(model_type, session_id)
        .ok_or_else(|| "Unsupported model type for native session".to_string())?;
    let command = wrap_with_transcript_capture(session_id, &base_command).unwrap_or_else(|e| {
        eprintln!("Failed to enable transcript capture for session {}: {}", session_id, e);
        base_command
    });
    run_native_terminal_command(working_dir, &command)
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

    launch_native_session_for_create(&working_dir, &model_type, &tool_session_id)?;

    Ok(session)
}
