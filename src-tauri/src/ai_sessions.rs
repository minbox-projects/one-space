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

fn run_native_terminal_command(working_dir: &str, command: &str) -> Result<(), String> {
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "cd '{}' && {}"
        end tell"#,
        working_dir, command
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
    let command = build_resume_command(model_type, session_id)
        .ok_or_else(|| "Unsupported model type for native session".to_string())?;
    run_native_terminal_command(working_dir, &command)
}

pub fn launch_native_session_for_create(
    working_dir: &str,
    model_type: &str,
    session_id: &str,
) -> Result<(), String> {
    let command = build_create_command(model_type, session_id)
        .ok_or_else(|| "Unsupported model type for native session".to_string())?;
    run_native_terminal_command(working_dir, &command)
}

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
