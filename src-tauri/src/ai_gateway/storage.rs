use super::migration::migrate_legacy_files;
use super::{
    now_ts, resolve_port, GatewayConfig, GatewayKey, GatewayUpstreamProvider, ModelPrice,
    CONFIG_FILE, GATEWAY_CONFIG_SCHEMA_VERSION, MAX_PROVIDER_WEIGHT, MIN_PROVIDER_WEIGHT,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub(in crate::ai_gateway) fn config_path() -> Result<PathBuf, String> {
    Ok(crate::config::get_app_dir()?.join(CONFIG_FILE))
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
    let mut value: serde_json::Value =
        serde_json::from_str(&decrypted).map_err(|e| e.to_string())?;
    // A missing version field reads as 0, the legacy schema. Every file older
    // than the current version always runs the legacy transformation and is
    // unconditionally stamped and rewritten at the current version, even when no
    // legacy shape had to change, so the version checkpoint always advances; an
    // already-current file is left byte-identical.
    let stored_version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    let needs_migration = stored_version < GATEWAY_CONFIG_SCHEMA_VERSION;
    if needs_migration {
        super::migration::migrate_legacy_config(&mut value);
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "schema_version".to_string(),
                serde_json::Value::from(GATEWAY_CONFIG_SCHEMA_VERSION),
            );
        }
    }
    let mut config: GatewayConfig = serde_json::from_value(value).map_err(|e| e.to_string())?;
    normalize_config(&mut config);
    if needs_migration {
        // Best-effort: a failed rewrite keeps the previous complete bytes on
        // disk and the next read retries the migration.
        let _ = write_config(&config);
    }
    Ok(Some(config))
}

/// Resolve the effective default key id.
///
/// Manual choice wins while it points at an enabled key; otherwise the search
/// advances to the next enabled key in list order (wrapping) so disabling or
/// deleting the current default transparently falls through.
pub(in crate::ai_gateway) fn resolve_default_key_id(
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

/// Canonicalize the stored-but-untrusted parts of a configuration without
/// touching the price table: resolve the port for this profile, clamp weights,
/// trim provider strings, drop unusable mappings and resolve the default key.
fn normalize_stored_config(config: &mut GatewayConfig) {
    config.port = resolve_port(config.port, cfg!(debug_assertions));
    for provider in &mut config.providers {
        if provider.weight < MIN_PROVIDER_WEIGHT || provider.weight > MAX_PROVIDER_WEIGHT {
            provider.weight = provider.weight.clamp(MIN_PROVIDER_WEIGHT, MAX_PROVIDER_WEIGHT);
        }
        provider.base_url = provider.base_url.trim().to_string();
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

/// Read-path normalization: canonicalize stored fields and repopulate template
/// prices/efforts. The price table is never migrated or dropped on a read; a
/// legacy file is scoped by the version-gated migration before it is parsed.
pub(in crate::ai_gateway) fn normalize_config(config: &mut GatewayConfig) {
    normalize_stored_config(config);
    normalize_template_prices_and_efforts(config);
}

/// Make every persisted price row provider-scoped: copy a legacy global row to
/// each provider that reaches its model, keep the first occurrence of a scoped
/// duplicate and drop rows whose provider/model is gone. Applied on the write
/// path only; the read path relies on the version-gated migration instead.
fn scope_model_prices(config: &mut GatewayConfig) {
    let reachable: Vec<(String, HashSet<String>)> = config
        .providers
        .iter()
        .map(|provider| {
            let mut models = HashSet::new();
            for mapping in &provider.mappings {
                if !mapping.upstream_model.trim().is_empty() {
                    models.insert(mapping.upstream_model.clone());
                }
            }
            if let Some(default_model) = provider.default_model.as_deref() {
                if !default_model.trim().is_empty() {
                    models.insert(default_model.to_string());
                }
            }
            (provider.id.clone(), models)
        })
        .collect();

    let global_rows: Vec<ModelPrice> = config
        .model_prices
        .iter()
        .filter(|row| row.provider_id.is_none())
        .cloned()
        .collect();
    for row in global_rows {
        for (provider_id, models) in &reachable {
            if !models.contains(&row.upstream_model) {
                continue;
            }
            let already_scoped = config.model_prices.iter().any(|existing| {
                existing.provider_id.as_deref() == Some(provider_id.as_str())
                    && existing.upstream_model == row.upstream_model
            });
            if already_scoped {
                continue;
            }
            let mut migrated = row.clone();
            migrated.provider_id = Some(provider_id.clone());
            config.model_prices.push(migrated);
        }
    }

    let mut seen: Vec<(String, String)> = Vec::new();
    config.model_prices.retain(|row| {
        let Some(provider_id) = row.provider_id.as_deref() else {
            return false;
        };
        let Some((_, models)) = reachable.iter().find(|(id, _)| id == provider_id) else {
            return false;
        };
        if !models.contains(&row.upstream_model) {
            return false;
        }
        let key = (provider_id.to_string(), row.upstream_model.clone());
        if seen.contains(&key) {
            return false;
        }
        seen.push(key);
        true
    });
}

/// Query the real reasoning effort levels supported by a model based on its model identifier.
pub(in crate::ai_gateway) fn query_model_reasoning_efforts(model: &str) -> Vec<String> {
    let lower = model.trim().to_lowercase();
    let base = lower.split('/').last().unwrap_or(&lower);
    let base = base.strip_suffix(":free").unwrap_or(base);
    let base = base.strip_suffix("-free").unwrap_or(base);

    if base.starts_with("gpt-5.5")
        || base.starts_with("gpt-5.6")
        || base.starts_with("gpt-6")
        || base.starts_with("claude-fable")
        || base.starts_with("claude-opus")
        || base.starts_with("claude-sonnet-5")
        || base == "glm-5.2"
        || base == "glm-5.2-fast"
        || base == "muse-spark-1.3"
    {
        vec![
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
            "xhigh".to_string(),
            "max".to_string(),
        ]
    } else if base.starts_with("gpt-5.3")
        || base.starts_with("gpt-5.4")
        || base.starts_with("muse-spark-1")
        || base == "grok-4.6"
    {
        vec![
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
            "xhigh".to_string(),
        ]
    } else if base == "claude-sonnet-4-6" {
        vec![
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
            "max".to_string(),
        ]
    } else if base.starts_with("qwen3.8-max") {
        vec![
            "low".to_string(),
            "medium".to_string(),
            "xhigh".to_string(),
        ]
    } else if base.starts_with("deepseek-v4")
        || base.starts_with("kimi-k")
        || base.starts_with("glm-5")
    {
        vec![
            "low".to_string(),
            "high".to_string(),
            "max".to_string(),
        ]
    } else if base.starts_with("gemini-3")
        || base.starts_with("step-3.5")
        || base.starts_with("step-3.7")
        || base == "grok-4.5"
    {
        vec![
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
        ]
    } else if base.starts_with("fugu-ultra") {
        vec![
            "high".to_string(),
            "max".to_string(),
            "xhigh".to_string(),
        ]
    } else {
        Vec::new()
    }
}

/// Synchronize pricing configurations from providers into template models and
/// ensure real reasoning efforts are populated for providers and templates.
pub(in crate::ai_gateway) fn normalize_template_prices_and_efforts(config: &mut GatewayConfig) {
    // 1. Ensure provider mappings have real reasoning efforts when empty
    for provider in &mut config.providers {
        for mapping in &mut provider.mappings {
            if mapping.reasoning_efforts.is_empty() {
                let efforts = query_model_reasoning_efforts(&mapping.upstream_model);
                if !efforts.is_empty() {
                    mapping.reasoning_efforts = efforts;
                }
            }
        }
    }

    // 2. Populate template model prices and reasoning efforts
    for state in &mut config.provider_templates {
        let Some(template) = &mut state.template else {
            continue;
        };
        for model in &mut template.models {
            // Populate pricing if unpriced
            if model.input == 0.0
                && model.output == 0.0
                && model.cache_read == 0.0
                && model.cache_write == 0.0
                && model.off_peaks.is_empty()
            {
                let matched_price = config
                    .model_prices
                    .iter()
                    .find(|p| {
                        p.upstream_model == model.upstream_model
                            && p.provider_id.as_deref().map_or(false, |pid| {
                                config.providers.iter().any(|prov| {
                                    prov.id == pid && prov.template_id.as_deref() == Some(&state.template_id)
                                })
                            })
                    })
                    .or_else(|| {
                        config
                            .model_prices
                            .iter()
                            .find(|p| p.upstream_model == model.upstream_model)
                    })
                    .or_else(|| {
                        let m_base = model.upstream_model.split('/').last().unwrap_or(&model.upstream_model).to_lowercase();
                        config.model_prices.iter().find(|p| {
                            let p_base = p.upstream_model.split('/').last().unwrap_or(&p.upstream_model).to_lowercase();
                            p_base == m_base
                        })
                    });

                if let Some(price) = matched_price {
                    if price.input > 0.0
                        || price.output > 0.0
                        || price.cache_read > 0.0
                        || price.cache_write > 0.0
                        || !price.off_peaks.is_empty()
                    {
                        model.input = price.input;
                        model.output = price.output;
                        model.cache_read = price.cache_read;
                        model.cache_write = price.cache_write;
                        model.off_peaks = price.off_peaks.clone();
                    }
                }
            }

            // Populate local_model if absent and matching provider mapping exists
            if model.local_model.is_none() {
                if let Some(local) = config.providers.iter().find_map(|prov| {
                    prov.mappings
                        .iter()
                        .find(|m| m.upstream_model == model.upstream_model && !m.local_model.trim().is_empty())
                        .map(|m| m.local_model.clone())
                }) {
                    model.local_model = Some(local);
                }
            }

            // Populate reasoning efforts
            if model.reasoning_efforts.is_empty() {
                let from_mapping = config.providers.iter().find_map(|prov| {
                    prov.mappings
                        .iter()
                        .find(|m| m.upstream_model == model.upstream_model && !m.reasoning_efforts.is_empty())
                        .map(|m| m.reasoning_efforts.clone())
                });

                if let Some(efforts) = from_mapping {
                    model.reasoning_efforts = efforts;
                } else {
                    let real_efforts = query_model_reasoning_efforts(&model.upstream_model);
                    if !real_efforts.is_empty() {
                        model.reasoning_efforts = real_efforts;
                    }
                }
            }
        }
    }
}

pub(in crate::ai_gateway) fn read_config() -> Result<GatewayConfig, String> {
    let path = config_path()?;
    if let Some(config) = read_config_file(&path)? {
        migrate_legacy_files();
        return Ok(config);
    }
    Ok(GatewayConfig::default())
}

/// Encrypt the entire configuration and write it atomically through a temp file
/// plus rename so a partial write can never corrupt the on-disk state. Every
/// write stamps the current schema version so an older file can never be
/// re-written without it.
pub(in crate::ai_gateway) fn write_config(config: &GatewayConfig) -> Result<(), String> {
    let path = config_path()?;
    let tmp = path.with_extension("tmp");
    write_config_through_temp(config, &path, &tmp)
}

/// Process-global configuration write lock.
///
/// Every serialized read-modify-write sequence runs under it so concurrent
/// writers cannot lose each other's changes. A panic inside a mutation poisons
/// the mutex; later writers recover through the poisoned guard instead of
/// failing permanently.
static CONFIG_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Monotonic suffix that keeps every serialized write's temporary file unique.
static CONFIG_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A temporary path in the configuration directory that no other write can
/// pick: the process id plus a process-monotonic sequence keeps concurrent
/// writers from colliding on one temp file.
fn unique_config_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| CONFIG_FILE.to_string());
    let sequence = CONFIG_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!("{file_name}.{}.{sequence}.tmp", std::process::id()))
}

/// Canonicalize, encrypt and atomically publish `config` at `path` through the
/// caller-owned temporary file `tmp`. The caller owns temp-path uniqueness and,
/// when serializing writers, the write lock.
fn write_config_through_temp(
    config: &GatewayConfig,
    path: &Path,
    tmp: &Path,
) -> Result<(), String> {
    let mut next = config.clone();
    normalize_stored_config(&mut next);
    scope_model_prices(&mut next);
    normalize_template_prices_and_efforts(&mut next);
    next.schema_version = GATEWAY_CONFIG_SCHEMA_VERSION;
    let json = serde_json::to_string(&next).map_err(|e| e.to_string())?;
    let password = crate::crypto::get_or_init_master_password()?;
    let encrypted = crate::crypto::encrypt(&json, &password)?;
    fs::write(tmp, encrypted).map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())?;
    migrate_legacy_files();
    Ok(())
}

/// Serialized configuration read-modify-write entry point.
///
/// `mutate` runs under the process-global write lock and receives the current
/// configuration. It returns `(changed, value)`: the mutated configuration is
/// persisted only when `changed` is true, through a per-write unique temp file,
/// and `value` is handed back to the caller. A writer panic poisons the lock
/// but the next call recovers through the poisoned guard. The closure must stay
/// local and must not wait on the network, so the lock is never held across an
/// upstream wait.
pub(in crate::ai_gateway) fn modify_config<T>(
    mutate: impl FnOnce(&mut GatewayConfig) -> Result<(bool, T), String>,
) -> Result<T, String> {
    let _guard = CONFIG_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut config = read_config()?;
    let (changed, value) = mutate(&mut config)?;
    if changed {
        let path = config_path()?;
        let tmp = unique_config_temp_path(&path);
        write_config_through_temp(&config, &path, &tmp)?;
    }
    Ok(value)
}

pub(in crate::ai_gateway) fn new_provider_id() -> String {
    format!("gw-{}", uuid::Uuid::new_v4().simple())
}

pub(in crate::ai_gateway) fn new_key_id() -> String {
    format!("key-{}", uuid::Uuid::new_v4().simple())
}

/// Generate a random local API key value from OS entropy so clients never
/// supply one themselves; 128 bits of randomness, no separators to copy wrong.
pub(in crate::ai_gateway) fn new_key_value() -> String {
    format!("sk-gateway-{}", uuid::Uuid::new_v4().simple())
}

pub(in crate::ai_gateway) fn find_provider_mut<'a>(
    config: &'a mut GatewayConfig,
    provider_id: &str,
) -> Option<&'a mut GatewayUpstreamProvider> {
    config
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
}

pub(in crate::ai_gateway) fn local_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

pub(in crate::ai_gateway) fn touch_key_created_at(key: &mut GatewayKey) {
    if key.created_at == 0 {
        key.created_at = now_ts();
    }
}
