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

pub fn extract_cloudflare_url(text: &str) -> Option<String> {
    let re = Regex::new(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com").ok()?;
    re.find(text).map(|m| m.as_str().to_string())
}

pub struct ActiveTunnel {
    pub id: String,
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

pub async fn start_tunnel(
    config: &TunnelConfig,
    app: Option<tauri::AppHandle>,
) -> Result<TunnelRuntimeStatus, String> {
    let binary_path = find_binary(config.mode)
        .ok_or_else(|| format!("{:?} binary not found on system", config.mode))?;

    // Stop existing if any
    let _ = stop_tunnel(&config.id, app.clone()).await;

    let initial_status = TunnelRuntimeStatus {
        id: config.id.clone(),
        state: "starting".to_string(),
        public_url: None,
        error: None,
        started_at: Some(chrono::Utc::now().timestamp_millis()),
        active_connections: 0,
    };

    let status_arc = Arc::new(Mutex::new(initial_status.clone()));
    let logs_arc = Arc::new(Mutex::new(Vec::new()));
    let child_arc = Arc::new(TokioMutex::new(None));

    if let Some(ref handle) = app {
        let _ = handle.emit("tunnel-status-changed", &initial_status);
    }

    let active = Arc::new(ActiveTunnel {
        id: config.id.clone(),
        status: status_arc.clone(),
        logs: logs_arc.clone(),
        child: child_arc.clone(),
    });

    {
        let mut supervisor = SUPERVISOR.lock().map_err(|e| e.to_string())?;
        supervisor.tunnels.insert(config.id.clone(), active);
    }

    // Spawn command
    let mut cmd = Command::new(&binary_path);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    match config.mode {
        TunnelMode::Cloudflare => {
            cmd.arg("tunnel");
            cmd.arg("--url");
            cmd.arg(format!("http://127.0.0.1:{}", config.local_port));
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

            cmd.arg("-c");
            cmd.arg(&config_path);
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn tunnel process: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    {
        let mut guard = child_arc.lock().await;
        *guard = Some(child);
    }

    // Spawn reader for stdout and stderr
    let mode = config.mode;
    let custom_domain = config.custom_domain.clone();
    let remote_port = config.remote_port;
    let frp_server = config.frp_server.clone();
    let app_handle = app.clone();
    let status_for_readers = status_arc.clone();
    let logs_for_readers = logs_arc.clone();

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
                                    let _ = handle.emit("tunnel-status-changed", &*st);
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
                                    let _ = handle.emit("tunnel-status-changed", &*st);
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
                    let _ = handle.emit("tunnel-status-changed", &*st);
                }
            }
        }
    });

    // Return the status snapshot
    let snapshot = status_arc
        .lock()
        .map(|s| s.clone())
        .map_err(|e| e.to_string())?;
    Ok(snapshot)
}

pub async fn stop_tunnel(
    id: &str,
    app: Option<tauri::AppHandle>,
) -> Result<TunnelRuntimeStatus, String> {
    let active = {
        let mut supervisor = SUPERVISOR.lock().map_err(|e| e.to_string())?;
        supervisor.tunnels.remove(id)
    };

    let stopped_status = if let Some(active) = active {
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
        st.clone()
    } else {
        TunnelRuntimeStatus {
            id: id.to_string(),
            state: "stopped".to_string(),
            public_url: None,
            error: None,
            started_at: None,
            active_connections: 0,
        }
    };

    if let Some(ref handle) = app {
        let _ = handle.emit("tunnel-status-changed", &stopped_status);
    }

    Ok(stopped_status)
}

pub fn get_status(id: &str) -> TunnelRuntimeStatus {
    let supervisor = match SUPERVISOR.lock() {
        Ok(s) => s,
        Err(_) => {
            return TunnelRuntimeStatus {
                id: id.to_string(),
                state: "stopped".to_string(),
                public_url: None,
                error: None,
                started_at: None,
                active_connections: 0,
            }
        }
    };

    if let Some(active) = supervisor.tunnels.get(id) {
        if let Ok(st) = active.status.lock() {
            return st.clone();
        }
    }

    TunnelRuntimeStatus {
        id: id.to_string(),
        state: "stopped".to_string(),
        public_url: None,
        error: None,
        started_at: None,
        active_connections: 0,
    }
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
    list.into_iter().map(|t| get_status(&t.id)).collect()
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
