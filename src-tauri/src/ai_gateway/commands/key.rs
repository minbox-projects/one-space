use crate::ai_gateway::selection::clear_key_runtime_state;
use crate::ai_gateway::storage::{
    find_provider_mut, modify_config, new_key_id, new_key_value, read_config,
    touch_key_created_at,
};
use crate::ai_gateway::{GatewayConfig, GatewayKey};

/// Clear one upstream key's runtime state without changing its `enabled` flag,
/// so a manual re-enable restores an auth- or quota-marked key. An unknown
/// provider or key id is an actionable error that writes nothing.
#[tauri::command]
pub fn ai_gateway_reenable_provider_key(
    provider_id: String,
    key_id: String,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let provider = find_provider_mut(config, &provider_id)
            .ok_or_else(|| format!("provider not found: {provider_id}"))?;
        let key = provider
            .keys
            .iter_mut()
            .find(|key| key.id == key_id)
            .ok_or_else(|| {
                format!("no upstream key matches '{key_id}' for provider '{provider_id}'")
            })?;
        clear_key_runtime_state(key);
        Ok((true, ()))
    })?;
    read_config()
}

#[tauri::command]
pub fn ai_gateway_upsert_key(mut key: GatewayKey) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        touch_key_created_at(&mut key);
        if key.id.trim().is_empty() {
            key.id = new_key_id();
        }
        if let Some(existing) = config.keys.iter_mut().find(|candidate| candidate.id == key.id) {
            if key.value.trim().is_empty() {
                key.value = existing.value.clone();
            }
            *existing = key;
        } else {
            if key.value.trim().is_empty() {
                key.value = new_key_value();
            }
            config.keys.push(key);
        }
        Ok((true, ()))
    })?;
    read_config()
}

#[tauri::command]
pub fn ai_gateway_delete_key(key_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        config.keys.retain(|key| key.id != key_id);
        Ok((true, ()))
    })?;
    read_config()
}

#[tauri::command]
pub fn ai_gateway_set_default_key(key_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let enabled = config
            .keys
            .iter()
            .any(|key| key.id == key_id && key.enabled);
        if !enabled {
            return Err(format!(
                "cannot switch default key: '{key_id}' is not an enabled local key"
            ));
        }
        config.default_key_id = Some(key_id);
        Ok((true, ()))
    })?;
    read_config()
}
