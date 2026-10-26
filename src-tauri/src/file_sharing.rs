mod http;
mod runtime;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use runtime::request_shutdown;

#[tauri::command]
pub fn file_sharing_networks() -> Result<Vec<types::FileSharingNetwork>, String> {
    runtime::networks()
}

#[tauri::command]
pub async fn file_sharing_start(
    app: tauri::AppHandle,
    input: types::FileSharingStartInput,
) -> Result<types::FileSharingSnapshot, String> {
    runtime::start(app, input).await
}

#[tauri::command]
pub fn file_sharing_status() -> Result<types::FileSharingSnapshot, String> {
    runtime::status()
}

#[tauri::command]
pub async fn file_sharing_stop(
    app: tauri::AppHandle,
) -> Result<types::FileSharingSnapshot, String> {
    runtime::stop(app).await
}

#[tauri::command]
pub async fn file_sharing_public_start(
    app: tauri::AppHandle,
) -> Result<types::FileSharingSnapshot, String> {
    runtime::start_public(app).await
}

#[tauri::command]
pub async fn file_sharing_public_stop(
    app: tauri::AppHandle,
) -> Result<types::FileSharingSnapshot, String> {
    runtime::stop_public(app).await
}
