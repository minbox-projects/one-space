mod ai_assistant;
mod ai_env;
mod ai_news;
mod ai_sessions;
mod app_store;
mod assistant_mcp;
mod backup;
mod cli_probe;
mod cli_updates;
mod claude_profiles;
mod config;
mod config_conflict;
mod crypto;
mod git;
mod mcp_export;
mod mcp_runtime;
mod mcp_servers;
mod mcp_templates;
mod messages;
mod proxy;
mod runtime_profiles;
mod secrets;
mod skills;
mod ssh_tunnels;
mod storage;
mod subagents;
mod version_detect;
mod workflows;
mod workspaces;

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tauri::{Manager, WindowEvent};
use tauri_plugin_opener::OpenerExt;

use std::sync::OnceLock;

static CACHED_HOSTNAME: OnceLock<String> = OnceLock::new();

pub(crate) fn get_hostname() -> String {
    CACHED_HOSTNAME
        .get_or_init(|| {
            hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "unknown-host".to_string())
        })
        .clone()
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let w = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = w.set_focus();
        });
    }
    ssh_tunnels::ssh_tunnels_on_window_show(app);
}

fn toggle_main_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        let is_minimized = window.is_minimized().unwrap_or(false);
        if is_visible && !is_minimized {
            #[cfg(target_os = "macos")]
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let _ = window.hide();
        } else {
            show_main_window(app);
        }
    }
}

#[tauri::command]
fn hide_window(window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let _ = window
        .app_handle()
        .set_activation_policy(tauri::ActivationPolicy::Accessory);
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_quick_ai_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("quick-ai") {
        window.hide().map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
fn show_quick_assistant_window(app: tauri::AppHandle) -> Result<(), String> {
    toggle_quick_assistant_window(&app);
    Ok(())
}

#[tauri::command]
fn hide_quick_assistant_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("quick-assistant") {
        window.hide().map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
fn show_selection_assistant_window(app: tauri::AppHandle) -> Result<(), String> {
    toggle_selection_assistant_window(&app);
    Ok(())
}

#[tauri::command]
fn hide_selection_assistant_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("selection-assistant") {
        window.hide().map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

pub(crate) fn get_data_dir() -> Result<PathBuf, String> {
    // Local-first mirror: all runtime reads/writes are resolved to local mirror,
    // then synced to selected shared backend (local/iCloud/git) in sync pipeline.
    config::get_local_data_dir()
}

#[derive(Serialize, Deserialize)]
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
    let port = tauri_plugin_oauth::start(move |url| {
        let _ = tx.send(url);
    })
    .map_err(|e| e.to_string())?;
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
    app.opener()
        .open_url(auth_url, None::<&str>)
        .map_err(|e| e.to_string())?;
    let url_str = rx
        .recv_timeout(std::time::Duration::from_secs(300))
        .map_err(|_| "OAuth login timed out after 5 minutes".to_string())?;
    let url = reqwest::Url::parse(&url_str).map_err(|e| e.to_string())?;
    let code = url
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .ok_or("No code found in redirect URL")?;
    Ok(OAuthResult { code, redirect_uri })
}

#[tauri::command]
fn open_local_path(path: &str) -> Result<(), String> {
    open_path_with_system(path)
}

pub(crate) fn open_path_with_system(path: &str) -> Result<(), String> {
    // Ensure the directory exists before opening
    std::fs::create_dir_all(path).map_err(|e| format!("Failed to create directory: {}", e))?;
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
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
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let ssh_config_path = home_dir.join(".ssh").join("config");
    if !ssh_config_path.exists() {
        return Ok(hosts);
    }
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
    let mut ssh_cmd = format!("ssh -p {} {}@{}", port, user, host);
    if auth_type == "key" && !auth_val.is_empty() {
        ssh_cmd = format!("ssh -i {} -p {} {}@{}", auth_val, port, user, host);
    }
    let script = if auth_type == "password" && !auth_val.is_empty() {
        format!(
            r#"tell application "Terminal"
            activate
            set newTab to do script "{}"
            delay 1.5
            do script "{}" in newTab
        end tell"#,
            ssh_cmd, auth_val
        )
    } else {
        format!(
            r#"tell application "Terminal"
            activate
            do script "{}"
        end tell"#,
            ssh_cmd
        )
    };
    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn exchange_google_token(
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
) -> Result<String, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER
        .get()
        .ok_or("Proxy manager not initialized")?;
    let client = proxy_mgr.get_client()?;
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
    res.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn refresh_google_token(
    refresh_token: String,
    client_id: String,
    client_secret: String,
) -> Result<String, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER
        .get()
        .ok_or("Proxy manager not initialized")?;
    let client = proxy_mgr.get_client()?;
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
    res.text().await.map_err(|e| e.to_string())
}

fn setup_proxy_monitor(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;

        loop {
            let interval_mins = crate::config::get_config()
                .ok()
                .and_then(|c| c.proxy.map(|p| p.check_interval))
                .unwrap_or(15);

            tokio::time::sleep(Duration::from_secs(interval_mins * 60)).await;

            if let Some(proxy_mgr) = crate::proxy::PROXY_MANAGER.get() {
                if proxy_mgr.is_enabled() {
                    match proxy_mgr.test_proxy().await {
                        Ok(status) => {
                            let _ = app.emit("proxy-status-update", &status);
                            if !status.is_available {
                                log::warn!("Proxy check failed: {}", status.message);
                            }
                        }
                        Err(e) => {
                            log::error!("Proxy test error: {}", e);
                        }
                    }
                }
            }
        }
    });
}

fn setup_sessions_history_sync_service(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let _ = app_store::run_sessions_history_sync_pass(app.clone()).await;

        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.tick().await;
        loop {
            interval.tick().await;
            let _ = app_store::run_sessions_history_sync_pass(app.clone()).await;
        }
    });
}

#[tauri::command]
async fn proxy_http_request(
    url: String,
    method: String,
    headers: Option<std::collections::HashMap<String, String>>,
    body: Option<String>,
) -> Result<String, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER
        .get()
        .ok_or("Proxy manager not initialized")?;
    let client = proxy_mgr.get_client()?;

    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|e| format!("Invalid method: {}", e))?;

    let mut req = client.request(method, &url);

    if let Some(h) = headers {
        for (key, value) in h {
            req = req.header(&key, &value);
        }
    }

    if let Some(b) = body {
        req = req.body(b);
    }

    let res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status();
    let text = res.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), text));
    }

    Ok(text)
}

#[allow(dead_code)]
fn get_brew_command() -> Command {
    static BREW_PATH: OnceLock<String> = OnceLock::new();
    let path = BREW_PATH.get_or_init(|| {
        if Command::new("brew")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return "brew".to_string();
        }
        for p in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
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
        if Command::new("git")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return "git".to_string();
        }
        for p in [
            "/opt/homebrew/bin/git",
            "/usr/local/bin/git",
            "/usr/bin/git",
            "/bin/git",
        ] {
            if std::path::Path::new(p).exists() {
                return p.to_string();
            }
        }
        "git".to_string()
    });
    Command::new(path)
}

const INTERNAL_CLI_RESOLVE_SESSION_COMMAND: &str = "__onespace_cli_resolve_session";
const INTERNAL_CLI_CLAUDE_PROFILE_SET_DEFAULT: &str = "__onespace_cli_claude_profile_set_default";
const INTERNAL_CLI_GET_CLAUDE_CONFIG_DIR: &str = "__onespace_cli_get_claude_config_dir";

fn handle_internal_cli_command() -> bool {
    let mut args = std::env::args();
    let _ = args.next();
    let Some(command) = args.next() else {
        return false;
    };

    match command.as_str() {
        INTERNAL_CLI_RESOLVE_SESSION_COMMAND => {
            let query = args.next().unwrap_or_default();
            match app_store::cli_lookup_session(&query) {
                Ok(Some(record)) => {
                    println!(
                        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                        record.tool, record.tool_session_id, record.working_dir, record.id
                    );
                    std::process::exit(0);
                }
                Ok(None) => {
                    eprintln!("Session not found: {}", query);
                    std::process::exit(1);
                }
                Err(err) => {
                    eprintln!("Failed to resolve session: {}", err);
                    std::process::exit(1);
                }
            }
        }
        INTERNAL_CLI_CLAUDE_PROFILE_SET_DEFAULT => {
            let profile_id = args.next().unwrap_or_default();
            let mut state = match app_store::load_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            if let Err(e) = crate::claude_profiles::set_default_claude_profile(&mut state, &profile_id) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
            if let Err(e) = app_store::save_providers_state(&state) {
                eprintln!("Failed to save providers: {}", e);
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        INTERNAL_CLI_GET_CLAUDE_CONFIG_DIR => {
            let profile_id = args.next().unwrap_or_default();
            let state = match crate::app_store::load_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            let provider = state.providers.iter()
                .find(|p| p.core.id == profile_id && p.core.tool == "claude");
            match provider {
                Some(p) => {
                    let dir_name = crate::claude_profiles::resolve_claude_dir_name(p);
                    let dir = match crate::claude_profiles::get_claude_profiles_dir() {
                        Ok(d) => d.join(&dir_name),
                        Err(e) => {
                            eprintln!("{}", e);
                            std::process::exit(1);
                        }
                    };
                    println!("{}", dir.to_string_lossy());
                    std::process::exit(0);
                }
                None => {
                    eprintln!("Claude profile not found: {}", profile_id);
                    std::process::exit(1);
                }
            }
        }
        _ => return false,
    }
}

#[tauri::command]
fn install_cli() -> Result<(), String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let local_bin = home_dir.join(".local").join("bin");
    if !local_bin.exists() {
        fs::create_dir_all(&local_bin).map_err(|e| e.to_string())?;
    }
    let script_path = local_bin.join("onespace");

    let data_dir = get_data_dir()?;
    let sessions_path = data_dir.join("ai_sessions.json");
    let providers_path = data_dir.join("providers.json");
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let app_bin = current_exe.to_string_lossy().to_string();

    let mut file = File::create(&script_path).map_err(|e| e.to_string())?;

    let script_content = format!(
        r#"#!/usr/bin/env bash

# OneSpace AI CLI Tool
# Usage: 
#   onespace ai <model_shortcut> [session_name]
#   onespace resume <session_id>
#   onespace env list
#   onespace env use <tool> <provider_name_or_id>

SESSIONS_FILE="{}"
PROVIDERS_FILE="{}"
APP_BIN="{}"
CONFIG_FILE="$HOME/.config/onespace/config.json"

resolve_current_data_dir() (
    # v2 local-first storage layout
    local default_local="$HOME/.config/onespace/local_data"
    echo "$default_local"
)

DATA_DIR=$(resolve_current_data_dir)
if [ -n "$DATA_DIR" ] && [ "$DATA_DIR" != "." ]; then
    SESSIONS_FILE="$DATA_DIR/ai_sessions.json"
    PROVIDERS_FILE="$DATA_DIR/providers.json"
fi

print_help() (
    cat <<'EOF'
OneSpace CLI

Usage:
  onespace <command> [options]

Commands:
  ai <model_shortcut> [session_name] [extra args...]
      Start an AI terminal session in current working directory.
      Models: claude, gemini, opencode, codex

  resume <session_id>
      Resume a saved session by Session ID from OneSpace AI Sessions.

  claude profile <subcommand>
      Manage Claude profiles (list, set, launch).

  env list
      List configured provider environments and active bindings.

  env use <tool> <provider_name_or_id>
      Switch active provider for a tool.

Options:
  -h, --help    Show this help message

Examples:
  onespace ai claude my_session
  onespace ai gemini
  onespace resume 9b6f4b6e-2c63-4a11-9f7a-demo
  onespace claude profile list
  onespace claude profile set work
  onespace claude profile work
  onespace env list
  onespace env use claude my-provider
EOF
)

print_claude_profile_help() (
    cat <<'EOF'
Usage:
  onespace claude profile list                           List Claude profiles
  onespace claude profile set <profile>                  Set default Claude profile
  onespace claude profile <profile> [-- <claude args>]   Launch Claude with profile

Examples:
  onespace claude profile list
  onespace claude profile set work
  onespace claude profile work -- --model opus
EOF
)

print_env_help() (
    cat <<'EOF'
Usage:
  onespace env list
  onespace env use <tool> <provider_name_or_id>
EOF
)

print_ai_help() (
    cat <<'EOF'
Usage:
  onespace ai <model_shortcut> [session_name] [extra args...]
Models:
  claude, gemini, opencode, codex
EOF
)

print_resume_help() (
    cat <<'EOF'
Usage:
  onespace resume <session_id>

Resume a saved OneSpace session by Session ID copied from AI Sessions.
OneSpace will choose the correct native resume command for the tool.
EOF
)

provider_name_by_id() (
    local provider_id="$1"
    if [ -z "$provider_id" ]; then
        return 0
    fi
    grep -o '"id":"'"$provider_id"'","name":"[^"]*"' "$PROVIDERS_FILE" | head -n1 | sed 's/"id":"[^"]*","name":"\([^"]*\)"/\1/'
)

resolve_session_record() (
    local lookup="$1"

    if [ -z "$lookup" ]; then
        echo "Usage: onespace resume <session_id>" >&2
        return 1
    fi

    if [ ! -x "$APP_BIN" ]; then
        echo "OneSpace app binary not found: $APP_BIN" >&2
        echo "Tip: reopen OneSpace and click Update CLI to refresh the installed script." >&2
        return 1
    fi

    "$APP_BIN" __onespace_cli_resolve_session "$lookup"
)

if [ -z "$1" ] || [ "$1" == "--help" ] || [ "$1" == "-h" ]; then
    print_help
    exit 0
fi

if [ "$1" == "resume" ]; then
    if [ -z "$2" ] || [ "$2" == "--help" ] || [ "$2" == "-h" ]; then
        print_resume_help
        exit 0
    fi

    SESSION_LOOKUP="$2"
    SESSION_RECORD=$(resolve_session_record "$SESSION_LOOKUP")
    STATUS=$?
    if [ $STATUS -ne 0 ]; then
        exit $STATUS
    fi

    IFS=$'\037' read -r SESSION_TOOL RESUME_TOOL_SESSION_ID SESSION_WORKING_DIR ONESPACE_SESSION_ID <<EOF
$SESSION_RECORD
EOF

    if [ -z "$RESUME_TOOL_SESSION_ID" ]; then
        echo "Session found, but native tool session ID is not available yet." >&2
        echo "Tip: wait for OneSpace history sync to finish, then retry." >&2
        exit 1
    fi

    case "$SESSION_TOOL" in
        claude)
            RESUME_CMD=(claude -r "$RESUME_TOOL_SESSION_ID")
            ;;
        gemini)
            RESUME_CMD=(gemini -r "$RESUME_TOOL_SESSION_ID")
            ;;
        opencode)
            RESUME_CMD=(opencode -s "$RESUME_TOOL_SESSION_ID")
            ;;
        codex)
            RESUME_CMD=(codex resume "$RESUME_TOOL_SESSION_ID")
            ;;
        *)
            echo "Unsupported session tool: $SESSION_TOOL" >&2
            exit 1
            ;;
    esac

    if [ -z "$SESSION_WORKING_DIR" ]; then
        echo "Session found, but working directory is missing." >&2
        echo "Refusing to resume outside the original session directory." >&2
        exit 1
    fi

    if [ ! -d "$SESSION_WORKING_DIR" ]; then
        echo "Original working directory not found: $SESSION_WORKING_DIR" >&2
        echo "Refusing to resume outside the original session directory." >&2
        exit 1
    fi

    cd "$SESSION_WORKING_DIR" || exit 1

    echo "Resuming OneSpace session: $SESSION_LOOKUP ($SESSION_TOOL)"
    exec "${{RESUME_CMD[@]}}"
fi

# --- Claude Profile Management ---
if [ "$1" == "claude" ]; then
    if [ -z "$2" ] || [ "$2" == "--help" ] || [ "$2" == "-h" ]; then
        cat <<'EOF'
Usage:
  onespace claude profile <subcommand>

Manage Claude profiles. Use "onespace claude profile --help" for details.
EOF
        exit 0
    fi

    if [ "$2" != "profile" ]; then
        echo "Unknown claude command: $2"
        echo "Usage: onespace claude profile <subcommand>"
        exit 1
    fi

    if [ -z "$3" ] || [ "$3" == "--help" ] || [ "$3" == "-h" ]; then
        print_claude_profile_help
        exit 0
    fi

    if [ "$3" == "list" ]; then
        if [ ! -f "$PROVIDERS_FILE" ]; then
            echo "No Claude profiles configured."
            echo "Tip: open OneSpace once to refresh provider snapshot, then rerun this command."
            exit 0
        fi
        echo "Claude Profiles:"
        echo "----------------"
        grep -o '"id":"[^"]*","name":"[^"]*","tool":"claude"[^}}]*}}' "$PROVIDERS_FILE" | while IFS= read -r entry; do
            PROFILE_ID=$(echo "$entry" | sed 's/.*"id":"\([^"]*\)".*/\1/')
            PROFILE_NAME=$(echo "$entry" | sed 's/.*"name":"\([^"]*\)".*/\1/')
            DEFAULT_MARK=""
            if grep -q '"active_claude"[[:space:]]*:[[:space:]]*"'"$PROFILE_ID"'"' "$PROVIDERS_FILE" 2>/dev/null; then
                DEFAULT_MARK=" [default]"
            fi
            CONFIG_DIR="$HOME/.config/onespace/claude_profiles/$PROFILE_ID"
            # Try to use `code` field if available
            if echo "$entry" | grep -q '"code"'; then
                PROFILE_CODE=$(echo "$entry" | sed 's/.*"code":"\([^"]*\)".*/\1/')
                if [ -n "$PROFILE_CODE" ]; then
                    CONFIG_DIR="$HOME/.config/onespace/claude_profiles/$PROFILE_CODE"
                fi
            fi
            echo "  $PROFILE_NAME ($PROFILE_ID)$DEFAULT_MARK"
            echo "    Config Dir: $CONFIG_DIR"
        done
        exit 0
    fi

    if [ "$3" == "set" ]; then
        if [ -z "$4" ]; then
            echo "Usage: onespace claude profile set <profile_id>"
            exit 1
        fi
        PROFILE_ID="$4"
        "$APP_BIN" __onespace_cli_claude_profile_set_default "$PROFILE_ID"
        STATUS=$?
        if [ $STATUS -eq 0 ]; then
            echo "Default Claude profile set to: $PROFILE_ID"
        fi
        exit $STATUS
    fi

    # onespace claude profile <profile> [-- <claude args>]
    PROFILE_ID="$3"
    shift 3

    # Resolve profile config dir
    CONFIG_DIR=$("$APP_BIN" __onespace_cli_get_claude_config_dir "$PROFILE_ID" 2>/dev/null)
    STATUS=$?
    if [ $STATUS -ne 0 ] || [ -z "$CONFIG_DIR" ]; then
        echo "Claude profile not found: $PROFILE_ID" >&2
        exit 1
    fi

    echo "Starting Claude with profile: $PROFILE_ID"
    echo "Config dir: $CONFIG_DIR"

    if [ $# -gt 0 ] && [ "$1" == "--" ]; then
        shift
    fi

    CLAUDE_CONFIG_DIR="$CONFIG_DIR" exec claude "$@"
fi

# --- Environment Management ---
if [ "$1" == "env" ]; then
    if [ -z "$2" ] || [ "$2" == "--help" ] || [ "$2" == "-h" ]; then
        print_env_help
        exit 0
    fi

    if [ "$2" == "list" ]; then
        if [ ! -f "$PROVIDERS_FILE" ]; then
            echo "No providers configured."
            echo "Tip: open OneSpace once to refresh provider snapshot, then rerun this command."
            exit 0
        fi
        echo "Available Environments (Providers):"
        echo "----------------------------------"
        grep -o '"id":"[^"]*","name":"[^"]*","tool":"[^"]*"' "$PROVIDERS_FILE" | sed 's/"id":"[^"]*","name":"\([^"]*\)","tool":"\([^"]*\)"/\2 -> \1/'
        echo ""
        echo "Current Active:"
        grep -o '"active_[^"]*":"[^"]*"' "$PROVIDERS_FILE" | while IFS= read -r item; do
            TOOL=$(echo "$item" | sed 's/"active_\([^"]*\)":"[^"]*"/\1/')
            PROVID_ID=$(echo "$item" | sed 's/"active_[^"]*":"\([^"]*\)"/\1/')
            if [ -z "$PROVID_ID" ]; then
                continue
            fi
            PROVID_NAME=$(provider_name_by_id "$PROVID_ID")
            if [ -z "$PROVID_NAME" ]; then
                PROVID_NAME="$PROVID_ID"
            fi
            echo "$TOOL -> $PROVID_NAME"
        done
        exit 0
    elif [ "$2" == "use" ]; then
        TOOL="$3"
        TARGET="$4"
        if [ -z "$TOOL" ] || [ -z "$TARGET" ]; then
            echo "Usage: onespace env use <tool> <provider_name_or_id>"
            exit 1
        fi
        
        # Find Provider ID by name if not already an ID
        PROVID_ID=$(grep -o '"id":"[^"]*","name":"'"$TARGET"'"' "$PROVIDERS_FILE" | cut -d'"' -f4)
        if [ -z "$PROVID_ID" ]; then
            # Maybe it's already an ID
            PROVID_ID=$(grep -o '"id":"'"$TARGET"'"' "$PROVIDERS_FILE" | cut -d'"' -f4)
        fi
        
        if [ -z "$PROVID_ID" ]; then
            echo "Provider not found: $TARGET"
            exit 1
        fi
        
        # Update active_<tool> in providers.json
        # Regex replacement for "active_tool":"old_id" to "active_tool":"new_id"
        sed -i '' 's/"active_'"$TOOL"'"\s*:\s*"[^"]*"/"active_'"$TOOL"'" : "'"$PROVID_ID"'"/g' "$PROVIDERS_FILE"
        echo "Switched $TOOL to environment: $TARGET ($PROVID_ID)"
        exit 0
    else
        echo "Unknown env command: $2"
        print_env_help
        exit 1
    fi
fi

# --- AI Session Launcher ---
if [ "$1" != "ai" ]; then
    echo "Unknown command: $1"
    print_help
    exit 1
fi

if [ -z "$2" ] || [ "$2" == "--help" ] || [ "$2" == "-h" ]; then
    print_ai_help
    exit 0
fi

MODEL_SHORTCUT="$2"
WORKING_DIR=$(pwd)
DIR_NAME=$(basename "$WORKING_DIR")

# Handle Session Name
if [ -n "$3" ]; then
    SESSION_NAME="$3"
else
    SESSION_NAME="${{DIR_NAME}}_ai"
fi

# Replace spaces and dots with underscores
SESSION_NAME=$(echo "$SESSION_NAME" | sed 's/[ .]/_/g')

# Map Models
case "$MODEL_SHORTCUT" in
    claude) 
        CMD="claude code"
        TOOL_ID="claude"
        ;;
    gemini) 
        CMD="gemini -y" 
        TOOL_ID="gemini"
        ;;
    opencode) 
        CMD="opencode"
        TOOL_ID="opencode"
        ;;
    codex) 
        CMD="codex"
        TOOL_ID="codex"
        ;;
    *) 
        echo "Unknown model: $MODEL_SHORTCUT"
        print_ai_help
        exit 1 
        ;;
esac

# Sync to OneSpace (Write to ai_sessions.json)
CREATED_AT=$(date +%s)
SESSION_ID=$(uuidgen 2>/dev/null || echo "$CREATED_AT")
TOOL_SESSION_ID="$SESSION_NAME"

# Create simple JSON object
NEW_SESSION_JSON=$(printf '{{"id":"%s","name":"%s","working_dir":"%s","model_type":"%s","tool_session_id":"%s","created_at":%s}}' \
    "$SESSION_ID" "$SESSION_NAME" "$WORKING_DIR" "$TOOL_ID" "$TOOL_SESSION_ID" "$CREATED_AT")

# Add to JSON file if it exists, otherwise create new list
if [ -f "$SESSIONS_FILE" ]; then
    CONTENT=$(cat "$SESSIONS_FILE")
    if [[ "$CONTENT" == "[]" ]]; then
        echo "[$NEW_SESSION_JSON]" > "$SESSIONS_FILE"
    else
        echo "[${{NEW_SESSION_JSON}},${{CONTENT:1}}" > "$SESSIONS_FILE"
    fi
else
    echo "[$NEW_SESSION_JSON]" > "$SESSIONS_FILE"
fi

# Execute Command
if [ -n "$3" ]; then
    shift 3
else
    shift 2
fi

if [ $# -gt 0 ]; then
    CMD="$CMD $@"
fi

echo "Starting OneSpace AI session: $SESSION_NAME ($MODEL_SHORTCUT)"
eval "$CMD"
"#,
        sessions_path.to_string_lossy(),
        providers_path.to_string_lossy(),
        app_bin
    );

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

use std::str::FromStr;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, WebviewUrl};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[tauri::command]
fn resize_window(window: tauri::Window, height: f64) -> Result<(), String> {
    window
        .set_size(tauri::LogicalSize::new(600.0, height))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn check_cli_installed() -> bool {
    let home_dir = match dirs::home_dir() {
        Some(path) => path,
        None => return false,
    };
    home_dir
        .join(".local")
        .join("bin")
        .join("onespace")
        .exists()
}

#[tauri::command]
fn update_shortcuts(app: tauri::AppHandle, main: String, quick: String) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Ok(s) = Shortcut::from_str(&main) {
        let _ = gs.on_shortcut(s, move |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_main_window(app.clone());
            }
        });
    }
    if let Ok(s) = Shortcut::from_str(&quick) {
        let _ = gs.on_shortcut(s, move |app, _, event| {
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
            let w = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(120));
                let _ = w.set_focus();
            });
        }
    } else {
        if let Ok(window) = tauri::WebviewWindowBuilder::new(
            app,
            "quick-ai",
            WebviewUrl::App("index.html?view=quick-ai".into()),
        )
        .title("Quick AI")
        .inner_size(600.0, 70.0)
        .resizable(true)
        .decorations(false)
        .always_on_top(true)
        .center()
        .transparent(true)
        .skip_taskbar(true)
        .build()
        {
            let _ = window.set_focus();
            let w = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(180));
                let _ = w.set_focus();
            });
        }
    }
}

fn toggle_quick_assistant_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("quick-assistant") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.show();
            let _ = window.set_focus();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    } else if let Ok(window) = tauri::WebviewWindowBuilder::new(
        app,
        "quick-assistant",
        WebviewUrl::App("index.html?view=quick-assistant".into()),
    )
    .title("Quick Assistant")
    .inner_size(760.0, 560.0)
    .min_inner_size(540.0, 420.0)
    .resizable(true)
    .decorations(false)
    .always_on_top(true)
    .center()
    .transparent(false)
    .skip_taskbar(true)
    .build()
    {
        let _ = window.set_focus();
        let w = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(180));
            let _ = w.set_focus();
        });
    }
}

fn toggle_selection_assistant_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("selection-assistant") {
        let _ = window.show();
        let _ = window.set_focus();
    } else if let Ok(window) = tauri::WebviewWindowBuilder::new(
        app,
        "selection-assistant",
        WebviewUrl::App("index.html?view=selection-assistant".into()),
    )
    .title("Selection Assistant")
    .inner_size(760.0, 560.0)
    .min_inner_size(540.0, 420.0)
    .resizable(true)
    .decorations(false)
    .always_on_top(true)
    .center()
    .transparent(false)
    .skip_taskbar(true)
    .build()
    {
        let _ = window.set_focus();
        let w = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(180));
            let _ = w.set_focus();
        });
    }
}

use tauri_plugin_global_shortcut::ShortcutState;
fn get_tray_label(lang: &str, id: &str) -> &'static str {
    match lang {
        "zh" => match id {
            "show" => "显示窗口",
            "quick" => "快速 AI 会话",
            "search" => "全局搜索",
            "launcher" => "启动台",
            "sessions" => "AI 会话",
            "environments" => "AI 环境",
            "notes" => "笔记",
            "snippets" => "代码片段",
            "settings" => "设置",
            "sync" => "立即同步",
            "quit" => "退出",
            _ => "",
        },
        _ => match id {
            "show" => "Show Window",
            "quick" => "Quick AI Session",
            "search" => "Global Search",
            "launcher" => "Launcher",
            "sessions" => "AI Sessions",
            "environments" => "AI Environments",
            "notes" => "Notes",
            "snippets" => "Snippets",
            "settings" => "Settings",
            "sync" => "Sync Now",
            "quit" => "Quit",
            _ => "",
        },
    }
}

#[derive(Clone, Serialize)]
struct TrayActionPayload {
    action: &'static str,
    target: &'static str,
}

fn emit_tray_action(app: &tauri::AppHandle, target: &'static str) {
    let payload = TrayActionPayload {
        action: "navigate",
        target,
    };
    let _ = app.emit("tray-action", payload);
}

fn create_tray_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    lang: &str,
) -> tauri::Result<Menu<R>> {
    let show_i = MenuItem::with_id(
        app,
        "show",
        get_tray_label(lang, "show"),
        true,
        None::<&str>,
    )?;
    let quick_i = MenuItem::with_id(
        app,
        "quick",
        get_tray_label(lang, "quick"),
        true,
        None::<&str>,
    )?;
    let search_i = MenuItem::with_id(
        app,
        "search",
        get_tray_label(lang, "search"),
        true,
        None::<&str>,
    )?;
    let launcher_i = MenuItem::with_id(
        app,
        "launcher",
        get_tray_label(lang, "launcher"),
        true,
        None::<&str>,
    )?;
    let sessions_i = MenuItem::with_id(
        app,
        "sessions",
        get_tray_label(lang, "sessions"),
        true,
        None::<&str>,
    )?;
    let environments_i = MenuItem::with_id(
        app,
        "environments",
        get_tray_label(lang, "environments"),
        true,
        None::<&str>,
    )?;
    let notes_i = MenuItem::with_id(
        app,
        "notes",
        get_tray_label(lang, "notes"),
        true,
        None::<&str>,
    )?;
    let snippets_i = MenuItem::with_id(
        app,
        "snippets",
        get_tray_label(lang, "snippets"),
        true,
        None::<&str>,
    )?;
    let sync_i = MenuItem::with_id(
        app,
        "sync",
        get_tray_label(lang, "sync"),
        true,
        None::<&str>,
    )?;
    let settings_i = MenuItem::with_id(
        app,
        "settings",
        get_tray_label(lang, "settings"),
        true,
        None::<&str>,
    )?;
    let quit_i = MenuItem::with_id(
        app,
        "quit",
        get_tray_label(lang, "quit"),
        true,
        None::<&str>,
    )?;
    Menu::with_items(
        app,
        &[
            &show_i,
            &quick_i,
            &search_i,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &launcher_i,
            &sessions_i,
            &environments_i,
            &notes_i,
            &snippets_i,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &sync_i,
            &settings_i,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &quit_i,
        ],
    )
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    let _ = ssh_tunnels::shutdown_runtime();
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
    if handle_internal_cli_command() {
        return;
    }
    tauri::Builder::default()
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                if window.label() == "main" {
                    let _ = window
                        .app_handle()
                        .set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);
            let cfg = config::get_config().unwrap_or_default();
            let lang = cfg.language.unwrap_or_else(|| "zh".to_string());
            let menu = create_tray_menu(app.handle(), &lang)?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        show_main_window(app.clone());
                    }
                    "quick" => {
                        toggle_quick_ai_window(app);
                    }
                    "search" => {
                        show_main_window(app.clone());
                        emit_tray_action(app, "omni-search");
                    }
                    "launcher" => {
                        show_main_window(app.clone());
                        emit_tray_action(app, "launcher");
                    }
                    "sessions" => {
                        show_main_window(app.clone());
                        emit_tray_action(app, "ai-sessions");
                    }
                    "environments" => {
                        show_main_window(app.clone());
                        emit_tray_action(app, "ai-environments");
                    }
                    "notes" => {
                        show_main_window(app.clone());
                        emit_tray_action(app, "notes");
                    }
                    "snippets" => {
                        show_main_window(app.clone());
                        emit_tray_action(app, "snippets");
                    }
                    "sync" => {
                        let _ = app.emit("trigger-sync", ());
                    }
                    "settings" => {
                        show_main_window(app.clone());
                        emit_tray_action(app, "settings");
                    }
                    "quit" => {
                        let _ = ssh_tunnels::shutdown_runtime();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            let main_s = cfg.main_shortcut.unwrap_or_else(|| "Alt+Space".to_string());
            let quick_s = cfg
                .quick_ai_shortcut
                .unwrap_or_else(|| "Alt+Shift+A".to_string());
            let gs = app.global_shortcut();
            if let Ok(s) = Shortcut::from_str(&main_s) {
                let _ = gs.on_shortcut(s, move |app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_main_window(app.clone());
                    }
                });
            }
            if let Ok(s) = Shortcut::from_str(&quick_s) {
                let _ = gs.on_shortcut(s, move |app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_quick_ai_window(app);
                    }
                });
            }

            crate::proxy::init_proxy_manager();
            setup_proxy_monitor(app.handle());
            setup_sessions_history_sync_service(app.handle());
            crate::ai_assistant::init_scheduler(app.handle().clone());
            ssh_tunnels::start_system_wake_observer(app.handle().clone());
            ssh_tunnels::start_sleep_resume_monitor(app.handle().clone());
            // Avoid running heavy migration work before first-run onboarding.
            // Otherwise startup may create default data and suppress onboarding.
            let should_show_onboarding = config::should_show_onboarding().unwrap_or(false);
            if !should_show_onboarding {
                let _ = app_store::ensure_migrated_on_startup();
                let _ = workflows::workflows_cleanup_runtime_profiles_on_startup();
                workspaces::schedule_sync_from_sessions(app.handle().clone());
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = ssh_tunnels::ssh_tunnels_bootstrap(app_handle).await;
                });
            }
            std::thread::spawn(|| loop {
                std::thread::sleep(std::time::Duration::from_secs(30 * 60));
                let _ = workflows::workflows_cleanup_runtime_profiles_on_startup();
            });
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::skills::skills_rescan_mirror(app_handle.clone()).await;
                let _ = crate::skills::skills_reconcile(app_handle.clone(), None, None, None).await;
                let _ = crate::subagents::subagents_rescan_mirror(app_handle.clone()).await;
                let _ = crate::subagents::subagents_reconcile(app_handle, None, None, None).await;
            });

            Ok(())
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_oauth::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            install_cli,
            get_ssh_hosts,
            connect_ssh,
            connect_ssh_custom,
            ssh_tunnels::ssh_tunnel_groups_list,
            ssh_tunnels::ssh_tunnel_group_upsert,
            ssh_tunnels::ssh_tunnel_group_delete,
            ssh_tunnels::ssh_tunnels_list,
            ssh_tunnels::ssh_tunnel_upsert,
            ssh_tunnels::ssh_tunnel_delete,
            ssh_tunnels::ssh_tunnel_connect,
            ssh_tunnels::ssh_tunnel_disconnect,
            ssh_tunnels::ssh_tunnel_group_connect,
            ssh_tunnels::ssh_tunnel_group_disconnect,
            ssh_tunnels::ssh_tunnel_probe_draft,
            ssh_tunnels::ssh_tunnel_probe_saved,
            ssh_tunnels::ssh_tunnels_refresh_status,
            ssh_tunnels::ssh_tunnels_snapshot,
            storage::read_snippets,
            storage::save_snippets,
            storage::read_bookmarks,
            storage::save_bookmarks,
            open_local_path,
            storage::read_notes,
            storage::save_notes,
            storage::read_game_data,
            storage::save_game_data,
            ai_news::ai_news_read,
            ai_news::ai_news_sync_now,
            ai_news::ai_news_sync_status_get,
            quit_app,
            exchange_google_token,
            refresh_google_token,
            start_google_oauth,
            config::get_storage_config,
            config::save_storage_config,
            config::save_shared_profile,
            config::should_show_onboarding,
            messages::messages_list,
            messages::messages_unread_count,
            messages::messages_create,
            messages::messages_mark_read,
            messages::messages_mark_all_read,
            ai_env::get_master_password,
            ai_env::change_master_password,
            ai_env::skip_claude_onboarding_login,
            ai_assistant::ai_workspace_bootstrap,
            ai_assistant::workspace_settings_get,
            ai_assistant::workspace_settings_save,
            ai_assistant::workspace_model_roles_get,
            ai_assistant::workspace_model_roles_save,
            ai_assistant::provider_connection_test,
            ai_assistant::provider_models_fetch,
            ai_assistant::workspace_assistants_list,
            ai_assistant::workspace_assistant_upsert,
            ai_assistant::workspace_assistant_delete,
            ai_assistant::workspace_assistant_test_run,
            assistant_mcp::workspace_assistant_mcp_catalog,
            assistant_mcp::mcp_tool_preview_refresh,
            ai_assistant::workspace_conversations_list,
            ai_assistant::workspace_conversation_get,
            ai_assistant::workspace_conversation_create,
            ai_assistant::workspace_conversation_update,
            ai_assistant::workspace_conversation_delete,
            ai_assistant::workspace_conversation_reset_context,
            ai_assistant::workspace_schedule_resolve_draft,
            ai_assistant::workspace_conversation_send,
            ai_assistant::workspace_automations_list,
            ai_assistant::workspace_automation_upsert,
            ai_assistant::workspace_automation_delete,
            ai_assistant::workspace_automation_toggle,
            ai_assistant::workspace_automation_run_now,
            ai_assistant::workspace_quick_assistant_get,
            ai_assistant::workspace_quick_assistant_save,
            ai_assistant::workspace_selection_assistant_get,
            ai_assistant::workspace_selection_assistant_save,
            secrets::get_secret,
            secrets::save_secret,
            secrets::delete_secret,
            update_shortcuts,
            update_tray_menu,
            hide_window,
            hide_quick_ai_window,
            show_quick_assistant_window,
            hide_quick_assistant_window,
            show_selection_assistant_window,
            hide_selection_assistant_window,
            resize_window,
            show_main_window,
            check_cli_installed,
            // MCP Servers
            mcp_servers::get_mcp_servers,
            mcp_servers::save_mcp_server,
            mcp_servers::delete_mcp_server,
            mcp_servers::link_mcp_to_providers,
            mcp_servers::get_mcp_model_switch_states,
            mcp_servers::refresh_mcp_local_install_state,
            mcp_servers::set_mcp_model_switch,
            mcp_servers::mcp_updates_check_background,
            mcp_servers::mcp_updates_status_get,
            mcp_servers::mcp_update_apply,
            mcp_servers::debug_decrypt_all,
            // MCP Templates
            mcp_templates::list_mcp_templates,
            mcp_templates::get_mcp_template,
            // Backup
            backup::create_backup,
            backup::list_backups,
            backup::restore_backup,
            backup::cleanup_old_backups,
            backup::delete_backup,
            // MCP Export/Import
            mcp_export::export_mcp_config,
            mcp_export::import_mcp_config,
            // Version Detection
            version_detect::detect_cli_version,
            version_detect::check_config_compatibility,
            version_detect::get_all_config_compatibility,
            // CLI Updates
            cli_updates::check_cli_update,
            cli_updates::apply_cli_update,
            // Config Conflict
            config_conflict::check_config_conflicts,
            config_conflict::apply_ai_environment_force,
            // Proxy
            proxy::get_proxy_config,
            proxy::save_proxy_config,
            proxy::test_proxy_connection,
            proxy_http_request,
            // New storage/domain/projection/sync/migration API
            app_store::storage_get_snapshot,
            app_store::providers_list,
            app_store::providers_list_synced_other_devices,
            app_store::dashboard_counts,
            app_store::cli_env_probe,
            app_store::providers_auto_import_from_system,
            app_store::providers_upsert,
            app_store::providers_delete,
            app_store::providers_set_active,
            app_store::providers_set_env_managed,
            app_store::providers_export,
            app_store::providers_import_preview,
            app_store::providers_import_apply,
            app_store::launcher_list,
            app_store::launcher_upsert,
            app_store::launcher_delete,
            app_store::launcher_reorder,
            app_store::launcher_mark_launched,
            app_store::launcher_set_trust,
            app_store::launcher_export,
            app_store::launcher_import,
            app_store::launcher_execute,
            app_store::launcher_resolve_app_icon,
            app_store::sessions_list,
            app_store::sessions_create,
            app_store::sessions_update,
            app_store::sessions_delete,
            app_store::sessions_launch,
            app_store::sessions_set_favorite,
            app_store::claude_profile_list,
            app_store::claude_profile_resolve,
            app_store::claude_profile_set_default,
            app_store::get_claude_config_dir,
            app_store::claude_profile_materialize,
            app_store::projection_apply,
            app_store::projection_dry_run,
            app_store::sync_enqueue,
            app_store::sync_run_now,
            app_store::sync_status,
            app_store::migration_status,
            app_store::migration_run,
            app_store::migration_rollback,
            workspaces::workspaces_list,
            workspaces::workspace_get,
            workspaces::workspace_create,
            workspaces::workspace_update_meta,
            workspaces::workspace_delete,
            workspaces::workspace_sessions_list,
            workspaces::workspace_mcp_binding_upsert,
            workspaces::workspace_launch_session,
            workspaces::workspace_copy,
            // Skills
            skills::skills_config_get,
            skills::skills_config_save,
            skills::skills_sources_export_to_path,
            skills::skills_list_installed,
            skills::skills_repo_list,
            skills::skills_repo_refresh,
            skills::skills_repo_refresh_background,
            skills::skills_repo_set_model,
            skills::skills_repo_delete,
            skills::skills_list_catalog,
            skills::skills_sync_now,
            skills::skills_sync_status_get,
            skills::skills_local_scan,
            skills::skills_repo_list_with_update,
            skills::skills_repo_import_folder,
            skills::skills_local_import,
            skills::skills_install,
            skills::skills_uninstall,
            skills::skills_detail_get,
            skills::skills_catalog_detail_get,
            skills::skills_catalog_open_folder,
            skills::skills_repo_detail_get,
            skills::skills_repo_reload_preview,
            skills::skills_repo_reload_apply,
            skills::skills_repo_auto_update_pending,
            skills::skills_update_check,
            skills::skills_update_diff_preview,
            skills::skills_update_apply,
            skills::skills_rescan_local,
            skills::skills_rescan_mirror,
            skills::skills_reconcile,
            skills::skills_open_folder,
            // Subagents
            subagents::subagents_config_get,
            subagents::subagents_config_save,
            subagents::subagents_sources_export_to_path,
            subagents::subagents_list_installed,
            subagents::subagents_repo_list,
            subagents::subagents_repo_refresh,
            subagents::subagents_repo_refresh_background,
            subagents::subagents_repo_set_model,
            subagents::subagents_repo_delete,
            subagents::subagents_list_catalog,
            subagents::subagents_source_diagnose,
            subagents::subagents_sync_now,
            subagents::subagents_sync_status_get,
            subagents::subagents_local_scan,
            subagents::subagents_repo_list_with_update,
            subagents::subagents_repo_import_folder,
            subagents::subagents_local_import,
            subagents::subagents_install,
            subagents::subagents_uninstall,
            subagents::subagents_detail_get,
            subagents::subagents_catalog_detail_get,
            subagents::subagents_catalog_open_folder,
            subagents::subagents_repo_detail_get,
            subagents::subagents_repo_reload_preview,
            subagents::subagents_repo_reload_apply,
            subagents::subagents_update_check,
            subagents::subagents_update_diff_preview,
            subagents::subagents_update_apply,
            subagents::subagents_rescan_local,
            subagents::subagents_rescan_mirror,
            subagents::subagents_reconcile,
            subagents::subagents_open_folder,
            // Workflows
            workflows::workflows_presets_list,
            workflows::workflows_preset_upsert,
            workflows::workflows_preset_delete,
            workflows::workflows_check_dependencies,
            workflows::workflows_apply_dependencies,
            workflows::workflows_launch_preset,
            workflows::workflows_replay_run,
            workflows::workflows_runs_list,
            workflows::workflows_run_update,
            workflows::workflows_run_delete
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                show_main_window(app_handle.clone());
            }
            tauri::RunEvent::Exit => {
                let _ = ssh_tunnels::shutdown_runtime();
            }
            _ => {}
        });
}
