use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use crate::crypto;

#[derive(Serialize, Deserialize, Default)]
pub struct Secrets {
    #[serde(default)]
    pub is_encrypted: bool,
    pub values: HashMap<String, String>,
}

fn get_secrets_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data").join("secrets");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.enc.json"))
}

fn get_legacy_secrets_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    Ok(data_dir.join("secrets.json"))
}

fn load_secrets() -> Result<Secrets, String> {
    let new_path = get_secrets_path()?;
    let legacy_path = get_legacy_secrets_path()?;
    let target = if new_path.exists() { new_path } else { legacy_path };
    if !target.exists() {
        return Ok(Secrets::default());
    }
    let content = fs::read_to_string(target).map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

fn write_secrets(secrets: &Secrets) -> Result<(), String> {
    let path = get_secrets_path()?;
    let json = serde_json::to_string_pretty(secrets).map_err(|e| e.to_string())?;
    let mut file = File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;

    let legacy_path = get_legacy_secrets_path()?;
    if legacy_path.exists() {
        let _ = fs::remove_file(legacy_path);
    }
    Ok(())
}

#[tauri::command]
pub fn get_secret(key: &str) -> Result<Option<String>, String> {
    let secrets = load_secrets()?;

    if let Some(val) = secrets.values.get(key) {
        if secrets.is_encrypted {
            let password = crypto::get_or_init_master_password()?;
            if let Ok(decrypted) = crypto::decrypt(val, &password) {
                return Ok(Some(decrypted));
            }
        }
        return Ok(Some(val.clone()));
    }

    Ok(None)
}

#[tauri::command]
pub async fn save_secret(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    let mut secrets = load_secrets()?;

    // Ensure all existing secrets are decrypted if we're going to re-encrypt
    let password = crypto::get_or_init_master_password()?;
    if secrets.is_encrypted {
        for val in secrets.values.values_mut() {
            if let Ok(decrypted) = crypto::decrypt(val, &password) {
                *val = decrypted;
            }
        }
    }

    // Add/Update secret
    secrets.values.insert(key, value);

    // Encrypt all
    let mut encrypted_secrets = Secrets {
        is_encrypted: true,
        values: HashMap::new(),
    };

    for (k, v) in secrets.values {
        encrypted_secrets.values.insert(k, crypto::encrypt(&v, &password)?);
    }

    write_secrets(&encrypted_secrets)?;

    // Auto sync
    let _ = crate::git::sync_git(app).await;

    Ok(())
}

#[tauri::command]
pub async fn delete_secret(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let mut secrets = load_secrets()?;

    if secrets.values.remove(&key).is_some() {
        write_secrets(&secrets)?;
        
        let _ = crate::git::sync_git(app).await;
    }

    Ok(())
}
