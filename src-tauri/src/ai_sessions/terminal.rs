use super::{
    build_create_seed_session_id, claude_new_command, claude_resume_command, codex_new_command,
    codex_resume_command, configured_create_command, gemini_new_command, gemini_resume_command,
    now_epoch_millis, opencode_new_command, resolve_native_session_id_after_create,
    save_ai_session, AiSession,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalPermissionMode {
    #[default]
    Default,
    FullAccess,
}

impl TerminalPermissionMode {
    pub fn from_str(value: &str) -> Self {
        match value {
            "full_access" => TerminalPermissionMode::FullAccess,
            _ => TerminalPermissionMode::Default,
        }
    }
}

/// Result of building a resume command: includes the command string and optional env overrides.
pub struct ResumeCommandResult {
    pub command: String,
    pub env: Option<HashMap<String, String>>,
}

pub(in crate::ai_sessions) fn build_resume_command(
    model_type: &str,
    session_id: &str,
    permission_mode: TerminalPermissionMode,
) -> Option<ResumeCommandResult> {
    let resume_id = session_id.trim();
    if resume_id.is_empty() {
        return None;
    }
    match model_type.to_lowercase().as_str() {
        "claude" => {
            let cmd = if permission_mode == TerminalPermissionMode::FullAccess {
                format!(
                    "claude --dangerously-skip-permissions -r {}",
                    shell_single_quote(resume_id)
                )
            } else {
                claude_resume_command(resume_id)
            };
            Some(ResumeCommandResult {
                command: cmd,
                env: None,
            })
        }
        "gemini" => {
            let cmd = if permission_mode == TerminalPermissionMode::FullAccess {
                format!(
                    "gemini --approval-mode=yolo -r {}",
                    shell_single_quote(resume_id)
                )
            } else {
                gemini_resume_command(resume_id)
            };
            Some(ResumeCommandResult {
                command: cmd,
                env: None,
            })
        }
        "codex" => {
            let cmd = if permission_mode == TerminalPermissionMode::FullAccess {
                format!(
                    "codex --dangerously-bypass-approvals-and-sandbox resume {}",
                    shell_single_quote(resume_id)
                )
            } else {
                codex_resume_command(resume_id)
            };
            Some(ResumeCommandResult {
                command: cmd,
                env: None,
            })
        }
        "opencode" => {
            let cmd = format!("opencode -s {}", shell_single_quote(resume_id));
            let env = if permission_mode == TerminalPermissionMode::FullAccess {
                Some(HashMap::from([(
                    "OPENCODE_PERMISSION".to_string(),
                    "allow".to_string(),
                )]))
            } else {
                None
            };
            Some(ResumeCommandResult { command: cmd, env })
        }
        _ => None,
    }
}

pub(in crate::ai_sessions) fn build_create_command(
    model_type: &str,
    session_id: Option<&str>,
) -> Result<String, String> {
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

pub(in crate::ai_sessions) fn escape_applescript_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(in crate::ai_sessions) fn clean_terminal_app_name(app_name: &str) -> String {
    let trimmed = app_name.trim();
    if trimmed.to_lowercase().ends_with(".app") {
        trimmed[..trimmed.len() - 4].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(in crate::ai_sessions) fn normalize_terminal_app_key(app_name: &str) -> String {
    clean_terminal_app_name(app_name).to_lowercase()
}

pub fn resolve_terminal_app_name() -> String {
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

pub(in crate::ai_sessions) fn shell_single_quote(input: &str) -> String {
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

pub(in crate::ai_sessions) fn env_prefix(env: &HashMap<String, String>) -> String {
    let mut pairs = env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
        .into_iter()
        .map(|(k, v)| format!("export {}={}", k, shell_single_quote(&v)))
        .collect::<Vec<_>>()
        .join(" && ")
}

pub(in crate::ai_sessions) fn build_shell_command(
    resolved_working_dir: &str,
    command: &str,
    env: Option<&HashMap<String, String>>,
) -> String {
    if let Some(vars) = env.filter(|vars| !vars.is_empty()) {
        format!(
            "cd {} && {} && {}",
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

pub(in crate::ai_sessions) fn normalize_initial_prompt(
    initial_prompt: Option<&str>,
) -> Option<String> {
    initial_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(in crate::ai_sessions) fn build_standard_terminal_applescript(
    terminal_app: &str,
    shell_cmd: &str,
    initial_prompt: Option<&str>,
) -> String {
    let terminal_app = escape_applescript_string(terminal_app);
    let shell_cmd = escape_applescript_string(shell_cmd);
    if let Some(prompt) = normalize_initial_prompt(initial_prompt) {
        let prompt = escape_applescript_string(&prompt);
        format!(
            r#"tell application "{}"
            do script "{}"
            delay 1
            do script "{}" in selected tab of front window
            activate
        end tell"#,
            terminal_app, shell_cmd, prompt
        )
    } else {
        format!(
            r#"tell application "{}"
            do script "{}"
            activate
        end tell"#,
            terminal_app, shell_cmd
        )
    }
}

pub(in crate::ai_sessions) fn build_ghostty_terminal_applescript(
    terminal_app: &str,
    resolved_working_dir: &str,
    shell_cmd: &str,
    initial_prompt: Option<&str>,
) -> String {
    let terminal_app = escape_applescript_string(terminal_app);
    let resolved_working_dir = escape_applescript_string(resolved_working_dir);
    let initial_input = if let Some(prompt) = normalize_initial_prompt(initial_prompt) {
        format!("{shell_cmd}\n{prompt}")
    } else {
        shell_cmd.to_string()
    };
    let initial_input = escape_applescript_string(&initial_input);
    format!(
        r#"tell application "{}"
            activate
            set launch_config to new surface configuration
            set initial working directory of launch_config to "{}"
            set initial input of launch_config to "{}" & linefeed
            new window with configuration launch_config
            activate
        end tell"#,
        terminal_app, resolved_working_dir, initial_input
    )
}

pub(in crate::ai_sessions) fn build_native_terminal_applescript(
    terminal_app: &str,
    resolved_working_dir: &str,
    shell_cmd: &str,
    initial_prompt: Option<&str>,
) -> String {
    if normalize_terminal_app_key(terminal_app) == "ghostty" {
        build_ghostty_terminal_applescript(
            terminal_app,
            resolved_working_dir,
            shell_cmd,
            initial_prompt,
        )
    } else {
        build_standard_terminal_applescript(terminal_app, shell_cmd, initial_prompt)
    }
}

pub(in crate::ai_sessions) fn execute_applescript(script: &str) -> Result<(), String> {
    Command::new("osascript")
        .arg("-e")
        .arg(script)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(in crate::ai_sessions) fn run_native_terminal_command_for_app_with_executor<F>(
    terminal_app: &str,
    working_dir: &str,
    command: &str,
    env: Option<&HashMap<String, String>>,
    initial_prompt: Option<&str>,
    execute_script: F,
) -> Result<(), String>
where
    F: FnOnce(String) -> Result<(), String>,
{
    let resolved_working_dir = normalize_working_dir_for_terminal(working_dir);
    let shell_cmd = build_shell_command(&resolved_working_dir, command, env);
    let script = build_native_terminal_applescript(
        terminal_app,
        &resolved_working_dir,
        &shell_cmd,
        initial_prompt,
    );
    execute_script(script)
}

pub fn run_native_terminal_command_for_update(
    terminal_app: &str,
    working_dir: &str,
    command: &str,
) -> Result<(), String> {
    run_native_terminal_command_for_app_with_executor(
        terminal_app,
        working_dir,
        command,
        None,
        None,
        |script| execute_applescript(&script),
    )
}

#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    pub env: Option<HashMap<String, String>>,
    pub initial_prompt: Option<String>,
}

pub fn launch_native_session_with_options(
    working_dir: &str,
    model_type: &str,
    session_id: &str,
    permission_mode: TerminalPermissionMode,
    options: &LaunchOptions,
) -> Result<(), String> {
    let result = build_resume_command(model_type, session_id, permission_mode)
        .ok_or_else(|| "Unsupported model type for native session".to_string())?;
    // Merge env: start with caller env, then overlay permission env so it takes precedence
    let mut merged_env = options.env.clone().unwrap_or_default();
    if let Some(cmd_env) = result.env {
        for (k, v) in cmd_env {
            merged_env.insert(k.clone(), v.clone());
        }
    }
    let env_ref = if merged_env.is_empty() {
        None
    } else {
        Some(&merged_env)
    };
    let terminal_app = resolve_terminal_app_name();
    run_native_terminal_command_for_app_with_executor(
        &terminal_app,
        working_dir,
        &result.command,
        env_ref,
        options.initial_prompt.as_deref(),
        |script| execute_applescript(&script),
    )
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
        TerminalPermissionMode::Default,
        &LaunchOptions::default(),
    )
}

pub fn launch_native_session_for_create_with_options(
    working_dir: &str,
    model_type: &str,
    requested_session_id: Option<&str>,
    permission_mode: TerminalPermissionMode,
    options: &LaunchOptions,
) -> Result<Option<String>, String> {
    let launch_started_at_ms = now_epoch_millis();
    let seed_session_id = build_create_seed_session_id(model_type, requested_session_id);
    let mut command = build_create_command(model_type, seed_session_id.as_deref())?;
    match model_type.to_lowercase().as_str() {
        "claude" if permission_mode == TerminalPermissionMode::FullAccess => {
            if !command.contains("--dangerously-skip-permissions") {
                command.push_str(" --dangerously-skip-permissions");
            }
        }
        "codex" if permission_mode == TerminalPermissionMode::FullAccess => {
            if !command.contains("--dangerously-bypass-approvals-and-sandbox") {
                command.push_str(" --dangerously-bypass-approvals-and-sandbox");
            }
        }
        "gemini" if permission_mode == TerminalPermissionMode::FullAccess => {
            if !command.contains("--approval-mode=yolo") {
                command.push_str(" --approval-mode=yolo");
            }
        }
        _ => {}
    }
    let terminal_app = resolve_terminal_app_name();
    run_native_terminal_command_for_app_with_executor(
        &terminal_app,
        working_dir,
        &command,
        options.env.as_ref(),
        options.initial_prompt.as_deref(),
        |script| execute_applescript(&script),
    )?;
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
        TerminalPermissionMode::Default,
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
