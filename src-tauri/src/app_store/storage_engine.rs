use super::{now_ts, sessions_state_write_lock, EncryptedBlob, SchemaMeta};
use crate::config;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::MutexGuard;

static ATOMIC_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct CanonicalStateWriteLease {
    // Closing the file releases the OS lock before the thread mutex is released.
    _file: File,
    _thread: MutexGuard<'static, ()>,
    path: PathBuf,
}

thread_local! {
    static CANONICAL_STATE_WRITE_LEASE: RefCell<Weak<CanonicalStateWriteLease>> =
        RefCell::new(Weak::new());
}

pub(crate) struct CanonicalStateWriteGuard {
    _lease: Rc<CanonicalStateWriteLease>,
}

/// One app-store transaction gate for providers, sessions and their metadata.
/// Nested loaders/migration helpers join the current thread's lease instead of
/// acquiring a second file lock or taking provider/session locks in reverse order.
pub(in crate::app_store) fn lock_canonical_state_write() -> Result<CanonicalStateWriteGuard, String>
{
    let path = config::get_app_dir()?.join("canonical-state.lock");
    if let Some(lease) = CANONICAL_STATE_WRITE_LEASE.with(|current| current.borrow().upgrade()) {
        if lease.path != path {
            return Err("cannot change app-store profile during a write transaction".to_string());
        }
        return Ok(CanonicalStateWriteGuard { _lease: lease });
    }
    let thread = sessions_state_write_lock()
        .lock()
        .map_err(|_| "sessions state write lock poisoned".to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("failed to open {}: {e}", path.display()))?;
    lock_canonical_file(&file).map_err(|e| format!("failed to lock {}: {e}", path.display()))?;
    let lease = Rc::new(CanonicalStateWriteLease {
        _file: file,
        _thread: thread,
        path,
    });
    CANONICAL_STATE_WRITE_LEASE.with(|current| *current.borrow_mut() = Rc::downgrade(&lease));
    Ok(CanonicalStateWriteGuard { _lease: lease })
}

#[cfg(unix)]
fn lock_canonical_file(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    extern "C" {
        fn flock(fd: std::os::raw::c_int, operation: std::os::raw::c_int) -> std::os::raw::c_int;
    }
    loop {
        // LOCK_EX is 2 on the supported Unix platforms. The descriptor remains
        // owned by the lease; its last close releases the lock, even on process exit.
        if unsafe { flock(file.as_raw_fd(), 2) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(windows)]
fn lock_canonical_file(file: &File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "kernel32")]
    extern "system" {
        fn LockFile(
            handle: *mut std::ffi::c_void,
            offset_low: u32,
            offset_high: u32,
            bytes_low: u32,
            bytes_high: u32,
        ) -> i32;
    }
    loop {
        // Lock a byte in the persistent sidecar, not the atomically replaced state.
        if unsafe { LockFile(file.as_raw_handle(), 0, 0, 1, 0) } != 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(33) {
            // ERROR_LOCK_VIOLATION
            return Err(error);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

pub(in crate::app_store) struct StorageEngine;

impl StorageEngine {
    pub(in crate::app_store) fn base_dir() -> Result<PathBuf, String> {
        let root = crate::get_data_dir()?;
        let target = root.join("data");
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        Ok(target)
    }

    pub(in crate::app_store) fn meta_dir() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("meta");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p)
    }

    pub(in crate::app_store) fn providers_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("providers");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    pub(in crate::app_store) fn provider_presets_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("provider_presets");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    pub(in crate::app_store) fn sessions_path() -> Result<PathBuf, String> {
        // AI sessions are always stored in local app data to keep history
        // independent from user-selected storage backends (git/iCloud/custom path).
        let p = config::get_app_dir()?
            .join("data")
            .join("data")
            .join("sessions");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    pub(in crate::app_store) fn sessions_path_in_selected_storage() -> Result<PathBuf, String> {
        let root = crate::get_data_dir()?;
        Ok(root.join("data").join("sessions").join("state.json"))
    }

    pub(in crate::app_store) fn launcher_path() -> Result<PathBuf, String> {
        // Launcher items are always stored in local app data so they do not
        // depend on user-selected storage backends (git/iCloud/custom path).
        let p = config::get_app_dir()?
            .join("data")
            .join("data")
            .join("launcher");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    pub(in crate::app_store) fn launcher_path_in_selected_storage() -> Result<PathBuf, String> {
        let root = crate::get_data_dir()?;
        Ok(root.join("data").join("launcher").join("state.json"))
    }

    pub(in crate::app_store) fn secrets_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("secrets");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.enc.json"))
    }

    pub(in crate::app_store) fn mcp_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("mcp");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("state.json"))
    }

    pub(in crate::app_store) fn content_path(name: &str) -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("content");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join(format!("{}.enc.json", name)))
    }

    pub(in crate::app_store) fn outbox_path() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("events");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p.join("outbox.json"))
    }

    pub(in crate::app_store) fn schema_path() -> Result<PathBuf, String> {
        Ok(Self::meta_dir()?.join("schema.json"))
    }

    pub(in crate::app_store) fn migration_state_path() -> Result<PathBuf, String> {
        Ok(Self::meta_dir()?.join("migration_state.json"))
    }

    pub(in crate::app_store) fn migration_report_path() -> Result<PathBuf, String> {
        Ok(Self::meta_dir()?.join("migration_report.json"))
    }

    pub(in crate::app_store) fn backup_root() -> Result<PathBuf, String> {
        let p = Self::base_dir()?.join("backups");
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p)
    }

    pub(in crate::app_store) fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = path.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            ATOMIC_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            file.write_all(content.as_bytes())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            fs::rename(&temp, path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    pub(in crate::app_store) fn read_json<T: for<'de> Deserialize<'de> + Default>(
        path: &Path,
    ) -> Result<T, String> {
        if !path.exists() {
            return Ok(T::default());
        }
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        if content.trim().is_empty() {
            return Ok(T::default());
        }
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    pub(in crate::app_store) fn write_json<T: Serialize>(
        path: &Path,
        value: &T,
    ) -> Result<(), String> {
        let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        Self::atomic_write(path, &content)
    }

    pub(in crate::app_store) fn load_schema() -> Result<SchemaMeta, String> {
        let _operation = lock_canonical_state_write()?;
        let path = Self::schema_path()?;
        if !path.exists() {
            let schema = SchemaMeta::default();
            Self::write_json(&path, &schema)?;
            return Ok(schema);
        }
        Self::read_json(&path)
    }

    pub(in crate::app_store) fn bump_revision() -> Result<SchemaMeta, String> {
        let _operation = lock_canonical_state_write()?;
        let mut schema = Self::load_schema()?;
        schema.revision = schema.revision.saturating_add(1);
        schema.last_migrated_at = now_ts();
        Self::write_json(&Self::schema_path()?, &schema)?;
        Ok(schema)
    }
}

pub(in crate::app_store) fn migrate_sessions_to_local_if_needed(
    local_path: &Path,
) -> Result<(), String> {
    let legacy_path = StorageEngine::sessions_path_in_selected_storage()?;
    if legacy_path == local_path || !legacy_path.exists() || local_path.exists() {
        return Ok(());
    }

    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&legacy_path, local_path).map_err(|e| e.to_string())?;
    Ok(())
}

pub(in crate::app_store) fn migrate_launcher_to_local_if_needed(
    local_path: &Path,
) -> Result<(), String> {
    let legacy_path = StorageEngine::launcher_path_in_selected_storage()?;
    if legacy_path == local_path || !legacy_path.exists() || local_path.exists() {
        return Ok(());
    }

    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&legacy_path, local_path).map_err(|e| e.to_string())?;
    Ok(())
}

pub(in crate::app_store) struct CryptoService;

impl CryptoService {
    pub(in crate::app_store) fn encrypt(value: &str) -> Result<String, String> {
        let password = crate::crypto::get_or_init_master_password()?;
        crate::crypto::encrypt(value, &password)
    }

    pub(in crate::app_store) fn decrypt(value: &str) -> Result<String, String> {
        let password = crate::crypto::get_or_init_master_password()?;
        crate::crypto::decrypt(value, &password)
    }

    pub(in crate::app_store) fn encrypt_json(value: &Value) -> Result<EncryptedBlob, String> {
        Ok(EncryptedBlob {
            is_encrypted: true,
            data: Self::encrypt(&value.to_string())?,
        })
    }

    pub(in crate::app_store) fn decrypt_json(blob: &EncryptedBlob) -> Result<Value, String> {
        if !blob.is_encrypted {
            return serde_json::from_str(&blob.data).map_err(|e| e.to_string());
        }
        let plain = Self::decrypt(&blob.data)?;
        serde_json::from_str(&plain).map_err(|e| e.to_string())
    }
}
