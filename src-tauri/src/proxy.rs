use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub use crate::config::ProxyConfig;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProxyStatus {
    pub is_available: bool,
    pub latency_ms: u32,
    pub message: String,
    pub proxy_type: String,
    pub proxy_host: String,
}

pub struct ProxyManager {
    config: RwLock<Option<ProxyConfig>>,
    client: RwLock<Option<Client>>,
}

impl ProxyManager {
    pub fn new() -> Self {
        Self {
            config: RwLock::new(None),
            client: RwLock::new(Some(Client::new())),
        }
    }

    pub fn update_config(&self, config: ProxyConfig) -> Result<(), String> {
        let mut config_guard = self.config.write().map_err(|_| "Lock error")?;
        *config_guard = Some(config);
        drop(config_guard);

        self.rebuild_client()
    }

    fn rebuild_client(&self) -> Result<(), String> {
        let config_guard = self.config.read().map_err(|_| "Lock error")?;
        let mut client_guard = self.client.write().map_err(|_| "Lock error")?;

        if let Some(cfg) = config_guard.as_ref() {
            if cfg.proxy_enabled {
                *client_guard = Some(create_proxy_client(cfg)?);
            } else {
                *client_guard = Some(Client::new());
            }
        } else {
            *client_guard = Some(Client::new());
        }
        Ok(())
    }

    pub fn get_client(&self) -> Result<Client, String> {
        let guard = self.client.read().map_err(|_| "Lock error")?;
        guard.clone().ok_or("Client not initialized".to_string())
    }

    pub async fn test_proxy(&self) -> Result<ProxyStatus, String> {
        let config_opt = self.config.read()
            .map_err(|_| "Lock error")?
            .clone();
        
        let cfg = config_opt.ok_or("No proxy config")?;

        if !cfg.proxy_enabled {
            return Ok(ProxyStatus {
                is_available: false,
                latency_ms: 0,
                message: "Proxy is disabled".into(),
                proxy_type: cfg.proxy_type.clone(),
                proxy_host: cfg.proxy_host.clone(),
            });
        }

        let client = create_proxy_client(&cfg)?;
        let start = std::time::Instant::now();

        let test_result = client
            .get("https://www.gstatic.com/generate_204")
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        let elapsed = start.elapsed().as_millis() as u32;

        match test_result {
            Ok(_) => Ok(ProxyStatus {
                is_available: true,
                latency_ms: elapsed,
                message: "Proxy is reachable".into(),
                proxy_type: cfg.proxy_type.clone(),
                proxy_host: cfg.proxy_host.clone(),
            }),
            Err(e) => Ok(ProxyStatus {
                is_available: false,
                latency_ms: elapsed,
                message: format!("Proxy unreachable: {}", e),
                proxy_type: cfg.proxy_type.clone(),
                proxy_host: cfg.proxy_host.clone(),
            }),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config
            .read()
            .ok()
            .and_then(|g| g.as_ref().map(|c| c.proxy_enabled))
            .unwrap_or(false)
    }

    #[allow(dead_code)]
    pub fn get_config(&self) -> Option<ProxyConfig> {
        self.config.read().ok().and_then(|g| g.clone())
    }
}

fn create_proxy_client(config: &ProxyConfig) -> Result<Client, String> {
    if config.proxy_host.trim().is_empty() {
        return Err("Proxy host cannot be empty".to_string());
    }
    if config.proxy_port == 0 {
        return Err("Proxy port must be greater than 0".to_string());
    }

    let mut builder = Client::builder();

    let proxy_url = build_proxy_url(config)?;

    let proxy = reqwest::Proxy::all(&proxy_url)
        .map_err(|e| format!("Failed to create proxy: {}", e))?;

    if let (Some(user), Some(pass)) = (&config.proxy_username, &config.proxy_password) {
        if !user.is_empty() && !pass.is_empty() {
            let auth_pass = resolve_proxy_password(pass)?;
            builder = builder.proxy(proxy.basic_auth(user, &auth_pass));
        } else {
            builder = builder.proxy(proxy);
        }
    } else {
        builder = builder.proxy(proxy);
    }

    builder = builder
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10));

    builder.build().map_err(|e| e.to_string())
}

fn build_proxy_url(config: &ProxyConfig) -> Result<String, String> {
    let proxy_url = match config.proxy_type.as_str() {
        "http" | "https" => {
            format!("{}://{}:{}", config.proxy_type, config.proxy_host, config.proxy_port)
        }
        "socks5" => format!("socks5://{}:{}", config.proxy_host, config.proxy_port),
        _ => {
            return Err(format!("Unsupported proxy type: {}", config.proxy_type));
        }
    };
    Ok(proxy_url)
}

fn resolve_proxy_password(pass: &str) -> Result<String, String> {
    let mut auth_pass = pass.to_string();
    if pass != "********" {
        let master_pass = crate::crypto::get_or_init_master_password()?;
        if let Ok(decrypted) = crate::crypto::decrypt(pass, &master_pass) {
            auth_pass = decrypted;
        }
    }
    Ok(auth_pass)
}

fn proxy_url_with_auth(config: &ProxyConfig) -> Result<String, String> {
    let proxy_url = build_proxy_url(config)?;
    let mut url = reqwest::Url::parse(&proxy_url).map_err(|e| e.to_string())?;
    if let (Some(user), Some(pass)) = (&config.proxy_username, &config.proxy_password) {
        if !user.is_empty() && !pass.is_empty() {
            let auth_pass = resolve_proxy_password(pass)?;
            url.set_username(user)
                .map_err(|_| "Invalid proxy username".to_string())?;
            url.set_password(Some(&auth_pass))
                .map_err(|_| "Invalid proxy password".to_string())?;
        }
    }
    Ok(url.to_string())
}

fn append_no_proxy_defaults(existing: Option<String>) -> String {
    let mut values: Vec<String> = existing
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for local in ["localhost", "127.0.0.1", "::1"] {
        if !values.iter().any(|v| v == local) {
            values.push(local.to_string());
        }
    }
    values.join(",")
}

pub fn apply_process_proxy_env(config: Option<&ProxyConfig>) -> Result<(), String> {
    let keys = [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ];

    if let Some(cfg) = config {
        if cfg.proxy_enabled {
            let proxy_url = proxy_url_with_auth(cfg)?;
            for key in keys {
                std::env::set_var(key, &proxy_url);
            }
            let merged_no_proxy = append_no_proxy_defaults(std::env::var("NO_PROXY").ok());
            std::env::set_var("NO_PROXY", &merged_no_proxy);
            std::env::set_var("no_proxy", merged_no_proxy);
            return Ok(());
        }
    }

    for key in keys {
        std::env::remove_var(key);
    }
    let merged_no_proxy = append_no_proxy_defaults(std::env::var("NO_PROXY").ok());
    std::env::set_var("NO_PROXY", &merged_no_proxy);
    std::env::set_var("no_proxy", merged_no_proxy);
    Ok(())
}

use std::sync::OnceLock;
pub static PROXY_MANAGER: OnceLock<Arc<ProxyManager>> = OnceLock::new();

pub fn init_proxy_manager() -> Arc<ProxyManager> {
    let mgr = PROXY_MANAGER
        .get_or_init(|| Arc::new(ProxyManager::new()))
        .clone();

    if let Ok(cfg) = crate::config::get_config() {
        if let Some(proxy_cfg) = cfg.proxy {
            let _ = mgr.update_config(proxy_cfg.clone());
            let _ = apply_process_proxy_env(Some(&proxy_cfg));
        } else {
            let _ = apply_process_proxy_env(None);
        }
    }

    mgr
}

#[tauri::command]
pub async fn get_proxy_config() -> Result<Option<ProxyConfig>, String> {
    let cfg = crate::config::get_storage_config()?;
    Ok(cfg.proxy)
}

#[tauri::command]
pub async fn save_proxy_config(
    app: tauri::AppHandle,
    proxy: ProxyConfig,
) -> Result<(), String> {
    let mut cfg = crate::config::get_config()?;
    cfg.proxy = Some(proxy);
    crate::config::save_storage_config(app, cfg).await
}

#[tauri::command]
pub async fn test_proxy_connection(config: Option<ProxyConfig>) -> Result<ProxyStatus, String> {
    let mgr = PROXY_MANAGER.get().ok_or("Proxy manager not initialized")?;
    
    // If config is provided, use it directly (for testing before saving)
    if let Some(mut cfg) = config {
        // If password is masked, get the real one from current config
        if let Some(pass) = &cfg.proxy_password {
            if pass == "********" {
                let current = crate::config::get_config()?;
                if let Some(p) = current.proxy {
                    cfg.proxy_password = p.proxy_password;
                }
            }
        }

        let temp_mgr = ProxyManager::new();
        temp_mgr.update_config(cfg)?;
        return temp_mgr.test_proxy().await;
    }
    
    // Otherwise, use the current manager config
    mgr.test_proxy().await
}
