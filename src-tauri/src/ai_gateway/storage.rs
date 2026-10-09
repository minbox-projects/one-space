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

/// Read, decrypt, migrate and normalize the configuration at `path` without
/// taking the write lock and without publishing anything. `Ok(None)` means the
/// file is absent or empty. The returned flag reports whether the on-disk file
/// still needed the version-gated migration, so a caller already holding the
/// write lock can stamp and rewrite it exactly once.
///
/// A missing version field reads as 0, the legacy schema. Every file older than
/// the current version always runs the legacy transformation and is
/// unconditionally stamped at the current version, even when no legacy shape
/// had to change, so the version checkpoint always advances; an already-current
/// file is left byte-identical.
fn read_config_locked(path: &Path) -> Result<Option<(GatewayConfig, bool)>, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if content.trim().is_empty() {
        return Ok(None);
    }
    let password = crate::crypto::get_or_init_master_password()?;
    let decrypted = crate::crypto::decrypt(content.trim(), &password)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&decrypted).map_err(|e| e.to_string())?;
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
    Ok(Some((config, needs_migration)))
}

/// One process-memory read-cache entry: the normalized configuration the last
/// in-process write published (or the last read parsed), keyed by the file
/// identity — path plus byte length and modification time. Any metadata change,
/// including a direct on-disk rewrite that calls no invalidation helper,
/// invalidates the entry because the identity no longer matches.
struct CachedConfig {
    path: PathBuf,
    len: u64,
    modified: Option<std::time::SystemTime>,
    config: GatewayConfig,
}

static CONFIG_READ_CACHE: Mutex<Vec<CachedConfig>> = Mutex::new(Vec::new());

/// The identity a read cache is keyed by: byte length plus modification time.
fn file_identity(path: &Path) -> Option<(u64, Option<std::time::SystemTime>)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.len(), metadata.modified().ok()))
}

/// Return the cached configuration when the file identity still matches.
fn cached_config(path: &Path) -> Option<GatewayConfig> {
    let (len, modified) = file_identity(path)?;
    let cache = CONFIG_READ_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .iter()
        .find(|entry| entry.path == path && entry.len == len && entry.modified == modified)
        .map(|entry| entry.config.clone())
}

/// Publish `config` into the read cache under the file's current identity.
///
/// Writers that just published (`write_config`, `modify_config`, the migration
/// branch under the write lock) call this so the cache is keyed by the identity
/// of the bytes they wrote.
fn store_cached_config(path: &Path, config: &GatewayConfig) {
    let Some(identity) = file_identity(path) else {
        invalidate_cached_config(path);
        return;
    };
    store_cached_config_at(path, identity, config);
}

/// Publish `config` into the read cache under a caller-supplied identity.
///
/// A reader that parsed the file without the write lock passes the identity it
/// captured *before* reading, so when a concurrent publication changes the file
/// between the read and this store the entry is keyed by the stale identity: the
/// next read sees the identity mismatch and re-reads the fresh bytes instead of
/// pinning the pre-write configuration under the newer identity.
fn store_cached_config_at(
    path: &Path,
    identity: (u64, Option<std::time::SystemTime>),
    config: &GatewayConfig,
) {
    let (len, modified) = identity;
    let mut cache = CONFIG_READ_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.retain(|entry| entry.path != path);
    cache.push(CachedConfig {
        path: path.to_path_buf(),
        len,
        modified,
        config: config.clone(),
    });
}

/// Drop any cached configuration for `path`.
fn invalidate_cached_config(path: &Path) {
    let mut cache = CONFIG_READ_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.retain(|entry| entry.path != path);
}

fn read_config_file(path: &PathBuf) -> Result<Option<GatewayConfig>, String> {
    if !path.exists() {
        invalidate_cached_config(path);
        return Ok(None);
    }
    if let Some(config) = cached_config(path) {
        return Ok(Some(config));
    }
    // Capture the identity of the bytes this read is about to parse, before
    // reading them. A concurrent publication may replace the file between the
    // read and the cache store; keying the parsed bytes by this pre-read
    // identity keeps a stale entry from being pinned under the new identity.
    let identity = file_identity(path);
    let Some((config, needs_migration)) = read_config_locked(path)? else {
        invalidate_cached_config(path);
        return Ok(None);
    };
    if needs_migration {
        // Lazy migration: serialize with every other writer, re-read and
        // re-check the version under the lock, and only then stamp and rewrite,
        // so a reader can never clobber a concurrent writer's newer
        // configuration.
        let _guard = CONFIG_WRITE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some((latest, still_needs_migration)) = read_config_locked(path)? else {
            invalidate_cached_config(path);
            return Ok(None);
        };
        if still_needs_migration {
            // Best-effort: a failed rewrite keeps the previous complete bytes
            // on disk and the next read retries the migration.
            let _ = write_config(&latest);
        } else {
            store_cached_config(path, &latest);
        }
        return Ok(Some(latest));
    }
    match identity {
        Some(identity) => store_cached_config_at(path, identity, &config),
        None => invalidate_cached_config(path),
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

/// Read-path normalization: canonicalize the stored-but-untrusted fields only.
/// Explicit user/source values are never guessed or cross-populated: a stored
/// empty effort list, a missing local model and an absent price stay exactly as
/// persisted. The price table is never migrated or dropped on a read; a legacy
/// file is scoped by the version-gated migration before it is parsed.
pub(in crate::ai_gateway) fn normalize_config(config: &mut GatewayConfig) {
    normalize_stored_config(config);
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

pub(in crate::ai_gateway) fn read_config() -> Result<GatewayConfig, String> {
    let path = config_path()?;
    if let Some(config) = read_config_file(&path)? {
        migrate_legacy_files();
        return Ok(config);
    }
    Ok(GatewayConfig::default())
}

/// Provider ids present in `config`.
fn provider_ids(config: &GatewayConfig) -> HashSet<String> {
    config
        .providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect()
}

/// Prune scheduler entries for provider ids that existed before a publication
/// and are absent from the published configuration.
///
/// Only a genuine deletion triggers pruning: an id absent from `published` but
/// never present in `before_ids` belongs to another configuration sharing the
/// process-global scheduler and must be left alone.
fn prune_removed_providers(before_ids: &HashSet<String>, published: &GatewayConfig) {
    let after_ids = provider_ids(published);
    let removed: HashSet<String> = before_ids
        .iter()
        .filter(|id| !after_ids.contains(*id))
        .cloned()
        .collect();
    super::selection::prune_weighted_scheduler(&removed);
}

/// Provider ids of the configuration currently persisted at `path`, read
/// without taking the write lock and without publishing anything. A missing or
/// unreadable file yields an empty set, in which case nothing is pruned.
fn persisted_provider_ids(path: &Path) -> HashSet<String> {
    if let Some(config) = cached_config(path) {
        return provider_ids(&config);
    }
    match read_config_locked(path) {
        Ok(Some((config, _))) => provider_ids(&config),
        _ => HashSet::new(),
    }
}

/// Encrypt the entire configuration and write it atomically through a unique
/// temp file plus rename so a partial write can never corrupt the on-disk
/// state. Every write stamps the current schema version so an older file can
/// never be re-written without it, and the published normalized value becomes
/// the new read-cache entry. After a successful publication, scheduler entries
/// of provider ids removed by this write are pruned; a failed write prunes
/// nothing.
pub(in crate::ai_gateway) fn write_config(config: &GatewayConfig) -> Result<(), String> {
    let path = config_path()?;
    let before_ids = persisted_provider_ids(&path);
    let tmp = unique_config_temp_path(&path);
    let published = write_config_through_temp(config, &path, &tmp)?;
    prune_removed_providers(&before_ids, &published);
    store_cached_config(&path, &published);
    Ok(())
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
/// caller-owned temporary file `tmp`, returning the normalized value that
/// actually landed on disk (the read cache stores this, not the caller's
/// pre-normalization input). The caller owns temp-path uniqueness and, when
/// serializing writers, the write lock.
fn write_config_through_temp(
    config: &GatewayConfig,
    path: &Path,
    tmp: &Path,
) -> Result<GatewayConfig, String> {
    let mut next = config.clone();
    normalize_stored_config(&mut next);
    scope_model_prices(&mut next);
    next.schema_version = GATEWAY_CONFIG_SCHEMA_VERSION;
    let json = serde_json::to_string(&next).map_err(|e| e.to_string())?;
    let password = crate::crypto::get_or_init_master_password()?;
    let encrypted = crate::crypto::encrypt(&json, &password)?;
    fs::write(tmp, encrypted).map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())?;
    migrate_legacy_files();
    // Scheduler pruning is the caller's responsibility once this publication
    // succeeds, so it can diff the previously persisted provider ids against
    // the published ones instead of pruning ids absent from this writer.
    Ok(next)
}

/// Serialized configuration read-modify-write entry point.
///
/// `mutate` runs under the process-global write lock and receives the current
/// configuration. It returns `(changed, value)`: the mutated configuration is
/// persisted only when `changed` is true, through a per-write unique temp file,
/// and `value` is handed back to the caller. An in-lock legacy migration counts
/// as an actual state change, so a legacy file is always migrated, stamped and
/// atomically rewritten even when the mutation reports no change. A writer
/// panic poisons the lock but the next call recovers through the poisoned
/// guard. The closure must stay local and must not wait on the network, so the
/// lock is never held across an upstream wait.
///
/// The read half never calls [`read_config`]: that would re-enter the lock and
/// could itself migrate and rewrite. It reads, parses and migrates in memory
/// through [`read_config_locked`], which performs no nested lock acquisition
/// and no nested rewrite.
pub(in crate::ai_gateway) fn modify_config<T>(
    mutate: impl FnOnce(&mut GatewayConfig) -> Result<(bool, T), String>,
) -> Result<T, String> {
    let _guard = CONFIG_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = config_path()?;
    let (mut config, needs_migration) = match cached_config(&path) {
        // A cache hit is always an already-migrated normalized configuration.
        Some(config) => (config, false),
        None => read_config_locked(&path)?.unwrap_or((GatewayConfig::default(), false)),
    };
    let before_ids = provider_ids(&config);
    let (changed, value) = mutate(&mut config)?;
    if changed || needs_migration {
        let tmp = unique_config_temp_path(&path);
        let published = write_config_through_temp(&config, &path, &tmp)?;
        prune_removed_providers(&before_ids, &published);
        store_cached_config(&path, &published);
    } else {
        // Warm the cache with the fresh parse so a no-op settlement still
        // leaves the next read decryption-free.
        store_cached_config(&path, &config);
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
