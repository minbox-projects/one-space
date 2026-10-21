use serde::Serialize;

use super::acme::{create_acme_order, verify_and_finalize_order};
use super::config::{load_config, save_config, SslConfig};
use super::dns_verifier::doh_check_txt;
use super::inspector::inspect_ssl;
use super::storage::{
    delete_issued_certificate, export_certificate_files, list_issued_certificates,
};
use super::types::{
    SslCheckResult, SslDohPrecheckResult, SslIssuedCertificate, SslOrderRequest, SslOrderResponse,
    SslWatchItem, SslWatchItemInput,
};
use super::watchlist::{add_watch_item, check_all_watchlist, load_watchlist, remove_watch_item};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SslExportResult {
    pub success: bool,
    pub export_path: String,
}

#[tauri::command]
pub async fn ssl_check_certificate(
    domain: String,
    port: Option<u16>,
) -> Result<SslCheckResult, String> {
    inspect_ssl(&domain, port.unwrap_or(443)).await
}

#[tauri::command]
pub fn ssl_watchlist_list() -> Result<Vec<SslWatchItem>, String> {
    load_watchlist()
}

#[tauri::command]
pub fn ssl_watchlist_add(input: SslWatchItemInput) -> Result<SslWatchItem, String> {
    add_watch_item(input)
}

#[tauri::command]
pub fn ssl_watchlist_remove(id: String) -> Result<(), String> {
    remove_watch_item(&id)
}

#[tauri::command]
pub async fn ssl_watchlist_check_all() -> Result<Vec<SslWatchItem>, String> {
    check_all_watchlist().await
}

#[tauri::command]
pub async fn ssl_certificate_create_order(
    request: SslOrderRequest,
) -> Result<SslOrderResponse, String> {
    create_acme_order(request).await
}

#[tauri::command]
pub async fn ssl_certificate_doh_precheck(
    record_name: String,
    expected_value: String,
) -> Result<SslDohPrecheckResult, String> {
    Ok(doh_check_txt(&record_name, &expected_value).await)
}

#[tauri::command]
pub async fn ssl_certificate_verify_and_finalize(
    order_id: String,
) -> Result<SslIssuedCertificate, String> {
    verify_and_finalize_order(&order_id).await
}

#[tauri::command]
pub fn ssl_certificate_list() -> Result<Vec<SslIssuedCertificate>, String> {
    list_issued_certificates()
}

#[tauri::command]
pub fn ssl_certificate_delete(id: String) -> Result<(), String> {
    delete_issued_certificate(&id)
}

#[tauri::command]
pub fn ssl_certificate_export(id: String, target_dir: String) -> Result<SslExportResult, String> {
    let export_path = export_certificate_files(&id, &target_dir)?;
    Ok(SslExportResult {
        success: true,
        export_path,
    })
}

#[tauri::command]
pub fn ssl_get_config() -> Result<SslConfig, String> {
    load_config()
}

#[tauri::command]
pub fn ssl_save_config(config: SslConfig) -> Result<SslConfig, String> {
    save_config(&config)
}
