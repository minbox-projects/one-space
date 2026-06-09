use super::{toggle_quick_assistant_window, toggle_selection_assistant_window};
use crate::{config, ssh_tunnels};
use std::path::PathBuf;
use tauri::Manager;

use std::sync::OnceLock;

pub(super) static CACHED_HOSTNAME: OnceLock<String> = OnceLock::new();

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
pub(super) fn show_main_window(app: tauri::AppHandle) {
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

pub(super) fn toggle_main_window(app: tauri::AppHandle) {
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
pub(super) fn hide_window(window: tauri::Window) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let _ = window
        .app_handle()
        .set_activation_policy(tauri::ActivationPolicy::Accessory);
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub(super) fn hide_quick_ai_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("quick-ai") {
        window.hide().map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub(super) fn show_quick_assistant_window(app: tauri::AppHandle) -> Result<(), String> {
    toggle_quick_assistant_window(&app);
    Ok(())
}

#[tauri::command]
pub(super) fn hide_quick_assistant_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("quick-assistant") {
        window.hide().map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub(super) fn show_selection_assistant_window(app: tauri::AppHandle) -> Result<(), String> {
    toggle_selection_assistant_window(&app);
    Ok(())
}

#[tauri::command]
pub(super) fn hide_selection_assistant_window(app: tauri::AppHandle) -> Result<(), String> {
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

#[cfg(test)]
pub(crate) fn lock_test_home_env() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static HOME_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    HOME_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}
