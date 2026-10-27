use std::fs;
use std::path::{Path, PathBuf};

use super::normalization::normalize_device_launch_commands;
use super::types::{
    apply_shared_profile, device_from_storage, shared_profile_from_storage, storage_from_device,
    DeviceConfig, SharedProfile, StorageConfig,
};

fn load_legacy_storage_config_from_device_file() -> Result<Option<StorageConfig>, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(serde_json::from_str::<StorageConfig>(&content).ok())
}

pub(super) fn config_path() -> Result<PathBuf, String> {
    Ok(get_app_dir()?.join("config.json"))
}

/// True for a debug build outside `cfg(test)`. Test compilation must resolve
/// the release profile so the existing test fixtures keep their paths.
pub(crate) fn is_dev_build() -> bool {
    cfg!(all(debug_assertions, not(test)))
}

/// Resolve the profile app directory for `home`: `~/.config/onespace-dev` for a
/// dev build and `~/.config/onespace` for a release build.
pub(crate) fn app_dir_for(home: &Path, is_dev: bool) -> PathBuf {
    if is_dev {
        home.join(".config").join("onespace-dev")
    } else {
        home.join(".config").join("onespace")
    }
}

/// Default storage backend for a build profile: a dev build always uses the
/// local root so it never touches the release profile's iCloud storage, while a
/// release build keeps the platform default (iCloud on macOS, local elsewhere).
pub(crate) fn default_storage_type_for(is_dev: bool) -> &'static str {
    if is_dev {
        "local"
    } else if cfg!(target_os = "macos") {
        "icloud"
    } else {
        "local"
    }
}

pub fn get_app_dir() -> Result<PathBuf, String> {
    #[cfg(test)]
    let home_dir = super::test_home::test_home_override().or_else(dirs::home_dir);
    #[cfg(not(test))]
    let home_dir = dirs::home_dir();
    let home_dir = home_dir.ok_or("Could not find home directory")?;
    let app_dir = app_dir_for(&home_dir, is_dev_build());
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    }
    Ok(app_dir)
}

/// Copy the release profile's master password and encrypted AI gateway config
/// into the dev profile over explicit directories.
///
/// Step A seeds `.local_key` first so Step B can compare both profiles' master
/// passwords. The gateway is copied only when its release source exists, the
/// dev destination is absent, and both `.local_key` files read as equal bytes;
/// a missing or unreadable key, or a password mismatch, skips the copy without
/// error. Missing release sources are a no-op and existing destination files are
/// never overwritten, so a later start only restores files the user deleted. A
/// failed copy removes its partial destination (best-effort) before returning
/// the error, so a later start retries from a clean state. No other file
/// (notably no usage database) is read or written.
///
/// Every file copy goes through the injectable `copy` seam so tests can simulate
/// a mid-copy failure; `seed_gateway_files` delegates with [`fs::copy`].
pub(super) fn seed_gateway_files_with<F>(
    release_dir: &Path,
    dev_dir: &Path,
    copy: F,
) -> Result<(), String>
where
    F: FnMut(&Path, &Path) -> Result<(), String>,
{
    let release_key = release_dir.join(".local_key");
    let dev_key = dev_dir.join(".local_key");
    let release_gateway = release_dir.join("ai_gateway.json");
    let dev_gateway = dev_dir.join("ai_gateway.json");

    let mut copy = copy;

    if release_key.is_file() && !dev_key.exists() {
        fs::create_dir_all(dev_dir).map_err(|e| e.to_string())?;
        if let Err(err) = copy(&release_key, &dev_key) {
            let _ = fs::remove_file(&dev_key);
            return Err(err);
        }
    }

    if release_gateway.is_file() && !dev_gateway.exists() {
        let release_key_bytes = fs::read(&release_key);
        let dev_key_bytes = fs::read(&dev_key);
        if let (Ok(release_key_bytes), Ok(dev_key_bytes)) = (release_key_bytes, dev_key_bytes) {
            if release_key_bytes == dev_key_bytes {
                if let Err(err) = copy(&release_gateway, &dev_gateway) {
                    let _ = fs::remove_file(&dev_gateway);
                    return Err(err);
                }
            }
        }
    }

    Ok(())
}

/// Thin production wrapper over [`seed_gateway_files_with`] that performs the
/// actual filesystem copy.
pub(crate) fn seed_gateway_files(release_dir: &Path, dev_dir: &Path) -> Result<(), String> {
    seed_gateway_files_with(release_dir, dev_dir, |src, dst| {
        fs::copy(src, dst).map(|_| ()).map_err(|e| e.to_string())
    })
}

/// Seed the dev profile's gateway files from the release profile on startup.
///
/// Release builds return immediately. Failures only log so startup continues
/// and the next start retries.
pub(crate) fn seed_dev_gateway_files_on_start() {
    if !is_dev_build() {
        return;
    }

    #[cfg(test)]
    let home_dir = super::test_home::test_home_override().or_else(dirs::home_dir);
    #[cfg(not(test))]
    let home_dir = dirs::home_dir();
    let Some(home_dir) = home_dir else {
        eprintln!("dev gateway seed skipped: could not find home directory");
        return;
    };

    let release_dir = app_dir_for(&home_dir, false);
    let dev_dir = match get_app_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("dev gateway seed skipped: {}", err);
            return;
        }
    };

    if let Err(err) = seed_gateway_files(&release_dir, &dev_dir) {
        eprintln!(
            "dev gateway seed failed ({} -> {}): {}",
            release_dir.display(),
            dev_dir.display(),
            err
        );
    }
}

pub(super) fn copy_tree_if_missing(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() || !src.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;

    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_tree_if_missing(&from, &to)?;
        } else if from.is_file() && !to.exists() {
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn copy_entry_if_missing(src_root: &Path, dst_root: &Path, rel: &str) -> Result<(), String> {
    let src = src_root.join(rel);
    let dst = dst_root.join(rel);
    if src.is_dir() {
        return copy_tree_if_missing(&src, &dst);
    }
    if src.is_file() && !dst.exists() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(src, dst).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Resolve the device-selected storage root against an explicit profile app
/// directory. The local default and the non-macOS iCloud fallback live under
/// `app_dir/data`; `git` lives under `app_dir/git_data`. A custom local/iCloud
/// path and the macOS iCloud default are absolute user/home paths as before.
pub(crate) fn resolve_selected_storage_root_at(
    cfg: &DeviceConfig,
    app_dir: &Path,
) -> Result<PathBuf, String> {
    let root = match cfg.storage_type.as_str() {
        "git" => app_dir.join("git_data"),
        "icloud" => {
            #[cfg(target_os = "macos")]
            {
                if let Some(ref custom_path) = cfg.icloud_storage_path {
                    PathBuf::from(custom_path)
                } else {
                    dirs::home_dir()
                        .ok_or("Home dir not found")?
                        .join("Library/Mobile Documents/com~apple~CloudDocs/onespace")
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                app_dir.join("data")
            }
        }
        _ => {
            if let Some(ref custom_path) = cfg.local_storage_path {
                PathBuf::from(custom_path)
            } else {
                app_dir.join("data")
            }
        }
    };

    Ok(root)
}

fn resolve_selected_storage_root_from_device(cfg: &DeviceConfig) -> Result<PathBuf, String> {
    let app_dir = get_app_dir()?;
    resolve_selected_storage_root_at(cfg, &app_dir)
}

fn local_data_init_marker(local_root: &Path) -> PathBuf {
    local_root.join(".local_mirror_initialized_v1")
}

pub(super) fn ensure_local_data_mirror_initialized_at(
    local_root: &Path,
    legacy_root: &Path,
) -> Result<(), String> {
    let marker = local_data_init_marker(local_root);
    if marker.exists() {
        return Ok(());
    }

    fs::create_dir_all(local_root).map_err(|e| e.to_string())?;

    if legacy_root.exists() && legacy_root != local_root {
        // Copy only known OneSpace data domains to avoid pulling huge storage trees
        // (e.g. git metadata or unrelated folders) into local mirror on first run.
        for rel in [
            "data",
            "shared",
            "ai_providers.json",
            "providers.json",
            "snippets.json",
            "bookmarks.json",
            "notes.json",
            "mcp_servers.json",
            "backups",
        ] {
            if let Err(err) = copy_entry_if_missing(legacy_root, local_root, rel) {
                eprintln!(
                    "local_data_mirror_init: failed to copy {} from {} -> {}: {}",
                    rel,
                    legacy_root.display(),
                    local_root.display(),
                    err
                );
            }
        }
    }

    fs::write(&marker, now_marker_value()).map_err(|e| e.to_string())?;
    Ok(())
}

fn ensure_local_data_mirror_initialized(local_root: &Path) -> Result<(), String> {
    let device_cfg = get_device_config()?;
    let legacy_root = resolve_selected_storage_root_from_device(&device_cfg)?;
    ensure_local_data_mirror_initialized_at(local_root, &legacy_root)
}

fn now_marker_value() -> String {
    format!("initialized_at={}", chrono::Utc::now().timestamp())
}

pub fn get_local_data_dir() -> Result<PathBuf, String> {
    let local_root = get_app_dir()?.join("local_data");
    if !local_root.exists() {
        fs::create_dir_all(&local_root).map_err(|e| e.to_string())?;
    }
    ensure_local_data_mirror_initialized(&local_root)?;
    Ok(local_root)
}

pub fn shared_profile_local_path() -> Result<PathBuf, String> {
    Ok(get_local_data_dir()?
        .join("shared")
        .join("profile")
        .join("skills_sources.json"))
}

pub fn load_shared_profile_local() -> Result<Option<SharedProfile>, String> {
    let path = shared_profile_local_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let parsed = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    Ok(Some(parsed))
}

pub fn save_shared_profile_local(profile: &SharedProfile) -> Result<(), String> {
    let path = shared_profile_local_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
    crate::atomic_write_string(&path, &content)
}

/// Shared-storage counterpart of [`resolve_selected_storage_root_at`]: the same
/// type mapping resolved against an explicit profile app directory.
pub(crate) fn resolve_shared_storage_root_at(
    config: &StorageConfig,
    app_dir: &Path,
) -> Result<PathBuf, String> {
    let root = match config.storage_type.as_str() {
        "git" => app_dir.join("git_data"),
        "icloud" => {
            #[cfg(target_os = "macos")]
            {
                if let Some(ref custom_path) = config.icloud_storage_path {
                    PathBuf::from(custom_path)
                } else {
                    dirs::home_dir()
                        .ok_or("Home dir not found")?
                        .join("Library/Mobile Documents/com~apple~CloudDocs/onespace")
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                app_dir.join("data")
            }
        }
        _ => {
            if let Some(ref custom_path) = config.local_storage_path {
                PathBuf::from(custom_path)
            } else {
                app_dir.join("data")
            }
        }
    };
    Ok(root)
}

pub fn resolve_shared_storage_root(config: &StorageConfig) -> Result<PathBuf, String> {
    let app_dir = get_app_dir()?;
    resolve_shared_storage_root_at(config, &app_dir)
}

pub fn get_shared_data_dir_for(config: &StorageConfig) -> Result<PathBuf, String> {
    let root = resolve_shared_storage_root(config)?;
    let shared = root.join("shared");
    fs::create_dir_all(&shared).map_err(|e| e.to_string())?;
    Ok(shared)
}

pub(super) fn get_device_config() -> Result<DeviceConfig, String> {
    let config_path = config_path()?;

    if !config_path.exists() {
        return Ok(DeviceConfig::default());
    }

    let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(DeviceConfig::default());
    }

    if let Ok(mut device) = serde_json::from_str::<DeviceConfig>(&content) {
        if normalize_device_launch_commands(&mut device) {
            let _ = save_device_config(&device);
        }
        return Ok(device);
    }

    if let Ok(legacy) = serde_json::from_str::<StorageConfig>(&content) {
        let mut device = device_from_storage(&legacy);
        let _ = normalize_device_launch_commands(&mut device);
        return Ok(device);
    }

    Ok(DeviceConfig::default())
}

pub(super) fn save_device_config(device: &DeviceConfig) -> Result<(), String> {
    let config_path = config_path()?;
    let content = serde_json::to_string_pretty(device).map_err(|e| e.to_string())?;
    crate::atomic_write_string(&config_path, &content)
}

pub fn get_config() -> Result<StorageConfig, String> {
    let device = get_device_config()?;
    let mut merged = storage_from_device(device);

    match load_shared_profile_local() {
        Ok(Some(shared)) => apply_shared_profile(&mut merged, &shared),
        Ok(None) => {
            if let Some(legacy) = load_legacy_storage_config_from_device_file()? {
                let profile = shared_profile_from_storage(&legacy);
                if !profile.is_effectively_empty() {
                    let _ = save_shared_profile_local(&profile);
                    apply_shared_profile(&mut merged, &profile);
                }
            } else {
                apply_shared_profile(&mut merged, &SharedProfile::default());
            }
        }
        Err(_) => {
            if let Some(legacy) = load_legacy_storage_config_from_device_file()? {
                let profile = shared_profile_from_storage(&legacy);
                if !profile.is_effectively_empty() {
                    let _ = save_shared_profile_local(&profile);
                    apply_shared_profile(&mut merged, &profile);
                }
            } else {
                apply_shared_profile(&mut merged, &SharedProfile::default());
            }
        }
    }

    Ok(merged)
}
