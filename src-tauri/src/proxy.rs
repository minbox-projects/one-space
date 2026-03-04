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
            client: RwLock::new(None),
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
            .get("http://example.com")
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

    pub fn get_config(&self) -> Option<ProxyConfig> {
        self.config.read().ok().and_then(|g| g.clone())
    }
}

fn create_proxy_client(config: &ProxyConfig) -> Result<Client, String> {
    let mut builder = Client::builder();

    let proxy_url = match config.proxy_type.as_str() {
        "http" | "https" => {
            format!("{}://{}:{}", config.proxy_type, config.proxy_host, config.proxy_port)
        }
        "socks5" => format!("socks5://{}:{}", config.proxy_host, config.proxy_port),
        _ => {
            return Err(format!("Unsupported proxy type: {}", config.proxy_type));
        }
    };

    let proxy = reqwest::Proxy::all(&proxy_url)
        .map_err(|e| format!("Failed to create proxy: {}", e))?;

    if let (Some(user), Some(pass)) = (&config.proxy_username, &config.proxy_password) {
        if !user.is_empty() && !pass.is_empty() {
            builder = builder.proxy(proxy.basic_auth(user, pass));
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

use std::sync::OnceLock;
pub static PROXY_MANAGER: OnceLock<Arc<ProxyManager>> = OnceLock::new();

pub fn init_proxy_manager() -> Arc<ProxyManager> {
    PROXY_MANAGER
        .get_or_init(|| Arc::new(ProxyManager::new()))
        .clone()
}

#[tauri::command]
pub async fn get_proxy_config() -> Result<Option<ProxyConfig>, String> {
    let cfg = crate::config::get_config()?;
    Ok(cfg.proxy)
}

#[tauri::command]
pub async fn save_proxy_config(
    app: tauri::AppHandle,
    mut proxy: ProxyConfig,
) -> Result<(), String> {
    let mut cfg = crate::config::get_config()?;

    // Only encrypt if password is provided and not empty
    if let Some(pass) = &proxy.proxy_password {
        if !pass.is_empty() {
            let password = crate::crypto::get_or_init_master_password()?;
            proxy.proxy_password = Some(crate::crypto::encrypt(pass, &password)?);
        } else {
            // Keep existing password if new one is empty
            if let Some(existing) = &cfg.proxy {
                proxy.proxy_password = existing.proxy_password.clone();
            }
        }
    }

    cfg.proxy = Some(proxy.clone());
    crate::config::save_storage_config(app.clone(), cfg).await?;

    if let Some(mgr) = PROXY_MANAGER.get() {
        mgr.update_config(proxy)?;
    }

    Ok(())
}

#[tauri::command]
pub async fn test_proxy_connection(config: Option<ProxyConfig>) -> Result<ProxyStatus, String> {
    let mgr = PROXY_MANAGER.get().ok_or("Proxy manager not initialized")?;
    
    // If config is provided, use it directly (for testing before saving)
    if let Some(cfg) = config {
        let temp_mgr = ProxyManager::new();
        temp_mgr.update_config(cfg)?;
        return temp_mgr.test_proxy().await;
    }
    
    // Otherwise, use the current manager config
    mgr.test_proxy().await
}
