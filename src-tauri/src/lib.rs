mod config;
mod git;
mod ai_env;
mod crypto;
mod secrets;
mod storage;
mod ai_sessions;
mod mcp_servers;
mod mcp_templates;
mod backup;
mod mcp_export;
mod version_detect;
mod config_conflict;
mod proxy;
mod app_store;
mod skills;

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

pub(crate) fn get_data_dir() -> Result<PathBuf, String> {
    let cfg = config::get_config()?;
    let data_dir = match cfg.storage_type.as_str() {
        "git" => {
            let app_dir = config::get_app_dir()?;
            let git_root = app_dir.join("git_data");
            if !git_root.exists() {
                fs::create_dir_all(&git_root).map_err(|e| e.to_string())?;
            }
            
            // Migration: if git_root has no files, but has a hostname dir, copy files up.
            let hostname = get_hostname();
            let host_dir = git_root.join(&hostname);
            if host_dir.exists() {
                let has_files_in_root = fs::read_dir(&git_root)
                    .map(|mut d| d.any(|e| e.map(|entry| entry.path().is_file()).unwrap_or(false)))
                    .unwrap_or(false);
                if !has_files_in_root {
                    if let Ok(entries) = fs::read_dir(&host_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() {
                                let _ = fs::copy(&path, git_root.join(path.file_name().unwrap()));
                            }
                        }
                    }
                }
            }
            git_root
        },
        "icloud" => {
            #[cfg(target_os = "macos")]
            {
                if let Some(ref custom_path) = cfg.icloud_storage_path {
                    PathBuf::from(custom_path)
                } else {
                    dirs::home_dir().ok_or("Could not find home directory")?.join("Library/Mobile Documents/com~apple~CloudDocs/onespace")
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                dirs::home_dir().ok_or("Could not find home directory")?.join(".config").join("onespace").join("data")
            }
        },
        _ => {
            if let Some(ref custom_path) = cfg.local_storage_path {
                PathBuf::from(custom_path)
            } else {
                let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
                home_dir.join(".config").join("onespace").join("data")
            }
        }
    };
    if !data_dir.exists() { fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?; }
    Ok(data_dir)
}

#[derive(Serialize, Deserialize)]
struct OAuthResult { code: String, redirect_uri: String }

#[tauri::command]
async fn start_google_oauth(app: tauri::AppHandle, client_id: String, scope: String) -> Result<OAuthResult, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let port = tauri_plugin_oauth::start(move |url| { let _ = tx.send(url); }).map_err(|e| e.to_string())?;
    let redirect_uri = format!("http://localhost:{}", port);
    let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").map_err(|e| e.to_string())?;
    url.query_pairs_mut().append_pair("client_id", &client_id).append_pair("redirect_uri", &redirect_uri).append_pair("response_type", "code").append_pair("scope", &scope).append_pair("access_type", "offline").append_pair("prompt", "consent");
    let auth_url = url.to_string();
    app.opener().open_url(auth_url, None::<&str>).map_err(|e| e.to_string())?;
    let url_str = rx.recv_timeout(std::time::Duration::from_secs(300)).map_err(|_| "OAuth login timed out after 5 minutes".to_string())?;
    let url = reqwest::Url::parse(&url_str).map_err(|e| e.to_string())?;
    let code = url.query_pairs().find(|(key, _)| key == "code").map(|(_, value)| value.to_string()).ok_or("No code found in redirect URL")?;
    Ok(OAuthResult { code, redirect_uri })
}

#[tauri::command]
fn open_local_path(path: &str) -> Result<(), String> {
    Command::new("open").arg(path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct SshHost { pub name: String, pub host_name: String, pub user: String, pub port: u16 }

#[tauri::command]
fn get_ssh_hosts() -> Result<Vec<SshHost>, String> {
    let mut hosts = Vec::new();
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let ssh_config_path = home_dir.join(".ssh").join("config");
    if !ssh_config_path.exists() { return Ok(hosts); }
    if let Ok(content) = fs::read_to_string(&ssh_config_path) {
        let mut current_host: Option<String> = None;
        let mut current_hostname = String::new();
        let mut current_user = String::new();
        let mut current_port = 22;
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() { continue; }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() { continue; }
            let key = parts[0].to_lowercase();
            if key == "host" && parts.len() > 1 {
                if let Some(name) = current_host.take() {
                    if name != "*" {
                        hosts.push(SshHost { name, host_name: if current_hostname.is_empty() { "Unknown".to_string() } else { current_hostname.clone() }, user: if current_user.is_empty() { "root".to_string() } else { current_user.clone() }, port: current_port });
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
                if let Ok(port) = parts[1].parse::<u16>() { current_port = port; }
            }
        }
        if let Some(name) = current_host {
            if name != "*" {
                hosts.push(SshHost { name, host_name: if current_hostname.is_empty() { "Unknown".to_string() } else { current_hostname.clone() }, user: if current_user.is_empty() { "root".to_string() } else { current_user.clone() }, port: current_port });
            }
        }
    }
    Ok(hosts)
}

#[tauri::command]
fn connect_ssh(host: &str) -> Result<(), String> {
    let script = format!(r#"tell application "Terminal"
        activate
        do script "ssh {}"
    end tell"#, host);
    Command::new("osascript").arg("-e").arg(&script).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn connect_ssh_custom(user: &str, host: &str, port: u16, auth_type: &str, auth_val: &str) -> Result<(), String> {
    let mut ssh_cmd = format!("ssh -p {} {}@{}", port, user, host);
    if auth_type == "key" && !auth_val.is_empty() { ssh_cmd = format!("ssh -i {} -p {} {}@{}", auth_val, port, user, host); }
    let script = if auth_type == "password" && !auth_val.is_empty() { 
        format!(r#"tell application "Terminal"
            activate
            set newTab to do script "{}"
            delay 1.5
            do script "{}" in newTab
        end tell"#, ssh_cmd, auth_val) 
    } else { 
        format!(r#"tell application "Terminal"
            activate
            do script "{}"
        end tell"#, ssh_cmd) 
    };
    Command::new("osascript").arg("-e").arg(&script).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn exchange_google_token(code: String, client_id: String, client_secret: String, redirect_uri: String) -> Result<String, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER.get()
        .ok_or("Proxy manager not initialized")?;
    let client = proxy_mgr.get_client()?;
    let res = client.post("https://oauth2.googleapis.com/token").form(&[("code", code.as_str()), ("client_id", client_id.as_str()), ("client_secret", client_secret.as_str()), ("redirect_uri", redirect_uri.as_str()), ("grant_type", "authorization_code")]).send().await.map_err(|e| e.to_string())?;
    res.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn refresh_google_token(refresh_token: String, client_id: String, client_secret: String) -> Result<String, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER.get()
        .ok_or("Proxy manager not initialized")?;
    let client = proxy_mgr.get_client()?;
    let res = client.post("https://oauth2.googleapis.com/token").form(&[("refresh_token", refresh_token.as_str()), ("client_id", client_id.as_str()), ("client_secret", client_secret.as_str()), ("grant_type", "refresh_token")]).send().await.map_err(|e| e.to_string())?;
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

#[tauri::command]
async fn proxy_http_request(
    url: String,
    method: String,
    headers: Option<std::collections::HashMap<String, String>>,
    body: Option<String>,
) -> Result<String, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER.get()
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
        if Command::new("brew").arg("--version").status().map(|s| s.success()).unwrap_or(false) { return "brew".to_string(); }
        for p in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] { if std::path::Path::new(p).exists() { return p.to_string(); } }
        "brew".to_string()
    });
    Command::new(path)
}

pub fn get_git_command() -> Command {
    static GIT_PATH: OnceLock<String> = OnceLock::new();
    let path = GIT_PATH.get_or_init(|| {
        if Command::new("git").arg("--version").status().map(|s| s.success()).unwrap_or(false) { return "git".to_string(); }
        for p in ["/opt/homebrew/bin/git", "/usr/local/bin/git", "/usr/bin/git", "/bin/git"] { if std::path::Path::new(p).exists() { return p.to_string(); } }
        "git".to_string()
    });
    Command::new(path)
}

#[tauri::command]
fn install_cli() -> Result<(), String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let local_bin = home_dir.join(".local").join("bin");
    if !local_bin.exists() { fs::create_dir_all(&local_bin).map_err(|e| e.to_string())?; }
    let script_path = local_bin.join("onespace");
    let mut file = File::create(&script_path).map_err(|e| e.to_string())?;
    let script_content = format!(r#"#!/usr/bin/env bash
if [ "$1" != "ai" ] || [ -z "$2" ]; then echo "Usage: onespace ai <model_shortcut> [session_name]"; exit 1; fi
MODEL_SHORTCUT="$2"
shift 2
case "$MODEL_SHORTCUT" in
    claude) CMD="claude code" ;;
    gemini) CMD="gemini -y" ;;
    opencode) CMD="opencode" ;;
    codex) CMD="codex" ;;
    *) echo "Unknown model: $MODEL_SHORTCUT"; exit 1 ;;
esac
if [ $# -gt 0 ]; then CMD="$CMD $@"; fi
eval "$CMD"
"#);
    file.write_all(script_content.as_bytes()).map_err(|e| e.to_string())?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?; }
    Ok(())
}

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder};
use tauri::{WebviewUrl, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use std::str::FromStr;

#[tauri::command]
fn resize_window(window: tauri::Window, height: f64) -> Result<(), String> {
    window.set_size(tauri::LogicalSize::new(600.0, height)).map_err(|e| e.to_string())
}

#[tauri::command]
fn check_cli_installed() -> bool {
    let home_dir = match dirs::home_dir() { Some(path) => path, None => return false };
    home_dir.join(".local").join("bin").join("onespace").exists()
}

#[tauri::command]
fn update_shortcuts(app: tauri::AppHandle, main: String, quick: String) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Ok(s) = Shortcut::from_str(&main) { let _ = gs.on_shortcut(s, move |app, _, event| { if event.state() == ShortcutState::Pressed { show_main_window(app.clone()); } }); }
    if let Ok(s) = Shortcut::from_str(&quick) { let _ = gs.on_shortcut(s, move |app, _, event| { if event.state() == ShortcutState::Pressed { toggle_quick_ai_window(app); } }); }
    Ok(())
}

fn toggle_quick_ai_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("quick-ai") { if window.is_visible().unwrap_or(false) { let _ = window.hide(); } else { let _ = window.show(); let _ = window.set_focus(); } }
    else { let _ = tauri::WebviewWindowBuilder::new(app, "quick-ai", WebviewUrl::App("index.html?view=quick-ai".into())).title("Quick AI").inner_size(600.0, 70.0).resizable(true).decorations(false).always_on_top(true).center().transparent(true).skip_taskbar(true).build(); }
}

use tauri_plugin_global_shortcut::ShortcutState;
fn get_tray_label(lang: &str, id: &str) -> &'static str {
    match lang { "zh" => match id { "show" => "显示窗口", "quick" => "快速 AI 会话", "sync" => "立即同步", "quit" => "退出", _ => "" }, _ => match id { "show" => "Show Window", "quick" => "Quick AI Session", "sync" => "Sync Now", "quit" => "Quit", _ => "" } }
}

fn create_tray_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>, lang: &str) -> tauri::Result<Menu<R>> {
    let show_i = MenuItem::with_id(app, "show", get_tray_label(lang, "show"), true, None::<&str>)?;
    let quick_i = MenuItem::with_id(app, "quick", get_tray_label(lang, "quick"), true, None::<&str>)?;
    let sync_i = MenuItem::with_id(app, "sync", get_tray_label(lang, "sync"), true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", get_tray_label(lang, "quit"), true, None::<&str>)?;
    Menu::with_items(app, &[&show_i, &quick_i, &tauri::menu::PredefinedMenuItem::separator(app)?, &sync_i, &tauri::menu::PredefinedMenuItem::separator(app)?, &quit_i])
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) { app.exit(0); }

#[tauri::command]
fn update_tray_menu(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    let menu = create_tray_menu(&app, &lang).map_err(|e| e.to_string())?;
    if let Some(tray) = app.tray_by_id("main") { let _ = tray.set_menu(Some(menu)); }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .on_window_event(|window, event| { if let WindowEvent::CloseRequested { api, .. } = event { api.prevent_close(); let _ = window.hide(); #[cfg(target_os = "macos")] if window.label() == "main" { let _ = window.app_handle().set_activation_policy(tauri::ActivationPolicy::Accessory); } } })
        .setup(|app| {
            #[cfg(target_os = "macos")] app.set_activation_policy(tauri::ActivationPolicy::Regular);
            let cfg = config::get_config().unwrap_or_default();
            let lang = cfg.language.unwrap_or_else(|| "zh".to_string());
            let menu = create_tray_menu(app.handle(), &lang)?;
            let _tray = TrayIconBuilder::with_id("main").icon(app.default_window_icon().unwrap().clone()).menu(&menu).show_menu_on_left_click(true).on_menu_event(|app, event| { match event.id.as_ref() { "show" => { show_main_window(app.clone()); } "quick" => { toggle_quick_ai_window(app); } "sync" => { let _ = app.emit("trigger-sync", ()); } "quit" => { app.exit(0); } _ => {} } }).build(app)?;
            let main_s = cfg.main_shortcut.unwrap_or_else(|| "Alt+Space".to_string());
            let quick_s = cfg.quick_ai_shortcut.unwrap_or_else(|| "Alt+Shift+A".to_string());
            let gs = app.global_shortcut();
            if let Ok(s) = Shortcut::from_str(&main_s) { let _ = gs.on_shortcut(s, move |app, _, event| { if event.state() == ShortcutState::Pressed { show_main_window(app.clone()); } }); }
            if let Ok(s) = Shortcut::from_str(&quick_s) { let _ = gs.on_shortcut(s, move |app, _, event| { if event.state() == ShortcutState::Pressed { toggle_quick_ai_window(app); } }); }
            
            crate::proxy::init_proxy_manager();
            setup_proxy_monitor(app.handle());
            let _ = app_store::ensure_migrated_on_startup();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::skills::skills_rescan_local(app_handle.clone()).await;
                let _ = crate::skills::skills_reconcile(app_handle, None).await;
            });
            
            Ok(())
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_oauth::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            install_cli, get_ssh_hosts, connect_ssh, connect_ssh_custom, 
            storage::read_snippets, storage::save_snippets, 
            storage::read_bookmarks, storage::save_bookmarks, 
            open_local_path, 
            storage::read_notes, storage::save_notes,
            storage::read_game_data, storage::save_game_data,
            quit_app, exchange_google_token, refresh_google_token, start_google_oauth, config::get_storage_config, config::save_storage_config, config::should_show_onboarding, ai_env::get_master_password, ai_env::change_master_password, secrets::get_secret, secrets::save_secret, secrets::delete_secret, update_shortcuts, update_tray_menu, hide_window, resize_window, show_main_window, check_cli_installed,
            // MCP Servers
            mcp_servers::get_mcp_servers, mcp_servers::save_mcp_server, mcp_servers::delete_mcp_server, mcp_servers::link_mcp_to_providers, mcp_servers::debug_decrypt_all,
            // MCP Templates
            mcp_templates::list_mcp_templates, mcp_templates::get_mcp_template,
            // Backup
            backup::create_backup, backup::list_backups, backup::restore_backup, backup::cleanup_old_backups, backup::delete_backup,
            // MCP Export/Import
            mcp_export::export_mcp_config, mcp_export::import_mcp_config,
            // Version Detection
            version_detect::detect_cli_version, version_detect::check_config_compatibility, version_detect::get_all_config_compatibility,
            // Config Conflict
            config_conflict::check_config_conflicts, config_conflict::apply_ai_environment_force,
            // Proxy
            proxy::get_proxy_config, proxy::save_proxy_config, proxy::test_proxy_connection, proxy_http_request,
            // New storage/domain/projection/sync/migration API
            app_store::storage_get_snapshot,
            app_store::providers_list,
            app_store::cli_env_probe,
            app_store::providers_auto_import_from_system,
            app_store::providers_upsert,
            app_store::providers_delete,
            app_store::providers_set_active,
            app_store::providers_set_env_managed,
            app_store::sessions_list,
            app_store::sessions_create,
            app_store::sessions_update,
            app_store::sessions_delete,
            app_store::sessions_launch,
            app_store::projection_apply,
            app_store::projection_dry_run,
            app_store::sync_enqueue,
            app_store::sync_run_now,
            app_store::sync_status,
            app_store::migration_status,
            app_store::migration_run,
            app_store::migration_rollback,
            // Skills
            skills::skills_config_get,
            skills::skills_config_save,
            skills::skills_list_installed,
            skills::skills_list_catalog,
            skills::skills_sync_now,
            skills::skills_sync_status_get,
            skills::skills_install,
            skills::skills_uninstall,
            skills::skills_detail_get,
            skills::skills_update_check,
            skills::skills_update_diff_preview,
            skills::skills_update_apply,
            skills::skills_rescan_local,
            skills::skills_reconcile,
            skills::skills_open_folder
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| { #[cfg(target_os = "macos")] if let tauri::RunEvent::Reopen { .. } = event { show_main_window(app_handle.clone()); } });
}
