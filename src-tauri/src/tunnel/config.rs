use std::fs;
use std::path::PathBuf;
use crate::app_runtime::atomic_write_string;
use crate::config::get_app_dir;
use super::types::{TunnelConfig, TunnelMode};

pub fn tunnels_path() -> Result<PathBuf, String> {
    Ok(get_app_dir()?.join("tunnels.json"))
}

pub fn validate_tunnel_config(config: &TunnelConfig) -> Result<(), String> {
    if config.id.trim().is_empty() {
        return Err("Tunnel ID cannot be empty".to_string());
    }
    if config.name.trim().is_empty() {
        return Err("Tunnel name cannot be empty".to_string());
    }
    if config.local_port == 0 {
        return Err("Local port must be between 1 and 65535".to_string());
    }

    match config.mode {
        TunnelMode::Cloudflare => {
            // Cloudflare quick tunnel only requires local_port
        }
        TunnelMode::Frp => {
            let server = config.frp_server.as_deref().unwrap_or("").trim();
            if server.is_empty() {
                return Err("FRP server address is required".to_string());
            }
            let has_remote_port = config.remote_port.map(|p| p > 0).unwrap_or(false);
            let has_custom_domain = config
                .custom_domain
                .as_deref()
                .map(|d| !d.trim().is_empty())
                .unwrap_or(false);

            if !has_remote_port && !has_custom_domain {
                return Err("FRP tunnel requires either a remote port or a custom domain".to_string());
            }
        }
    }

    Ok(())
}

pub fn load_tunnels() -> Result<Vec<TunnelConfig>, String> {
    let path = tunnels_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read tunnels.json: {}", e))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse tunnels.json: {}", e))
}

pub fn save_tunnel(mut config: TunnelConfig) -> Result<TunnelConfig, String> {
    validate_tunnel_config(&config)?;
    let now = chrono::Utc::now().timestamp_millis();
    if config.created_at == 0 {
        config.created_at = now;
    }
    config.updated_at = now;

    let path = tunnels_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut list = load_tunnels()?;
    if let Some(pos) = list.iter().position(|t| t.id == config.id) {
        list[pos] = config.clone();
    } else {
        list.push(config.clone());
    }

    let json = serde_json::to_string_pretty(&list)
        .map_err(|e| format!("Failed to serialize tunnels: {}", e))?;
    atomic_write_string(&path, &json)?;

    Ok(config)
}

pub fn delete_tunnel(id: &str) -> Result<(), String> {
    let path = tunnels_path()?;
    if !path.exists() {
        return Ok(());
    }

    let mut list = load_tunnels()?;
    let prev_len = list.len();
    list.retain(|t| t.id != id);

    if list.len() != prev_len {
        let json = serde_json::to_string_pretty(&list)
            .map_err(|e| format!("Failed to serialize tunnels: {}", e))?;
        atomic_write_string(&path, &json)?;
    }

    Ok(())
}
