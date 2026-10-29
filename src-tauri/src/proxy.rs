use reqwest::{Client, ClientBuilder, NoProxy};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::{Arc, OnceLock, RwLock, RwLockReadGuard};
use std::time::{Duration, Instant};

pub use crate::config::ProxyConfig;

pub const PROXY_CONFIG_UPDATED_EVENT: &str = "proxy-config-updated";

const LOOPBACK_NO_PROXY: &str = "localhost,127.0.0.1,::1";
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const PROXY_ENV_KEYS: [&str; 6] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProxyStatus {
    pub is_available: bool,
    pub latency_ms: u32,
    pub message: String,
    pub proxy_type: String,
    pub proxy_host: String,
}

#[derive(Clone)]
pub struct ProxySnapshot {
    pub generation: u64,
    pub config: Option<ProxyConfig>,
}

impl ProxySnapshot {
    pub fn is_enabled(&self) -> bool {
        self.config
            .as_ref()
            .map(|config| config.proxy_enabled)
            .unwrap_or(false)
    }
}

struct MaskedConfig<'a>(&'a ProxyConfig);

impl fmt::Debug for MaskedConfig<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let config = self.0;
        formatter
            .debug_struct("ProxyConfig")
            .field("proxy_enabled", &config.proxy_enabled)
            .field("proxy_type", &config.proxy_type)
            .field("proxy_host", &config.proxy_host)
            .field("proxy_port", &config.proxy_port)
            .field(
                "proxy_username",
                &config.proxy_username.as_ref().map(|_| "********"),
            )
            .field(
                "proxy_password",
                &config.proxy_password.as_ref().map(|_| "********"),
            )
            .field("check_interval", &config.check_interval)
            .finish()
    }
}

impl fmt::Debug for ProxySnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProxySnapshot")
            .field("generation", &self.generation)
            .field("config", &self.config.as_ref().map(MaskedConfig))
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
enum ProxyIdentity {
    Direct,
    Proxied {
        proxy_type: String,
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
    },
}

pub struct PreparedProxy {
    snapshot: ProxySnapshot,
    identity: ProxyIdentity,
    client: Client,
}

struct ProxyState {
    generation: u64,
    snapshot: Arc<ProxySnapshot>,
    identity: ProxyIdentity,
    client: Option<Client>,
}

pub struct ProxyManager {
    state: RwLock<ProxyState>,
}

impl ProxyManager {
    pub fn new() -> Self {
        let snapshot = Arc::new(ProxySnapshot {
            generation: 0,
            config: None,
        });
        let client = apply_proxy_policy(default_client_builder(), &snapshot)
            .ok()
            .and_then(|builder| builder.build().ok());
        Self {
            state: RwLock::new(ProxyState {
                generation: 0,
                snapshot,
                identity: ProxyIdentity::Direct,
                client,
            }),
        }
    }

    pub fn generation(&self) -> u64 {
        self.read_state().generation
    }

    pub fn current_snapshot(&self) -> Arc<ProxySnapshot> {
        self.read_state().snapshot.clone()
    }

    pub fn prepare(&self, config: Option<ProxyConfig>) -> Result<PreparedProxy, String> {
        let identity = effective_identity(config.as_ref())?;
        let snapshot = ProxySnapshot {
            generation: 0,
            config,
        };
        let client = apply_proxy_policy(default_client_builder(), &snapshot)?
            .build()
            .map_err(|_| "Failed to prepare proxy transport".to_string())?;
        Ok(PreparedProxy {
            snapshot,
            identity,
            client,
        })
    }

    pub fn publish(&self, prepared: PreparedProxy) -> u64 {
        let PreparedProxy {
            snapshot,
            identity,
            client,
        } = prepared;
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|error| error.into_inner());

        if state.identity == identity {
            let generation = state.generation;
            state.snapshot = Arc::new(ProxySnapshot {
                generation,
                config: snapshot.config,
            });
            if state.client.is_none() {
                state.client = Some(client);
            }
            return generation;
        }

        let generation = state.generation.saturating_add(1);
        state.generation = generation;
        state.identity = identity;
        state.snapshot = Arc::new(ProxySnapshot {
            generation,
            config: snapshot.config,
        });
        state.client = Some(client);
        generation
    }

    pub fn get_client(&self) -> Result<Client, String> {
        self.read_state()
            .client
            .clone()
            .ok_or_else(|| "Client not initialized".to_string())
    }

    pub fn apply_builder(&self, builder: ClientBuilder) -> Result<ClientBuilder, String> {
        apply_proxy_policy(builder, &self.current_snapshot())
    }

    pub fn is_enabled(&self) -> bool {
        self.current_snapshot().is_enabled()
    }

    pub async fn test_proxy(&self) -> Result<ProxyStatus, String> {
        let snapshot = self.current_snapshot();
        let config = snapshot
            .config
            .clone()
            .ok_or_else(|| "No proxy config".to_string())?;

        if !config.proxy_enabled {
            return Ok(disabled_status(&config));
        }

        let client = self.get_client()?;
        probe_proxy(&config, &client).await
    }

    pub async fn test_draft(config: Option<ProxyConfig>) -> Result<ProxyStatus, String> {
        let snapshot = ProxySnapshot {
            generation: 0,
            config,
        };
        let config = snapshot
            .config
            .clone()
            .ok_or_else(|| "No proxy config".to_string())?;
        let client = apply_proxy_policy(Client::builder(), &snapshot)?
            .build()
            .map_err(|_| "Failed to prepare proxy transport".to_string())?;

        if !config.proxy_enabled {
            return Ok(disabled_status(&config));
        }

        probe_proxy(&config, &client).await
    }

    fn read_state(&self) -> RwLockReadGuard<'_, ProxyState> {
        self.state
            .read()
            .unwrap_or_else(|error| error.into_inner())
    }
}

pub fn apply_proxy_policy(
    mut builder: ClientBuilder,
    snapshot: &ProxySnapshot,
) -> Result<ClientBuilder, String> {
    builder = builder.no_proxy();

    if !snapshot.is_enabled() {
        return Ok(builder);
    }

    let config = snapshot
        .config
        .as_ref()
        .ok_or_else(|| "No proxy config".to_string())?;
    let proxy_url = proxy_url(config)?;
    let mut proxy = reqwest::Proxy::all(&proxy_url)
        .map_err(|_| "Failed to create proxy".to_string())?
        .no_proxy(NoProxy::from_string(LOOPBACK_NO_PROXY));

    if let Some((username, password)) = effective_auth(config) {
        proxy = proxy.basic_auth(&username, &password);
    }

    Ok(builder.proxy(proxy))
}

fn default_client_builder() -> ClientBuilder {
    Client::builder()
        .timeout(DEFAULT_REQUEST_TIMEOUT)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
}

pub fn apply_current_proxy_to_builder(builder: ClientBuilder) -> Result<ClientBuilder, String> {
    apply_proxy_policy(builder, &current_proxy_snapshot())
}

fn current_proxy_snapshot() -> Arc<ProxySnapshot> {
    PROXY_MANAGER
        .get()
        .map(|manager| manager.current_snapshot())
        .unwrap_or_else(|| {
            Arc::new(ProxySnapshot {
                generation: 0,
                config: None,
            })
        })
}

pub fn apply_current_proxy_env_to_command(command: &mut std::process::Command) {
    apply_proxy_env_to_command(command, &current_proxy_snapshot());
}

pub fn proxy_env_vars(snapshot: &ProxySnapshot) -> Vec<(String, String)> {
    proxy_env_vars_checked(snapshot).unwrap_or_else(|_| {
        vec![
            ("NO_PROXY".to_string(), LOOPBACK_NO_PROXY.to_string()),
            ("no_proxy".to_string(), LOOPBACK_NO_PROXY.to_string()),
        ]
    })
}

pub fn apply_proxy_env_to_command(command: &mut std::process::Command, snapshot: &ProxySnapshot) {
    for key in PROXY_ENV_KEYS {
        command.env_remove(key);
    }
    for (key, value) in proxy_env_vars(snapshot) {
        command.env(key, value);
    }
}

pub fn apply_process_proxy_env(config: Option<&ProxyConfig>) -> Result<(), String> {
    let snapshot = ProxySnapshot {
        generation: 0,
        config: config.cloned(),
    };
    let variables = proxy_env_vars_checked(&snapshot)?;

    for key in PROXY_ENV_KEYS {
        std::env::remove_var(key);
    }
    std::env::remove_var("NO_PROXY");
    std::env::remove_var("no_proxy");
    for (key, value) in variables {
        std::env::set_var(key, value);
    }
    Ok(())
}

fn proxy_env_vars_checked(snapshot: &ProxySnapshot) -> Result<Vec<(String, String)>, String> {
    let mut variables = Vec::with_capacity(8);

    if snapshot.is_enabled() {
        let config = snapshot
            .config
            .as_ref()
            .ok_or_else(|| "No proxy config".to_string())?;
        let proxy_url = proxy_url_with_auth(config)?;
        for key in PROXY_ENV_KEYS {
            variables.push((key.to_string(), proxy_url.clone()));
        }
    }

    variables.push(("NO_PROXY".to_string(), LOOPBACK_NO_PROXY.to_string()));
    variables.push(("no_proxy".to_string(), LOOPBACK_NO_PROXY.to_string()));
    Ok(variables)
}

fn effective_identity(config: Option<&ProxyConfig>) -> Result<ProxyIdentity, String> {
    let Some(config) = config else {
        return Ok(ProxyIdentity::Direct);
    };
    if !config.proxy_enabled {
        return Ok(ProxyIdentity::Direct);
    }

    let proxy_type = normalized_proxy_type(config)?;
    let host = config.proxy_host.trim();
    if host.is_empty() {
        return Err("Proxy host cannot be empty".to_string());
    }
    if config.proxy_port == 0 {
        return Err("Proxy port must be greater than 0".to_string());
    }

    let auth = effective_auth(config);
    Ok(ProxyIdentity::Proxied {
        proxy_type,
        host: host.to_string(),
        port: config.proxy_port,
        username: auth.as_ref().map(|(username, _)| username.clone()),
        password: auth.map(|(_, password)| password),
    })
}

fn normalized_proxy_type(config: &ProxyConfig) -> Result<String, String> {
    let proxy_type = config.proxy_type.trim().to_ascii_lowercase();
    match proxy_type.as_str() {
        "http" | "https" | "socks5" => Ok(proxy_type),
        _ => Err(format!("Unsupported proxy type: {}", config.proxy_type)),
    }
}

fn proxy_url(config: &ProxyConfig) -> Result<String, String> {
    let proxy_type = normalized_proxy_type(config)?;
    let host = config.proxy_host.trim();
    if host.is_empty() {
        return Err("Proxy host cannot be empty".to_string());
    }
    if config.proxy_port == 0 {
        return Err("Proxy port must be greater than 0".to_string());
    }

    Ok(format!("{}://{}:{}", proxy_type, host, config.proxy_port))
}

fn proxy_url_with_auth(config: &ProxyConfig) -> Result<String, String> {
    let mut url = reqwest::Url::parse(&proxy_url(config)?)
        .map_err(|_| "Invalid proxy configuration".to_string())?;

    if let Some((username, password)) = effective_auth(config) {
        url.set_username(&username)
            .map_err(|_| "Invalid proxy username".to_string())?;
        url.set_password(Some(&password))
            .map_err(|_| "Invalid proxy password".to_string())?;
    }

    Ok(url.to_string())
}

fn effective_auth(config: &ProxyConfig) -> Option<(String, String)> {
    let username = config.proxy_username.as_deref().unwrap_or("");
    let password = config.proxy_password.as_deref().unwrap_or("");
    if username.is_empty() || password.is_empty() {
        return None;
    }

    Some((
        username.to_string(),
        resolve_proxy_password(password),
    ))
}

fn resolve_proxy_password(password: &str) -> String {
    if password == "********" {
        return password.to_string();
    }

    let Ok(master_password) = crate::crypto::get_or_init_master_password() else {
        return password.to_string();
    };

    crate::crypto::decrypt(password, &master_password).unwrap_or_else(|_| password.to_string())
}

fn disabled_status(config: &ProxyConfig) -> ProxyStatus {
    ProxyStatus {
        is_available: false,
        latency_ms: 0,
        message: "Proxy is disabled".to_string(),
        proxy_type: config.proxy_type.clone(),
        proxy_host: config.proxy_host.clone(),
    }
}

async fn probe_proxy(config: &ProxyConfig, client: &Client) -> Result<ProxyStatus, String> {
    let start = Instant::now();
    let result = client
        .get("https://www.gstatic.com/generate_204")
        .timeout(Duration::from_secs(5))
        .send()
        .await;
    let elapsed = start.elapsed().as_millis() as u32;

    match result {
        Ok(_) => Ok(ProxyStatus {
            is_available: true,
            latency_ms: elapsed,
            message: "Proxy is reachable".to_string(),
            proxy_type: config.proxy_type.clone(),
            proxy_host: config.proxy_host.clone(),
        }),
        Err(error) => Ok(ProxyStatus {
            is_available: false,
            latency_ms: elapsed,
            message: format!("Proxy unreachable: {}", error),
            proxy_type: config.proxy_type.clone(),
            proxy_host: config.proxy_host.clone(),
        }),
    }
}

pub static PROXY_MANAGER: OnceLock<Arc<ProxyManager>> = OnceLock::new();

pub fn init_proxy_manager() -> Arc<ProxyManager> {
    let manager = PROXY_MANAGER
        .get_or_init(|| Arc::new(ProxyManager::new()))
        .clone();

    let saved_config = crate::config::get_config()
        .ok()
        .and_then(|config| config.proxy);
    let effective_config = match manager.prepare(saved_config.clone()) {
        Ok(prepared) => {
            manager.publish(prepared);
            saved_config
        }
        Err(_) => {
            if let Ok(prepared) = manager.prepare(None) {
                manager.publish(prepared);
            }
            None
        }
    };

    let _ = apply_process_proxy_env(effective_config.as_ref());
    manager
}

#[tauri::command]
pub fn proxy_generation() -> u64 {
    PROXY_MANAGER
        .get()
        .map(|manager| manager.generation())
        .unwrap_or(0)
}

#[tauri::command]
pub async fn test_proxy_connection(config: Option<ProxyConfig>) -> Result<ProxyStatus, String> {
    if let Some(mut config) = config {
        if let Some(password) = &config.proxy_password {
            if password == "********" {
                let current = crate::config::get_config()?;
                if let Some(saved) = current.proxy {
                    config.proxy_password = saved.proxy_password;
                }
            }
        }

        return ProxyManager::test_draft(Some(config)).await;
    }

    let manager = PROXY_MANAGER.get().ok_or("Proxy manager not initialized")?;
    manager.test_proxy().await
}

#[cfg(test)]
#[path = "proxy/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "proxy/test_support.rs"]
pub mod test_support;
