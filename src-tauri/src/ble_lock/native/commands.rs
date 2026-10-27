//! Native IPC commands. Every command returns the one shared status snapshot.

use crate::ble_lock::{BleLockConfig, BleLockError, BleLockStatus};

use super::central_bridge::dispatch_sync_central;
use super::controller::BleLockPlatform;
use super::platform::request_accessibility;
use super::runtime::{
    commit_config, current_status, emit_devices, fallback_status, hardware_plan,
    mark_screen_locked, publish_status, runtime_guard,
};

/// Return the shared status snapshot (platform availability, radio/presence
/// state, selection, config and last safe error).
#[tauri::command]
pub fn ble_lock_get_status() -> Result<BleLockStatus, BleLockError> {
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Validate, persist and apply a new configuration.
///
/// Screensaver lock and display-off are mutually exclusive; the caller's copy is
/// normalized (screensaver wins) before persistence so both can never be true.
#[tauri::command]
pub fn ble_lock_save_config(config: BleLockConfig) -> Result<BleLockStatus, BleLockError> {
    let mut config = config;
    // Screensaver lock and display-off are mutually exclusive; screensaver wins.
    if config.use_screensaver_to_lock && config.sleep_display {
        config.sleep_display = false;
    }
    let app = commit_config(config)?;
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Start a user discovery scan without halting configured monitoring.
#[tauri::command]
pub fn ble_lock_scan_start() -> Result<BleLockStatus, BleLockError> {
    let (app, selection, mode, scan, background) = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.discovering = true;
        guard.monitor.set_scanning(true);
        guard.devices.clear();
        guard.last_status = None;
        let plan = hardware_plan(&guard);
        (guard.app.clone(), plan.0, plan.1, plan.2, plan.3)
    };
    dispatch_sync_central(&app, selection, mode, scan, background, false);
    emit_devices(&app);
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Stop user discovery; passive monitoring keeps its own scan if configured.
#[tauri::command]
pub fn ble_lock_scan_stop() -> Result<BleLockStatus, BleLockError> {
    let (app, selection, mode, scan, background) = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.discovering = false;
        guard.monitor.set_scanning(false);
        guard.last_status = None;
        let plan = hardware_plan(&guard);
        (guard.app.clone(), plan.0, plan.1, plan.2, plan.3)
    };
    dispatch_sync_central(&app, selection, mode, scan, background, false);
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Select the single monitored device, persisting before any runtime mutation.
#[tauri::command]
pub fn ble_lock_select_device(
    device_id: String,
    device_name: Option<String>,
) -> Result<BleLockStatus, BleLockError> {
    if device_id.trim().is_empty() {
        return Err(BleLockError::device_not_selected());
    }
    let mut config = {
        let guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.config.clone()
    };
    config.device_id = Some(device_id);
    config.device_name = device_name;
    let app = commit_config(config)?;
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Clear the selection, cancel connection/scan and persist the cleared state.
#[tauri::command]
pub fn ble_lock_clear_device() -> Result<BleLockStatus, BleLockError> {
    {
        if let Some(mut guard) = runtime_guard() {
            guard.discovering = false;
            guard.devices.clear();
        }
    }
    let mut config = {
        let guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.config.clone()
    };
    config.device_id = None;
    config.device_name = None;
    let app = commit_config(config)?;
    emit_devices(&app);
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Store the login password in the profile-local Keychain item (never returned).
#[tauri::command]
pub fn ble_lock_set_password(password: String) -> Result<BleLockStatus, BleLockError> {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.controller.set_password(&password)?;
        guard.monitor.set_password_configured(true);
        guard.last_status = None;
        guard.app.clone()
    };
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Delete the profile-local Keychain item.
#[tauri::command]
pub fn ble_lock_delete_password() -> Result<BleLockStatus, BleLockError> {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.controller.delete_password()?;
        guard.monitor.set_password_configured(false);
        guard.last_status = None;
        guard.app.clone()
    };
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Explicitly request Accessibility trust (asynchronous system prompt).
#[tauri::command]
pub fn ble_lock_request_accessibility() -> Result<BleLockStatus, BleLockError> {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        let trusted = request_accessibility();
        let observed = guard.controller.platform().accessibility_trusted() || trusted;
        guard.monitor.set_accessibility_trusted(observed);
        guard.last_status = None;
        guard.app.clone()
    };
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Manual lock: independent of proximity, Bluetooth and Keychain.
#[tauri::command]
pub fn ble_lock_lock_now() -> Result<BleLockStatus, BleLockError> {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.controller.lock_now();
        mark_screen_locked(&mut guard);
        guard.last_status = None;
        guard.app.clone()
    };
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}
