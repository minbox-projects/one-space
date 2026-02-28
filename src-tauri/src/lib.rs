use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Serialize, Deserialize)]
pub struct TmuxSession {
    pub name: String,
    pub created: u64,
    pub attached: bool,
    pub path: String,
}

#[tauri::command]
fn get_tmux_sessions() -> Result<Vec<TmuxSession>, String> {
    let output = Command::new("tmux")
        .arg("ls")
        .arg("-F")
        .arg("#{session_name}|#{session_created}|#{session_attached}|#{pane_current_path}")
        .output();

    match output {
        Ok(out) => {
            if !out.status.success() {
                // Usually means no server running or no sessions
                return Ok(vec![]);
            }

            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut sessions = Vec::new();

            for line in stdout.lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() == 4 {
                    sessions.push(TmuxSession {
                        name: parts[0].to_string(),
                        created: parts[1].parse().unwrap_or(0),
                        attached: parts[2] != "0",
                        path: parts[3].to_string(),
                    });
                }
            }

            // Sort by created time descending
            sessions.sort_by(|a, b| b.created.cmp(&a.created));
            Ok(sessions)
        }
        Err(e) => Err(format!("Failed to execute tmux: {}", e)),
    }
}

#[tauri::command]
fn create_tmux_session(session_name: &str, working_dir: &str, command: &str) -> Result<(), String> {
    // Create the session in the background
    let mut args = vec!["new-session", "-d", "-s", session_name, "-c", working_dir];
    if !command.is_empty() {
        args.push(command);
    }

    let status = Command::new("tmux")
        .args(&args)
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() {
        return Err("Failed to create tmux session. Please check if the command is valid.".into());
    }

    Ok(())
}

#[tauri::command]
fn attach_tmux_session(session_name: &str) -> Result<(), String> {
    // Use AppleScript to open Mac Terminal and attach to the session
    // We open a new window and execute the attach command
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "tmux attach -t {}"
        end tell"#,
        session_name
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn kill_tmux_session(session_name: &str) -> Result<(), String> {
    let status = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() {
        return Err("Failed to kill session.".into());
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_tmux_sessions,
            create_tmux_session,
            attach_tmux_session,
            kill_tmux_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
