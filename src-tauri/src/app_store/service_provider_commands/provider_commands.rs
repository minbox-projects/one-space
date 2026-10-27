use super::records::{
    append_provider_history_if_changed, lock_service_provider_operation,
    service_provider_from_value,
};
use crate::app_store::provider_activation::activate_provider;
use crate::app_store::{
    api_error, api_ok, apply_opencode_remove_projection, apply_provider_id_map_to_dependent_state,
    enqueue_sync_event, generate_provider_uuid, get_meta, is_uuid_v4, load_service_providers_state,
    materialize_isolated_claude_profile_async, normalize_protocol_router_wire_api,
    normalize_service_provider_ids, now_ts, process_sync_queue, save_service_providers_internal,
    service_provider_to_legacy, validate_provider_uuid_param, ApiErr, ApiMeta, ApiOk,
    ServiceProviderRecord, ServiceProvidersState,
};
use serde_json::{json, Value};
use std::fs::{self};
use std::path::Path;

pub(super) fn redact_provider_secrets(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if matches!(key.as_str(), "api_key" | "apiKey") {
                    if child
                        .as_str()
                        .map(|value| !value.is_empty())
                        .unwrap_or(false)
                    {
                        *child = Value::String("********".to_string());
                    }
                } else {
                    redact_provider_secrets(child);
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                redact_provider_secrets(child);
            }
        }
        _ => {}
    }
}

#[tauri::command]
pub fn service_providers_list() -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let providers: Vec<Value> = state
        .providers
        .iter()
        .map(|provider| {
            let mut value = service_provider_to_legacy(provider);
            // Don't redact api_key for claude and codex tools to allow plaintext display
            if provider.tool != "claude" && provider.tool != "codex" {
                redact_provider_secrets(&mut value);
            }
            value
        })
        .collect();
    let payload = json!({
        "active": state.active,
        "active_claude": state.active.get("claude"),
        "active_codex": state.active.get("codex"),
        "active_antigravity": state.active.get("antigravity"),
        "active_opencode": state.active_opencode,
        "providers": providers,
    });
    api_ok(payload, get_meta().map_err(|e| api_error("io_error", e))?)
}

pub(super) fn read_opencode_provider_config_at_home(
    home_dir: &Path,
    provider_key: &str,
) -> Result<Value, String> {
    let path = home_dir
        .join(".config")
        .join("opencode")
        .join("opencode.json");
    let content =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let root: Value = serde_json::from_str(&content)
        .map_err(|e| format!("invalid JSON in {}: {e}", path.display()))?;
    let provider_map = root
        .as_object()
        .ok_or_else(|| "OpenCode config root must be an object".to_string())?
        .get("provider")
        .and_then(Value::as_object)
        .ok_or_else(|| "OpenCode config provider must be an object".to_string())?;
    let provider = provider_map
        .get(provider_key)
        .ok_or_else(|| format!("OpenCode provider key not found: {provider_key}"))?;
    if !provider.is_object() {
        return Err(format!(
            "OpenCode provider '{provider_key}' must be an object"
        ));
    }
    Ok(provider.clone())
}

#[tauri::command]
pub fn service_provider_read_opencode_config(provider_key: String) -> Result<ApiOk<Value>, ApiErr> {
    if provider_key.is_empty() {
        return Err(api_error("invalid_payload", "provider_key is required"));
    }
    let home_dir =
        dirs::home_dir().ok_or_else(|| api_error("io_error", "home directory not found"))?;
    let provider = read_opencode_provider_config_at_home(&home_dir, &provider_key)
        .map_err(|e| api_error("opencode_config_read_failed", e))?;
    api_ok(provider, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn service_providers_upsert(
    app: tauri::AppHandle,
    provider: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    let response = service_providers_upsert_inner(provider).await?;
    enqueue_sync_event("service_providers", "service_providers_upsert")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    Ok(response)
}

pub(in crate::app_store) async fn service_providers_upsert_inner(
    provider: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    let operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let obj = provider
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "provider must be object"))?;
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tool = obj
        .get("tool")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if id.is_empty() || tool.is_empty() {
        return Err(api_error("invalid_payload", "provider id/tool required"));
    }

    // Validate code for Claude
    if tool == "claude" {
        if let Some(ref code) = obj.get("code").and_then(|v| v.as_str()) {
            let code_trim = code.trim().to_lowercase();
            if code_trim.is_empty() {
                return Err(api_error("invalid_payload", "code is required"));
            }
            if !code_trim
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(api_error(
                    "invalid_payload",
                    "code can only contain ASCII letters, numbers, hyphens, and underscores",
                ));
            }
        }
    }

    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;

    // Validate code uniqueness
    if tool == "claude" {
        if let Some(ref code) = obj.get("code").and_then(|v| v.as_str()) {
            let code_trim = code.trim().to_lowercase();
            let duplicate = state.providers.iter().any(|p| {
                p.tool == "claude"
                    && p.id != id
                    && p.code.as_ref().map(|c| c.trim().to_lowercase()) == Some(code_trim.clone())
            });
            if duplicate {
                return Err(api_error(
                    "invalid_payload",
                    format!(
                        "code '{}' is already used by another Claude service provider",
                        code_trim
                    ),
                ));
            }
        }
    }

    let existing = state.providers.iter().find(|p| p.id == id).cloned();
    let final_id = existing
        .as_ref()
        .map(|provider| provider.id.clone())
        .unwrap_or_else(|| {
            if is_uuid_v4(&id) {
                id.clone()
            } else {
                generate_provider_uuid()
            }
        });
    let mut normalized_obj = obj.clone();
    normalized_obj.insert("id".to_string(), Value::String(final_id.clone()));

    // Handle secret placeholder: if api_key is ******** and existing has a real key, preserve it
    let mut record = service_provider_from_value(Value::Object(normalized_obj), existing.as_ref());
    if record.api_key == "********" {
        if let Some(ref ex) = existing {
            if !ex.api_key.is_empty() && ex.api_key != "********" {
                record.api_key = ex.api_key.clone();
            }
        }
    }

    // Ensure default claude fields
    if record.claude_api_format.is_empty() {
        record.claude_api_format = "anthropic_messages".to_string();
    }
    if record.claude_connection_mode.is_empty() {
        record.claude_connection_mode = "native_anthropic".to_string();
    }
    if record.tool == "claude"
        && (record.claude_api_format == "open_ai_chat"
            || record.claude_api_format == "open_ai_responses")
    {
        record.claude_connection_mode = "protocol_router".to_string();
        record.protocol_router_wire_api =
            normalize_protocol_router_wire_api(&record.claude_api_format);
    }
    record.protocol_router_wire_api =
        normalize_protocol_router_wire_api(&record.protocol_router_wire_api);
    if record.claude_auth_env_key.is_empty() {
        record.claude_auth_env_key = "ANTHROPIC_API_KEY".to_string();
    }
    append_provider_history_if_changed(existing.as_ref(), &mut record, "upsert");

    if let Some(pos) = state.providers.iter().position(|p| p.id == record.id) {
        state.providers[pos] = record.clone();
    } else {
        state.providers.push(record.clone());
    }

    let (id_map, normalized_ids) = normalize_service_provider_ids(&mut state);
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    if normalized_ids {
        apply_provider_id_map_to_dependent_state(&id_map).map_err(|e| api_error("io_error", e))?;
    }
    drop(operation);
    if record.tool == "claude" {
        materialize_isolated_claude_profile_async(&record)
            .await
            .map_err(|e| api_error("profile_failed", e))?;
    }

    api_ok(
        service_provider_to_legacy(&record),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_providers_delete(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    validate_provider_uuid_param(&provider_id).map_err(|e| api_error("invalid_payload", e))?;
    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let original = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let deleted = original
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .cloned()
        .ok_or_else(|| api_error("not_found", "service provider not found"))?;
    let mut next = original;
    next.providers.retain(|provider| provider.id != provider_id);
    next.active.retain(|_, active_id| *active_id != provider_id);
    next.active_opencode
        .retain(|active_id| *active_id != provider_id);
    let schema = save_service_providers_internal(&next).map_err(|e| api_error("io_error", e))?;
    // opencode 工具删除服务商时同步删除 opencode.json 中的对应 provider 配置。
    if deleted.tool == "opencode" {
        let has_provider_key = deleted
            .provider_key
            .as_deref()
            .map(str::trim)
            .map(|value| !value.is_empty())
            .unwrap_or(false);
        if has_provider_key {
            apply_opencode_remove_projection(&deleted)
                .map_err(|e| api_error("projection_failed", e))?;
        }
    }
    enqueue_sync_event("service_providers", "service_providers_delete")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_providers_set_active(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    let response = activate_provider(&tool, &provider_id)?;
    enqueue_sync_event("service_providers", "service_providers_set_active")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    Ok(response)
}

pub(in crate::app_store) fn set_service_provider_favorite_impl(
    state: &mut ServiceProvidersState,
    provider_id: &str,
    favorite: bool,
) -> Result<ServiceProviderRecord, ApiErr> {
    let record = state
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| api_error("not_found", "service provider not found"))?;

    if favorite {
        if record.favorite_at.is_none() {
            record.favorite_at = Some(now_ts());
        }
    } else {
        record.favorite_at = None;
    }

    Ok(record.clone())
}

#[tauri::command]
pub async fn service_providers_set_favorite(
    app: tauri::AppHandle,
    provider_id: String,
    favorite: bool,
) -> Result<ApiOk<Value>, ApiErr> {
    validate_provider_uuid_param(&provider_id).map_err(|e| api_error("invalid_payload", e))?;
    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let updated = set_service_provider_favorite_impl(&mut state, &provider_id, favorite)?;
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_set_favorite")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        service_provider_to_legacy(&updated),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}
