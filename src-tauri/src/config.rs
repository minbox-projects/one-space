use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StorageConfig {
    pub storage_type: String, // "local" or "git"
    pub git_url: Option<String>,
    pub auth_method: Option<String>, // "http" or "ssh"
    pub http_username: Option<String>,
    pub http_token: Option<String>,
    pub ssh_key_path: Option<String>,

    // 新增：快捷键与路径配置
    pub main_shortcut: Option<String>,     // 默认 ALT+Space
    pub quick_ai_shortcut: Option<String>, // 默认 ALT+Shift+A
    pub default_ai_dir: Option<String>,
    pub language: Option<String>, // "en" or "zh"
    
    #[serde(default)]
    pub is_encrypted: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_type: "local".to_string(),
            git_url: None,
            auth_method: Some("http".to_string()),
            http_username: None,
            http_token: None,
            ssh_key_path: None,
            main_shortcut: Some("Alt+Space".to_string()),
            quick_ai_shortcut: Some("Alt+Shift+A".to_string()),
            default_ai_dir: None,
            language: Some("zh".to_string()),
            is_encrypted: false,
        }
    }
}

pub fn get_app_dir() -> Result<PathBuf, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let app_dir = home_dir.join(".config").join("onespace");
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    }
    Ok(app_dir)
}

pub fn get_config() -> Result<StorageConfig, String> {
    let app_dir = get_app_dir()?;
    let config_path = app_dir.join("config.json");

    if !config_path.exists() {
        return Ok(StorageConfig::default());
    }

    let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let config: StorageConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config)
}

#[tauri::command]
pub fn get_storage_config() -> Result<StorageConfig, String> {
    let mut config = get_config()?;
    if config.is_encrypted {
        if let Some(token) = &config.http_token {
            if !token.is_empty() {
                let password = crate::crypto::get_or_init_master_password()?;
                if let Ok(decrypted) = crate::crypto::decrypt(token, &password) {
                    config.http_token = Some(decrypted);
                }
            }
        }
    }
    Ok(config)
}

#[tauri::command]
pub async fn save_storage_config(app: tauri::AppHandle, mut config: StorageConfig) -> Result<(), String> {
    let old_config = get_config()?;
    let app_dir = get_app_dir()?;
    let config_path = app_dir.join("config.json");

    // Check if storage type changed to migrate data
    if old_config.storage_type != config.storage_type {
        let hostname = crate::get_hostname();
        let local_data_dir = dirs::home_dir().ok_or("Home dir not found")?.join(".config").join("onespace").join("data");
        let git_data_dir = app_dir.join("git_data").join(&hostname);

        let (src, dst) = if config.storage_type == "git" {
            (local_data_dir, git_data_dir)
        } else {
            (git_data_dir, local_data_dir)
        };

        if src.exists() {
            if !dst.exists() {
                fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
            }
            // Copy files if destination doesn't have them
            if let Ok(entries) = fs::read_dir(&src) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let dest_file = dst.join(path.file_name().unwrap());
                        if !dest_file.exists() {
                            fs::copy(&path, &dest_file).map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
        }
    }

    // Encrypt http_token before saving
    if let Some(token) = &config.http_token {
        if !token.is_empty() {
            let password = crate::crypto::get_or_init_master_password()?;
            config.http_token = Some(crate::crypto::encrypt(token, &password)?);
            config.is_encrypted = true;
        }
    }

    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| e.to_string())?;

    if config.storage_type == "git" {
        // Run git sync
        let _ = crate::git::sync_git(app).await;
    }

    Ok(())
}
