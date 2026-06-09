use super::{
    load_service_providers_state, load_sessions_state, local_workflow_presets_path,
    local_workflow_runs_path, normalize_service_provider_record,
    restore_missing_service_provider_api_keys_from_legacy, save_sessions_state,
    shared_profile_path, CryptoService, EncryptedBlob, ServiceProvidersState, StorageEngine,
};
use crate::config;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self};
use std::path::Path;
use uuid::Uuid;

pub(in crate::app_store) fn normalize_runtime_mode(input: Option<&str>) -> String {
    let value = input.unwrap_or("").trim().to_lowercase();
    if value == "strict" {
        "strict".to_string()
    } else {
        "shared".to_string()
    }
}

pub(in crate::app_store) fn generate_provider_uuid() -> String {
    Uuid::new_v4().to_string()
}

pub(in crate::app_store) fn is_uuid_v4(value: &str) -> bool {
    Uuid::parse_str(value.trim())
        .map(|uuid| uuid.get_version_num() == 4)
        .unwrap_or(false)
}

pub(in crate::app_store) fn provider_id_needs_uuid_migration(value: &str) -> bool {
    !is_uuid_v4(value)
}

pub(in crate::app_store) fn validate_provider_uuid_param(value: &str) -> Result<(), String> {
    if is_uuid_v4(value) {
        Ok(())
    } else {
        Err("provider id must be a UUID v4".to_string())
    }
}

pub(in crate::app_store) fn validate_provider_uuid_option(
    value: Option<&str>,
) -> Result<(), String> {
    if let Some(provider_id) = value.map(str::trim).filter(|value| !value.is_empty()) {
        validate_provider_uuid_param(provider_id)?;
    }
    Ok(())
}

pub(crate) fn validate_service_provider_reference(
    tool: &str,
    provider_id: &str,
) -> Result<(), String> {
    validate_provider_uuid_param(provider_id)?;
    let normalized_tool = tool.trim().to_lowercase();
    let state = load_service_providers_state()?;
    if state
        .providers
        .iter()
        .any(|provider| provider.tool == normalized_tool && provider.id == provider_id)
    {
        Ok(())
    } else {
        Err(format!("service provider not found: {provider_id}"))
    }
}

pub(in crate::app_store) fn remap_provider_id(
    value: &str,
    id_map: &HashMap<String, String>,
) -> Option<String> {
    id_map.get(value.trim()).cloned()
}

pub(in crate::app_store) fn remap_provider_id_option(
    value: &mut Option<String>,
    id_map: &HashMap<String, String>,
) -> bool {
    let Some(current) = value.as_deref() else {
        return false;
    };
    let Some(next) = remap_provider_id(current, id_map) else {
        return false;
    };
    *value = Some(next);
    true
}

pub(in crate::app_store) fn remap_provider_string_field(
    obj: &mut Map<String, Value>,
    key: &str,
    id_map: &HashMap<String, String>,
) -> bool {
    let Some(raw) = obj.get(key).and_then(|v| v.as_str()) else {
        return false;
    };
    let Some(next) = remap_provider_id(raw, id_map) else {
        return false;
    };
    obj.insert(key.to_string(), Value::String(next));
    true
}

pub(in crate::app_store) fn normalize_service_provider_ids(
    state: &mut ServiceProvidersState,
) -> (HashMap<String, String>, bool) {
    let mut id_map = HashMap::new();
    let mut used_ids: HashSet<String> = state.providers.iter().map(|p| p.id.clone()).collect();
    let mut changed = false;

    for provider in state.providers.iter_mut() {
        if !provider_id_needs_uuid_migration(&provider.id) {
            continue;
        }
        let old_id = provider.id.clone();
        let mut new_id = generate_provider_uuid();
        while used_ids.contains(&new_id) {
            new_id = generate_provider_uuid();
        }
        used_ids.remove(&old_id);
        used_ids.insert(new_id.clone());
        provider.id = new_id.clone();
        id_map.insert(old_id, new_id);
        changed = true;
    }

    if !id_map.is_empty() {
        for active_id in state.active.values_mut() {
            if let Some(next) = remap_provider_id(active_id, &id_map) {
                *active_id = next;
                changed = true;
            }
        }
        for provider in state.providers.iter_mut() {
            if remap_provider_id_option(&mut provider.protocol_router_upstream_provider_id, &id_map)
            {
                changed = true;
            }
        }
    }

    (id_map, changed)
}

pub(in crate::app_store) fn normalize_loaded_service_providers_state(
    state: &mut ServiceProvidersState,
) -> Result<(HashMap<String, String>, bool), String> {
    let mut changed = false;
    for provider in state.providers.iter_mut() {
        normalize_service_provider_record(provider);
    }
    if restore_missing_service_provider_api_keys_from_legacy(state)? {
        changed = true;
    }
    let (id_map, changed_ids) = normalize_service_provider_ids(state);
    if changed_ids {
        changed = true;
    }
    Ok((id_map, changed))
}

pub(in crate::app_store) fn apply_provider_id_map_to_sessions(
    id_map: &HashMap<String, String>,
) -> Result<bool, String> {
    if id_map.is_empty() {
        return Ok(false);
    }
    let path = StorageEngine::sessions_path()?;
    if !path.exists() {
        return Ok(false);
    }
    let mut state = load_sessions_state()?;
    let mut changed = false;
    for session in state.sessions.iter_mut() {
        if remap_provider_id_option(&mut session.provider_id, id_map) {
            changed = true;
        }
    }
    if changed {
        save_sessions_state(&state)?;
    }
    Ok(changed)
}

pub(in crate::app_store) fn remap_provider_ids_in_json_value(
    value: &mut Value,
    id_map: &HashMap<String, String>,
) -> bool {
    match value {
        Value::Object(obj) => {
            let mut changed = false;
            for key in [
                "provider_id",
                "active_provider_id",
                "claude_provider_id",
                "upstream_provider_id",
                "protocol_router_upstream_provider_id",
            ] {
                if remap_provider_string_field(obj, key, id_map) {
                    changed = true;
                }
            }
            if let Some(Value::Array(items)) = obj.get_mut("linked_provider_ids") {
                for item in items.iter_mut() {
                    if let Some(raw) = item.as_str() {
                        if let Some(next) = remap_provider_id(raw, id_map) {
                            *item = Value::String(next);
                            changed = true;
                        }
                    }
                }
            }
            if let Some(Value::Object(active)) = obj.get_mut("active") {
                for value in active.values_mut() {
                    if let Some(raw) = value.as_str() {
                        if let Some(next) = remap_provider_id(raw, id_map) {
                            *value = Value::String(next);
                            changed = true;
                        }
                    }
                }
            }
            for key in [
                "active_claude",
                "active_codex",
                "active_gemini",
                "active_opencode",
            ] {
                if remap_provider_string_field(obj, key, id_map) {
                    changed = true;
                }
            }
            for child in obj.values_mut() {
                if remap_provider_ids_in_json_value(child, id_map) {
                    changed = true;
                }
            }
            changed
        }
        Value::Array(items) => {
            let mut changed = false;
            for item in items {
                if remap_provider_ids_in_json_value(item, id_map) {
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}

pub(in crate::app_store) fn apply_provider_id_map_to_plain_json_file(
    path: &Path,
    id_map: &HashMap<String, String>,
) -> Result<bool, String> {
    if id_map.is_empty() || !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(false);
    }
    let mut value: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    if !remap_provider_ids_in_json_value(&mut value, id_map) {
        return Ok(false);
    }
    let next = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    StorageEngine::atomic_write(path, &next)?;
    Ok(true)
}

pub(in crate::app_store) fn apply_provider_id_map_to_encrypted_json_file(
    path: &Path,
    id_map: &HashMap<String, String>,
) -> Result<bool, String> {
    if id_map.is_empty() || !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(false);
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        let mut value = CryptoService::decrypt_json(&blob)?;
        if !remap_provider_ids_in_json_value(&mut value, id_map) {
            return Ok(false);
        }
        let next_blob = CryptoService::encrypt_json(&value)?;
        StorageEngine::write_json(path, &next_blob)?;
        return Ok(true);
    }

    apply_provider_id_map_to_plain_json_file(path, id_map)
}

pub(in crate::app_store) fn migrate_claude_profile_dirs_for_provider_id_map(
    id_map: &HashMap<String, String>,
) -> Result<bool, String> {
    if id_map.is_empty() {
        return Ok(false);
    }
    let profiles_dir = crate::claude_profiles::get_claude_profiles_dir()?;
    if !profiles_dir.exists() {
        return Ok(false);
    }

    let mut changed = false;
    for (old_id, new_id) in id_map {
        let old_dir = profiles_dir.join(crate::claude_profiles::safe_dir_name(old_id));
        let new_dir = profiles_dir.join(crate::claude_profiles::safe_dir_name(new_id));
        if !old_dir.exists() || new_dir.exists() {
            continue;
        }
        copy_dir_recursive(&old_dir, &new_dir)?;
        changed = true;
    }
    Ok(changed)
}

pub(in crate::app_store) fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let entry_path = entry.path();
        let target_path = dst.join(entry.file_name());
        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &target_path)?;
        } else if entry_path.is_file() {
            fs::copy(&entry_path, &target_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(in crate::app_store) fn apply_provider_id_map_to_dependent_state(
    id_map: &HashMap<String, String>,
) -> Result<(), String> {
    if id_map.is_empty() {
        return Ok(());
    }

    let _ = apply_provider_id_map_to_sessions(id_map)?;
    let _ = apply_provider_id_map_to_plain_json_file(&local_workflow_presets_path()?, id_map)?;
    let _ = apply_provider_id_map_to_plain_json_file(&local_workflow_runs_path()?, id_map)?;
    let _ = apply_provider_id_map_to_encrypted_json_file(&StorageEngine::mcp_path()?, id_map)?;
    let _ = crate::protocol_router::remap_service_provider_route_stats(id_map)?;
    let _ = migrate_claude_profile_dirs_for_provider_id_map(id_map)?;

    if let Ok(cfg) = config::get_storage_config() {
        if let Ok(path) = shared_profile_path(&cfg, "workflow_presets.json") {
            let _ = apply_provider_id_map_to_plain_json_file(&path, id_map);
        }
        if let Ok(path) = shared_profile_path(&cfg, "mcp.json") {
            let _ = apply_provider_id_map_to_plain_json_file(&path, id_map);
        }
        if let Ok(path) = shared_profile_path(&cfg, "providers.json") {
            let _ = apply_provider_id_map_to_plain_json_file(&path, id_map);
        }
    }

    Ok(())
}
