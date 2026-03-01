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
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_type: "local".to_string(),
            git_url: None,
            auth_method: None,
            http_username: None,
            http_token: None,
            ssh_key_path: None,
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
    get_config()
}

#[tauri::command]
pub fn save_storage_config(config: StorageConfig) -> Result<(), String> {
    let app_dir = get_app_dir()?;
    let config_path = app_dir.join("config.json");

    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| e.to_string())?;

    if config.storage_type == "git" {
        super::git::init_or_pull_git_repo(&config)?;
    }

    Ok(())
}
