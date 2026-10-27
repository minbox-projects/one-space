use crate::{crypto, get_data_dir};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use super::state::{default_group_record, normalize_state, parse_state_payload, records_lock};
use super::types::{
    default_common_ports, EncryptedBlob, SshTunnelRecord, SshTunnelState,
};

pub(in crate::ssh_tunnels) fn get_tunnels_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    let dir = data_dir.join("data").join("ssh_tunnels");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.enc.json"))
}

/// Identity of an on-disk file used to key the decoded-state cache: the exact
/// resolved path plus byte length and modification time. This is metadata only,
/// never a content hash; any change invalidates the corresponding cache entry.
#[derive(Clone, PartialEq, Eq)]
struct CachedFileIdentity {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

fn file_identity(path: &Path) -> Option<CachedFileIdentity> {
    let metadata = fs::metadata(path).ok()?;
    Some(CachedFileIdentity {
        path: path.to_path_buf(),
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

/// The `.local_key` generation token: its file identity. A missing key file is
/// represented by `None`, so a key appearing or disappearing also invalidates.
fn master_key_identity() -> Option<CachedFileIdentity> {
    let path = crypto::get_local_key_path().ok()?;
    file_identity(&path)
}

/// One process-memory decoded SSH-tunnel state, keyed by the state file identity
/// AND the master-key file identity. The cached value is the already-normalized
/// state the current operations consume; no raw secret is stored beyond what the
/// existing behavior kept in memory.
struct SshStateCacheEntry {
    state_file: CachedFileIdentity,
    key_file: Option<CachedFileIdentity>,
    state: SshTunnelState,
}

static SSH_STATE_CACHE: Mutex<Option<SshStateCacheEntry>> = Mutex::new(None);

/// Return the cached state only when both the state file and the key file still
/// carry the identity captured when the entry was stored. A profile switch
/// resolves a different path and therefore never hits a cached entry.
fn cached_ssh_state(path: &Path) -> Option<SshTunnelState> {
    let state_file = file_identity(path)?;
    let key_file = master_key_identity();
    let cache = SSH_STATE_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = cache.as_ref()?;
    if entry.state_file == state_file && entry.key_file == key_file {
        Some(entry.state.clone())
    } else {
        None
    }
}

fn store_ssh_state_cache(
    state_file: Option<CachedFileIdentity>,
    key_file: Option<CachedFileIdentity>,
    state: &SshTunnelState,
) {
    let Some(state_file) = state_file else {
        invalidate_ssh_state_cache();
        return;
    };
    {
        let mut cache = SSH_STATE_CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *cache = Some(SshStateCacheEntry {
            state_file,
            key_file,
            state: state.clone(),
        });
    }
    record_ssh_cache_fill();
}

fn invalidate_ssh_state_cache() {
    let mut cache = SSH_STATE_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache = None;
}

#[cfg(test)]
static SSH_CACHE_FILLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static SSH_CACHE_MISSES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static SSH_CACHE_DECRYPTS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
fn record_ssh_cache_fill() {
    SSH_CACHE_FILLS.fetch_add(1, Ordering::Relaxed);
}
#[cfg(not(test))]
fn record_ssh_cache_fill() {}

#[cfg(test)]
fn record_ssh_cache_miss() {
    SSH_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
}
#[cfg(not(test))]
fn record_ssh_cache_miss() {}

#[cfg(test)]
fn record_ssh_cache_decrypt() {
    SSH_CACHE_DECRYPTS.fetch_add(1, Ordering::Relaxed);
}
#[cfg(not(test))]
fn record_ssh_cache_decrypt() {}

/// Test-only decoded-state cache counters: `fills` counts stored entries,
/// `misses` counts loads that had to read the file, and `decrypts` counts
/// ciphertext decryptions.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ssh_tunnels) struct SshStateCacheStats {
    pub(in crate::ssh_tunnels) fills: usize,
    pub(in crate::ssh_tunnels) misses: usize,
    pub(in crate::ssh_tunnels) decrypts: usize,
}

#[cfg(test)]
pub(in crate::ssh_tunnels) fn ssh_state_cache_stats() -> SshStateCacheStats {
    SshStateCacheStats {
        fills: SSH_CACHE_FILLS.load(Ordering::Relaxed),
        misses: SSH_CACHE_MISSES.load(Ordering::Relaxed),
        decrypts: SSH_CACHE_DECRYPTS.load(Ordering::Relaxed),
    }
}

/// Test-only hook that drops the decoded-state cache and zeroes the counters so
/// a scenario can be measured from a clean baseline.
#[cfg(test)]
pub(in crate::ssh_tunnels) fn reset_ssh_state_cache() {
    invalidate_ssh_state_cache();
    SSH_CACHE_FILLS.store(0, Ordering::Relaxed);
    SSH_CACHE_MISSES.store(0, Ordering::Relaxed);
    SSH_CACHE_DECRYPTS.store(0, Ordering::Relaxed);
}

pub(in crate::ssh_tunnels) fn load_state_unlocked() -> Result<SshTunnelState, String> {
    let path = get_tunnels_path()?;
    if !path.exists() {
        invalidate_ssh_state_cache();
        return Ok(SshTunnelState {
            groups: vec![default_group_record()],
            tunnels: Vec::new(),
            common_ports: default_common_ports(),
        });
    }
    if let Some(state) = cached_ssh_state(&path) {
        return Ok(state);
    }
    record_ssh_cache_miss();
    // Capture the identity of the bytes this read is about to parse, before
    // reading them, so a concurrent publication cannot pin stale state under the
    // newer identity.
    let state_file_identity = file_identity(&path);
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        invalidate_ssh_state_cache();
        return Ok(SshTunnelState {
            groups: vec![default_group_record()],
            tunnels: Vec::new(),
            common_ports: default_common_ports(),
        });
    }
    if let Ok(mut state) = parse_state_payload(&content) {
        normalize_state(&mut state);
        store_ssh_state_cache(state_file_identity, master_key_identity(), &state);
        return Ok(state);
    }
    let blob: EncryptedBlob = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    if blob.is_encrypted {
        let password = crypto::get_or_init_master_password()?;
        let key_file_identity = master_key_identity();
        record_ssh_cache_decrypt();
        match crypto::decrypt(&blob.data, &password) {
            Ok(plain) => {
                let mut state = parse_state_payload(&plain)?;
                normalize_state(&mut state);
                store_ssh_state_cache(state_file_identity, key_file_identity, &state);
                return Ok(state);
            }
            Err(error) => {
                // Wrong password (or a rotated key) must never fall back to a
                // stale cached value: drop the entry and surface the failure.
                invalidate_ssh_state_cache();
                return Err(error);
            }
        }
    }
    let mut state = parse_state_payload(&blob.data)?;
    normalize_state(&mut state);
    store_ssh_state_cache(state_file_identity, master_key_identity(), &state);
    Ok(state)
}

pub(in crate::ssh_tunnels) fn write_state_unlocked(state: &SshTunnelState) -> Result<(), String> {
    let path = get_tunnels_path()?;
    let password = crypto::get_or_init_master_password()?;
    let key_file_identity = master_key_identity();
    let mut normalized = state.clone();
    normalize_state(&mut normalized);
    let json = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    let encrypted = crypto::encrypt(&json, &password)?;
    let blob = EncryptedBlob {
        is_encrypted: true,
        data: encrypted,
    };
    let wrapped = serde_json::to_string_pretty(&blob).map_err(|e| e.to_string())?;
    fs::write(&path, wrapped).map_err(|e| e.to_string())?;
    // A successful write refreshes the cache under the identity of the bytes
    // just written, so the next read sees it without a redundant decrypt.
    store_ssh_state_cache(file_identity(&path), key_file_identity, &normalized);
    Ok(())
}

pub(in crate::ssh_tunnels) fn load_state() -> Result<SshTunnelState, String> {
    let _guard = records_lock().lock().map_err(|e| e.to_string())?;
    load_state_unlocked()
}

pub(in crate::ssh_tunnels) fn mutate_state<T>(
    mutator: impl FnOnce(&mut SshTunnelState) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = records_lock().lock().map_err(|e| e.to_string())?;
    let mut state = load_state_unlocked()?;
    let result = mutator(&mut state)?;
    write_state_unlocked(&state)?;
    Ok(result)
}

pub(in crate::ssh_tunnels) fn load_records() -> Result<Vec<SshTunnelRecord>, String> {
    Ok(load_state()?.tunnels)
}

pub(in crate::ssh_tunnels) fn mutate_records<T>(
    mutator: impl FnOnce(&mut Vec<SshTunnelRecord>) -> Result<T, String>,
) -> Result<T, String> {
    mutate_state(|state| mutator(&mut state.tunnels))
}
