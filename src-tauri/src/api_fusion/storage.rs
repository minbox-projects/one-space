use super::{now_ts, FusionConfig, FusionKey, FusionUpstreamProvider, CONFIG_FILE, DEFAULT_PORT};
use std::fs;
use std::path::PathBuf;

pub(in crate::api_fusion) fn config_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(CONFIG_FILE))
}

/// Resolve the effective default key id.
///
/// Manual choice wins while it points at an enabled key; otherwise the search
/// advances to the next enabled key in list order (wrapping) so disabling or
/// deleting the current default transparently falls through.
pub(in crate::api_fusion) fn resolve_default_key_id(
    keys: &[FusionKey],
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

pub(in crate::api_fusion) fn effective_default_key(
    config: &FusionConfig,
) -> Option<&FusionKey> {
    let id = config.default_key_id.as_deref()?;
    config.keys.iter().find(|key| key.id == id && key.enabled)
}

pub(in crate::api_fusion) fn normalize_config(config: &mut FusionConfig) {
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

pub(in crate::api_fusion) fn read_config() -> Result<FusionConfig, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(FusionConfig::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(FusionConfig::default());
    }
    let password = crate::crypto::get_or_init_master_password()?;
    let decrypted = crate::crypto::decrypt(content.trim(), &password)?;
    let mut config: FusionConfig = serde_json::from_str(&decrypted).map_err(|e| e.to_string())?;
    normalize_config(&mut config);
    Ok(config)
}

/// Encrypt the entire configuration and write it atomically through a temp file
/// plus rename so a partial write can never corrupt the on-disk state.
pub(in crate::api_fusion) fn write_config(config: &FusionConfig) -> Result<(), String> {
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

pub(in crate::api_fusion) fn new_provider_id() -> String {
    format!("fus-{}", uuid::Uuid::new_v4().simple())
}

pub(in crate::api_fusion) fn new_key_id() -> String {
    format!("key-{}", uuid::Uuid::new_v4().simple())
}

pub(in crate::api_fusion) fn find_provider_mut<'a>(
    config: &'a mut FusionConfig,
    provider_id: &str,
) -> Option<&'a mut FusionUpstreamProvider> {
    config
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
}

pub(in crate::api_fusion) fn local_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

pub(in crate::api_fusion) fn touch_key_created_at(key: &mut FusionKey) {
    if key.created_at == 0 {
        key.created_at = now_ts();
    }
}
