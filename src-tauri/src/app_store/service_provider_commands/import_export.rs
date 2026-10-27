use super::records::{
    lock_service_provider_operation, merge_imported_service_provider, service_provider_from_value,
    service_provider_to_value,
};
use crate::app_store::{
    api_error, api_ok, apply_provider_id_map_to_dependent_state, enqueue_sync_event,
    expand_home_dir_path, generate_provider_uuid, get_meta, load_service_providers_state,
    normalize_service_provider_ids, now_ts, process_sync_queue, provider_import_key,
    save_service_providers_internal, ApiErr, ApiMeta, ApiOk, ProviderImportDecision,
    ProviderImportPreviewItem, ProvidersImportPreview, ServiceProviderRecord,
    ServiceProvidersState, StorageEngine, PROVIDERS_EXPORT_VERSION,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{self};

#[tauri::command]
pub fn service_providers_export(output_path: String) -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let providers: Vec<Value> = state
        .providers
        .iter()
        .map(service_provider_to_value)
        .collect();
    let payload = json!({
        "format": "onespace-service-providers",
        "version": PROVIDERS_EXPORT_VERSION,
        "exported_at": now_ts(),
        "active": state.active,
        "providers": providers,
    });
    let content = serde_json::to_string_pretty(&payload)
        .map_err(|e| api_error("serialize_error", e.to_string()))?;
    let expanded_output_path =
        expand_home_dir_path(&output_path).map_err(|e| api_error("io_error", e))?;
    let final_output_path = if expanded_output_path.is_dir() {
        expanded_output_path.join("onespace-service-providers-export.json")
    } else {
        expanded_output_path
    };
    StorageEngine::atomic_write(&final_output_path, &content)
        .map_err(|e| api_error("io_error", e))?;
    api_ok(
        json!({
            "path": final_output_path.to_string_lossy().to_string(),
            "count": payload.get("providers").and_then(|v| v.as_array()).map(|arr| arr.len()).unwrap_or(0)
        }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn service_providers_import_preview(import_path: String) -> Result<ApiOk<Value>, ApiErr> {
    let expanded =
        expand_home_dir_path(&import_path).map_err(|e| api_error("invalid_payload", e))?;
    let content =
        fs::read_to_string(&expanded).map_err(|e| api_error("io_error", e.to_string()))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| api_error("invalid_payload", e.to_string()))?;
    let imported = parse_service_providers_import_value(value)?;
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let preview = build_service_providers_import_preview(&state, &imported);
    api_ok(
        serde_json::to_value(preview).map_err(|e| api_error("serialize_error", e.to_string()))?,
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn service_providers_import_apply(
    app: tauri::AppHandle,
    import_path: String,
    decisions: Vec<ProviderImportDecision>,
) -> Result<ApiOk<Value>, ApiErr> {
    let expanded =
        expand_home_dir_path(&import_path).map_err(|e| api_error("invalid_payload", e))?;
    let content =
        fs::read_to_string(&expanded).map_err(|e| api_error("io_error", e.to_string()))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| api_error("invalid_payload", e.to_string()))?;

    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let imported = parse_service_providers_import_value(value)?;
    let candidates = service_provider_import_candidates(&state, &imported);
    let decision_map =
        decisions
            .into_iter()
            .try_fold(HashMap::<String, String>::new(), |mut acc, decision| {
                let action = decision.action.trim().to_lowercase();
                if action != "overwrite" && action != "new" {
                    return Err(api_error(
                        "invalid_payload",
                        format!("invalid import action: {}", decision.action),
                    ));
                }
                acc.insert(decision.import_key, action);
                Ok(acc)
            })?;

    let mut id_map = HashMap::<String, String>::new();
    let mut overwritten = 0usize;
    let mut created = 0usize;
    for candidate in candidates {
        let mut record = candidate.record;
        let action = if candidate.conflict_existing_id.is_some() {
            decision_map
                .get(&candidate.import_key)
                .map(|v| v.as_str())
                .unwrap_or("overwrite")
        } else {
            "new"
        };
        let original_id = record.id.clone();
        if let Some(existing_id) = candidate.conflict_existing_id {
            if action == "overwrite" {
                record.id = existing_id.clone();
                merge_imported_service_provider(&mut state, record);
                id_map.insert(candidate.import_key, existing_id);
                overwritten = overwritten.saturating_add(1);
            } else {
                record.id = generate_provider_uuid();
                let new_id = record.id.clone();
                merge_imported_service_provider(&mut state, record);
                id_map.insert(candidate.import_key, new_id);
                created = created.saturating_add(1);
            }
        } else {
            if state
                .providers
                .iter()
                .any(|provider| provider.id == record.id)
            {
                record.id = generate_provider_uuid();
            }
            let final_id = record.id.clone();
            merge_imported_service_provider(&mut state, record);
            id_map.insert(candidate.import_key, final_id);
            created = created.saturating_add(1);
        }
        id_map
            .entry(provider_import_key("", &original_id))
            .or_insert_with(String::new);
    }

    let mut active_restored = 0usize;
    for (tool, provider_id) in imported.active {
        let key = provider_import_key(&tool, &provider_id);
        if let Some(final_id) = id_map.get(&key).filter(|value| !value.is_empty()) {
            state.active.insert(tool, final_id.clone());
            active_restored = active_restored.saturating_add(1);
        }
    }

    let (normalized_id_map, _) = normalize_service_provider_ids(&mut state);
    if !normalized_id_map.is_empty() {
        apply_provider_id_map_to_dependent_state(&normalized_id_map)
            .map_err(|e| api_error("io_error", e))?;
    }
    state.active.retain(|tool, provider_id| {
        state
            .providers
            .iter()
            .any(|provider| provider.tool == *tool && provider.id == *provider_id)
    });

    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_import_apply")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({
            "imported": overwritten.saturating_add(created),
            "overwritten": overwritten,
            "created": created,
            "active_restored": active_restored,
            "total": state.providers.len(),
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

struct ServiceProviderImportCandidate {
    import_key: String,
    record: ServiceProviderRecord,
    conflict_existing_id: Option<String>,
    conflict_existing_name: Option<String>,
    conflict_reason: Option<String>,
}

fn parse_service_providers_import_value(value: Value) -> Result<ServiceProvidersState, ApiErr> {
    if let Ok(imported) = serde_json::from_value::<ServiceProvidersState>(value.clone()) {
        return Ok(imported);
    }
    let Some(obj) = value.as_object() else {
        return Err(api_error(
            "invalid_payload",
            "import payload must be a service providers object",
        ));
    };
    let providers = obj
        .get("providers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| api_error("invalid_payload", "import payload must contain providers"))?
        .iter()
        .enumerate()
        .map(|(idx, value)| {
            let Some(provider_obj) = value.as_object() else {
                return Err(api_error(
                    "invalid_payload",
                    format!("provider #{} must be an object", idx.saturating_add(1)),
                ));
            };
            if provider_obj.contains_key("core") {
                return Err(api_error(
                    "invalid_payload",
                    format!(
                        "provider #{} uses old ProvidersState core schema",
                        idx.saturating_add(1)
                    ),
                ));
            }
            for key in ["id", "name", "tool"] {
                let valid = provider_obj
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false);
                if !valid {
                    return Err(api_error(
                        "invalid_payload",
                        format!("provider #{} missing {key}", idx.saturating_add(1)),
                    ));
                }
            }
            Ok(service_provider_from_value(value.clone(), None))
        })
        .collect::<Result<Vec<_>, ApiErr>>()?;
    let active = obj
        .get("active")
        .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
        .unwrap_or_default();
    Ok(ServiceProvidersState {
        active,
        active_opencode: Vec::new(),
        providers,
    })
}

fn service_provider_import_candidates(
    state: &ServiceProvidersState,
    imported: &ServiceProvidersState,
) -> Vec<ServiceProviderImportCandidate> {
    imported
        .providers
        .iter()
        .map(|record| {
            let by_id = state
                .providers
                .iter()
                .find(|existing| existing.id == record.id);
            let by_name = state.providers.iter().find(|existing| {
                existing.tool == record.tool
                    && !record.name.trim().is_empty()
                    && existing
                        .name
                        .trim()
                        .eq_ignore_ascii_case(record.name.trim())
            });
            let conflict = by_id.or(by_name);
            ServiceProviderImportCandidate {
                import_key: provider_import_key(&record.tool, &record.id),
                record: record.clone(),
                conflict_existing_id: conflict.map(|existing| existing.id.clone()),
                conflict_existing_name: conflict.map(|existing| existing.name.clone()),
                conflict_reason: if by_id.is_some() {
                    Some("id".to_string())
                } else if by_name.is_some() {
                    Some("name".to_string())
                } else {
                    None
                },
            }
        })
        .collect()
}

fn build_service_providers_import_preview(
    state: &ServiceProvidersState,
    imported: &ServiceProvidersState,
) -> ProvidersImportPreview {
    let candidates = service_provider_import_candidates(state, imported);
    let items = candidates
        .into_iter()
        .map(|candidate| ProviderImportPreviewItem {
            import_key: candidate.import_key,
            id: candidate.record.id,
            name: candidate.record.name,
            tool: candidate.record.tool,
            model: candidate.record.model,
            conflict: candidate.conflict_existing_id.is_some(),
            conflict_reason: candidate.conflict_reason,
            existing_id: candidate.conflict_existing_id,
            existing_name: candidate.conflict_existing_name,
        })
        .collect::<Vec<_>>();
    ProvidersImportPreview {
        active: imported.active.clone(),
        total: items.len(),
        conflicts: items.iter().filter(|item| item.conflict).count(),
        items,
    }
}
