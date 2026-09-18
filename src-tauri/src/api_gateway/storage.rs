use super::{
    now_ts, GatewayConfig, GatewayKey, GatewayUpstreamProvider, CONFIG_FILE, DEFAULT_PORT,
    LEGACY_CONFIG_FILE,
};
use std::fs;
use std::path::PathBuf;

pub(in crate::api_gateway) fn config_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(CONFIG_FILE))
}

pub(in crate::api_gateway) fn legacy_config_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(LEGACY_CONFIG_FILE))
}

fn read_config_file(path: &PathBuf) -> Result<Option<GatewayConfig>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let password = crate::crypto::get_or_init_master_password()?;
    let decrypted = crate::crypto::decrypt(content.trim(), &password)?;
    let mut config: GatewayConfig = serde_json::from_str(&decrypted).map_err(|e| e.to_string())?;
    normalize_config(&mut config);
    Ok(Some(config))
}

/// Resolve the effective default key id.
///
/// Manual choice wins while it points at an enabled key; otherwise the search
/// advances to the next enabled key in list order (wrapping) so disabling or
/// deleting the current default transparently falls through.
pub(in crate::api_gateway) fn resolve_default_key_id(
    keys: &[GatewayKey],
    stored: Option<&str>,
) -> Option<String> {
    if keys.is_empty() {
        return None;
    }
    match stored.and_then(|id| keys.iter().position(|key| key.id == id)) {
        Some(index) => {
            if keys[index].enabled {
                return Some(keys[index].id.clone());
            }
            for offset in 1..=keys.len() {
                let candidate = (index + offset) % keys.len();
                if keys[candidate].enabled {
                    return Some(keys[candidate].id.clone());
                }
            }
            None
        }
        None => keys.iter().find(|key| key.enabled).map(|key| key.id.clone()),
    }
}

pub(in crate::api_gateway) fn effective_default_key(
    config: &GatewayConfig,
) -> Option<&GatewayKey> {
    let id = config.default_key_id.as_deref()?;
    config.keys.iter().find(|key| key.id == id && key.enabled)
}

pub(in crate::api_gateway) fn normalize_config(config: &mut GatewayConfig) {
    if config.port == 0 {
        config.port = DEFAULT_PORT;
    }
    for provider in &mut config.providers {
        provider.base_url = provider.base_url.trim().to_string();
        provider.api_key = provider.api_key.trim().to_string();
        provider.default_model = provider
            .default_model
            .take()
            .map(|model| model.trim().to_string())
            .filter(|model| !model.is_empty());
        provider.mappings.retain(|mapping| {
            !mapping.local_model.trim().is_empty() && !mapping.upstream_model.trim().is_empty()
        });
    }
    config.default_key_id = resolve_default_key_id(&config.keys, config.default_key_id.as_deref());
}

pub(in crate::api_gateway) fn read_config() -> Result<GatewayConfig, String> {
    let path = config_path()?;
    if let Some(config) = read_config_file(&path)? {
        return Ok(config);
    }
    // One-time read-only migration: a legacy `api_fusion.json` payload is read
    // through the same decrypt path and rewritten to `api_gateway.json`.
    // The legacy file is never deleted. Writes always target the new file.
    let legacy = legacy_config_path()?;
    if let Some(config) = read_config_file(&legacy)? {
        let _ = write_config(&config);
        return Ok(config);
    }
    Ok(GatewayConfig::default())
}

/// Encrypt the entire configuration and write it atomically through a temp file
/// plus rename so a partial write can never corrupt the on-disk state.
pub(in crate::api_gateway) fn write_config(config: &GatewayConfig) -> Result<(), String> {
    let mut next = config.clone();
    normalize_config(&mut next);
    let json = serde_json::to_string(&next).map_err(|e| e.to_string())?;
    let password = crate::crypto::get_or_init_master_password()?;
    let encrypted = crate::crypto::encrypt(&json, &password)?;
    let path = config_path()?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, encrypted).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub(in crate::api_gateway) fn new_provider_id() -> String {
    format!("gw-{}", uuid::Uuid::new_v4().simple())
}

pub(in crate::api_gateway) fn new_key_id() -> String {
    format!("key-{}", uuid::Uuid::new_v4().simple())
}

/// Generate a random local API key value from OS entropy so clients never
/// supply one themselves; 128 bits of randomness, no separators to copy wrong.
pub(in crate::api_gateway) fn new_key_value() -> String {
    format!("sk-gateway-{}", uuid::Uuid::new_v4().simple())
}

pub(in crate::api_gateway) fn find_provider_mut<'a>(
    config: &'a mut GatewayConfig,
    provider_id: &str,
) -> Option<&'a mut GatewayUpstreamProvider> {
    config
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
}

pub(in crate::api_gateway) fn local_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

pub(in crate::api_gateway) fn touch_key_created_at(key: &mut GatewayKey) {
    if key.created_at == 0 {
        key.created_at = now_ts();
    }
}
