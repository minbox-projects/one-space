use crate::ai_gateway::auto_refresh::request_rearm;
use crate::ai_gateway::runtime_http::{
    emit_config_updated, server_status, start_server, stop_server,
};
use crate::ai_gateway::storage::{modify_config, read_config};
use crate::ai_gateway::{
    normalize_template_auto_refresh_minutes, validate_gateway_port, validate_gateway_settings,
    validate_template_auto_refresh_minutes, GatewayConfig, GatewaySettings, TemplateAutoRefreshStatus,
};

#[tauri::command]
pub fn ai_gateway_get_config() -> Result<GatewayConfig, String> {
    read_config()
}

#[tauri::command]
pub fn ai_gateway_get_settings() -> Result<GatewaySettings, String> {
    let config = read_config()?;
    Ok(config.settings)
}

#[tauri::command]
pub async fn ai_gateway_save_settings(
    app: tauri::AppHandle,
    settings: GatewaySettings,
    port: Option<u16>,
) -> Result<GatewayConfig, String> {
    validate_gateway_settings(&settings)?;
    if let Some(p) = port {
        validate_gateway_port(p)?;
    }

    let current = read_config()?;
    let current_port = current.port;
    let new_port = port.unwrap_or(current_port);
    let port_changed = new_port != current_port;
    let bind_changed = settings.bind_address != current.settings.bind_address;
    let needs_restart = port_changed || bind_changed;

    let is_running = server_status()?.running;

    if needs_restart && is_running {
        stop_server().await?;

        let update_res = modify_config(|config| {
            config.port = new_port;
            config.settings = settings.clone();
            Ok((true, ()))
        });

        if let Err(e) = update_res {
            let _ = start_server(Some(app)).await;
            return Err(e);
        }

        if let Err(e) = start_server(Some(app.clone())).await {
            let _ = modify_config(|config| {
                config.port = current_port;
                config.settings = current.settings.clone();
                Ok((true, ()))
            });
            let _ = start_server(Some(app)).await;
            return Err(format!("failed to bind new address/port: {e}"));
        }
    } else {
        modify_config(|config| {
            if let Some(p) = port {
                config.port = p;
            }
            config.settings = settings;
            Ok((true, ()))
        })?;
    }

    emit_config_updated();
    read_config()
}
#[tauri::command]
pub fn ai_gateway_template_auto_refresh_get() -> Result<u32, String> {
    Ok(normalize_template_auto_refresh_minutes(
        read_config()?.template_auto_refresh_minutes,
    ))
}

/// Replace only the automatic provider-template refresh interval; values other
/// than `0` (disabled) or 10-1440 minutes are rejected with an actionable error
/// and are never persisted. A successful save re-arms the process scheduler
/// without starting an immediate batch.
#[tauri::command]
pub fn ai_gateway_template_auto_refresh_save(minutes: i64) -> Result<u32, String> {
    let validated = validate_template_auto_refresh_minutes(minutes)?;
    modify_config(|config| {
        config.template_auto_refresh_minutes = validated;
        Ok((true, ()))
    })?;
    request_rearm();
    Ok(read_config()?.template_auto_refresh_minutes)
}

/// The process-memory automatic-refresh failure snapshot, so opening a page
/// receives existing inline errors without starting another schedule.
#[tauri::command]
pub fn ai_gateway_template_auto_refresh_status() -> Result<TemplateAutoRefreshStatus, String> {
    Ok(crate::ai_gateway::auto_refresh::failure_snapshot())
}
