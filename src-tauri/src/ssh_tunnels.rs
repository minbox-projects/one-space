use crate::{crypto, get_data_dir};
use serde::{Deserialize, Serialize};
use ssh2::{CheckResult, KnownHostFileKind, KeyboardInteractivePrompt, Prompt, Session};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

const SSH_TUNNELS_UPDATED_EVENT: &str = "ssh-tunnels-updated";
const SSH_TUNNEL_CONNECT_FAILED_EVENT: &str = "ssh-tunnel-connect-failed";
const PASSWORD_SECRET_PREFIX: &str = "onespace_ssh_tunnel_password:";
const LOCAL_BIND_HOST: &str = "127.0.0.1";
const REMOTE_BIND_HOST: &str = "127.0.0.1";
const SSH_IO_TIMEOUT: Duration = Duration::from_millis(1000);
const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

static RECORDS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNTIME_MANAGER: OnceLock<Mutex<HashMap<String, RunningTunnel>>> = OnceLock::new();

fn records_lock() -> &'static Mutex<()> {
    RECORDS_LOCK.get_or_init(|| Mutex::new(()))
}

fn runtime_manager() -> &'static Mutex<HashMap<String, RunningTunnel>> {
    RUNTIME_MANAGER.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedBlob {
    #[serde(default)]
    is_encrypted: bool,
    data: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SshTunnelSourceKind {
    SavedHost,
    Custom,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SshTunnelAuthKind {
    Password,
    Key,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SshTunnelForwardMode {
    Local,
    Remote,
    Dynamic,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SshTunnelStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelCustomConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_kind: SshTunnelAuthKind,
    #[serde(default)]
    pub key_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelForwardConfig {
    pub mode: SshTunnelForwardMode,
    #[serde(default)]
    pub local_bind_host: Option<String>,
    #[serde(default)]
    pub local_port: Option<u16>,
    #[serde(default)]
    pub remote_bind_host: Option<String>,
    #[serde(default)]
    pub remote_port: Option<u16>,
    #[serde(default)]
    pub target_host: Option<String>,
    #[serde(default)]
    pub target_port: Option<u16>,
    #[serde(default)]
    pub dynamic_probe_host: Option<String>,
    #[serde(default)]
    pub dynamic_probe_port: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SshTunnelRecord {
    pub id: String,
    pub name: String,
    pub source_kind: SshTunnelSourceKind,
    #[serde(default)]
    pub saved_host_name: Option<String>,
    #[serde(default)]
    pub custom: Option<SshTunnelCustomConfig>,
    pub forward: SshTunnelForwardConfig,
    #[serde(default)]
    pub auto_connect: bool,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub last_connected_at: Option<u64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelCustomView {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_kind: SshTunnelAuthKind,
    #[serde(default)]
    pub key_path: Option<String>,
    pub has_password: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelView {
    pub id: String,
    pub name: String,
    pub source_kind: SshTunnelSourceKind,
    #[serde(default)]
    pub saved_host_name: Option<String>,
    #[serde(default)]
    pub custom: Option<SshTunnelCustomView>,
    pub forward: SshTunnelForwardConfig,
    pub auto_connect: bool,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub last_connected_at: Option<u64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelRuntimeView {
    pub id: String,
    pub status: SshTunnelStatus,
    pub active_client_count: usize,
    pub mode: SshTunnelForwardMode,
    pub summary: String,
    #[serde(default)]
    pub resolved_server_host: Option<String>,
    #[serde(default)]
    pub listening_addr: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelProbeResult {
    pub ok: bool,
    pub mode: SshTunnelForwardMode,
    pub summary: String,
    pub message: String,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelCustomInput {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_kind: SshTunnelAuthKind,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub preserve_password: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelUpsertInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub source_kind: SshTunnelSourceKind,
    #[serde(default)]
    pub saved_host_name: Option<String>,
    #[serde(default)]
    pub custom: Option<SshTunnelCustomInput>,
    pub forward: SshTunnelForwardConfig,
    #[serde(default)]
    pub auto_connect: bool,
}

pub type SshTunnelProbeDraftInput = SshTunnelUpsertInput;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SshTunnelFailureEvent {
    pub id: String,
    pub name: String,
    pub error: String,
    pub auto_connect: bool,
}

#[derive(Debug, Clone)]
struct RuntimeState {
    status: SshTunnelStatus,
    mode: SshTunnelForwardMode,
    summary: String,
    resolved_server_host: Option<String>,
    listening_addr: Option<String>,
    last_error: Option<String>,
}

struct RunningTunnel {
    stop: Arc<AtomicBool>,
    active_clients: Arc<AtomicUsize>,
    state: Arc<Mutex<RuntimeState>>,
    join: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
enum ResolvedAuth {
    Password(String),
    Key { path: PathBuf },
    Agent,
}

#[derive(Debug, Clone)]
struct ResolvedSshConfig {
    host: String,
    port: u16,
    user: String,
    auth: ResolvedAuth,
    source_label: String,
}

#[derive(Debug, Default, Clone)]
struct ParsedSshAlias {
    host_name: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
    proxy_command: Option<String>,
    proxy_jump: Option<String>,
}

#[derive(Debug, Clone)]
struct StartupSuccess {
    listening_addr: Option<String>,
    resolved_server_host: String,
}

fn get_tunnels_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    let dir = data_dir.join("data").join("ssh_tunnels");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.enc.json"))
}

fn load_records_unlocked() -> Result<Vec<SshTunnelRecord>, String> {
    let path = get_tunnels_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(records) = serde_json::from_str::<Vec<SshTunnelRecord>>(&content) {
        return Ok(records);
    }
    let blob: EncryptedBlob = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let plain = if blob.is_encrypted {
        let password = crypto::get_or_init_master_password()?;
        crypto::decrypt(&blob.data, &password)?
    } else {
        blob.data
    };
    serde_json::from_str(&plain).map_err(|e| e.to_string())
}

fn write_records_unlocked(records: &[SshTunnelRecord]) -> Result<(), String> {
    let path = get_tunnels_path()?;
    let password = crypto::get_or_init_master_password()?;
    let json = serde_json::to_string_pretty(records).map_err(|e| e.to_string())?;
    let encrypted = crypto::encrypt(&json, &password)?;
    let blob = EncryptedBlob {
        is_encrypted: true,
        data: encrypted,
    };
    let wrapped = serde_json::to_string_pretty(&blob).map_err(|e| e.to_string())?;
    fs::write(path, wrapped).map_err(|e| e.to_string())
}

fn load_records() -> Result<Vec<SshTunnelRecord>, String> {
    let _guard = records_lock().lock().map_err(|e| e.to_string())?;
    load_records_unlocked()
}

fn mutate_records<T>(
    mutator: impl FnOnce(&mut Vec<SshTunnelRecord>) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = records_lock().lock().map_err(|e| e.to_string())?;
    let mut records = load_records_unlocked()?;
    let result = mutator(&mut records)?;
    write_records_unlocked(&records)?;
    Ok(result)
}

fn secret_key_for_tunnel(id: &str) -> String {
    format!("{PASSWORD_SECRET_PREFIX}{id}")
}

fn password_exists(id: &str) -> bool {
    crate::secrets::get_secret(&secret_key_for_tunnel(id))
        .ok()
        .flatten()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn password_for_tunnel(id: &str) -> Result<Option<String>, String> {
    crate::secrets::get_secret(&secret_key_for_tunnel(id))
}

fn to_view(record: &SshTunnelRecord) -> SshTunnelView {
    let custom = record.custom.as_ref().map(|custom| SshTunnelCustomView {
        host: custom.host.clone(),
        port: custom.port,
        user: custom.user.clone(),
        auth_kind: custom.auth_kind.clone(),
        key_path: custom.key_path.clone(),
        has_password: matches!(custom.auth_kind, SshTunnelAuthKind::Password)
            && password_exists(&record.id),
    });
    SshTunnelView {
        id: record.id.clone(),
        name: record.name.clone(),
        source_kind: record.source_kind.clone(),
        saved_host_name: record.saved_host_name.clone(),
        custom,
        forward: record.forward.clone(),
        auto_connect: record.auto_connect,
        created_at: record.created_at,
        updated_at: record.updated_at,
        last_connected_at: record.last_connected_at,
        last_error: record.last_error.clone(),
    }
}

fn tunnel_summary(forward: &SshTunnelForwardConfig) -> String {
    match forward.mode {
        SshTunnelForwardMode::Local => format!(
            "L {}:{} -> {}:{}",
            forward
                .local_bind_host
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(LOCAL_BIND_HOST),
            forward.local_port.unwrap_or(0),
            forward.target_host.as_deref().unwrap_or("127.0.0.1"),
            forward.target_port.unwrap_or(0)
        ),
        SshTunnelForwardMode::Remote => format!(
            "R {}:{} <- {}:{}",
            forward
                .remote_bind_host
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(REMOTE_BIND_HOST),
            forward.remote_port.unwrap_or(0),
            forward.target_host.as_deref().unwrap_or("127.0.0.1"),
            forward.target_port.unwrap_or(0)
        ),
        SshTunnelForwardMode::Dynamic => {
            let mut summary = format!(
                "D {}:{} (SOCKS5)",
                forward
                    .local_bind_host
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(LOCAL_BIND_HOST),
                forward.local_port.unwrap_or(0)
            );
            if let (Some(host), Some(port)) = (
                forward
                    .dynamic_probe_host
                    .as_deref()
                    .filter(|value| !value.trim().is_empty()),
                forward.dynamic_probe_port,
            ) {
                summary.push_str(&format!(" | Probe: {}:{}", host, port));
            }
            summary
        }
    }
}

fn default_runtime_view(record: &SshTunnelRecord) -> SshTunnelRuntimeView {
    SshTunnelRuntimeView {
        id: record.id.clone(),
        status: SshTunnelStatus::Disconnected,
        active_client_count: 0,
        mode: record.forward.mode.clone(),
        summary: tunnel_summary(&record.forward),
        resolved_server_host: None,
        listening_addr: None,
        last_error: record.last_error.clone(),
    }
}

fn runtime_view(record: &SshTunnelRecord, running: Option<&RunningTunnel>) -> SshTunnelRuntimeView {
    if let Some(running) = running {
        let state = running.state.lock().map_err(|e| e.to_string()).ok();
        if let Some(state) = state {
            return SshTunnelRuntimeView {
                id: record.id.clone(),
                status: state.status.clone(),
                active_client_count: running.active_clients.load(Ordering::Relaxed),
                mode: state.mode.clone(),
                summary: state.summary.clone(),
                resolved_server_host: state.resolved_server_host.clone(),
                listening_addr: state.listening_addr.clone(),
                last_error: state.last_error.clone().or_else(|| record.last_error.clone()),
            };
        }
    }
    default_runtime_view(record)
}

fn update_runtime_state(
    app: &AppHandle,
    id: &str,
    updater: impl FnOnce(&mut RuntimeState),
) -> Result<(), String> {
    let mut manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    if let Some(running) = manager.get_mut(id) {
        let mut state = running.state.lock().map_err(|e| e.to_string())?;
        updater(&mut state);
    }
    emit_tunnels_updated(app);
    Ok(())
}

fn emit_tunnels_updated(app: &AppHandle) {
    let _ = app.emit(SSH_TUNNELS_UPDATED_EVENT, ());
}

fn emit_connect_failed(app: &AppHandle, record: &SshTunnelRecord, error: &str) {
    let _ = app.emit(
        SSH_TUNNEL_CONNECT_FAILED_EVENT,
        SshTunnelFailureEvent {
            id: record.id.clone(),
            name: record.name.clone(),
            error: error.to_string(),
            auto_connect: record.auto_connect,
        },
    );
}

fn update_record_connection_success(id: &str) -> Result<(), String> {
    mutate_records(|records| {
        let record = records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| "Tunnel not found".to_string())?;
        record.last_connected_at = Some(now_ts());
        record.last_error = None;
        Ok(())
    })
}

fn update_record_error(id: &str, error: &str) -> Result<(), String> {
    mutate_records(|records| {
        let record = records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| "Tunnel not found".to_string())?;
        record.last_error = Some(error.to_string());
        record.updated_at = now_ts();
        Ok(())
    })
}

fn validate_input(
    input: &SshTunnelUpsertInput,
    existing: Option<&SshTunnelRecord>,
) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("Tunnel name is required".to_string());
    }

    match input.source_kind {
        SshTunnelSourceKind::SavedHost => {
            if input
                .saved_host_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err("Please choose an SSH server alias".to_string());
            }
        }
        SshTunnelSourceKind::Custom => {
            let custom = input
                .custom
                .as_ref()
                .ok_or_else(|| "Custom SSH server details are required".to_string())?;
            if custom.host.trim().is_empty() || custom.user.trim().is_empty() {
                return Err("Custom SSH host and username are required".to_string());
            }
            if custom.port == 0 {
                return Err("Custom SSH port is invalid".to_string());
            }
            match custom.auth_kind {
                SshTunnelAuthKind::Password => {
                    let password_supplied = custom
                        .password
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some();
                    let preserve = custom.preserve_password.unwrap_or(false);
                    let existing_password = existing
                        .map(|record| matches!(record.source_kind, SshTunnelSourceKind::Custom))
                        .unwrap_or(false)
                        && existing
                            .map(|record| password_exists(&record.id))
                            .unwrap_or(false);
                    if !password_supplied && !(preserve && existing_password) {
                        return Err("Password authentication requires a password".to_string());
                    }
                }
                SshTunnelAuthKind::Key => {
                    if custom
                        .key_path
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                    {
                        return Err("Key authentication requires a key file".to_string());
                    }
                }
            }
        }
    }

    let forward = &input.forward;
    match forward.mode {
        SshTunnelForwardMode::Local => {
            if forward.local_port.unwrap_or(0) == 0 {
                return Err("Local forwarding requires a local port".to_string());
            }
            if forward
                .target_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
                || forward.target_port.unwrap_or(0) == 0
            {
                return Err("Local forwarding requires a target host and target port".to_string());
            }
        }
        SshTunnelForwardMode::Remote => {
            if forward.remote_port.unwrap_or(0) == 0 {
                return Err("Remote forwarding requires a remote port".to_string());
            }
            if forward
                .target_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
                || forward.target_port.unwrap_or(0) == 0
            {
                return Err(
                    "Remote forwarding requires a local target host and target port".to_string(),
                );
            }
        }
        SshTunnelForwardMode::Dynamic => {
            if forward.local_port.unwrap_or(0) == 0 {
                return Err("Dynamic forwarding requires a local SOCKS port".to_string());
            }
            let probe_host = forward
                .dynamic_probe_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string());
            let probe_port = forward.dynamic_probe_port;
            if probe_host.is_some() ^ probe_port.is_some() {
                return Err("Dynamic probe host and port must be provided together".to_string());
            }
        }
    }

    Ok(())
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.trim_matches('"').to_string()
}

fn read_ssh_config_sections() -> Result<Vec<(Vec<String>, HashMap<String, String>)>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let config_path = home.join(".ssh").join("config");
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let mut sections = Vec::new();
    let mut patterns = Vec::new();
    let mut options = HashMap::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        let value = parts.collect::<Vec<_>>().join(" ");
        if key.eq_ignore_ascii_case("host") {
            if !patterns.is_empty() || !options.is_empty() {
                sections.push((patterns.clone(), options.clone()));
            }
            patterns = value
                .split_whitespace()
                .map(|item| item.trim().to_string())
                .collect();
            options.clear();
        } else if !patterns.is_empty() {
            options.insert(key.to_ascii_lowercase(), value.trim().to_string());
        }
    }

    if !patterns.is_empty() || !options.is_empty() {
        sections.push((patterns, options));
    }

    Ok(sections)
}

fn load_saved_host_alias(alias: &str) -> Result<ParsedSshAlias, String> {
    let sections = read_ssh_config_sections()?;
    let mut parsed = ParsedSshAlias::default();

    for (patterns, options) in sections {
        let matched = patterns.iter().any(|pattern| pattern == "*" || pattern == alias);
        if !matched {
            continue;
        }
        if let Some(value) = options.get("hostname") {
            parsed.host_name = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("user") {
            parsed.user = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("port") {
            if let Ok(port) = value.trim().parse::<u16>() {
                parsed.port = Some(port);
            }
        }
        if let Some(value) = options.get("identityfile") {
            parsed.identity_file = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("proxycommand") {
            parsed.proxy_command = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("proxyjump") {
            parsed.proxy_jump = Some(value.trim().to_string());
        }
    }

    if parsed.host_name.is_none() {
        return Err(format!("SSH server alias '{}' was not found in ~/.ssh/config", alias));
    }
    if parsed.proxy_command.is_some() || parsed.proxy_jump.is_some() {
        return Err(
            "This SSH server alias uses ProxyCommand/ProxyJump and is not supported yet. Please use a custom SSH tunnel instead."
                .to_string(),
        );
    }

    Ok(parsed)
}

fn resolve_ssh_config_from_record(record: &SshTunnelRecord) -> Result<ResolvedSshConfig, String> {
    match record.source_kind {
        SshTunnelSourceKind::SavedHost => {
            let alias = record
                .saved_host_name
                .as_deref()
                .ok_or_else(|| "Missing SSH server alias".to_string())?;
            let parsed = load_saved_host_alias(alias)?;
            let user = parsed
                .user
                .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "root".to_string()));
            let auth = if let Some(identity_file) = parsed.identity_file {
                ResolvedAuth::Key {
                    path: PathBuf::from(expand_tilde(&identity_file)),
                }
            } else {
                ResolvedAuth::Agent
            };
            Ok(ResolvedSshConfig {
                host: parsed.host_name.unwrap_or_else(|| alias.to_string()),
                port: parsed.port.unwrap_or(22),
                user,
                auth,
                source_label: alias.to_string(),
            })
        }
        SshTunnelSourceKind::Custom => {
            let custom = record
                .custom
                .as_ref()
                .ok_or_else(|| "Missing custom SSH configuration".to_string())?;
            let auth = match custom.auth_kind {
                SshTunnelAuthKind::Password => {
                    let password = password_for_tunnel(&record.id)?
                        .ok_or_else(|| "Missing saved password for this SSH tunnel".to_string())?;
                    ResolvedAuth::Password(password)
                }
                SshTunnelAuthKind::Key => ResolvedAuth::Key {
                    path: PathBuf::from(expand_tilde(
                        custom
                            .key_path
                            .as_deref()
                            .ok_or_else(|| "Missing SSH key path".to_string())?,
                    )),
                },
            };
            Ok(ResolvedSshConfig {
                host: custom.host.clone(),
                port: custom.port,
                user: custom.user.clone(),
                auth,
                source_label: format!("{}@{}:{}", custom.user, custom.host, custom.port),
            })
        }
    }
}

fn resolve_ssh_config_from_input(input: &SshTunnelProbeDraftInput) -> Result<ResolvedSshConfig, String> {
    match input.source_kind {
        SshTunnelSourceKind::SavedHost => {
            let alias = input
                .saved_host_name
                .as_deref()
                .ok_or_else(|| "Missing SSH server alias".to_string())?;
            let parsed = load_saved_host_alias(alias)?;
            Ok(ResolvedSshConfig {
                host: parsed.host_name.unwrap_or_else(|| alias.to_string()),
                port: parsed.port.unwrap_or(22),
                user: parsed
                    .user
                    .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "root".to_string())),
                auth: if let Some(identity_file) = parsed.identity_file {
                    ResolvedAuth::Key {
                        path: PathBuf::from(expand_tilde(&identity_file)),
                    }
                } else {
                    ResolvedAuth::Agent
                },
                source_label: alias.to_string(),
            })
        }
        SshTunnelSourceKind::Custom => {
            let custom = input
                .custom
                .as_ref()
                .ok_or_else(|| "Missing custom SSH configuration".to_string())?;
            let auth = match custom.auth_kind {
                SshTunnelAuthKind::Password => ResolvedAuth::Password(
                    custom
                        .password
                        .clone()
                        .ok_or_else(|| "Password is required for this probe".to_string())?,
                ),
                SshTunnelAuthKind::Key => ResolvedAuth::Key {
                    path: PathBuf::from(expand_tilde(
                        custom
                            .key_path
                            .as_deref()
                            .ok_or_else(|| "SSH key path is required".to_string())?,
                    )),
                },
            };
            Ok(ResolvedSshConfig {
                host: custom.host.clone(),
                port: custom.port,
                user: custom.user.clone(),
                auth,
                source_label: format!("{}@{}:{}", custom.user, custom.host, custom.port),
            })
        }
    }
}

fn known_hosts_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let ssh_dir = home.join(".ssh");
    fs::create_dir_all(&ssh_dir).map_err(|e| e.to_string())?;
    Ok(ssh_dir.join("known_hosts"))
}

fn verify_host_key(session: &Session, host: &str, port: u16) -> Result<(), String> {
    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| "The SSH server did not provide a host key".to_string())?;
    let mut known_hosts = session.known_hosts().map_err(|e| e.to_string())?;
    let path = known_hosts_path()?;
    if path.exists() {
        let _ = known_hosts.read_file(&path, KnownHostFileKind::OpenSSH);
    }
    match known_hosts.check_port(host, port, key) {
        CheckResult::Match => Ok(()),
        CheckResult::NotFound => {
            let host_entry = if port == 22 {
                host.to_string()
            } else {
                format!("[{}]:{}", host, port)
            };
            known_hosts
                .add(&host_entry, key, &host_entry, key_type.into())
                .map_err(|e| e.to_string())?;
            known_hosts
                .write_file(&path, KnownHostFileKind::OpenSSH)
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        CheckResult::Mismatch => Err(format!(
            "Host key mismatch for {}:{}. Please inspect ~/.ssh/known_hosts before retrying.",
            host, port
        )),
        CheckResult::Failure => Err("Failed to verify the SSH host key".to_string()),
    }
}

struct PasswordPrompter {
    password: String,
}

impl KeyboardInteractivePrompt for PasswordPrompter {
    fn prompt<'a>(
        &mut self,
        _username: &str,
        _instructions: &str,
        prompts: &[Prompt<'a>],
    ) -> Vec<String> {
        prompts
            .iter()
            .map(|_prompt| self.password.clone())
            .collect::<Vec<_>>()
    }
}

fn authenticate_session(session: &Session, config: &ResolvedSshConfig) -> Result<(), String> {
    match &config.auth {
        ResolvedAuth::Password(password) => {
            if session.userauth_password(&config.user, password).is_err() {
                let mut prompter = PasswordPrompter {
                    password: password.clone(),
                };
                session
                    .userauth_keyboard_interactive(&config.user, &mut prompter)
                    .map_err(|e| e.to_string())?;
            }
        }
        ResolvedAuth::Key { path } => {
            session
                .userauth_pubkey_file(&config.user, None, path, None)
                .map_err(|e| e.to_string())?;
        }
        ResolvedAuth::Agent => {
            let mut agent = session.agent().map_err(|e| e.to_string())?;
            agent.connect().map_err(|e| e.to_string())?;
            agent.list_identities().map_err(|e| e.to_string())?;
            let identities = agent.identities().map_err(|e| e.to_string())?;
            let mut authenticated = false;
            for identity in identities {
                if agent.userauth(&config.user, &identity).is_ok() {
                    authenticated = true;
                    break;
                }
            }
            if !authenticated {
                return Err(format!(
                    "SSH agent authentication failed for '{}'. If this server requires a password, please create the tunnel with Custom SSH instead.",
                    config.source_label
                ));
            }
        }
    }

    if session.authenticated() {
        Ok(())
    } else {
        Err("SSH authentication failed".to_string())
    }
}

fn open_authenticated_session(config: &ResolvedSshConfig) -> Result<Session, String> {
    let addr = format!("{}:{}", config.host, config.port);
    let socket_addr = addr
        .parse::<SocketAddr>()
        .ok()
        .or_else(|| {
            (config.host.as_str(), config.port)
                .to_socket_addrs()
                .ok()
                .and_then(|mut addrs| addrs.next())
        })
        .ok_or_else(|| format!("Could not resolve SSH server {}", addr))?;
    let tcp = TcpStream::connect_timeout(&socket_addr, SSH_CONNECT_TIMEOUT)
        .map_err(|e| format!("Failed to connect to SSH server {}: {}", addr, e))?;
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(SSH_CONNECT_TIMEOUT)).ok();
    tcp.set_write_timeout(Some(SSH_CONNECT_TIMEOUT)).ok();

    let mut session = Session::new().map_err(|e| e.to_string())?;
    session.set_tcp_stream(tcp);
    session.set_timeout(SSH_CONNECT_TIMEOUT.as_millis() as u32);
    session.handshake().map_err(|e| e.to_string())?;
    verify_host_key(&session, &config.host, config.port)?;
    authenticate_session(&session, config)?;
    session.set_timeout(SSH_IO_TIMEOUT.as_millis() as u32);
    session.set_keepalive(true, 30);
    Ok(session)
}

fn port_conflict_details(port: u16) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.lines().nth(1)?.trim().to_string();
        if line.is_empty() {
            None
        } else {
            Some(line)
        }
    }
    #[cfg(target_os = "windows")]
    {
        let netstat = Command::new("netstat")
            .args(["-ano", "-p", "tcp"])
            .output()
            .ok()?;
        if !netstat.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&netstat.stdout);
        let pid = stdout
            .lines()
            .find(|line| line.contains(&format!(":{port}")) && line.contains("LISTENING"))
            .and_then(|line| line.split_whitespace().last())
            .map(|value| value.to_string())?;
        let tasklist = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .ok()?;
        let proc_name = if tasklist.status.success() {
            String::from_utf8_lossy(&tasklist.stdout)
                .lines()
                .skip(3)
                .find_map(|line| line.split_whitespace().next())
                .unwrap_or("unknown")
                .to_string()
        } else {
            "unknown".to_string()
        };
        Some(format!("PID {} ({})", pid, proc_name))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = port;
        None
    }
}

fn ensure_local_port_available(port: u16) -> Result<(), String> {
    match TcpListener::bind((LOCAL_BIND_HOST, port)) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(error) => {
            let details = port_conflict_details(port)
                .map(|details| format!(" Occupied by {}.", details))
                .unwrap_or_default();
            Err(format!(
                "Local port {} is unavailable: {}.{}",
                port, error, details
            ))
        }
    }
}

fn ensure_local_target_reachable(host: &str, port: u16) -> Result<(), String> {
    let addr = format!("{}:{}", host, port);
    let socket_addr = addr
        .parse::<SocketAddr>()
        .ok()
        .or_else(|| {
            (host, port)
                .to_socket_addrs()
                .ok()
                .and_then(|mut addrs| addrs.next())
        })
        .ok_or_else(|| format!("Could not resolve target {}", addr))?;
    TcpStream::connect_timeout(&socket_addr, SSH_CONNECT_TIMEOUT)
        .map(|stream| {
            drop(stream);
        })
        .map_err(|error| format!("Target service {} is unreachable: {}", addr, error))
}

fn probe_forward(
    forward: &SshTunnelForwardConfig,
    resolved: &ResolvedSshConfig,
) -> Result<String, String> {
    match forward.mode {
        SshTunnelForwardMode::Local => {
            let local_port = forward.local_port.ok_or_else(|| "Missing local port".to_string())?;
            ensure_local_port_available(local_port)?;
            let session = open_authenticated_session(resolved)?;
            let target_host = forward
                .target_host
                .as_deref()
                .ok_or_else(|| "Missing target host".to_string())?;
            let target_port = forward
                .target_port
                .ok_or_else(|| "Missing target port".to_string())?;
            let mut channel = session
                .channel_direct_tcpip(target_host, target_port, None)
                .map_err(|e| format!("Target {}:{} is unreachable: {}", target_host, target_port, e))?;
            let _ = channel.close();
            Ok(format!(
                "SSH login succeeded and remote target {}:{} is reachable.",
                target_host, target_port
            ))
        }
        SshTunnelForwardMode::Remote => {
            let target_host = forward
                .target_host
                .as_deref()
                .ok_or_else(|| "Missing target host".to_string())?;
            let target_port = forward
                .target_port
                .ok_or_else(|| "Missing target port".to_string())?;
            ensure_local_target_reachable(target_host, target_port)?;
            let session = open_authenticated_session(resolved)?;
            let remote_port = forward
                .remote_port
                .ok_or_else(|| "Missing remote port".to_string())?;
            let remote_host = forward
                .remote_bind_host
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(REMOTE_BIND_HOST);
            let (_listener, bound_port) = session
                .channel_forward_listen(remote_port, Some(remote_host), Some(16))
                .map_err(|e| format!("Failed to reserve remote port {}:{}: {}", remote_host, remote_port, e))?;
            Ok(format!(
                "SSH login succeeded, remote port {}:{} is available, and local target {}:{} is reachable.",
                remote_host, bound_port, target_host, target_port
            ))
        }
        SshTunnelForwardMode::Dynamic => {
            let local_port = forward.local_port.ok_or_else(|| "Missing local port".to_string())?;
            ensure_local_port_available(local_port)?;
            let session = open_authenticated_session(resolved)?;
            drop(session);
            if let (Some(host), Some(port)) = (
                forward
                    .dynamic_probe_host
                    .as_deref()
                    .filter(|value| !value.trim().is_empty()),
                forward.dynamic_probe_port,
            ) {
                probe_dynamic_via_temp_proxy(resolved.clone(), host.to_string(), port)?;
                Ok(format!(
                    "SOCKS5 proxy probe to {}:{} succeeded.",
                    host, port
                ))
            } else {
                Ok("SSH login succeeded and the SOCKS5 proxy can be started.".to_string())
            }
        }
    }
}

fn bind_local_listener(port: u16) -> Result<TcpListener, String> {
    let listener = TcpListener::bind((LOCAL_BIND_HOST, port))
        .map_err(|e| format!("Failed to bind local port {}: {}", port, e))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("Failed to set non-blocking listener on {}: {}", port, e))?;
    Ok(listener)
}

fn write_all_channel(channel: &mut ssh2::Channel, stop: &Arc<AtomicBool>, data: &[u8]) -> Result<(), String> {
    let mut offset = 0;
    while offset < data.len() {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match channel.write(&data[offset..]) {
            Ok(0) => return Err("SSH channel closed while forwarding data".to_string()),
            Ok(written) => offset += written,
            Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {
                continue;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn write_all_socket(stream: &mut TcpStream, data: &[u8]) -> Result<(), String> {
    let mut offset = 0;
    while offset < data.len() {
        match stream.write(&data[offset..]) {
            Ok(0) => return Err("Local socket closed while forwarding data".to_string()),
            Ok(written) => offset += written,
            Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {
                continue;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn bridge_streams(
    socket: TcpStream,
    channel: ssh2::Channel,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    socket.set_read_timeout(Some(SSH_IO_TIMEOUT)).ok();
    socket.set_write_timeout(Some(SSH_IO_TIMEOUT)).ok();
    let socket_read = socket
        .try_clone()
        .map_err(|e| format!("Failed to clone local socket: {}", e))?;
    let mut socket_write = socket;
    let mut channel_write = channel.clone();
    let mut channel_close = channel.clone();
    let mut channel_read = channel;
    let stop_a = stop.clone();
    let stop_b = stop.clone();

    let a = thread::spawn(move || -> Result<(), String> {
        let mut socket_read = socket_read;
        let mut buf = [0u8; 8192];
        loop {
            if stop_a.load(Ordering::Relaxed) {
                let _ = channel_write.send_eof();
                return Ok(());
            }
            match socket_read.read(&mut buf) {
                Ok(0) => {
                    let _ = channel_write.send_eof();
                    let _ = channel_write.flush();
                    return Ok(());
                }
                Ok(read) => write_all_channel(&mut channel_write, &stop_a, &buf[..read])?,
                Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    });

    let b = thread::spawn(move || -> Result<(), String> {
        let mut buf = [0u8; 8192];
        loop {
            if stop_b.load(Ordering::Relaxed) {
                let _ = socket_write.shutdown(Shutdown::Both);
                return Ok(());
            }
            match channel_read.read(&mut buf) {
                Ok(0) => {
                    let _ = socket_write.shutdown(Shutdown::Write);
                    return Ok(());
                }
                Ok(read) => write_all_socket(&mut socket_write, &buf[..read])?,
                Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    });

    let res_a = a.join().map_err(|_| "Forwarding thread panicked".to_string())?;
    let res_b = b.join().map_err(|_| "Forwarding thread panicked".to_string())?;
    let _ = channel_close.close();
    let _ = channel_close.wait_close();
    res_a?;
    res_b?;
    Ok(())
}

fn start_local_runtime(
    app: AppHandle,
    record: SshTunnelRecord,
    resolved: ResolvedSshConfig,
    state: Arc<Mutex<RuntimeState>>,
    stop: Arc<AtomicBool>,
    active_clients: Arc<AtomicUsize>,
    startup: mpsc::Sender<Result<StartupSuccess, String>>,
) {
    let summary = tunnel_summary(&record.forward);
    let local_port = match record.forward.local_port {
        Some(port) => port,
        None => {
            let _ = startup.send(Err("Missing local port".to_string()));
            return;
        }
    };
    let target_host = match record.forward.target_host.clone() {
        Some(host) => host,
        None => {
            let _ = startup.send(Err("Missing target host".to_string()));
            return;
        }
    };
    let target_port = match record.forward.target_port {
        Some(port) => port,
        None => {
            let _ = startup.send(Err("Missing target port".to_string()));
            return;
        }
    };

    if let Err(error) = ensure_local_port_available(local_port) {
        let _ = startup.send(Err(error));
        return;
    }

    if let Err(error) = open_authenticated_session(&resolved)
        .and_then(|session| {
            let mut channel = session
                .channel_direct_tcpip(&target_host, target_port, None)
                .map_err(|e| e.to_string())?;
            let _ = channel.close();
            Ok(())
        })
    {
        let _ = startup.send(Err(error));
        return;
    }

    let listener = match bind_local_listener(local_port) {
        Ok(listener) => listener,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };

    {
        let mut state_guard = state.lock().expect("runtime state poisoned");
        state_guard.status = SshTunnelStatus::Connected;
        state_guard.summary = summary.clone();
        state_guard.resolved_server_host = Some(format!("{}:{}", resolved.host, resolved.port));
        state_guard.listening_addr = Some(format!("{}:{}", LOCAL_BIND_HOST, local_port));
        state_guard.last_error = None;
    }
    let _ = update_record_connection_success(&record.id);
    emit_tunnels_updated(&app);
    let _ = startup.send(Ok(StartupSuccess {
        listening_addr: Some(format!("{}:{}", LOCAL_BIND_HOST, local_port)),
        resolved_server_host: format!("{}:{}", resolved.host, resolved.port),
    }));

    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((socket, _peer)) => {
                let stop_for_client = stop.clone();
                let resolved_for_client = resolved.clone();
                let app_for_client = app.clone();
                let tunnel_id = record.id.clone();
                let target_host_for_client = target_host.clone();
                active_clients.fetch_add(1, Ordering::Relaxed);
                let active_clients_for_worker = active_clients.clone();
                thread::spawn(move || {
                    let result = (|| -> Result<(), String> {
                        let session = open_authenticated_session(&resolved_for_client)?;
                        let channel = session
                            .channel_direct_tcpip(&target_host_for_client, target_port, None)
                            .map_err(|e| e.to_string())?;
                        bridge_streams(socket, channel, stop_for_client)
                    })();
                    active_clients_for_worker.fetch_sub(1, Ordering::Relaxed);
                    if let Err(error) = result {
                        let _ = update_record_error(&tunnel_id, &error);
                        let _ = update_runtime_state(&app_for_client, &tunnel_id, |state| {
                            state.last_error = Some(error.clone());
                        });
                    } else {
                        emit_tunnels_updated(&app_for_client);
                    }
                });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(150));
            }
            Err(error) => {
                let _ = startup.send(Err(error.to_string()));
                let _ = update_record_error(&record.id, &error.to_string());
                let _ = update_runtime_state(&app, &record.id, |state| {
                    state.status = SshTunnelStatus::Error;
                    state.last_error = Some(error.to_string());
                });
                return;
            }
        }
    }

    let _ = update_runtime_state(&app, &record.id, |state| {
        state.status = SshTunnelStatus::Disconnected;
    });
}

fn read_socks_address(socket: &mut TcpStream) -> Result<(String, u16), String> {
    let mut header = [0u8; 4];
    socket.read_exact(&mut header).map_err(|e| e.to_string())?;
    if header[0] != 5 {
        return Err("Invalid SOCKS5 request version".to_string());
    }
    if header[1] != 1 {
        return Err("Only SOCKS5 CONNECT is supported".to_string());
    }
    let host = match header[3] {
        1 => {
            let mut addr = [0u8; 4];
            socket.read_exact(&mut addr).map_err(|e| e.to_string())?;
            std::net::Ipv4Addr::from(addr).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            socket.read_exact(&mut len).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; len[0] as usize];
            socket.read_exact(&mut buf).map_err(|e| e.to_string())?;
            String::from_utf8(buf).map_err(|e| e.to_string())?
        }
        4 => {
            let mut addr = [0u8; 16];
            socket.read_exact(&mut addr).map_err(|e| e.to_string())?;
            std::net::Ipv6Addr::from(addr).to_string()
        }
        _ => return Err("Unsupported SOCKS5 address type".to_string()),
    };
    let mut port_buf = [0u8; 2];
    socket.read_exact(&mut port_buf).map_err(|e| e.to_string())?;
    Ok((host, u16::from_be_bytes(port_buf)))
}

fn write_socks_success(socket: &mut TcpStream) -> Result<(), String> {
    socket
        .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
        .map_err(|e| e.to_string())
}

fn write_socks_error(socket: &mut TcpStream, code: u8) {
    let _ = socket.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]);
}

fn handle_dynamic_client(
    socket: TcpStream,
    resolved: ResolvedSshConfig,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    socket.set_read_timeout(Some(SSH_IO_TIMEOUT)).ok();
    socket.set_write_timeout(Some(SSH_IO_TIMEOUT)).ok();
    let mut socket = socket;
    let mut hello = [0u8; 2];
    socket.read_exact(&mut hello).map_err(|e| e.to_string())?;
    if hello[0] != 5 {
        return Err("Invalid SOCKS5 greeting".to_string());
    }
    let mut methods = vec![0u8; hello[1] as usize];
    socket.read_exact(&mut methods).map_err(|e| e.to_string())?;
    if !methods.contains(&0) {
        socket
            .write_all(&[5, 0xff])
            .map_err(|e| e.to_string())?;
        return Err("SOCKS5 client does not support no-auth mode".to_string());
    }
    socket.write_all(&[5, 0]).map_err(|e| e.to_string())?;
    let (target_host, target_port) = read_socks_address(&mut socket)?;
    let session = open_authenticated_session(&resolved)?;
    match session.channel_direct_tcpip(&target_host, target_port, None) {
        Ok(channel) => {
            write_socks_success(&mut socket)?;
            bridge_streams(socket, channel, stop)
        }
        Err(error) => {
            write_socks_error(&mut socket, 5);
            Err(format!(
                "SOCKS probe target {}:{} is unreachable: {}",
                target_host, target_port, error
            ))
        }
    }
}

fn serve_dynamic_listener(
    app: AppHandle,
    record: SshTunnelRecord,
    resolved: ResolvedSshConfig,
    state: Arc<Mutex<RuntimeState>>,
    stop: Arc<AtomicBool>,
    active_clients: Arc<AtomicUsize>,
    startup: mpsc::Sender<Result<StartupSuccess, String>>,
) {
    let local_port = match record.forward.local_port {
        Some(port) => port,
        None => {
            let _ = startup.send(Err("Missing local port".to_string()));
            return;
        }
    };
    if let Err(error) = ensure_local_port_available(local_port) {
        let _ = startup.send(Err(error));
        return;
    }
    if let Err(error) = open_authenticated_session(&resolved) {
        let _ = startup.send(Err(error));
        return;
    }
    let listener = match bind_local_listener(local_port) {
        Ok(listener) => listener,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };

    {
        let mut state_guard = state.lock().expect("runtime state poisoned");
        state_guard.status = SshTunnelStatus::Connected;
        state_guard.resolved_server_host = Some(format!("{}:{}", resolved.host, resolved.port));
        state_guard.listening_addr = Some(format!("{}:{}", LOCAL_BIND_HOST, local_port));
        state_guard.last_error = None;
    }
    let _ = update_record_connection_success(&record.id);
    emit_tunnels_updated(&app);
    let _ = startup.send(Ok(StartupSuccess {
        listening_addr: Some(format!("{}:{}", LOCAL_BIND_HOST, local_port)),
        resolved_server_host: format!("{}:{}", resolved.host, resolved.port),
    }));

    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((socket, _peer)) => {
                let stop_for_client = stop.clone();
                let resolved_for_client = resolved.clone();
                let app_for_client = app.clone();
                let tunnel_id = record.id.clone();
                let active_clients_for_worker = active_clients.clone();
                active_clients_for_worker.fetch_add(1, Ordering::Relaxed);
                thread::spawn(move || {
                    let result = handle_dynamic_client(socket, resolved_for_client, stop_for_client);
                    active_clients_for_worker.fetch_sub(1, Ordering::Relaxed);
                    if let Err(error) = result {
                        let _ = update_record_error(&tunnel_id, &error);
                        let _ = update_runtime_state(&app_for_client, &tunnel_id, |state| {
                            state.last_error = Some(error.clone());
                        });
                    } else {
                        emit_tunnels_updated(&app_for_client);
                    }
                });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(150));
            }
            Err(error) => {
                let _ = update_record_error(&record.id, &error.to_string());
                let _ = update_runtime_state(&app, &record.id, |state| {
                    state.status = SshTunnelStatus::Error;
                    state.last_error = Some(error.to_string());
                });
                return;
            }
        }
    }

    let _ = update_runtime_state(&app, &record.id, |state| {
        state.status = SshTunnelStatus::Disconnected;
    });
}

fn start_remote_runtime(
    app: AppHandle,
    record: SshTunnelRecord,
    resolved: ResolvedSshConfig,
    state: Arc<Mutex<RuntimeState>>,
    stop: Arc<AtomicBool>,
    active_clients: Arc<AtomicUsize>,
    startup: mpsc::Sender<Result<StartupSuccess, String>>,
) {
    let target_host = match record.forward.target_host.clone() {
        Some(host) => host,
        None => {
            let _ = startup.send(Err("Missing target host".to_string()));
            return;
        }
    };
    let target_port = match record.forward.target_port {
        Some(port) => port,
        None => {
            let _ = startup.send(Err("Missing target port".to_string()));
            return;
        }
    };
    if let Err(error) = ensure_local_target_reachable(&target_host, target_port) {
        let _ = startup.send(Err(error));
        return;
    }
    let session = match open_authenticated_session(&resolved) {
        Ok(session) => session,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    let remote_port = match record.forward.remote_port {
        Some(port) => port,
        None => {
            let _ = startup.send(Err("Missing remote port".to_string()));
            return;
        }
    };
    let remote_host = record
        .forward
        .remote_bind_host
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(REMOTE_BIND_HOST)
        .to_string();
    let (mut listener, bound_port) = match session.channel_forward_listen(remote_port, Some(&remote_host), Some(16)) {
        Ok(result) => result,
        Err(error) => {
            let _ = startup.send(Err(format!(
                "Failed to reserve remote port {}:{}: {}",
                remote_host, remote_port, error
            )));
            return;
        }
    };

    {
        let mut state_guard = state.lock().expect("runtime state poisoned");
        state_guard.status = SshTunnelStatus::Connected;
        state_guard.resolved_server_host = Some(format!("{}:{}", resolved.host, resolved.port));
        state_guard.listening_addr = Some(format!("{}:{}", remote_host, bound_port));
        state_guard.last_error = None;
    }
    let _ = update_record_connection_success(&record.id);
    emit_tunnels_updated(&app);
    let _ = startup.send(Ok(StartupSuccess {
        listening_addr: Some(format!("{}:{}", remote_host, bound_port)),
        resolved_server_host: format!("{}:{}", resolved.host, resolved.port),
    }));

    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok(channel) => {
                let stop_for_client = stop.clone();
                let app_for_client = app.clone();
                let tunnel_id = record.id.clone();
                let active_clients_for_worker = active_clients.clone();
                let target_host_for_worker = target_host.clone();
                active_clients_for_worker.fetch_add(1, Ordering::Relaxed);
                thread::spawn(move || {
                    let result = (|| -> Result<(), String> {
                        let addr = format!("{}:{}", target_host_for_worker, target_port);
                        let socket_addr = addr
                            .parse::<SocketAddr>()
                            .ok()
                            .or_else(|| {
                                (target_host_for_worker.as_str(), target_port)
                                    .to_socket_addrs()
                                    .ok()
                                    .and_then(|mut addrs| addrs.next())
                            })
                            .ok_or_else(|| format!("Could not resolve local target {}", addr))?;
                        let socket = TcpStream::connect_timeout(&socket_addr, SSH_CONNECT_TIMEOUT)
                            .map_err(|e| format!("Failed to connect to local target {}: {}", addr, e))?;
                        bridge_streams(socket, channel, stop_for_client)
                    })();
                    active_clients_for_worker.fetch_sub(1, Ordering::Relaxed);
                    if let Err(error) = result {
                        let _ = update_record_error(&tunnel_id, &error);
                        let _ = update_runtime_state(&app_for_client, &tunnel_id, |state| {
                            state.last_error = Some(error.clone());
                        });
                    } else {
                        emit_tunnels_updated(&app_for_client);
                    }
                });
            }
            Err(error) if error.to_string().to_lowercase().contains("timed out") => {
                continue;
            }
            Err(error) => {
                let err = error.to_string();
                let _ = update_record_error(&record.id, &err);
                let _ = update_runtime_state(&app, &record.id, |state| {
                    state.status = if stop.load(Ordering::Relaxed) {
                        SshTunnelStatus::Disconnected
                    } else {
                        SshTunnelStatus::Error
                    };
                    state.last_error = Some(err.clone());
                });
                return;
            }
        }
    }

    let _ = update_runtime_state(&app, &record.id, |state| {
        state.status = SshTunnelStatus::Disconnected;
    });
}

fn spawn_runtime_thread(
    app: AppHandle,
    record: SshTunnelRecord,
    resolved: ResolvedSshConfig,
) -> Result<RunningTunnel, String> {
    let state = Arc::new(Mutex::new(RuntimeState {
        status: SshTunnelStatus::Connecting,
        mode: record.forward.mode.clone(),
        summary: tunnel_summary(&record.forward),
        resolved_server_host: None,
        listening_addr: None,
        last_error: None,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let active_clients = Arc::new(AtomicUsize::new(0));
    let (startup_tx, startup_rx) = mpsc::channel();
    let state_for_thread = state.clone();
    let stop_for_thread = stop.clone();
    let active_for_thread = active_clients.clone();
    let record_for_thread = record.clone();
    let app_for_thread = app.clone();
    let resolved_for_thread = resolved.clone();

    let join = thread::spawn(move || match record_for_thread.forward.mode {
        SshTunnelForwardMode::Local => start_local_runtime(
            app_for_thread,
            record_for_thread,
            resolved_for_thread,
            state_for_thread,
            stop_for_thread,
            active_for_thread,
            startup_tx,
        ),
        SshTunnelForwardMode::Remote => start_remote_runtime(
            app_for_thread,
            record_for_thread,
            resolved_for_thread,
            state_for_thread,
            stop_for_thread,
            active_for_thread,
            startup_tx,
        ),
        SshTunnelForwardMode::Dynamic => serve_dynamic_listener(
            app_for_thread,
            record_for_thread,
            resolved_for_thread,
            state_for_thread,
            stop_for_thread,
            active_for_thread,
            startup_tx,
        ),
    });

    match startup_rx.recv_timeout(Duration::from_secs(20)) {
        Ok(Ok(startup)) => {
            let mut state_guard = state.lock().map_err(|e| e.to_string())?;
            state_guard.status = SshTunnelStatus::Connected;
            state_guard.resolved_server_host = Some(startup.resolved_server_host);
            state_guard.listening_addr = startup.listening_addr;
            drop(state_guard);
            Ok(RunningTunnel {
                stop,
                active_clients,
                state,
                join: Some(join),
            })
        }
        Ok(Err(error)) => {
            stop.store(true, Ordering::Relaxed);
            let _ = join.join();
            Err(error)
        }
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            let _ = join.join();
            Err("Timed out while establishing the SSH tunnel".to_string())
        }
    }
}

fn probe_dynamic_via_temp_proxy(
    resolved: ResolvedSshConfig,
    target_host: String,
    target_port: u16,
) -> Result<(), String> {
    let listener = TcpListener::bind((LOCAL_BIND_HOST, 0))
        .map_err(|e| format!("Failed to start temporary SOCKS5 probe: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = stop.clone();
    let handle = thread::spawn(move || -> Result<(), String> {
        let (socket, _) = listener.accept().map_err(|e| e.to_string())?;
        handle_dynamic_client(socket, resolved, stop_for_thread)
    });

    let mut client = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        SSH_CONNECT_TIMEOUT,
    )
    .map_err(|e| format!("Failed to connect to temporary SOCKS5 probe: {}", e))?;
    client.set_read_timeout(Some(SSH_CONNECT_TIMEOUT)).ok();
    client.set_write_timeout(Some(SSH_CONNECT_TIMEOUT)).ok();
    client.write_all(&[5, 1, 0]).map_err(|e| e.to_string())?;
    let mut hello = [0u8; 2];
    client.read_exact(&mut hello).map_err(|e| e.to_string())?;
    if hello != [5, 0] {
        stop.store(true, Ordering::Relaxed);
        let _ = handle.join();
        return Err("The temporary SOCKS5 probe failed during negotiation".to_string());
    }
    let host_bytes = target_host.as_bytes();
    let mut request = vec![5, 1, 0, 3, host_bytes.len() as u8];
    request.extend_from_slice(host_bytes);
    request.extend_from_slice(&target_port.to_be_bytes());
    client.write_all(&request).map_err(|e| e.to_string())?;
    let mut response = [0u8; 10];
    client.read_exact(&mut response).map_err(|e| e.to_string())?;
    stop.store(true, Ordering::Relaxed);
    let thread_result = handle.join().map_err(|_| "Dynamic probe thread panicked".to_string())?;
    if response[1] != 0 {
        return Err(format!(
            "The SOCKS5 proxy could not reach {}:{} (reply code {}).",
            target_host, target_port, response[1]
        ));
    }
    thread_result
}

fn disconnect_runtime(id: &str) -> Result<(), String> {
    let maybe_running = runtime_manager()
        .lock()
        .map_err(|e| e.to_string())?
        .remove(id);

    if let Some(mut running) = maybe_running {
        running.stop.store(true, Ordering::Relaxed);
        if let Some(join) = running.join.take() {
            let _ = join.join();
        }
    }

    Ok(())
}

fn connect_internal(app: AppHandle, id: String, emit_failure_event: bool) -> Result<SshTunnelRuntimeView, String> {
    let record = load_records()?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| "Tunnel not found".to_string())?;

    let _ = disconnect_runtime(&id);
    let resolved = resolve_ssh_config_from_record(&record)?;
    let running = match spawn_runtime_thread(app.clone(), record.clone(), resolved) {
        Ok(running) => running,
        Err(error) => {
            let _ = update_record_error(&record.id, &error);
            if emit_failure_event {
                emit_connect_failed(&app, &record, &error);
            }
            return Err(error);
        }
    };

    let view = runtime_view(&record, Some(&running));
    let mut manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    manager.insert(record.id.clone(), running);
    drop(manager);
    emit_tunnels_updated(&app);
    Ok(view)
}

#[tauri::command]
pub fn ssh_tunnels_list() -> Result<Vec<SshTunnelView>, String> {
    let mut records = load_records()?;
    records.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(records.iter().map(to_view).collect())
}

#[tauri::command]
pub async fn ssh_tunnel_upsert(
    app: AppHandle,
    input: SshTunnelUpsertInput,
) -> Result<SshTunnelView, String> {
    let existing = input
        .id
        .as_ref()
        .and_then(|id| load_records().ok()?.into_iter().find(|record| record.id == *id));
    validate_input(&input, existing.as_ref())?;

    if let Some(id) = input.id.as_ref() {
        let _ = disconnect_runtime(id);
    }

    let now = now_ts();
    let tunnel_id = input
        .id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let custom_config = match input.source_kind {
        SshTunnelSourceKind::Custom => {
            let custom = input
                .custom
                .clone()
                .ok_or_else(|| "Missing custom SSH configuration".to_string())?;
            Some(SshTunnelCustomConfig {
                host: custom.host.trim().to_string(),
                port: custom.port,
                user: custom.user.trim().to_string(),
                auth_kind: custom.auth_kind,
                key_path: custom
                    .key_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string()),
            })
        }
        SshTunnelSourceKind::SavedHost => None,
    };

    let record = SshTunnelRecord {
        id: tunnel_id.clone(),
        name: input.name.trim().to_string(),
        source_kind: input.source_kind.clone(),
        saved_host_name: input
            .saved_host_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string()),
        custom: custom_config,
        forward: SshTunnelForwardConfig {
            mode: input.forward.mode.clone(),
            local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
            local_port: input.forward.local_port,
            remote_bind_host: Some(REMOTE_BIND_HOST.to_string()),
            remote_port: input.forward.remote_port,
            target_host: input
                .forward
                .target_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string()),
            target_port: input.forward.target_port,
            dynamic_probe_host: input
                .forward
                .dynamic_probe_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string()),
            dynamic_probe_port: input.forward.dynamic_probe_port,
        },
        auto_connect: input.auto_connect,
        created_at: existing.as_ref().map(|record| record.created_at).unwrap_or(now),
        updated_at: now,
        last_connected_at: existing.as_ref().and_then(|record| record.last_connected_at),
        last_error: None,
    };

    mutate_records(|records| {
        if let Some(index) = records.iter().position(|item| item.id == record.id) {
            records[index] = record.clone();
        } else {
            records.push(record.clone());
        }
        Ok(())
    })?;

    let secret_key = secret_key_for_tunnel(&record.id);
    let should_remove_password = !matches!(
        input.custom.as_ref().map(|custom| &custom.auth_kind),
        Some(SshTunnelAuthKind::Password)
    );
    if should_remove_password {
        let _ = crate::secrets::delete_secret(app.clone(), secret_key.clone()).await;
    } else if let Some(custom) = input.custom.as_ref() {
        if let Some(password) = custom
            .password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            crate::secrets::save_secret(app.clone(), secret_key.clone(), password.to_string()).await?;
        } else if !custom.preserve_password.unwrap_or(false) {
            let _ = crate::secrets::delete_secret(app.clone(), secret_key.clone()).await;
        }
    }

    emit_tunnels_updated(&app);
    Ok(to_view(&record))
}

#[tauri::command]
pub async fn ssh_tunnel_delete(app: AppHandle, id: String) -> Result<(), String> {
    let _ = disconnect_runtime(&id);
    mutate_records(|records| {
        let before = records.len();
        records.retain(|record| record.id != id);
        if before == records.len() {
            return Err("Tunnel not found".to_string());
        }
        Ok(())
    })?;
    let _ = crate::secrets::delete_secret(app.clone(), secret_key_for_tunnel(&id)).await;
    emit_tunnels_updated(&app);
    Ok(())
}

#[tauri::command]
pub fn ssh_tunnel_connect(app: AppHandle, id: String) -> Result<SshTunnelRuntimeView, String> {
    connect_internal(app, id, false)
}

#[tauri::command]
pub fn ssh_tunnel_disconnect(app: AppHandle, id: String) -> Result<SshTunnelRuntimeView, String> {
    let record = load_records()?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| "Tunnel not found".to_string())?;
    disconnect_runtime(&record.id)?;
    emit_tunnels_updated(&app);
    Ok(default_runtime_view(&record))
}

#[tauri::command]
pub fn ssh_tunnel_probe_draft(input: SshTunnelProbeDraftInput) -> Result<SshTunnelProbeResult, String> {
    validate_input(&input, None)?;
    let resolved = resolve_ssh_config_from_input(&input)?;
    let summary = tunnel_summary(&input.forward);
    match probe_forward(&input.forward, &resolved) {
        Ok(message) => Ok(SshTunnelProbeResult {
            ok: true,
            mode: input.forward.mode.clone(),
            summary,
            message,
            last_error: None,
        }),
        Err(error) => Ok(SshTunnelProbeResult {
            ok: false,
            mode: input.forward.mode.clone(),
            summary,
            message: error.clone(),
            last_error: Some(error),
        }),
    }
}

#[tauri::command]
pub fn ssh_tunnel_probe_saved(id: String) -> Result<SshTunnelProbeResult, String> {
    let record = load_records()?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| "Tunnel not found".to_string())?;
    let resolved = resolve_ssh_config_from_record(&record)?;
    let summary = tunnel_summary(&record.forward);
    match probe_forward(&record.forward, &resolved) {
        Ok(message) => Ok(SshTunnelProbeResult {
            ok: true,
            mode: record.forward.mode.clone(),
            summary,
            message,
            last_error: None,
        }),
        Err(error) => Ok(SshTunnelProbeResult {
            ok: false,
            mode: record.forward.mode.clone(),
            summary,
            message: error.clone(),
            last_error: Some(error),
        }),
    }
}

#[tauri::command]
pub fn ssh_tunnels_refresh_status() -> Result<Vec<SshTunnelRuntimeView>, String> {
    let records = load_records()?;
    let mut manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    let finished_ids = manager
        .iter()
        .filter_map(|(id, running)| {
            if running
                .join
                .as_ref()
                .map(|handle| handle.is_finished())
                .unwrap_or(false)
            {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for id in finished_ids {
        if let Some(mut running) = manager.remove(&id) {
            if let Some(join) = running.join.take() {
                let _ = join.join();
            }
        }
    }
    let views = records
        .iter()
        .map(|record| runtime_view(record, manager.get(&record.id)))
        .collect::<Vec<_>>();
    Ok(views)
}

pub async fn ssh_tunnels_bootstrap(app: AppHandle) -> Result<(), String> {
    let records = load_records()?;
    for record in records.into_iter().filter(|record| record.auto_connect) {
        if let Err(error) = connect_internal(app.clone(), record.id.clone(), true) {
            let _ = update_record_error(&record.id, &error);
        }
    }
    Ok(())
}

pub fn shutdown_runtime() -> Result<(), String> {
    let ids = runtime_manager()
        .lock()
        .map_err(|e| e.to_string())?
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for id in ids {
        let _ = disconnect_runtime(&id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_forward() -> SshTunnelForwardConfig {
        SshTunnelForwardConfig {
            mode: SshTunnelForwardMode::Local,
            local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
            local_port: Some(5432),
            remote_bind_host: Some(REMOTE_BIND_HOST.to_string()),
            remote_port: None,
            target_host: Some("127.0.0.1".to_string()),
            target_port: Some(5432),
            dynamic_probe_host: None,
            dynamic_probe_port: None,
        }
    }

    #[test]
    fn forward_summary_local() {
        assert_eq!(
            tunnel_summary(&local_forward()),
            "L 127.0.0.1:5432 -> 127.0.0.1:5432"
        );
    }

    #[test]
    fn forward_summary_remote() {
        let forward = SshTunnelForwardConfig {
            mode: SshTunnelForwardMode::Remote,
            local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
            local_port: None,
            remote_bind_host: Some(REMOTE_BIND_HOST.to_string()),
            remote_port: Some(15432),
            target_host: Some("127.0.0.1".to_string()),
            target_port: Some(5432),
            dynamic_probe_host: None,
            dynamic_probe_port: None,
        };
        assert_eq!(
            tunnel_summary(&forward),
            "R 127.0.0.1:15432 <- 127.0.0.1:5432"
        );
    }

    #[test]
    fn forward_summary_dynamic() {
        let forward = SshTunnelForwardConfig {
            mode: SshTunnelForwardMode::Dynamic,
            local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
            local_port: Some(1080),
            remote_bind_host: Some(REMOTE_BIND_HOST.to_string()),
            remote_port: None,
            target_host: None,
            target_port: None,
            dynamic_probe_host: Some("example.com".to_string()),
            dynamic_probe_port: Some(443),
        };
        assert_eq!(
            tunnel_summary(&forward),
            "D 127.0.0.1:1080 (SOCKS5) | Probe: example.com:443"
        );
    }

    #[test]
    fn validate_dynamic_probe_pair() {
        let input = SshTunnelUpsertInput {
            id: None,
            name: "dynamic".to_string(),
            source_kind: SshTunnelSourceKind::SavedHost,
            saved_host_name: Some("dev".to_string()),
            custom: None,
            forward: SshTunnelForwardConfig {
                mode: SshTunnelForwardMode::Dynamic,
                local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
                local_port: Some(1080),
                remote_bind_host: Some(REMOTE_BIND_HOST.to_string()),
                remote_port: None,
                target_host: None,
                target_port: None,
                dynamic_probe_host: Some("example.com".to_string()),
                dynamic_probe_port: None,
            },
            auto_connect: false,
        };
        assert!(validate_input(&input, None).is_err());
    }

    #[test]
    fn validate_custom_password_requires_secret() {
        let input = SshTunnelUpsertInput {
            id: None,
            name: "local".to_string(),
            source_kind: SshTunnelSourceKind::Custom,
            saved_host_name: None,
            custom: Some(SshTunnelCustomInput {
                host: "1.2.3.4".to_string(),
                port: 22,
                user: "dev".to_string(),
                auth_kind: SshTunnelAuthKind::Password,
                key_path: None,
                password: None,
                preserve_password: Some(false),
            }),
            forward: local_forward(),
            auto_connect: false,
        };
        assert!(validate_input(&input, None).is_err());
    }
}
