use super::records::lock_service_provider_operation;
use crate::app_store::{
    api_error, api_ok, apply_provider_id_map_to_dependent_state,
    auto_import_system_provider_into_service_state, cli_has_system_config, detect_cli_installation,
    enqueue_sync_event, get_meta, is_managed_tool, list_synced_device_providers,
    load_service_providers_state, normalize_service_provider_ids, process_sync_queue,
    read_system_provider, run_migration_impl, save_service_providers_internal,
    service_provider_matches_system_default, ApiErr, ApiMeta, ApiOk,
};
use serde_json::{json, Value};

#[tauri::command]
pub fn service_providers_list_synced_other_devices() -> Result<ApiOk<Vec<Value>>, ApiErr> {
    let devices = list_synced_device_providers()?
        .into_iter()
        .map(|device| serde_json::to_value(device).unwrap_or(Value::Null))
        .collect();
    api_ok(devices, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn service_providers_auto_import_from_system(
    app: tauri::AppHandle,
    tool: String,
) -> Result<ApiOk<Value>, ApiErr> {
    {
        let _migration_guard = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
        if let Err(e) = run_migration_impl() {
            return Err(api_error("migration_failed", e));
        }
    }
    if !is_managed_tool(&tool) {
        return Err(api_error(
            "invalid_tool",
            "tool does not support env managed import",
        ));
    }

    let (installed, _) = detect_cli_installation(&tool);
    if !installed {
        return api_ok(
            json!({ "imported": false, "reason": "not_installed" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }
    if !cli_has_system_config(&tool) {
        return api_ok(
            json!({ "imported": false, "reason": "not_configured" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    if state
        .providers
        .iter()
        .any(|provider| service_provider_matches_system_default(provider, &tool))
    {
        return api_ok(
            json!({ "imported": false, "reason": "provider_exists" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let provider = read_system_provider(&tool).ok_or_else(|| {
        api_error(
            "import_failed",
            "failed to parse system config for selected tool",
        )
    })?;
    let outcome = auto_import_system_provider_into_service_state(&mut state, &tool, provider)
        .map_err(|e| api_error("import_failed", e))?;
    if !outcome.imported {
        return api_ok(
            json!({
                "imported": false,
                "reason": outcome.reason.unwrap_or("skipped")
            }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let (id_map, changed_ids) = normalize_service_provider_ids(&mut state);
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    if changed_ids {
        apply_provider_id_map_to_dependent_state(&id_map).map_err(|e| api_error("io_error", e))?;
    }
    enqueue_sync_event("service_providers", "auto_import_system_config")
        .map_err(|e| api_error("sync_error", e))?;

    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({
            "imported": true,
            "provider_id": outcome.provider_id.unwrap_or_default(),
            "tool": tool,
            "activated": outcome.activated,
            "missing_fields": outcome.missing_fields
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}
