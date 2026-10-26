use std::collections::HashMap;
use std::fs;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use regex::Regex;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex as TokioMutex;
use crate::config::get_app_dir;
use super::environment::find_binary;
use super::types::{TunnelConfig, TunnelMode, TunnelProtocol, TunnelRuntimeStatus};

const MAX_LOG_LINES: usize = 200;

/// Id prefix reserved for short-lived tunnels owned by a running feature.
pub const EPHEMERAL_TUNNEL_ID_PREFIX: &str = "file-sharing-";

/// Status event used by saved Port Tunnel entries.
const DEFAULT_STATUS_EVENT: &str = "tunnel-status-changed";

pub fn extract_cloudflare_url(text: &str) -> Option<String> {
    let re = Regex::new(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com").ok()?;
    re.find(text).map(|m| m.as_str().to_string())
}

pub(crate) fn is_ephemeral_tunnel_id(id: &str) -> bool {
    id.starts_with(EPHEMERAL_TUNNEL_ID_PREFIX) && id.len() > EPHEMERAL_TUNNEL_ID_PREFIX.len()
}

pub(crate) fn validate_ephemeral_tunnel_request(
    id: &str,
    target_url: &str,
    event_name: &str,
) -> Result<(), String> {
    if !is_ephemeral_tunnel_id(id) {
        return Err(format!(
            "Ephemeral tunnel id must start with '{}' and carry a non-empty suffix",
            EPHEMERAL_TUNNEL_ID_PREFIX
        ));
    }

    let parsed = url::Url::parse(target_url)
        .map_err(|error| format!("Invalid target URL '{}': {}", target_url, error))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!(
            "Target URL must be absolute http or https, got '{}'",
            parsed.scheme()
        ));
    }
    match parsed.host_str() {
        Some(host) if !host.is_empty() => {}
        _ => return Err(format!("Target URL must include a host: '{}'", target_url)),
    }

    if event_name.trim().is_empty() {
        return Err("Event name must not be blank".to_string());
    }

    Ok(())
}

pub(crate) fn cloudflare_tunnel_args(target_url: &str) -> Vec<String> {
    vec![
        "tunnel".to_string(),
        "--url".to_string(),
        target_url.to_string(),
    ]
}

pub struct ActiveTunnel {
    pub id: String,
    pub event_name: String,
    pub status: Arc<Mutex<TunnelRuntimeStatus>>,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub child: Arc<TokioMutex<Option<Child>>>,
}

pub struct SupervisorState {
    tunnels: HashMap<String, Arc<ActiveTunnel>>,
}

impl SupervisorState {
    pub fn new() -> Self {
        Self {
            tunnels: HashMap::new(),
        }
    }
}

static SUPERVISOR: std::sync::LazyLock<Mutex<SupervisorState>> =
    std::sync::LazyLock::new(|| Mutex::new(SupervisorState::new()));

fn starting_status(id: &str) -> TunnelRuntimeStatus {
    TunnelRuntimeStatus {
        id: id.to_string(),
        state: "starting".to_string(),
        public_url: None,
        error: None,
        started_at: Some(chrono::Utc::now().timestamp_millis()),
        active_connections: 0,
    }
}

fn stopped_status(id: &str) -> TunnelRuntimeStatus {
    TunnelRuntimeStatus {
        id: id.to_string(),
        state: "stopped".to_string(),
        public_url: None,
        error: None,
        started_at: None,
        active_connections: 0,
    }
}

fn register_active_tunnel(id: &str, event_name: &str) -> Result<Arc<ActiveTunnel>, String> {
    let active = Arc::new(ActiveTunnel {
        id: id.to_string(),
        event_name: event_name.to_string(),
        status: Arc::new(Mutex::new(starting_status(id))),
        logs: Arc::new(Mutex::new(Vec::new())),
        child: Arc::new(TokioMutex::new(None)),
    });

    let mut supervisor = SUPERVISOR.lock().map_err(|e| e.to_string())?;
    supervisor.tunnels.insert(id.to_string(), active.clone());
    Ok(active)
}

fn remove_active_tunnel(id: &str) -> Option<Arc<ActiveTunnel>> {
    SUPERVISOR
        .lock()
        .ok()
        .and_then(|mut supervisor| supervisor.tunnels.remove(id))
}

/// Removes `id` from the registry, kills its child and reports the stopped status
/// together with the event name that this tunnel was using.
async fn stop_active_tunnel(
    id: &str,
) -> Result<Option<(TunnelRuntimeStatus, String)>, String> {
    let Some(active) = remove_active_tunnel(id) else {
        return Ok(None);
    };

    {
        let mut child_guard = active.child.lock().await;
        if let Some(ref mut child) = *child_guard {
            let _ = child.kill().await;
        }
        *child_guard = None;
    }

    let mut st = active.status.lock().map_err(|e| e.to_string())?;
    st.state = "stopped".to_string();
    st.public_url = None;
    st.error = None;
    st.started_at = None;
    let status = st.clone();
    drop(st);

    Ok(Some((status, active.event_name.clone())))
}

async fn launch_active_tunnel(
    active: Arc<ActiveTunnel>,
    mut cmd: Command,
    mode: TunnelMode,
    custom_domain: Option<String>,
    remote_port: Option<u16>,
    frp_server: Option<String>,
    app: Option<tauri::AppHandle>,
) -> Result<(), String> {
    if let Some(ref handle) = app {
        if let Ok(status) = active.status.lock() {
            let _ = handle.emit(&active.event_name, &*status);
        }
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            remove_active_tunnel(&active.id);
            return Err(format!("Failed to spawn tunnel process: {}", error));
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    {
        let mut guard = active.child.lock().await;
        *guard = Some(child);
    }

    let app_handle = app;
    let status_for_readers = active.status.clone();
    let logs_for_readers = active.logs.clone();
    let event_name = active.event_name.clone();

    tokio::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(100);

        if let Some(out) = stdout {
            let tx_out = tx.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(out).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if tx_out.send(line).await.is_err() {
                        break;
                    }
                }
            });
        }

        if let Some(err) = stderr {
            let tx_err = tx.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(err).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if tx_err.send(line).await.is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        let mut url_detected = false;

        while let Some(line) = rx.recv().await {
            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
            let log_entry = format!("[{}] {}", timestamp, line);

            if let Ok(mut logs) = logs_for_readers.lock() {
                if logs.len() >= MAX_LOG_LINES {
                    logs.remove(0);
                }
                logs.push(log_entry);
            }

            if !url_detected {
                match mode {
                    TunnelMode::Cloudflare => {
                        if let Some(url) = extract_cloudflare_url(&line) {
                            url_detected = true;
                            if let Ok(mut st) = status_for_readers.lock() {
                                st.state = "running".to_string();
                                st.public_url = Some(url);
                                st.error = None;
                                if let Some(ref handle) = app_handle {
                                    let _ = handle.emit(&event_name, &*st);
                                }
                            }
                        }
                    }
                    TunnelMode::Frp => {
                        if line.contains("start proxy success") || line.contains("login to server success") {
                            url_detected = true;
                            let url = if let Some(ref domain) = custom_domain {
                                format!("http://{}", domain)
                            } else if let (Some(ref s), Some(p)) = (frp_server.as_ref(), remote_port) {
                                format!("{}:{}", s, p)
                            } else {
                                "connected".to_string()
                            };

                            if let Ok(mut st) = status_for_readers.lock() {
                                st.state = "running".to_string();
                                st.public_url = Some(url);
                                st.error = None;
                                if let Some(ref handle) = app_handle {
                                    let _ = handle.emit(&event_name, &*st);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Process exited or pipes closed
        if let Ok(mut st) = status_for_readers.lock() {
            if st.state != "stopped" {
                st.state = "error".to_string();
                if st.public_url.is_none() {
                    st.error = Some("Tunnel process exited unexpectedly".to_string());
                }
                if let Some(ref handle) = app_handle {
                    let _ = handle.emit(&event_name, &*st);
                }
            }
        }
    });

    Ok(())
}

pub async fn start_tunnel(
    config: &TunnelConfig,
    app: Option<tauri::AppHandle>,
) -> Result<TunnelRuntimeStatus, String> {
    let binary_path = find_binary(config.mode)
        .ok_or_else(|| format!("{:?} binary not found on system", config.mode))?;

    // Stop existing if any
    let _ = stop_tunnel(&config.id, app.clone()).await;

    let active = register_active_tunnel(&config.id, DEFAULT_STATUS_EVENT)?;

    // Spawn command
    let mut cmd = Command::new(&binary_path);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut custom_domain = None;
    let mut remote_port = None;
    let mut frp_server = None;

    match config.mode {
        TunnelMode::Cloudflare => {
            let target_url = format!("http://127.0.0.1:{}", config.local_port);
            cmd.args(cloudflare_tunnel_args(&target_url));
        }
        TunnelMode::Frp => {
            let app_dir = get_app_dir()?;
            let config_dir = app_dir.join("tunnels_frp");
            let _ = fs::create_dir_all(&config_dir);
            let config_path = config_dir.join(format!("frpc_{}.ini", config.id));

            let server = config.frp_server.as_deref().unwrap_or("127.0.0.1");
            let token = config.frp_token.as_deref().unwrap_or("");
            let protocol_str = match config.protocol {
                TunnelProtocol::Http => "http",
                TunnelProtocol::Tcp => "tcp",
            };

            let mut ini = format!(
                "[common]\nserver_addr = {}\nserver_port = 7000\n",
                server
            );
            if !token.is_empty() {
                ini.push_str(&format!("token = {}\n", token));
            }
            ini.push_str(&format!(
                "\n[{}]\ntype = {}\nlocal_ip = 127.0.0.1\nlocal_port = {}\n",
                config.name, protocol_str, config.local_port
            ));

            if let Some(ref domain) = config.custom_domain {
                if !domain.trim().is_empty() {
                    ini.push_str(&format!("custom_domains = {}\n", domain.trim()));
                }
            }
            if let Some(port) = config.remote_port {
                if port > 0 {
                    ini.push_str(&format!("remote_port = {}\n", port));
                }
            }

            fs::write(&config_path, &ini).map_err(|e| e.to_string())?;

            custom_domain = config.custom_domain.clone();
            remote_port = config.remote_port;
            frp_server = config.frp_server.clone();

            cmd.arg("-c");
            cmd.arg(&config_path);
        }
    }

    launch_active_tunnel(
        active.clone(),
        cmd,
        config.mode,
        custom_domain,
        remote_port,
        frp_server,
        app,
    )
    .await?;

    // Return the status snapshot
    active
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|e| e.to_string())
}

/// Starts a short-lived Cloudflare Quick Tunnel for a feature-owned target.
///
/// The tunnel is tracked in the same registry as saved tunnels but is never
/// persisted, and every status notification for it is emitted on `event_name`.
pub async fn start_ephemeral_cloudflare_tunnel(
    id: String,
    target_url: String,
    event_name: String,
    app: Option<tauri::AppHandle>,
) -> Result<TunnelRuntimeStatus, String> {
    validate_ephemeral_tunnel_request(&id, &target_url, &event_name)?;

    let binary_path = find_binary(TunnelMode::Cloudflare)
        .ok_or_else(|| format!("{:?} binary not found on system", TunnelMode::Cloudflare))?;

    if let Some((stopped, previous_event)) = stop_active_tunnel(&id).await? {
        if let Some(ref handle) = app {
            let _ = handle.emit(&previous_event, &stopped);
        }
    }

    let active = register_active_tunnel(&id, &event_name)?;

    let mut cmd = Command::new(&binary_path);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.args(cloudflare_tunnel_args(&target_url));

    launch_active_tunnel(
        active.clone(),
        cmd,
        TunnelMode::Cloudflare,
        None,
        None,
        None,
        app,
    )
    .await?;

    active
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|e| e.to_string())
}

pub async fn stop_tunnel(
    id: &str,
    app: Option<tauri::AppHandle>,
) -> Result<TunnelRuntimeStatus, String> {
    let (stopped, event_name) = match stop_active_tunnel(id).await? {
        Some((status, event_name)) => (status, event_name),
        None => (stopped_status(id), DEFAULT_STATUS_EVENT.to_string()),
    };

    if let Some(ref handle) = app {
        let _ = handle.emit(&event_name, &stopped);
    }

    Ok(stopped)
}

/// Synchronous best-effort stop used by shutdown and unexpected-listener paths.
/// Returns `false` when no active tunnel is registered under `id`.
pub fn kill_tunnel(id: &str) -> bool {
    let Some(active) = remove_active_tunnel(id) else {
        return false;
    };

    if let Ok(mut guard) = active.child.try_lock() {
        if let Some(ref mut child) = *guard {
            let _ = child.start_kill();
        }
        *guard = None;
    }

    if let Ok(mut st) = active.status.lock() {
        st.state = "stopped".to_string();
        st.public_url = None;
        st.error = None;
        st.started_at = None;
    }

    true
}

pub fn get_status(id: &str) -> TunnelRuntimeStatus {
    let supervisor = match SUPERVISOR.lock() {
        Ok(s) => s,
        Err(_) => return stopped_status(id),
    };

    if let Some(active) = supervisor.tunnels.get(id) {
        if let Ok(st) = active.status.lock() {
            return st.clone();
        }
    }

    stopped_status(id)
}

pub fn get_logs(id: &str) -> Vec<String> {
    let supervisor = match SUPERVISOR.lock() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    if let Some(active) = supervisor.tunnels.get(id) {
        if let Ok(logs) = active.logs.lock() {
            return logs.clone();
        }
    }

    Vec::new()
}

pub fn get_all_status() -> Vec<TunnelRuntimeStatus> {
    let list = match crate::tunnel::config::load_tunnels() {
        Ok(l) => l,
        Err(_) => return Vec::new(),
    };
    list.into_iter()
        .filter(|tunnel| !is_ephemeral_tunnel_id(&tunnel.id))
        .map(|tunnel| get_status(&tunnel.id))
        .collect()
}

pub fn shutdown_all() {
    let tunnels = {
        if let Ok(mut supervisor) = SUPERVISOR.lock() {
            let map = std::mem::take(&mut supervisor.tunnels);
            map
        } else {
            HashMap::new()
        }
    };

    for (_, active) in tunnels {
        if let Ok(mut guard) = active.child.try_lock() {
            if let Some(ref mut child) = *guard {
                let _ = child.start_kill();
            }
            *guard = None;
        }
    }
}
