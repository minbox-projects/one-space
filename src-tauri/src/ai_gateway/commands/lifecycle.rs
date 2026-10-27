use super::capture_gateway_app_handle;
use crate::ai_gateway::migration::migrate_legacy_files;
use crate::ai_gateway::runtime_http::{autostart, server_status, start_server, stop_server};
use crate::ai_gateway::storage::modify_config;
use crate::ai_gateway::GatewayStatus;

/// Persist the enable intent after a successful start/stop so a later autostart
/// restores the last state. Re-reads the config first so other fields (and any
/// concurrent edits) are preserved; the flag is only written once the listener
/// transition succeeded.
fn persist_enabled(enabled: bool) -> Result<(), String> {
    modify_config(|config| {
        if config.enabled != enabled {
            config.enabled = enabled;
            Ok((true, ()))
        } else {
            Ok((false, ()))
        }
    })
}

#[tauri::command]
pub async fn ai_gateway_start(app: tauri::AppHandle) -> Result<GatewayStatus, String> {
    start_inner(Some(app)).await
}

/// Start the listener and persist the enable intent, with the application
/// handle injected so the server can broadcast runtime-state transitions.
pub(in crate::ai_gateway) async fn start_inner(
    app: Option<tauri::AppHandle>,
) -> Result<GatewayStatus, String> {
    if let Some(handle) = app.as_ref() {
        capture_gateway_app_handle(handle.clone());
    }
    let status = start_server(app).await?;
    persist_enabled(true)?;
    Ok(status)
}

#[tauri::command]
pub async fn ai_gateway_stop() -> Result<GatewayStatus, String> {
    let status = stop_server().await?;
    persist_enabled(false)?;
    Ok(status)
}

#[tauri::command]
pub fn ai_gateway_status() -> Result<GatewayStatus, String> {
    server_status()
}

pub async fn ai_gateway_autostart(app: tauri::AppHandle) -> Result<GatewayStatus, String> {
    capture_gateway_app_handle(app.clone());
    migrate_legacy_files();
    autostart(Some(app)).await
}
/// Start the process-owned template scheduler exactly once. Called from the app
/// setup after gateway autostart; a second call is a no-op.
pub fn start_template_auto_refresh_scheduler(app: tauri::AppHandle) {
    crate::ai_gateway::auto_refresh::start_scheduler(app);
}

/// Stop the process-owned template scheduler. Idempotent and safe when no
/// scheduler runs.
pub fn stop_template_auto_refresh_scheduler() {
    crate::ai_gateway::auto_refresh::stop_scheduler();
}

/// Stop the gateway listener from a synchronous shutdown path: signal the
/// running listener and clear the slot. Idempotent and safe when nothing runs.
pub fn stop_gateway_for_shutdown() {
    crate::ai_gateway::runtime_http::stop_server_now();
}
