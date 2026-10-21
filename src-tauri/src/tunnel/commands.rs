use tauri::AppHandle;
use super::config::{delete_tunnel, load_tunnels, save_tunnel};
use super::environment::check_environment;
use super::supervisor::{get_logs, get_status, start_tunnel, stop_tunnel};
use super::types::{EnvironmentStatus, TunnelConfig, TunnelMode, TunnelRuntimeStatus};

#[tauri::command]
pub fn tunnel_list() -> Result<Vec<TunnelConfig>, String> {
    load_tunnels()
}

#[tauri::command]
pub fn tunnel_save(config: TunnelConfig) -> Result<TunnelConfig, String> {
    save_tunnel(config)
}

#[tauri::command]
pub fn tunnel_delete(id: String) -> Result<(), String> {
    delete_tunnel(&id)
}

#[tauri::command]
pub async fn tunnel_start(app: AppHandle, id: String) -> Result<TunnelRuntimeStatus, String> {
    let list = load_tunnels()?;
    let config = list
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("Tunnel not found: {}", id))?;

    start_tunnel(&config, Some(app)).await
}

#[tauri::command]
pub async fn tunnel_stop(app: AppHandle, id: String) -> Result<TunnelRuntimeStatus, String> {
    stop_tunnel(&id, Some(app)).await
}

#[tauri::command]
pub fn tunnel_status(id: String) -> Result<TunnelRuntimeStatus, String> {
    Ok(get_status(&id))
}

#[tauri::command]
pub fn tunnel_list_status() -> Result<Vec<TunnelRuntimeStatus>, String> {
    Ok(super::supervisor::get_all_status())
}

#[tauri::command]
pub fn tunnel_get_logs(id: String) -> Result<Vec<String>, String> {
    Ok(get_logs(&id))
}

#[tauri::command]
pub fn tunnel_check_environment(mode: TunnelMode) -> Result<EnvironmentStatus, String> {
    Ok(check_environment(mode))
}
