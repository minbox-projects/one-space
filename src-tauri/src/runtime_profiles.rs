use crate::get_data_dir;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sanitize_profile_id(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        }
    }
    if out.is_empty() {
        format!("rp-{}", now_ts())
    } else {
        out
    }
}

pub fn runtime_profiles_root() -> Result<PathBuf, String> {
    let root = get_data_dir()?.join("data").join("runtime_profiles");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

pub fn runtime_profile_dir(profile_id: &str) -> Result<PathBuf, String> {
    Ok(runtime_profiles_root()?.join(sanitize_profile_id(profile_id)))
}

fn set_dir_mode_700(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn set_file_mode_600(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    set_dir_mode_700(path)
}

pub fn runtime_env_for_profile(profile_id: &str) -> Result<HashMap<String, String>, String> {
    let profile_dir = runtime_profile_dir(profile_id)?;
    let home_dir = profile_dir.join("home");
    let xdg_config_home = profile_dir.join("xdg_config");
    let xdg_data_home = profile_dir.join("xdg_data");

    if !profile_dir.exists() {
        return Err(format!("runtime profile not found: {}", profile_id));
    }

    ensure_dir(&home_dir)?;
    ensure_dir(&xdg_config_home)?;
    ensure_dir(&xdg_data_home)?;

    let mut env = HashMap::new();
    env.insert("HOME".to_string(), home_dir.to_string_lossy().to_string());
    env.insert(
        "XDG_CONFIG_HOME".to_string(),
        xdg_config_home.to_string_lossy().to_string(),
    );
    env.insert(
        "XDG_DATA_HOME".to_string(),
        xdg_data_home.to_string_lossy().to_string(),
    );

    let touch = profile_dir.join(".last_used");
    fs::write(&touch, now_ts().to_string()).map_err(|e| e.to_string())?;
    set_file_mode_600(&touch)?;

    Ok(env)
}
