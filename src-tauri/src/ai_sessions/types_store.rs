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
