use super::{
    app_dir, database_path_in, export, read_config, validation_errors, write_config,
    AiRequestCaptureClearResult, AiRequestCaptureConfig, AiRequestCaptureConfigApplyResult,
    AiRequestCaptureCurlResult, AiRequestCaptureDetail, AiRequestCaptureExportInput,
    AiRequestCaptureExportResult, AiRequestCaptureListResult, AiRequestCaptureStatus,
    CaptureListQuery, CaptureStore,
};
use serde_json::json;
use std::path::Path;
use tauri::Emitter;

fn failed_status(port: u16, error: String) -> AiRequestCaptureStatus {
    AiRequestCaptureStatus {
        running: false,
        listen_address: "127.0.0.1".to_string(),
        port,
        last_error: Some(error),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn recover_from_dir(app_dir: &Path, enabled: bool) -> AiRequestCaptureStatus {
    let mut config = super::config::read_config_in(app_dir).unwrap_or_default();
    config.enabled = enabled;
    let status = match CaptureStore::open(database_path_in(app_dir)) {
        Ok(store) => {
            match store.recover_interrupted_and_cleanup(chrono::Utc::now().timestamp_millis()) {
                Ok(_) if config.enabled => failed_status(
                    config.port,
                    "AI request capture runtime recovery requires the asynchronous startup path"
                        .to_string(),
                ),
                Ok(_) => AiRequestCaptureStatus::stopped(config.port),
                Err(error) => failed_status(config.port, error),
            }
        }
        Err(error) => failed_status(config.port, error),
    };
    super::runtime::set_recovery_status(status)
}

fn store() -> Result<CaptureStore, String> {
    CaptureStore::open(database_path_in(&app_dir()?))
}

pub async fn ai_request_capture_autostart() -> AiRequestCaptureStatus {
    match app_dir().and_then(|dir| read_config().map(|config| (dir, config))) {
        Ok((dir, config)) => match CaptureStore::open(database_path_in(&dir)).and_then(|store| {
            store.recover_interrupted_and_cleanup(chrono::Utc::now().timestamp_millis())
        }) {
            Ok(_) => super::runtime::apply_config(&dir, config, None).await,
            Err(error) => super::runtime::set_recovery_status(failed_status(config.port, error)),
        },
        Err(error) => super::runtime::set_recovery_status(failed_status(17688, error)),
    }
}

#[tauri::command]
pub fn ai_request_capture_get_config() -> Result<AiRequestCaptureConfig, String> {
    read_config()
}

#[tauri::command]
pub async fn ai_request_capture_save_config(
    app: tauri::AppHandle,
    config: AiRequestCaptureConfig,
) -> Result<AiRequestCaptureConfigApplyResult, String> {
    let validation_errors = validation_errors(&config);
    if !validation_errors.is_empty() {
        return Ok(AiRequestCaptureConfigApplyResult {
            config,
            status: super::runtime::current_status(),
            validation_errors,
        });
    }
    write_config(&config)?;
    let status = match app_dir() {
        Ok(dir) => super::runtime::apply_config(&dir, config.clone(), Some(app.clone())).await,
        Err(error) => super::runtime::set_recovery_status(failed_status(config.port, error)),
    };
    Ok(AiRequestCaptureConfigApplyResult {
        config,
        status,
        validation_errors: Vec::new(),
    })
}

#[tauri::command]
pub async fn ai_request_capture_start(
    app: tauri::AppHandle,
) -> Result<AiRequestCaptureStatus, String> {
    let config = read_config()?;
    Ok(super::runtime::start(&app_dir()?, config, Some(app)).await)
}

#[tauri::command]
pub async fn ai_request_capture_stop(
    app: tauri::AppHandle,
) -> Result<AiRequestCaptureStatus, String> {
    Ok(super::runtime::stop_with_port(read_config()?.port, Some(app)).await)
}

#[tauri::command]
pub fn ai_request_capture_status() -> Result<AiRequestCaptureStatus, String> {
    Ok(super::runtime::current_status())
}

#[tauri::command]
pub fn ai_request_capture_list(
    query: Option<CaptureListQuery>,
) -> Result<AiRequestCaptureListResult, String> {
    store()?.list(query.unwrap_or_default())
}

#[tauri::command]
pub fn ai_request_capture_get(id: String) -> Result<AiRequestCaptureDetail, String> {
    store()?
        .get(&id)?
        .ok_or_else(|| format!("capture not found: {id}"))
}

#[tauri::command]
pub fn ai_request_capture_clear(
    app: tauri::AppHandle,
) -> Result<AiRequestCaptureClearResult, String> {
    let result = AiRequestCaptureClearResult {
        cleared: store()?.clear()?,
    };
    let _ = app.emit("ai-request-capture-updated", json!({ "kind": "cleared" }));
    Ok(result)
}

#[tauri::command]
pub fn ai_request_capture_export_har(
    input: AiRequestCaptureExportInput,
) -> Result<AiRequestCaptureExportResult, String> {
    export::export_har(&store()?, input)
}

#[tauri::command]
pub fn ai_request_capture_generate_curl(id: String) -> Result<AiRequestCaptureCurlResult, String> {
    let record = store()?
        .get(&id)?
        .ok_or_else(|| format!("capture not found: {id}"))?;
    Ok(export::curl_command(&record))
}
