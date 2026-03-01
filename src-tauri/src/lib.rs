mod config;
mod git;
mod ai_env;

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

fn get_data_dir() -> Result<PathBuf, String> {
    let cfg = config::get_config()?;
    let data_dir = if cfg.storage_type == "git" {
        let app_dir = config::get_app_dir()?;
        app_dir.join("git_data")
    } else {
        let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
        home_dir.join(".config").join("onespace").join("data")
    };

    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    }

    Ok(data_dir)
}

#[derive(Serialize)]
struct OAuthResult {
    code: String,
    redirect_uri: String,
}

#[tauri::command]
async fn start_google_oauth(
    app: tauri::AppHandle,
    client_id: String,
    scope: String,
) -> Result<OAuthResult, String> {
    let (tx, rx) = std::sync::mpsc::channel();

    // Start the local server
    // Note: tauri_plugin_oauth usually binds to localhost (127.0.0.1)
    let port = tauri_plugin_oauth::start(move |url| {
        let _ = tx.send(url);
    })
    .map_err(|e| e.to_string())?;

    // Construct the Authorization URL safely
    let redirect_uri = format!("http://localhost:{}", port);

    let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|e| e.to_string())?;

    url.query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &scope)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");

    let auth_url = url.to_string();

    println!("Opening auth URL: {}", auth_url); // simple logging

    // Open the browser
    app.opener()
        .open_url(auth_url, None::<&str>)
        .map_err(|e| e.to_string())?;

    // Wait for the response
    // This blocks the async thread, which is acceptable here as it's a separate task
    // Add a 5 minute timeout
    let url_str = rx
        .recv_timeout(std::time::Duration::from_secs(300))
        .map_err(|_| "OAuth login timed out after 5 minutes".to_string())?;

    // Parse the code from the URL
    let url = reqwest::Url::parse(&url_str).map_err(|e| e.to_string())?;
    let code = url
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .ok_or("No code found in redirect URL")?;

    Ok(OAuthResult { code, redirect_uri })
}

#[tauri::command]
fn read_snippets() -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let snippets_path = data_dir.join("snippets.json");

    if !snippets_path.exists() {
        // Return empty array as string if file doesn't exist
        return Ok("[]".to_string());
    }

    fs::read_to_string(snippets_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_snippets(snippets_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let snippets_path = data_dir.join("snippets.json");

    let mut file = File::create(snippets_path).map_err(|e| e.to_string())?;
    file.write_all(snippets_json.as_bytes())
        .map_err(|e| e.to_string())?;

    // Auto sync
    std::thread::spawn(|| {
        let _ = git::sync_git();
    });

    Ok(())
}

#[tauri::command]
fn read_bookmarks() -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let bookmarks_path = data_dir.join("bookmarks.json");

    if !bookmarks_path.exists() {
        return Ok("[]".to_string());
    }

    fs::read_to_string(bookmarks_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_bookmarks(bookmarks_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let bookmarks_path = data_dir.join("bookmarks.json");

    let mut file = File::create(bookmarks_path).map_err(|e| e.to_string())?;
    file.write_all(bookmarks_json.as_bytes())
        .map_err(|e| e.to_string())?;

    std::thread::spawn(|| {
        let _ = git::sync_git();
    });

    Ok(())
}

#[tauri::command]
fn open_local_path(path: &str) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct SshHost {
    pub name: String,
    pub host_name: String,
    pub user: String,
    pub port: u16,
}

#[tauri::command]
fn get_ssh_hosts() -> Result<Vec<SshHost>, String> {
    let mut hosts = Vec::new();

    // Get home directory
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let ssh_config_path = home_dir.join(".ssh").join("config");

    if !ssh_config_path.exists() {
        return Ok(hosts); // Return empty list if no config
    }

    // Manual parsing since ssh_cfg crate might be too strict
    if let Ok(content) = fs::read_to_string(&ssh_config_path) {
        let mut current_host: Option<String> = None;
        let mut current_hostname = String::new();
        let mut current_user = String::new();
        let mut current_port = 22;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let key = parts[0].to_lowercase();

            if key == "host" && parts.len() > 1 {
                // If we were building a host, save it (skip wildcard hosts like '*')
                if let Some(name) = current_host.take() {
                    if name != "*" {
                        hosts.push(SshHost {
                            name,
                            host_name: if current_hostname.is_empty() {
                                "Unknown".to_string()
                            } else {
                                current_hostname.clone()
                            },
                            user: if current_user.is_empty() {
                                "root".to_string()
                            } else {
                                current_user.clone()
                            },
                            port: current_port,
                        });
                    }
                }

                // Start new host
                current_host = Some(parts[1].to_string());
                current_hostname.clear();
                current_user.clear();
                current_port = 22;
            } else if key == "hostname" && parts.len() > 1 && current_host.is_some() {
                current_hostname = parts[1].to_string();
            } else if key == "user" && parts.len() > 1 && current_host.is_some() {
                current_user = parts[1].to_string();
            } else if key == "port" && parts.len() > 1 && current_host.is_some() {
                if let Ok(port) = parts[1].parse::<u16>() {
                    current_port = port;
                }
            }
        }

        // Don't forget the last host
        if let Some(name) = current_host {
            if name != "*" {
                hosts.push(SshHost {
                    name,
                    host_name: if current_hostname.is_empty() {
                        "Unknown".to_string()
                    } else {
                        current_hostname.clone()
                    },
                    user: if current_user.is_empty() {
                        "root".to_string()
                    } else {
                        current_user.clone()
                    },
                    port: current_port,
                });
            }
        }
    }

    Ok(hosts)
}

#[tauri::command]
fn connect_ssh(host: &str) -> Result<(), String> {
    // Open a new Terminal window and run the SSH command
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "ssh {}"
        end tell"#,
        host
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn connect_ssh_custom(
    user: &str,
    host: &str,
    port: u16,
    auth_type: &str,
    auth_val: &str,
) -> Result<(), String> {
    // Build the base SSH command
    let mut ssh_cmd = format!("ssh -p {} {}@{}", port, user, host);

    // If using identity file, append it
    if auth_type == "key" && !auth_val.is_empty() {
        ssh_cmd = format!("ssh -i {} -p {} {}@{}", auth_val, port, user, host);
    }

    let script;

    // If using password, we need a slightly more complex AppleScript or sshpass
    // For simplicity and macOS compatibility, we'll try to just paste the password
    // or rely on user typing it if it's too complex to inject cleanly without sshpass.
    // A safe approach for Terminal.app without installing external tools:
    if auth_type == "password" && !auth_val.is_empty() {
        script = format!(
            r#"tell application "Terminal"
                activate
                set newTab to do script "{}"
                delay 1.5
                do script "{}" in newTab
            end tell"#,
            ssh_cmd, auth_val
        );
    } else {
        script = format!(
            r#"tell application "Terminal"
                activate
                do script "{}"
            end tell"#,
            ssh_cmd
        );
    }

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn read_notes() -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let notes_path = data_dir.join("notes.json");

    if !notes_path.exists() {
        return Ok("[]".to_string());
    }

    fs::read_to_string(notes_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_notes(notes_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let notes_path = data_dir.join("notes.json");

    let mut file = File::create(notes_path).map_err(|e| e.to_string())?;
    file.write_all(notes_json.as_bytes())
        .map_err(|e| e.to_string())?;

    std::thread::spawn(|| {
        let _ = git::sync_git();
    });

    Ok(())
}

#[tauri::command]
async fn exchange_google_token(
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = res.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}

#[tauri::command]
async fn refresh_google_token(
    refresh_token: String,
    client_id: String,
    client_secret: String,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("refresh_token", refresh_token.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = res.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}

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

#[tauri::command]
fn rename_tmux_session(old_name: &str, new_name: &str) -> Result<(), String> {
    let status = Command::new("tmux")
        .args(["rename-session", "-t", old_name, new_name])
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() {
        return Err("Failed to rename tmux session.".into());
    }

    Ok(())
}

#[tauri::command]
fn install_cli() -> Result<(), String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let local_bin = home_dir.join(".local").join("bin");

    if !local_bin.exists() {
        fs::create_dir_all(&local_bin).map_err(|e| e.to_string())?;
    }

    let script_path = local_bin.join("onespace");
    let mut file = File::create(&script_path).map_err(|e| e.to_string())?;

    let script_content = r#"#!/usr/bin/env bash

if [ "$1" != "ai" ] || [ -z "$2" ]; then
    echo "Usage: onespace ai <model_shortcut> [session_name] [model_args...]"
    echo "Examples:"
    echo "  onespace ai claude"
    echo "  onespace ai gemini my_project"
    echo "  onespace ai claude my_project --dangerously-skip-permissions -c"
    echo "  onespace ai claude --dangerously-skip-permissions -c"
    exit 1
fi

MODEL_SHORTCUT="$2"
shift 2

SESSION_NAME=""
MODEL_ARGS=""

# Check if the first remaining argument is not an option (doesn't start with '-')
if [ $# -gt 0 ] && [[ "$1" != -* ]]; then
    SESSION_NAME="$1"
    shift 1
fi

if [ -z "$SESSION_NAME" ]; then
    SESSION_NAME="${PWD##*/}_ai"
fi
SESSION_NAME=$(echo "$SESSION_NAME" | sed 's/[. ]/_/g')

case "$MODEL_SHORTCUT" in
    claude) CMD="claude code" ;;
    gemini) CMD="gemini -y" ;;
    opencode) CMD="opencode" ;;
    *) echo "Unknown model: $MODEL_SHORTCUT"; exit 1 ;;
esac

# Collect all remaining arguments as model arguments
if [ $# -gt 0 ]; then
    CMD="$CMD $@"
fi

echo "Starting AI session '$SESSION_NAME' using $MODEL_SHORTCUT in $PWD..."
tmux new-session -d -s "$SESSION_NAME" -c "$PWD" "$CMD"

if [ $? -eq 0 ]; then
    echo "Session created successfully."
    echo "Attaching to session '$SESSION_NAME'..."
    tmux attach -t "$SESSION_NAME"
else
    echo "Failed to create session."
fi
"#;

    file.write_all(script_content.as_bytes())
        .map_err(|e| e.to_string())?;

    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;

    Ok(())
}

use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // Setup global shortcut
    let _shortcut = Shortcut::new(Some(Modifiers::ALT), Code::Space);

    builder = builder.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }
            })
            .build(),
    );

    builder
        .plugin(tauri_plugin_oauth::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_tmux_sessions,
            create_tmux_session,
            attach_tmux_session,
            kill_tmux_session,
            rename_tmux_session,
            install_cli,
            get_ssh_hosts,
            connect_ssh,
            connect_ssh_custom,
            read_snippets,
            save_snippets,
            read_bookmarks,
            save_bookmarks,
            open_local_path,
            read_notes,
            save_notes,
            exchange_google_token,
            refresh_google_token,
            start_google_oauth,
            config::get_storage_config,
            config::save_storage_config,
            git::sync_git,
            ai_env::get_ai_providers,
            ai_env::save_ai_providers,
            ai_env::apply_ai_environment
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
