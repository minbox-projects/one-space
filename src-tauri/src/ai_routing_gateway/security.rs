use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
#[cfg(target_os = "macos")]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use rusqlite::Connection;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
#[cfg(windows)]
use std::os::windows::{
    ffi::OsStrExt,
    fs::OpenOptionsExt,
    io::{AsRawHandle, RawHandle},
};

use super::error::{GatewayError, GatewayErrorCategory};

const CIPHER_VERSION: i64 = 1;
const ROOT_KEY_LENGTH: usize = 32;
const NONCE_LENGTH: usize = 12;
const ROOT_KEY_FILE: &str = "ai-routing-gateway-root-key-v1";
const ROOT_KEY_LOCK_FILE: &str = "ai-routing-gateway-root-key-v1.lock";
const LEGACY_CLEANUP_MARKER_FILE: &str = "ai-routing-gateway-root-key-v1.legacy-cleanup-pending";
const GATEWAY_API_KEY_RECORD_TYPE: &str = "gateway_api_key";
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "com.onespace.ai-routing-gateway";
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "root-data-key-v1";

pub(crate) struct RootKey([u8; ROOT_KEY_LENGTH]);

impl std::fmt::Debug for RootKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RootKey([REDACTED])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EncryptedCredential {
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) nonce: [u8; NONCE_LENGTH],
    pub(crate) cipher_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecurityLockReason {
    StorageUnavailable,
    RootKeyMissing,
    CredentialStoreUnavailable,
    RootKeyInvalid,
    MigrationUnavailable,
    MigrationValidationFailed,
}

#[derive(Debug)]
pub(crate) enum SecurityState {
    Ready(RootKey),
    Locked(SecurityLockReason),
}

pub(crate) trait InitializationLock {}

pub(crate) trait RootKeyStore {
    fn load(&self) -> Result<Option<Vec<u8>>, GatewayError>;
    fn store(&self, key: &[u8]) -> Result<(), GatewayError>;

    fn legacy_cleanup_pending(&self) -> Result<bool, GatewayError> {
        Ok(false)
    }

    fn mark_legacy_cleanup_pending(&self) -> Result<(), GatewayError> {
        Err(storage_error())
    }

    fn clear_legacy_cleanup_pending(&self) -> Result<(), GatewayError> {
        Ok(())
    }

    fn remove(&self) -> Result<(), GatewayError> {
        Err(storage_error())
    }

    fn acquire_initialization_lock(&self)
        -> Result<Box<dyn InitializationLock + '_>, GatewayError>;
}

pub(crate) trait LegacyRootKeyStore {
    fn load(&self) -> Result<Option<Vec<u8>>, GatewayError>;
    fn delete(&self) -> Result<(), GatewayError>;
}

#[derive(Debug, Clone)]
pub(crate) struct LocalRootKeyStore {
    path: Option<PathBuf>,
    #[cfg(test)]
    fail_directory_sync_after_rename: bool,
    #[cfg(test)]
    fail_directory_parent_sync: bool,
}

impl Default for LocalRootKeyStore {
    fn default() -> Self {
        Self {
            path: dirs::home_dir()
                .map(|home| home.join(".config").join("onespace").join(ROOT_KEY_FILE)),
            #[cfg(test)]
            fail_directory_sync_after_rename: false,
            #[cfg(test)]
            fail_directory_parent_sync: false,
        }
    }
}

impl LocalRootKeyStore {
    #[cfg(test)]
    pub(crate) fn at(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            fail_directory_sync_after_rename: false,
            fail_directory_parent_sync: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn failing_after_rename_sync(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            fail_directory_sync_after_rename: true,
            fail_directory_parent_sync: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn failing_directory_parent_sync(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            fail_directory_sync_after_rename: false,
            fail_directory_parent_sync: true,
        }
    }

    fn path(&self) -> Result<&Path, GatewayError> {
        self.path.as_deref().ok_or_else(storage_error)
    }

    fn directory(&self) -> Result<&Path, GatewayError> {
        self.path()?.parent().ok_or_else(storage_error)
    }

    fn prepare_directory(&self) -> Result<(), GatewayError> {
        let directory = self.directory()?;
        ensure_directory(directory).map_err(|_| storage_error())?;
        #[cfg(unix)]
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| storage_error())?;
        #[cfg(test)]
        if self.fail_directory_parent_sync {
            return Err(storage_error());
        }
        sync_directory(parent_directory(directory)).map_err(|_| storage_error())?;
        Ok(())
    }

    fn legacy_cleanup_marker_path(&self) -> Result<PathBuf, GatewayError> {
        Ok(self.directory()?.join(LEGACY_CLEANUP_MARKER_FILE))
    }

    fn write_legacy_cleanup_marker(&self) -> Result<(), GatewayError> {
        let path = self.legacy_cleanup_marker_path()?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => return Ok(()),
            Ok(_) => return Err(storage_error()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(storage_error()),
        }

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut marker = options.open(&path).map_err(|_| storage_error())?;
        marker
            .write_all(b"legacy_keychain_cleanup_pending\n")
            .map_err(|_| storage_error())?;
        marker.sync_all().map_err(|_| storage_error())?;
        sync_directory(self.directory()?).map_err(|_| storage_error())
    }

    fn remove_legacy_cleanup_marker(&self) -> Result<(), GatewayError> {
        let path = self.legacy_cleanup_marker_path()?;
        match fs::remove_file(path) {
            Ok(()) => sync_directory(self.directory()?).map_err(|_| storage_error()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(storage_error()),
        }
    }

    fn cleanup_stale_temporary_files(&self) -> Result<(), GatewayError> {
        let directory = self.directory()?;
        let mut removed = false;
        for entry in fs::read_dir(directory).map_err(|_| storage_error())? {
            let entry = entry.map_err(|_| storage_error())?;
            if !is_root_key_temporary_name(&entry.file_name()) {
                continue;
            }
            let file_type = entry.file_type().map_err(|_| storage_error())?;
            if !file_type.is_file() && !file_type.is_symlink() {
                continue;
            }
            fs::remove_file(entry.path()).map_err(|_| storage_error())?;
            removed = true;
        }
        if removed {
            sync_directory(directory).map_err(|_| storage_error())?;
        }
        Ok(())
    }

    fn sync_after_rename(&self) -> Result<(), GatewayError> {
        #[cfg(test)]
        if self.fail_directory_sync_after_rename {
            return Err(storage_error());
        }
        sync_directory(self.directory()?).map_err(|_| storage_error())
    }

    fn remove_final_after_failed_persistence(&self, path: &Path) -> Result<(), GatewayError> {
        match fs::remove_file(path) {
            Ok(()) => sync_directory(self.directory()?).map_err(|_| storage_error()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(storage_error()),
        }
    }
}

impl RootKeyStore for LocalRootKeyStore {
    fn load(&self) -> Result<Option<Vec<u8>>, GatewayError> {
        self.prepare_directory()?;
        let path = self.path()?;
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(storage_error()),
        };
        if !metadata.file_type().is_file() {
            return Err(storage_error());
        }
        #[cfg(unix)]
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|_| storage_error())?;
        let mut file = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|_| storage_error())?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|_| storage_error())?;
        Ok(Some(bytes))
    }

    fn store(&self, key: &[u8]) -> Result<(), GatewayError> {
        if key.len() != ROOT_KEY_LENGTH {
            return Err(storage_error());
        }
        self.prepare_directory()?;
        let path = self.path()?;
        let temporary_path = self.directory()?.join(format!(
            ".{ROOT_KEY_FILE}.tmp-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut renamed = false;
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut temporary = options.open(&temporary_path).map_err(|_| storage_error())?;
            temporary.write_all(key).map_err(|_| storage_error())?;
            temporary.sync_all().map_err(|_| storage_error())?;
            #[cfg(unix)]
            fs::set_permissions(&temporary_path, fs::Permissions::from_mode(0o600))
                .map_err(|_| storage_error())?;
            rename_temporary(&temporary_path, path).map_err(|_| storage_error())?;
            renamed = true;
            self.sync_after_rename()?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&temporary_path);
                if renamed {
                    self.remove_final_after_failed_persistence(path)?;
                }
                Err(error)
            }
        }
    }

    fn legacy_cleanup_pending(&self) -> Result<bool, GatewayError> {
        let path = self.legacy_cleanup_marker_path()?;
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_file() => Ok(true),
            Ok(_) => Err(storage_error()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(storage_error()),
        }
    }

    fn mark_legacy_cleanup_pending(&self) -> Result<(), GatewayError> {
        self.write_legacy_cleanup_marker()
    }

    fn clear_legacy_cleanup_pending(&self) -> Result<(), GatewayError> {
        self.remove_legacy_cleanup_marker()
    }

    fn remove(&self) -> Result<(), GatewayError> {
        match fs::remove_file(self.path()?) {
            Ok(()) => sync_directory(self.directory()?).map_err(|_| storage_error()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(storage_error()),
        }
    }

    fn acquire_initialization_lock(
        &self,
    ) -> Result<Box<dyn InitializationLock + '_>, GatewayError> {
        self.prepare_directory()?;
        #[cfg(any(unix, windows))]
        {
            let lock_path = self.directory()?.join(ROOT_KEY_LOCK_FILE);
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true);
            #[cfg(unix)]
            options.mode(0o600);
            #[cfg(windows)]
            options.share_mode(0x0000_0007);
            let file = options.open(&lock_path).map_err(|_| storage_error())?;
            #[cfg(unix)]
            fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600))
                .map_err(|_| storage_error())?;
            lock_file(&file)?;
            let lock = FileInitializationLock(file);
            self.cleanup_stale_temporary_files()?;
            Ok(Box::new(lock))
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(storage_error())
        }
    }
}

#[cfg(any(unix, windows))]
struct FileInitializationLock(File);

#[cfg(any(unix, windows))]
impl InitializationLock for FileInitializationLock {}

#[cfg(unix)]
impl Drop for FileInitializationLock {
    fn drop(&mut self) {
        unsafe {
            flock(self.0.as_raw_fd(), LOCK_UN);
        }
    }
}

#[cfg(windows)]
impl Drop for FileInitializationLock {
    fn drop(&mut self) {
        unsafe {
            let mut overlapped = WindowsOverlapped::zeroed();
            let _ = UnlockFileEx(self.0.as_raw_handle(), 0, 1, 0, &mut overlapped);
        }
    }
}

#[cfg(unix)]
const LOCK_EX: i32 = 2;
#[cfg(unix)]
const LOCK_UN: i32 = 8;

#[cfg(unix)]
extern "C" {
    fn flock(file_descriptor: i32, operation: i32) -> i32;
}

#[cfg(unix)]
fn lock_file(file: &File) -> Result<(), GatewayError> {
    if unsafe { flock(file.as_raw_fd(), LOCK_EX) } == 0 {
        Ok(())
    } else {
        Err(storage_error())
    }
}

#[cfg(windows)]
#[repr(C)]
struct WindowsOverlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    event: *mut std::ffi::c_void,
}

#[cfg(windows)]
impl WindowsOverlapped {
    fn zeroed() -> Self {
        Self {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            event: std::ptr::null_mut(),
        }
    }
}

#[cfg(windows)]
fn lock_file(file: &File) -> Result<(), GatewayError> {
    let mut overlapped = WindowsOverlapped::zeroed();
    let result = unsafe {
        LockFileEx(
            file.as_raw_handle(),
            LOCKFILE_EXCLUSIVE_LOCK,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if result != 0 {
        Ok(())
    } else {
        Err(storage_error())
    }
}

#[cfg(windows)]
const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LockFileEx(
        file: RawHandle,
        flags: u32,
        reserved: u32,
        bytes_to_lock_low: u32,
        bytes_to_lock_high: u32,
        overlapped: *mut WindowsOverlapped,
    ) -> i32;
    fn UnlockFileEx(
        file: RawHandle,
        reserved: u32,
        bytes_to_unlock_low: u32,
        bytes_to_unlock_high: u32,
        overlapped: *mut WindowsOverlapped,
    ) -> i32;
    fn MoveFileExW(existing_file_name: *const u16, new_file_name: *const u16, flags: u32) -> i32;
}

fn rename_temporary(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        let source: Vec<u16> = source
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let destination: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let result = unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if result != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)
    }
}

#[cfg(windows)]
const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
#[cfg(windows)]
const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

fn sync_directory(directory: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(directory).and_then(|directory| directory.sync_all())
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        Ok(())
    }
}

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn ensure_directory(directory: &Path) -> io::Result<()> {
    let mut current = PathBuf::new();
    for component in directory.components() {
        match component {
            Component::CurDir => current.push("."),
            Component::ParentDir => current.push(".."),
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                current.push(component.as_os_str())
            }
        }

        match fs::metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "root key directory is not a directory",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        let metadata = fs::metadata(&current)?;
                        if !metadata.file_type().is_dir() {
                            return Err(io::Error::new(
                                io::ErrorKind::AlreadyExists,
                                "root key directory is not a directory",
                            ));
                        }
                    }
                    Err(error) => return Err(error),
                }
                sync_directory(parent_directory(&current))?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn is_root_key_temporary_name(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let Some(suffix) = name.strip_prefix(&format!(".{ROOT_KEY_FILE}.tmp-")) else {
        return false;
    };
    let Some((pid, uuid)) = suffix.split_once('-') else {
        return false;
    };
    !pid.is_empty()
        && pid.chars().all(|character| character.is_ascii_digit())
        && uuid::Uuid::parse_str(uuid).is_ok()
}

#[cfg(target_os = "macos")]
pub(crate) struct MacOsKeychainStore;

#[cfg(target_os = "macos")]
impl LegacyRootKeyStore for MacOsKeychainStore {
    fn load(&self) -> Result<Option<Vec<u8>>, GatewayError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(|_| {
            GatewayError::new(GatewayErrorCategory::CredentialStoreUnavailable, None)
        })?;
        match entry.get_password() {
            Ok(encoded) => STANDARD
                .decode(encoded)
                .map(Some)
                .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialInvalid, None)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(GatewayError::new(
                GatewayErrorCategory::CredentialStoreUnavailable,
                None,
            )),
        }
    }

    fn delete(&self) -> Result<(), GatewayError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(|_| {
            GatewayError::new(GatewayErrorCategory::CredentialStoreUnavailable, None)
        })?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(storage_error()),
        }
    }
}

pub(crate) fn initialize_security(
    connection: &Connection,
    key_store: &dyn RootKeyStore,
) -> SecurityState {
    initialize_security_with_migration(connection, key_store, None)
}

pub(crate) fn initialize_security_with_migration(
    connection: &Connection,
    key_store: &dyn RootKeyStore,
    legacy_store: Option<&dyn LegacyRootKeyStore>,
) -> SecurityState {
    let _initialization_guard = match key_store.acquire_initialization_lock() {
        Ok(guard) => guard,
        Err(_) => return SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable),
    };
    let local_key = match key_store.load() {
        Ok(Some(bytes)) => match RootKey::try_from(bytes) {
            Ok(key) => {
                retry_legacy_cleanup(key_store, legacy_store);
                return SecurityState::Ready(key);
            }
            Err(_) => Some(SecurityLockReason::RootKeyInvalid),
        },
        Ok(None) => None,
        Err(_) => return SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable),
    };
    let encrypted_records = match connection.query_row(
        "SELECT (SELECT COUNT(*) FROM ai_gateway_credentials) + (SELECT COUNT(*) FROM ai_gateway_api_keys WHERE ciphertext IS NOT NULL)",
        [],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(count) => count,
        Err(_) => return SecurityState::Locked(SecurityLockReason::StorageUnavailable),
    };

    if encrypted_records == 0 {
        if let Some(reason) = local_key {
            return SecurityState::Locked(reason);
        }
        if let Some(legacy_store) = legacy_store {
            match legacy_store.load() {
                Ok(Some(bytes)) => {
                    let candidate = match RootKey::try_from(bytes) {
                        Ok(key) => key,
                        Err(_) => {
                            return SecurityState::Locked(SecurityLockReason::MigrationUnavailable)
                        }
                    };
                    return migrate_legacy_key(connection, key_store, legacy_store, candidate);
                }
                Ok(None) => {}
                Err(_) => return SecurityState::Locked(SecurityLockReason::MigrationUnavailable),
            }
        }
        let mut bytes = [0u8; ROOT_KEY_LENGTH];
        OsRng.fill_bytes(&mut bytes);
        if key_store.store(&bytes).is_err() {
            return SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable);
        }
        return match key_store
            .load()
            .and_then(|bytes| bytes.ok_or_else(storage_error).and_then(RootKey::try_from))
        {
            Ok(key) => SecurityState::Ready(key),
            Err(_) => SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable),
        };
    }

    let Some(legacy_store) = legacy_store else {
        return SecurityState::Locked(local_key.unwrap_or(SecurityLockReason::RootKeyMissing));
    };
    let candidate = match legacy_store.load() {
        Ok(Some(bytes)) => match RootKey::try_from(bytes) {
            Ok(key) => key,
            Err(_) => return SecurityState::Locked(SecurityLockReason::MigrationUnavailable),
        },
        Ok(None) | Err(_) => {
            return SecurityState::Locked(SecurityLockReason::MigrationUnavailable)
        }
    };
    if validate_existing_ciphertexts(connection, &candidate).is_err() {
        return SecurityState::Locked(SecurityLockReason::MigrationValidationFailed);
    }
    if key_store.store(&candidate.0).is_err() {
        return SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable);
    }
    let persisted = match key_store
        .load()
        .and_then(|bytes| bytes.ok_or_else(storage_error).and_then(RootKey::try_from))
    {
        Ok(key) if key.0 == candidate.0 => key,
        _ => {
            let _ = key_store.remove();
            return SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable);
        }
    };
    finish_legacy_migration(key_store, legacy_store, persisted)
}

fn migrate_legacy_key(
    connection: &Connection,
    key_store: &dyn RootKeyStore,
    legacy_store: &dyn LegacyRootKeyStore,
    candidate: RootKey,
) -> SecurityState {
    if validate_existing_ciphertexts(connection, &candidate).is_err() {
        return SecurityState::Locked(SecurityLockReason::MigrationValidationFailed);
    }
    if key_store.store(&candidate.0).is_err() {
        return SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable);
    }
    let persisted = match key_store
        .load()
        .and_then(|bytes| bytes.ok_or_else(storage_error).and_then(RootKey::try_from))
    {
        Ok(key) if key.0 == candidate.0 => key,
        _ => {
            let _ = key_store.remove();
            return SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable);
        }
    };
    finish_legacy_migration(key_store, legacy_store, persisted)
}

fn finish_legacy_migration(
    key_store: &dyn RootKeyStore,
    legacy_store: &dyn LegacyRootKeyStore,
    persisted: RootKey,
) -> SecurityState {
    if key_store.mark_legacy_cleanup_pending().is_err() {
        log::warn!("ai routing gateway legacy root key cleanup state could not be persisted");
    }
    match legacy_store.delete() {
        Ok(()) => {
            if key_store.clear_legacy_cleanup_pending().is_err() {
                log::warn!("ai routing gateway legacy root key cleanup state could not be cleared");
            }
        }
        Err(_) => {
            log::warn!("ai routing gateway legacy root key cleanup is pending");
        }
    }
    SecurityState::Ready(persisted)
}

fn retry_legacy_cleanup(
    key_store: &dyn RootKeyStore,
    legacy_store: Option<&dyn LegacyRootKeyStore>,
) {
    let pending = match key_store.legacy_cleanup_pending() {
        Ok(pending) => pending,
        Err(_) => {
            log::warn!("ai routing gateway legacy root key cleanup state could not be read");
            return;
        }
    };
    if !pending {
        return;
    }
    let Some(legacy_store) = legacy_store else {
        log::warn!("ai routing gateway legacy root key cleanup is pending without a legacy store");
        return;
    };
    match legacy_store.delete() {
        Ok(()) => {
            if key_store.clear_legacy_cleanup_pending().is_err() {
                log::warn!("ai routing gateway legacy root key cleanup state could not be cleared");
            }
        }
        Err(_) => log::warn!("ai routing gateway legacy root key cleanup retry is pending"),
    }
}

fn validate_existing_ciphertexts(
    connection: &Connection,
    root_key: &RootKey,
) -> Result<(), GatewayError> {
    let mut credentials = connection
        .prepare("SELECT account_id, record_type, ciphertext, nonce, cipher_version FROM ai_gateway_credentials")
        .map_err(|_| storage_error())?;
    let credential_rows = credentials
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|_| storage_error())?;
    for row in credential_rows {
        let (id, record_type, ciphertext, nonce, cipher_version) =
            row.map_err(|_| storage_error())?;
        validate_ciphertext(
            root_key,
            &record_type,
            &id,
            ciphertext,
            nonce,
            cipher_version,
        )?;
    }

    let mut api_keys = connection
        .prepare("SELECT id, ciphertext, nonce, cipher_version FROM ai_gateway_api_keys WHERE ciphertext IS NOT NULL")
        .map_err(|_| storage_error())?;
    let api_key_rows = api_keys
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|_| storage_error())?;
    for row in api_key_rows {
        let (id, ciphertext, nonce, cipher_version) = row.map_err(|_| storage_error())?;
        validate_ciphertext(
            root_key,
            GATEWAY_API_KEY_RECORD_TYPE,
            &id,
            ciphertext,
            nonce,
            cipher_version,
        )?;
    }
    Ok(())
}

fn validate_ciphertext(
    root_key: &RootKey,
    record_type: &str,
    id: &str,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    cipher_version: i64,
) -> Result<(), GatewayError> {
    let nonce = nonce
        .try_into()
        .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialInvalid, Some(id)))?;
    decrypt_credential(
        root_key,
        record_type,
        id,
        &EncryptedCredential {
            ciphertext,
            nonce,
            cipher_version,
        },
    )
    .map(|_| ())
}

fn storage_error() -> GatewayError {
    GatewayError::new(GatewayErrorCategory::CredentialStoreUnavailable, None)
}

impl TryFrom<Vec<u8>> for RootKey {
    type Error = GatewayError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let bytes: [u8; ROOT_KEY_LENGTH] = value
            .try_into()
            .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialInvalid, None))?;
        Ok(Self(bytes))
    }
}

pub(crate) fn encrypt_credential(
    root_key: &RootKey,
    record_type: &str,
    record_id: &str,
    plaintext: &[u8],
) -> Result<EncryptedCredential, GatewayError> {
    validate_identity(record_type, record_id)?;
    let cipher = Aes256Gcm::new_from_slice(&root_key.0)
        .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialInvalid, Some(record_id)))?;
    let mut nonce = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce);
    let aad = credential_aad(record_type, record_id);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| {
            GatewayError::new(
                GatewayErrorCategory::CredentialAuthenticationFailed,
                Some(record_id),
            )
        })?;
    Ok(EncryptedCredential {
        ciphertext,
        nonce,
        cipher_version: CIPHER_VERSION,
    })
}

pub(crate) fn decrypt_credential(
    root_key: &RootKey,
    record_type: &str,
    record_id: &str,
    credential: &EncryptedCredential,
) -> Result<Vec<u8>, GatewayError> {
    validate_identity(record_type, record_id)?;
    if credential.cipher_version != CIPHER_VERSION {
        return Err(GatewayError::new(
            GatewayErrorCategory::CredentialVersionUnsupported,
            Some(record_id),
        ));
    }
    let cipher = Aes256Gcm::new_from_slice(&root_key.0)
        .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialInvalid, Some(record_id)))?;
    let aad = credential_aad(record_type, record_id);
    cipher
        .decrypt(
            Nonce::from_slice(&credential.nonce),
            Payload {
                msg: &credential.ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| {
            GatewayError::new(
                GatewayErrorCategory::CredentialAuthenticationFailed,
                Some(record_id),
            )
        })
}

fn validate_identity(record_type: &str, record_id: &str) -> Result<(), GatewayError> {
    if record_type.is_empty() || record_id.is_empty() {
        return Err(GatewayError::new(
            GatewayErrorCategory::CredentialInvalid,
            (!record_id.is_empty()).then_some(record_id),
        ));
    }
    Ok(())
}

fn credential_aad(record_type: &str, record_id: &str) -> Vec<u8> {
    format!("onespace.ai-routing-gateway.v1\0{record_type}\0{record_id}").into_bytes()
}
