mod config;
mod git;
mod ai_env;

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tauri::{Manager, WindowEvent};
use tauri_plugin_opener::OpenerExt;

use std::sync::OnceLock;

static CACHED_HOSTNAME: OnceLock<String> = OnceLock::new();

pub(crate) fn get_hostname() -> String {
    CACHED_HOSTNAME.get_or_init(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "unknown-host".to_string())
    }).clone()
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);

        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();

        // 延迟再次聚焦，确保 macOS Dock 状态刷新并加载正确的图标
        let w = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = w.set_focus();
        });
    }
}

#[tauri::command]
fn hide_window(window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let _ = window.app_handle().set_activation_policy(tauri::ActivationPolicy::Accessory);
    window.hide().map_err(|e| e.to_string())
}

fn get_data_dir() -> Result<PathBuf, String> {
    let cfg = config::get_config()?;
    let data_dir = if cfg.storage_type == "git" {
        let app_dir = config::get_app_dir()?;
        let git_root = app_dir.join("git_data");
        if !git_root.exists() {
            fs::create_dir_all(&git_root).map_err(|e| e.to_string())?;
        }
        git_root.join(get_hostname())
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
fn save_snippets(app: tauri::AppHandle, snippets_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let snippets_path = data_dir.join("snippets.json");

    let mut file = File::create(snippets_path).map_err(|e| e.to_string())?;
    file.write_all(snippets_json.as_bytes())
        .map_err(|e| e.to_string())?;

    // Auto sync - Use async runtime to avoid blocking UI
    tauri::async_runtime::spawn(async move {
        let _ = git::sync_git(app).await;
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
fn save_bookmarks(app: tauri::AppHandle, bookmarks_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let bookmarks_path = data_dir.join("bookmarks.json");

    let mut file = File::create(bookmarks_path).map_err(|e| e.to_string())?;
    file.write_all(bookmarks_json.as_bytes())
        .map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn(async move {
        let _ = git::sync_git(app).await;
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
fn save_notes(app: tauri::AppHandle, notes_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let notes_path = data_dir.join("notes.json");

    let mut file = File::create(notes_path).map_err(|e| e.to_string())?;
    file.write_all(notes_json.as_bytes())
        .map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn(async move {
        let _ = git::sync_git(app).await;
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
    pub start_command: String,
}

fn get_tmux_command() -> Command {
    static TMUX_PATH: OnceLock<String> = OnceLock::new();
    let path = TMUX_PATH.get_or_init(|| {
        let paths = [
            "/opt/homebrew/bin/tmux",
            "/usr/local/bin/tmux",
            "/usr/bin/tmux",
            "/bin/tmux",
        ];

        // First check if 'tmux' is already in PATH
        if Command::new("tmux")
            .arg("-V")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return "tmux".to_string();
        }

        // Then check common absolute paths
        for p in paths {
            if std::path::Path::new(p).exists() {
                return p.to_string();
            }
        }

        "tmux".to_string()
    });
    Command::new(path)
}

fn get_brew_command() -> Command {
    static BREW_PATH: OnceLock<String> = OnceLock::new();
    let path = BREW_PATH.get_or_init(|| {
        let paths = [
            "/opt/homebrew/bin/brew",
            "/usr/local/bin/brew",
        ];

        if Command::new("brew")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return "brew".to_string();
        }

        for p in paths {
            if std::path::Path::new(p).exists() {
                return p.to_string();
            }
        }

        "brew".to_string()
    });
    Command::new(path)
}

pub fn get_git_command() -> Command {
    static GIT_PATH: OnceLock<String> = OnceLock::new();
    let path = GIT_PATH.get_or_init(|| {
        let paths = [
            "/opt/homebrew/bin/git",
            "/usr/local/bin/git",
            "/usr/bin/git",
            "/bin/git",
        ];

        if Command::new("git")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return "git".to_string();
        }

        for p in paths {
            if std::path::Path::new(p).exists() {
                return p.to_string();
            }
        }

        "git".to_string()
    });
    Command::new(path)
}

#[tauri::command]
fn get_tmux_sessions() -> Result<Vec<TmuxSession>, String> {
    let output = get_tmux_command()
        .arg("ls")
        .arg("-F")
        .arg("#{session_name}|#{session_created}|#{session_attached}|#{pane_current_path}|#{pane_start_command}")
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
                if parts.len() >= 4 {
                    let start_cmd = if parts.len() > 4 { parts[4].to_string() } else { "".to_string() };
                    sessions.push(TmuxSession {
                        name: parts[0].to_string(),
                        created: parts[1].parse().unwrap_or(0),
                        attached: parts[2] != "0",
                        path: parts[3].to_string(),
                        start_command: start_cmd.replace("\"", ""), // Remove quotes from command
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

    let status = get_tmux_command()
        .args(&args)
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() {
        return Err("Failed to create tmux session. Please check if the command is valid.".into());
    }

    // Enable mouse support for scrolling and set a larger history limit
    let _ = get_tmux_command()
        .args(["set-option", "-t", session_name, "mouse", "on"])
        .status();

    let _ = get_tmux_command()
        .args(["set-option", "-t", session_name, "history-limit", "50000"])
        .status();

    Ok(())
}

#[tauri::command]
fn attach_tmux_session(session_name: &str) -> Result<(), String> {
    // Use AppleScript to open Mac Terminal and attach to the session
    // We open a new window and execute the attach command
    let tmux_path = get_tmux_command().get_program().to_string_lossy().into_owned();
    
    // Sanitize session name to prevent injection in AppleScript
    let sanitized_session_name: String = session_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect();

    if sanitized_session_name.is_empty() {
        return Err("Invalid session name.".into());
    }

    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "'{1}' set-option -t {0} mouse on; '{1}' attach -t {0}"
        end tell"#,
        sanitized_session_name,
        tmux_path
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
    let status = get_tmux_command()
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
    let status = get_tmux_command()
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

    let tmux_bin = get_tmux_command().get_program().to_string_lossy().into_owned();

    let script_content = format!(r#"#!/usr/bin/env bash

TMUX_BIN="{tmux_bin}"

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
    SESSION_NAME="${{PWD##*/}}_ai"
fi
SESSION_NAME=$(echo "$SESSION_NAME" | sed 's/[. ]/_/g')

    case "$MODEL_SHORTCUT" in
    claude) CMD="claude code" ;;
    gemini) CMD="gemini -y" ;;
    opencode) CMD="opencode" ;;
    codex) CMD="codex" ;;
    *) echo "Unknown model: $MODEL_SHORTCUT"; exit 1 ;;
esac

# Collect all remaining arguments as model arguments
if [ $# -gt 0 ]; then
    CMD="$CMD $@"
fi

echo "Starting AI session '$SESSION_NAME' using $MODEL_SHORTCUT in $PWD..."
"$TMUX_BIN" new-session -d -s "$SESSION_NAME" -c "$PWD" "$CMD"

if [ $? -eq 0 ]; then
    # Enable mouse support and set scrollback history limit
    "$TMUX_BIN" set-option -t "$SESSION_NAME" mouse on
    "$TMUX_BIN" set-option -t "$SESSION_NAME" history-limit 50000
    
    echo "Session created successfully."
    echo "Attaching to session '$SESSION_NAME'..."
    "$TMUX_BIN" attach -t "$SESSION_NAME"
else
    echo "Failed to create session."
fi
"#, tmux_bin = tmux_bin);

    file.write_all(script_content.as_bytes())
        .map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIcon};
use tauri::{WebviewUrl, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use std::str::FromStr;

#[tauri::command]
fn resize_window(window: tauri::Window, height: f64) -> Result<(), String> {
    window.set_size(tauri::LogicalSize::new(600.0, height)).map_err(|e| e.to_string())
}

#[tauri::command]
fn check_cli_installed() -> bool {
    let home_dir = match dirs::home_dir() {
        Some(path) => path,
        None => return false,
    };
    let script_path = home_dir.join(".local").join("bin").join("onespace");
    script_path.exists()
}

#[tauri::command]
fn check_tmux_installed() -> bool {
    get_tmux_command()
        .arg("-V")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tauri::command]
async fn install_tmux() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        // Check if brew is installed
        let brew_check = get_brew_command()
            .arg("--version")
            .status();
        
        if brew_check.is_err() || !brew_check.unwrap().success() {
            return Err("Homebrew is not installed. Please install Homebrew first (https://brew.sh) or install tmux manually.".to_string());
        }
        
        // Run brew install tmux
        let output = get_brew_command()
            .arg("install")
            .arg("tmux")
            .output()
            .map_err(|e| format!("Failed to run brew install: {}", e))?;
        
        if output.status.success() {
            Ok("Tmux installed successfully!".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to install tmux: {}", stderr))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("One-click install is currently only supported on macOS. Please install tmux manually using your package manager (e.g., sudo apt install tmux).".to_string())
    }
}

#[tauri::command]
fn update_shortcuts(app: tauri::AppHandle, main: String, quick: String) -> Result<(), String> {
    let global_shortcut = app.global_shortcut();
    let _ = global_shortcut.unregister_all();

    // Register Main Toggle
    if let Ok(s) = Shortcut::from_str(&main) {
        let _ = global_shortcut.on_shortcut(s, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                show_main_window(app.clone());
            }
        });
    }

    // Register Quick AI Toggle
    if let Ok(s) = Shortcut::from_str(&quick) {
        let _ = global_shortcut.on_shortcut(s, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_quick_ai_window(app);
            }
        });
    }

    Ok(())
}

fn toggle_quick_ai_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("quick-ai") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    } else {
        // Create the quick-ai window if it doesn't exist
        let _ = tauri::WebviewWindowBuilder::new(
            app,
            "quick-ai",
            WebviewUrl::App("index.html?view=quick-ai".into()),
        )
        .title("Quick AI Session")
        .inner_size(600.0, 70.0)
        .resizable(true)
        .decorations(false)
        .always_on_top(true)
        .center()
        .transparent(true)
        .skip_taskbar(true)
        .build();
    }
}

use tauri_plugin_global_shortcut::ShortcutState;

fn get_tray_label(lang: &str, id: &str) -> &'static str {
    match lang {
        "zh" => match id {
            "show" => "显示窗口",
            "quick" => "快速 AI 会话",
            "sync" => "立即同步",
            "quit" => "退出",
            _ => "",
        },
        _ => match id {
            "show" => "Show Window",
            "quick" => "Quick AI Session",
            "sync" => "Sync Now",
            "quit" => "Quit",
            _ => "",
        },
    }
}

fn create_tray_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>, lang: &str) -> tauri::Result<Menu<R>> {
    let show_i = MenuItem::with_id(app, "show", get_tray_label(lang, "show"), true, None::<&str>)?;
    let quick_i = MenuItem::with_id(app, "quick", get_tray_label(lang, "quick"), true, None::<&str>)?;
    let sync_i = MenuItem::with_id(app, "sync", get_tray_label(lang, "sync"), true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", get_tray_label(lang, "quit"), true, None::<&str>)?;
    
    Menu::with_items(app, &[
        &show_i, 
        &quick_i, 
        &tauri::menu::PredefinedMenuItem::separator(app)?, 
        &sync_i, 
        &tauri::menu::PredefinedMenuItem::separator(app)?, 
        &quit_i
    ])
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn update_tray_menu(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    let menu = create_tray_menu(&app, &lang).map_err(|e| e.to_string())?;
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_menu(Some(menu));
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    builder = builder.on_window_event(|window, event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window.hide();
            #[cfg(target_os = "macos")]
            if window.label() == "main" {
                let _ = window.app_handle().set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
        }
    });

    builder = builder.setup(|app| {
        #[cfg(target_os = "macos")]
        app.set_activation_policy(tauri::ActivationPolicy::Regular);

        // Setup Tray
        let cfg = config::get_config().unwrap_or_default();
        let lang = cfg.language.unwrap_or_else(|| "zh".to_string());
        let menu = create_tray_menu(app.handle(), &lang)?;

        let _tray = TrayIconBuilder::with_id("main")
            .icon(app.default_window_icon().unwrap().clone())
            .menu(&menu)
            .show_menu_on_left_click(true)
            .on_menu_event(|app, event| {
                match event.id.as_ref() {
                    "show" => { show_main_window(app.clone()); }
                    "quick" => { toggle_quick_ai_window(app); }
                    "sync" => { let _ = app.emit("trigger-sync", ()); }
                    "quit" => { app.exit(0); }
                    _ => {}
                }
            })
            .on_tray_icon_event(|_tray: &TrayIcon, _event| {
                // Left click now shows the menu via show_menu_on_left_click(true)
                // Right click events are not explicitly handled to show the menu
            })
            .build(app)?;

        // Initial Shortcut Setup
        let main_s = cfg.main_shortcut.unwrap_or_else(|| "Alt+Space".to_string());
        let quick_s = cfg.quick_ai_shortcut.unwrap_or_else(|| "Alt+Shift+A".to_string());

        let global_shortcut = app.global_shortcut();
        
        if let Ok(s) = Shortcut::from_str(&main_s) {
            let _ = global_shortcut.on_shortcut(s, move |app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    show_main_window(app.clone());
                }
            });
        }

        if let Ok(s) = Shortcut::from_str(&quick_s) {
            let _ = global_shortcut.on_shortcut(s, move |app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    toggle_quick_ai_window(app);
                }
            });
        }

        Ok(())
    });

    let app = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_oauth::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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
            quit_app,
            exchange_google_token,
            refresh_google_token,
            start_google_oauth,
            config::get_storage_config,
            config::save_storage_config,
            git::sync_git,
            ai_env::get_ai_providers,
            ai_env::save_ai_providers,
            ai_env::apply_ai_environment,
            ai_env::remove_ai_environment,
            update_shortcuts,
            update_tray_menu,
            hide_window,
            resize_window,
            show_main_window,
            check_cli_installed,
            check_tmux_installed,
            install_tmux
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_main_window(app_handle.clone());
        }
    });
}
