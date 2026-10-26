//! Tauri command boundary for the server monitor runtime.
//!
//! Snapshot and history commands read local state only. Upsert, enable,
//! settings and delete commands publish a new safe state only after the
//! encrypted configuration write succeeds.

use crate::server_monitor::runtime::{global, ServerMonitorRuntime};
use crate::server_monitor::types::{
    ServerMonitorHistoryRange, ServerMonitorSample, ServerMonitorSettings, ServerMonitorState,
    ServerMonitorTargetDraft, ServerMonitorTestResult,
};
use std::sync::Arc;

fn runtime() -> Result<Arc<ServerMonitorRuntime>, String> {
    global()
}

/// Installs the process runtime once from application setup.
pub(crate) fn init_runtime(app: tauri::AppHandle) -> Result<(), String> {
    crate::server_monitor::runtime::init_global(app).map(|_| ())
}

#[tauri::command]
pub(crate) fn server_monitor_snapshot() -> Result<ServerMonitorState, String> {
    let runtime = runtime()?;
    if let Some(error) = runtime.configuration_error() {
        return Err(error);
    }
    Ok(runtime.snapshot())
}

#[tauri::command]
pub(crate) fn server_monitor_upsert(
    draft: ServerMonitorTargetDraft,
) -> Result<ServerMonitorState, String> {
    runtime()?.upsert_target(draft)
}

#[tauri::command]
pub(crate) fn server_monitor_delete(target_id: String) -> Result<ServerMonitorState, String> {
    runtime()?.delete_target(&target_id)
}

#[tauri::command]
pub(crate) fn server_monitor_set_enabled(
    target_id: String,
    enabled: bool,
) -> Result<ServerMonitorState, String> {
    runtime()?.set_target_enabled(&target_id, enabled)
}

#[tauri::command]
pub(crate) async fn server_monitor_refresh(
    target_id: String,
) -> Result<ServerMonitorState, String> {
    tauri::async_runtime::spawn_blocking(move || runtime()?.refresh_target(&target_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn server_monitor_test_connection(
    draft: ServerMonitorTargetDraft,
) -> Result<ServerMonitorTestResult, String> {
    tauri::async_runtime::spawn_blocking(move || runtime()?.test_connection(draft))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) fn server_monitor_history(
    target_id: String,
    range: ServerMonitorHistoryRange,
) -> Result<Vec<ServerMonitorSample>, String> {
    runtime()?.history(&target_id, range)
}

#[tauri::command]
pub(crate) fn server_monitor_get_settings() -> Result<ServerMonitorSettings, String> {
    let runtime = runtime()?;
    if let Some(error) = runtime.configuration_error() {
        return Err(error);
    }
    runtime.get_settings()
}

#[tauri::command]
pub(crate) fn server_monitor_save_settings(
    settings: ServerMonitorSettings,
) -> Result<ServerMonitorState, String> {
    runtime()?.save_settings(settings)
}
