use super::forwarder::execute_replay;
use super::server::WebhookServerManager;
use super::storage::{
    clear_captured_requests, delete_endpoint, get_captured_requests, load_endpoints,
    upsert_endpoint,
};
use super::types::{
    CapturedWebhookRequest, ReplayRequest, ReplayResult, WebhookEndpoint, WebhookServerStatus,
};

#[tauri::command]
pub async fn webhook_get_server_status() -> Result<WebhookServerStatus, String> {
    let manager = WebhookServerManager::global();
    Ok(manager.get_status())
}

#[tauri::command]
pub async fn webhook_start_server(port: Option<u16>) -> Result<u16, String> {
    let manager = WebhookServerManager::global();
    manager.start(port).await
}

#[tauri::command]
pub async fn webhook_stop_server() -> Result<(), String> {
    let manager = WebhookServerManager::global();
    manager.stop()
}

#[tauri::command]
pub async fn webhook_list_endpoints() -> Result<Vec<WebhookEndpoint>, String> {
    load_endpoints()
}

#[tauri::command]
pub async fn webhook_save_endpoint(mut endpoint: WebhookEndpoint) -> Result<WebhookEndpoint, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if endpoint.id.trim().is_empty() {
        endpoint.id = uuid::Uuid::new_v4().to_string();
        endpoint.created_at = now;
    }

    if !endpoint.path.starts_with('/') {
        endpoint.path = format!("/{}", endpoint.path);
    }

    upsert_endpoint(endpoint)
}

#[tauri::command]
pub async fn webhook_delete_endpoint(id: String) -> Result<(), String> {
    delete_endpoint(&id)
}

#[tauri::command]
pub async fn webhook_list_requests(
    endpoint_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<CapturedWebhookRequest>, String> {
    Ok(get_captured_requests(endpoint_id.as_deref(), limit))
}

#[tauri::command]
pub async fn webhook_clear_requests(endpoint_id: Option<String>) -> Result<(), String> {
    clear_captured_requests(endpoint_id.as_deref());
    Ok(())
}

#[tauri::command]
pub async fn webhook_execute_replay(param: ReplayRequest) -> Result<ReplayResult, String> {
    execute_replay(param).await
}
