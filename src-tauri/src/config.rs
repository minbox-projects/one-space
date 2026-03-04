use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProxyConfig {
    pub proxy_enabled: bool,
    pub proxy_type: String,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub proxy_username: Option<String>,
    pub proxy_password: Option<String>,
    pub check_interval: u64,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            proxy_enabled: false,
            proxy_type: "socks5".to_string(),
            proxy_host: String::new(),
            proxy_port: 1080,
            proxy_username: None,
            proxy_password: None,
            check_interval: 15,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StorageConfig {
    pub storage_type: String, // "local" or "git"
    pub git_url: Option<String>,
    pub auth_method: Option<String>, // "http" or "ssh"
    pub http_username: Option<String>,
    pub http_token: Option<String>,
    pub ssh_key_path: Option<String>,

    pub main_shortcut: Option<String>,
    pub quick_ai_shortcut: Option<String>,
    pub default_ai_dir: Option<String>,
    pub default_ai_model: Option<String>,
    pub language: Option<String>,
    
    pub local_storage_path: Option<String>,
    pub icloud_storage_path: Option<String>,
    
    pub proxy: Option<ProxyConfig>,
    
    #[serde(default)]
    pub is_encrypted: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let storage_type = "icloud".to_string();
        #[cfg(not(target_os = "macos"))]
        let storage_type = "local".to_string();

        Self {
            storage_type,
            git_url: None,
            auth_method: Some("http".to_string()),
            http_username: None,
            http_token: None,
            ssh_key_path: None,
            main_shortcut: Some("Alt+Space".to_string()),
            quick_ai_shortcut: Some("Alt+Shift+A".to_string()),
            default_ai_dir: None,
            default_ai_model: Some("claude".to_string()),
            language: Some("zh".to_string()),
            local_storage_path: None,
            icloud_storage_path: None,
            proxy: Some(ProxyConfig::default()),
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
    let password = crate::crypto::get_or_init_master_password()?;
    
    if config.is_encrypted {
        if let Some(token) = &config.http_token {
            if !token.is_empty() {
                if let Ok(decrypted) = crate::crypto::decrypt(token, &password) {
                    config.http_token = Some(decrypted);
                }
            }
        }
    }

    // Mask proxy password
    if let Some(ref mut proxy) = config.proxy {
        if let Some(ref pass) = proxy.proxy_password {
            if !pass.is_empty() {
                proxy.proxy_password = Some("********".to_string());
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
    if old_config.storage_type != config.storage_type || 
       (config.storage_type == "local" && old_config.local_storage_path != config.local_storage_path) ||
       (config.storage_type == "icloud" && old_config.icloud_storage_path != config.icloud_storage_path) {
        
        let old_local_path = if let Some(p) = &old_config.local_storage_path {
            PathBuf::from(p)
        } else {
            dirs::home_dir().ok_or("Home dir not found")?.join(".config").join("onespace").join("data")
        };
        
        let new_local_path = if let Some(p) = &config.local_storage_path {
            PathBuf::from(p)
        } else {
            dirs::home_dir().ok_or("Home dir not found")?.join(".config").join("onespace").join("data")
        };

        let git_data_dir = app_dir.join("git_data");

        #[cfg(target_os = "macos")]
        let old_icloud_path = if let Some(p) = &old_config.icloud_storage_path {
            PathBuf::from(p)
        } else {
            dirs::home_dir().ok_or("Home dir not found")?.join("Library/Mobile Documents/com~apple~CloudDocs/onespace")
        };
        #[cfg(not(target_os = "macos"))]
        let old_icloud_path = dirs::home_dir().ok_or("Home dir not found")?.join(".config").join("onespace").join("data");

        #[cfg(target_os = "macos")]
        let new_icloud_path = if let Some(p) = &config.icloud_storage_path {
            PathBuf::from(p)
        } else {
            dirs::home_dir().ok_or("Home dir not found")?.join("Library/Mobile Documents/com~apple~CloudDocs/onespace")
        };
        #[cfg(not(target_os = "macos"))]
        let new_icloud_path = dirs::home_dir().ok_or("Home dir not found")?.join(".config").join("onespace").join("data");

        let get_dir_for_type = |st: &str, local_p: &PathBuf, icloud_p: &PathBuf| -> PathBuf {
            match st {
                "git" => git_data_dir.clone(),
                "icloud" => icloud_p.clone(),
                _ => local_p.clone(),
            }
        };

        let src = get_dir_for_type(&old_config.storage_type, &old_local_path, &old_icloud_path);
        let dst = get_dir_for_type(&config.storage_type, &new_local_path, &new_icloud_path);

        if src.exists() && src != dst {
            if !dst.exists() {
                fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
            }
            
            // Files that might be in the app root (old location)
            let root_files = ["ai_providers.json"];
            let app_root = app_dir.clone();
            
            for file_name in root_files {
                let old_file = app_root.join(file_name);
                if old_file.exists() {
                    let dest_file = dst.join(file_name);
                    if !dest_file.exists() {
                        fs::copy(&old_file, &dest_file).map_err(|e| e.to_string())?;
                    }
                }
            }

            // Copy files if destination doesn't have them (new location)
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

    let master_pass = crate::crypto::get_or_init_master_password()?;

    // Handle proxy password
    if let Some(ref mut proxy) = config.proxy {
        if let Some(pass) = &proxy.proxy_password {
            if pass == "********" {
                // Keep old encrypted password
                proxy.proxy_password = old_config.proxy.as_ref().and_then(|p| p.proxy_password.clone());
            } else if pass.is_empty() {
                proxy.proxy_password = None;
            } else {
                // New password, encrypt it
                proxy.proxy_password = Some(crate::crypto::encrypt(pass, &master_pass)?);
            }
        }
        
        // Update ProxyManager
        if let Some(mgr) = crate::proxy::PROXY_MANAGER.get() {
            mgr.update_config(proxy.clone())?;
        }
    }

    // Encrypt http_token before saving
    if let Some(token) = &config.http_token {
        if !token.is_empty() {
            config.http_token = Some(crate::crypto::encrypt(token, &master_pass)?);
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
