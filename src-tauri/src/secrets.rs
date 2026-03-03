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
    Ok(data_dir.join("secrets.json"))
}

#[tauri::command]
pub fn get_secret(key: &str) -> Result<Option<String>, String> {
    let path = get_secrets_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let secrets: Secrets = serde_json::from_str(&content).unwrap_or_default();

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
    let path = get_secrets_path()?;
    let mut secrets = if path.exists() {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Secrets::default()
    };

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

    let json = serde_json::to_string_pretty(&encrypted_secrets).map_err(|e| e.to_string())?;
    let mut file = File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;

    // Auto sync
    let _ = crate::git::sync_git(app).await;

    Ok(())
}

#[tauri::command]
pub async fn delete_secret(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let path = get_secrets_path()?;
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut secrets: Secrets = serde_json::from_str(&content).unwrap_or_default();

    if secrets.values.remove(&key).is_some() {
        let json = serde_json::to_string_pretty(&secrets).map_err(|e| e.to_string())?;
        let mut file = File::create(&path).map_err(|e| e.to_string())?;
        file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        
        let _ = crate::git::sync_git(app).await;
    }

    Ok(())
}
