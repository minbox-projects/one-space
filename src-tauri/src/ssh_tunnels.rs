use crate::{crypto, get_data_dir};
use serde::{Deserialize, Serialize};
use ssh2::{CheckResult, KeyboardInteractivePrompt, KnownHostFileKind, Prompt, Session};
use std::collections::{HashMap, HashSet};
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
const DEFAULT_TUNNEL_GROUP_ID: &str = "default";
const DEFAULT_TUNNEL_GROUP_NAME: &str = "Default Group";
const SSH_IO_TIMEOUT: Duration = Duration::from_millis(1000);
const SSH_IO_RETRY_BACKOFF: Duration = Duration::from_millis(10);
const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SSH_SESSION_POOL_MAX_IDLE: usize = 4;

/// Default SSH key files to try when no IdentityFile is specified
const DEFAULT_SSH_KEYS: &[&str] = &[
    "id_ed25519",
    "id_ed25519_sk",
    "id_ecdsa",
    "id_ecdsa_sk",
    "id_rsa",
];

/// Find existing default SSH key files in ~/.ssh/ using OpenSSH-like priority order.
fn find_default_ssh_keys() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let ssh_dir = home.join(".ssh");
    DEFAULT_SSH_KEYS
        .iter()
        .map(|key_name| ssh_dir.join(key_name))
        .filter(|path| path.exists())
        .collect()
}

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

fn default_group_name() -> String {
    DEFAULT_TUNNEL_GROUP_NAME.to_string()
}

fn default_group_id() -> String {
    DEFAULT_TUNNEL_GROUP_ID.to_string()
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
    #[serde(default = "default_group_id")]
    pub group_id: String,
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
struct SshTunnelGroupRecord {
    pub id: String,
    #[serde(default = "default_group_name")]
    pub name: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SshTunnelState {
    #[serde(default)]
    pub groups: Vec<SshTunnelGroupRecord>,
    #[serde(default)]
    pub tunnels: Vec<SshTunnelRecord>,
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
    pub group_id: String,
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
pub struct SshTunnelGroupView {
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_default: bool,
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
pub struct SshTunnelBatchOperationResult {
    pub operation: String,
    pub group_id: String,
    pub group_name: String,
    pub success_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub total_count: usize,
    pub failures: Vec<SshTunnelBatchFailureDetail>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelBatchFailureDetail {
    pub tunnel_id: String,
    pub tunnel_name: String,
    pub error: String,
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
pub struct SshTunnelsSnapshot {
    pub groups: Vec<SshTunnelGroupView>,
    pub tunnels: Vec<SshTunnelView>,
    pub runtime: Vec<SshTunnelRuntimeView>,
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
    #[serde(default)]
    pub group_id: Option<String>,
    pub source_kind: SshTunnelSourceKind,
    #[serde(default)]
    pub saved_host_name: Option<String>,
    #[serde(default)]
    pub custom: Option<SshTunnelCustomInput>,
    pub forward: SshTunnelForwardConfig,
    #[serde(default)]
    pub auto_connect: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelGroupUpsertInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
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
    Key {
        paths: Vec<PathBuf>,
        allow_agent_fallback: bool,
    },
    Agent,
}

#[derive(Debug, Clone)]
struct ResolvedSshConfig {
    host: String,
    port: u16,
    user: String,
    auth: ResolvedAuth,
    source_label: String,
    host_key_name: String,
    known_hosts_paths: Vec<PathBuf>,
}

#[derive(Debug, Default, Clone)]
struct ParsedSshAlias {
    host_name: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_files: Vec<String>,
    identities_only: Option<bool>,
    host_key_alias: Option<String>,
    user_known_hosts_file: Option<String>,
    proxy_command: Option<String>,
    proxy_jump: Option<String>,
}

#[derive(Debug, Clone)]
struct StartupSuccess {
    listening_addr: Option<String>,
    resolved_server_host: String,
}

struct SessionPool {
    resolved: ResolvedSshConfig,
    idle: Mutex<Vec<Session>>,
    max_idle: usize,
}

impl SessionPool {
    fn with_initial_session(resolved: ResolvedSshConfig, session: Session) -> Self {
        Self {
            resolved,
            idle: Mutex::new(vec![session]),
            max_idle: SSH_SESSION_POOL_MAX_IDLE,
        }
    }

    fn acquire(&self) -> Result<Session, String> {
        loop {
            let idle_session = {
                let mut idle = self.idle.lock().map_err(|e| e.to_string())?;
                idle.pop()
            };

            match idle_session {
                Some(session) if session.authenticated() => {
                    session.set_blocking(true);
                    set_session_timeout(&session, SSH_IO_TIMEOUT);
                    session.set_keepalive(true, 30);
                    return Ok(session);
                }
                Some(_) => continue,
                None => return open_authenticated_session(&self.resolved),
            }
        }
    }

    fn release(&self, session: Session) {
        if !session.authenticated() {
            return;
        }
        if let Ok(mut idle) = self.idle.lock() {
            if idle.len() < self.max_idle {
                idle.push(session);
                return;
            }
        }
        let _ = session.disconnect(None, "Closing excess idle SSH session", None);
    }
}

fn get_tunnels_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    let dir = data_dir.join("data").join("ssh_tunnels");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.enc.json"))
}

fn default_group_record() -> SshTunnelGroupRecord {
    let now = now_ts();
    SshTunnelGroupRecord {
        id: DEFAULT_TUNNEL_GROUP_ID.to_string(),
        name: DEFAULT_TUNNEL_GROUP_NAME.to_string(),
        created_at: now,
        updated_at: now,
        is_default: true,
    }
}

fn is_reserved_default_group_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "default group" | "默认分组"
    )
}

fn canonical_group_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn normalize_group_id(group_id: Option<&str>, groups: &[SshTunnelGroupRecord]) -> String {
    let requested = group_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TUNNEL_GROUP_ID);
    if groups.iter().any(|group| group.id == requested) {
        requested.to_string()
    } else {
        DEFAULT_TUNNEL_GROUP_ID.to_string()
    }
}

fn normalize_state(state: &mut SshTunnelState) {
    let default_template = default_group_record();
    let mut seen_group_ids = HashSet::new();
    let mut normalized_groups = Vec::new();
    let mut default_group = None;

    for group in state.groups.drain(..) {
        let trimmed_id = group.id.trim();
        if trimmed_id.is_empty() || !seen_group_ids.insert(trimmed_id.to_string()) {
            continue;
        }
        if trimmed_id == DEFAULT_TUNNEL_GROUP_ID {
            default_group = Some(SshTunnelGroupRecord {
                id: DEFAULT_TUNNEL_GROUP_ID.to_string(),
                name: DEFAULT_TUNNEL_GROUP_NAME.to_string(),
                created_at: if group.created_at > 0 {
                    group.created_at
                } else {
                    default_template.created_at
                },
                updated_at: if group.updated_at > 0 {
                    group.updated_at
                } else {
                    default_template.updated_at
                },
                is_default: true,
            });
            continue;
        }

        let trimmed_name = group.name.trim();
        if trimmed_name.is_empty() {
            continue;
        }

        normalized_groups.push(SshTunnelGroupRecord {
            id: trimmed_id.to_string(),
            name: trimmed_name.to_string(),
            created_at: group.created_at,
            updated_at: group.updated_at,
            is_default: false,
        });
    }

    let mut groups = Vec::with_capacity(normalized_groups.len() + 1);
    groups.push(default_group.unwrap_or(default_template));
    groups.extend(normalized_groups);

    let valid_group_ids = groups
        .iter()
        .map(|group| group.id.as_str())
        .collect::<HashSet<_>>();
    for tunnel in &mut state.tunnels {
        if !valid_group_ids.contains(tunnel.group_id.trim()) {
            tunnel.group_id = DEFAULT_TUNNEL_GROUP_ID.to_string();
        } else {
            tunnel.group_id = tunnel.group_id.trim().to_string();
        }
    }

    state.groups = groups;
}

fn parse_state_payload(content: &str) -> Result<SshTunnelState, String> {
    let value = serde_json::from_str::<serde_json::Value>(content).map_err(|e| e.to_string())?;
    match value {
        serde_json::Value::Array(_) => {
            let records =
                serde_json::from_value::<Vec<SshTunnelRecord>>(value).map_err(|e| e.to_string())?;
            Ok(SshTunnelState {
                groups: vec![default_group_record()],
                tunnels: records,
            })
        }
        serde_json::Value::Object(ref map)
            if map.contains_key("groups") || map.contains_key("tunnels") =>
        {
            serde_json::from_value::<SshTunnelState>(value).map_err(|e| e.to_string())
        }
        _ => Err("Unrecognized SSH tunnel state payload".to_string()),
    }
}

fn load_state_unlocked() -> Result<SshTunnelState, String> {
    let path = get_tunnels_path()?;
    if !path.exists() {
        return Ok(SshTunnelState {
            groups: vec![default_group_record()],
            tunnels: Vec::new(),
        });
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(SshTunnelState {
            groups: vec![default_group_record()],
            tunnels: Vec::new(),
        });
    }
    if let Ok(mut state) = parse_state_payload(&content) {
        normalize_state(&mut state);
        return Ok(state);
    }
    let blob: EncryptedBlob = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let plain = if blob.is_encrypted {
        let password = crypto::get_or_init_master_password()?;
        crypto::decrypt(&blob.data, &password)?
    } else {
        blob.data
    };
    let mut state = parse_state_payload(&plain)?;
    normalize_state(&mut state);
    Ok(state)
}

fn write_state_unlocked(state: &SshTunnelState) -> Result<(), String> {
    let path = get_tunnels_path()?;
    let password = crypto::get_or_init_master_password()?;
    let mut normalized = state.clone();
    normalize_state(&mut normalized);
    let json = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    let encrypted = crypto::encrypt(&json, &password)?;
    let blob = EncryptedBlob {
        is_encrypted: true,
        data: encrypted,
    };
    let wrapped = serde_json::to_string_pretty(&blob).map_err(|e| e.to_string())?;
    fs::write(path, wrapped).map_err(|e| e.to_string())
}

fn load_state() -> Result<SshTunnelState, String> {
    let _guard = records_lock().lock().map_err(|e| e.to_string())?;
    load_state_unlocked()
}

fn mutate_state<T>(
    mutator: impl FnOnce(&mut SshTunnelState) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = records_lock().lock().map_err(|e| e.to_string())?;
    let mut state = load_state_unlocked()?;
    let result = mutator(&mut state)?;
    write_state_unlocked(&state)?;
    Ok(result)
}

fn load_records() -> Result<Vec<SshTunnelRecord>, String> {
    Ok(load_state()?.tunnels)
}

fn mutate_records<T>(
    mutator: impl FnOnce(&mut Vec<SshTunnelRecord>) -> Result<T, String>,
) -> Result<T, String> {
    mutate_state(|state| mutator(&mut state.tunnels))
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
        group_id: record.group_id.clone(),
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

fn to_group_view(group: &SshTunnelGroupRecord) -> SshTunnelGroupView {
    SshTunnelGroupView {
        id: group.id.clone(),
        name: group.name.clone(),
        created_at: group.created_at,
        updated_at: group.updated_at,
        is_default: group.is_default,
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
                last_error: state
                    .last_error
                    .clone()
                    .or_else(|| record.last_error.clone()),
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
    {
        let mut manager = runtime_manager().lock().map_err(|e| e.to_string())?;
        if let Some(running) = manager.get_mut(id) {
            let mut state = running.state.lock().map_err(|e| e.to_string())?;
            updater(&mut state);
        }
    }
    emit_tunnels_updated(app);
    Ok(())
}

fn emit_tunnels_updated(app: &AppHandle) {
    if let Ok(snapshot) = snapshot_state() {
        let _ = app.emit(SSH_TUNNELS_UPDATED_EVENT, snapshot);
    } else {
        let _ = app.emit(SSH_TUNNELS_UPDATED_EVENT, ());
    }
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

fn validate_group_name(
    groups: &[SshTunnelGroupRecord],
    name: &str,
    editing_id: Option<&str>,
) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Environment group name is required".to_string());
    }
    if editing_id != Some(DEFAULT_TUNNEL_GROUP_ID) && is_reserved_default_group_name(trimmed) {
        return Err("The default environment group name is reserved".to_string());
    }
    let canonical = canonical_group_name(trimmed);
    let duplicate = groups.iter().any(|group| {
        if editing_id.is_some() && editing_id == Some(group.id.as_str()) {
            return false;
        }
        canonical_group_name(&group.name) == canonical
    });
    if duplicate {
        return Err("An environment group with this name already exists".to_string());
    }
    Ok(trimmed.to_string())
}

fn sort_groups(groups: &mut [SshTunnelGroupRecord]) {
    groups.sort_by(|a, b| match (a.is_default, b.is_default) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.created_at.cmp(&b.created_at).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        }),
    });
}

fn sort_tunnels(tunnels: &mut [SshTunnelRecord]) {
    tunnels.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
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
            let normalized_key = key.to_ascii_lowercase();
            match normalized_key.as_str() {
                "identityfile" | "userknownhostsfile" => {
                    options
                        .entry(normalized_key)
                        .and_modify(|existing: &mut String| {
                            existing.push('\n');
                            existing.push_str(value.trim());
                        })
                        .or_insert_with(|| value.trim().to_string());
                }
                _ => {
                    options.insert(normalized_key, value.trim().to_string());
                }
            }
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
        let matched = patterns
            .iter()
            .any(|pattern| pattern == "*" || pattern == alias);
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
            parsed.identity_files.extend(
                value
                    .lines()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string()),
            );
        }
        if let Some(value) = options.get("identitiesonly") {
            parsed.identities_only = match value.trim().to_ascii_lowercase().as_str() {
                "yes" | "true" | "on" => Some(true),
                "no" | "false" | "off" => Some(false),
                _ => parsed.identities_only,
            };
        }
        if let Some(value) = options.get("hostkeyalias") {
            parsed.host_key_alias = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("userknownhostsfile") {
            parsed.user_known_hosts_file = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("proxycommand") {
            parsed.proxy_command = Some(value.trim().to_string());
        }
        if let Some(value) = options.get("proxyjump") {
            parsed.proxy_jump = Some(value.trim().to_string());
        }
    }

    if parsed.host_name.is_none() {
        return Err(format!(
            "SSH server alias '{}' was not found in ~/.ssh/config",
            alias
        ));
    }
    if parsed.proxy_command.is_some() || parsed.proxy_jump.is_some() {
        return Err(
            "This SSH server alias uses ProxyCommand/ProxyJump and is not supported yet. Please use a custom SSH tunnel instead."
                .to_string(),
        );
    }

    Ok(parsed)
}

fn resolve_host_key_name(
    requested_host: &str,
    host_name: &str,
    host_key_alias: Option<&str>,
) -> String {
    host_key_alias
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(requested_host)
        .to_string()
        .if_empty_then(host_name)
}

trait StringFallbackExt {
    fn if_empty_then(self, fallback: &str) -> String;
}

impl StringFallbackExt for String {
    fn if_empty_then(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

fn known_hosts_paths_from_option(value: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(vec![known_hosts_path()?]);
    };
    if value.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    let paths = value
        .split_whitespace()
        .filter_map(|item| {
            let trimmed = item.trim().trim_matches('"');
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
                None
            } else {
                Some(PathBuf::from(expand_tilde(trimmed)))
            }
        })
        .collect::<Vec<_>>();
    if paths.is_empty() {
        Ok(vec![known_hosts_path()?])
    } else {
        Ok(paths)
    }
}

fn resolved_key_paths(identity_files: &[String]) -> Vec<PathBuf> {
    identity_files
        .iter()
        .map(|path| PathBuf::from(expand_tilde(path)))
        .collect()
}

fn candidate_saved_host_keys(parsed: &ParsedSshAlias) -> Vec<PathBuf> {
    let mut candidates = resolved_key_paths(&parsed.identity_files);
    if !parsed.identities_only.unwrap_or(false) || candidates.is_empty() {
        for path in find_default_ssh_keys() {
            if !candidates.iter().any(|existing| existing == &path) {
                candidates.push(path);
            }
        }
    }
    candidates
}

fn resolve_ssh_config_from_record(record: &SshTunnelRecord) -> Result<ResolvedSshConfig, String> {
    match record.source_kind {
        SshTunnelSourceKind::SavedHost => {
            let alias = record
                .saved_host_name
                .as_deref()
                .ok_or_else(|| "Missing SSH server alias".to_string())?;
            let parsed = load_saved_host_alias(alias)?;
            let host = parsed
                .host_name
                .clone()
                .unwrap_or_else(|| alias.to_string());
            let candidate_keys = candidate_saved_host_keys(&parsed);
            let user = parsed
                .user
                .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "root".to_string()));
            let auth = if !candidate_keys.is_empty() {
                ResolvedAuth::Key {
                    paths: candidate_keys,
                    allow_agent_fallback: !parsed.identities_only.unwrap_or(false),
                }
            } else {
                ResolvedAuth::Agent
            };
            Ok(ResolvedSshConfig {
                host: host.clone(),
                port: parsed.port.unwrap_or(22),
                user,
                auth,
                source_label: alias.to_string(),
                host_key_name: resolve_host_key_name(
                    alias,
                    &host,
                    parsed.host_key_alias.as_deref(),
                ),
                known_hosts_paths: known_hosts_paths_from_option(
                    parsed.user_known_hosts_file.as_deref(),
                )?,
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
                    paths: vec![PathBuf::from(expand_tilde(
                        custom
                            .key_path
                            .as_deref()
                            .ok_or_else(|| "Missing SSH key path".to_string())?,
                    ))],
                    allow_agent_fallback: false,
                },
            };
            Ok(ResolvedSshConfig {
                host: custom.host.clone(),
                port: custom.port,
                user: custom.user.clone(),
                auth,
                source_label: format!("{}@{}:{}", custom.user, custom.host, custom.port),
                host_key_name: custom.host.clone(),
                known_hosts_paths: known_hosts_paths_from_option(None)?,
            })
        }
    }
}

fn resolve_ssh_config_from_input(
    input: &SshTunnelProbeDraftInput,
) -> Result<ResolvedSshConfig, String> {
    match input.source_kind {
        SshTunnelSourceKind::SavedHost => {
            let alias = input
                .saved_host_name
                .as_deref()
                .ok_or_else(|| "Missing SSH server alias".to_string())?;
            let parsed = load_saved_host_alias(alias)?;
            let host = parsed
                .host_name
                .clone()
                .unwrap_or_else(|| alias.to_string());
            let candidate_keys = candidate_saved_host_keys(&parsed);
            Ok(ResolvedSshConfig {
                host: host.clone(),
                port: parsed.port.unwrap_or(22),
                user: parsed.user.unwrap_or_else(|| {
                    std::env::var("USER").unwrap_or_else(|_| "root".to_string())
                }),
                auth: if !candidate_keys.is_empty() {
                    ResolvedAuth::Key {
                        paths: candidate_keys,
                        allow_agent_fallback: !parsed.identities_only.unwrap_or(false),
                    }
                } else {
                    ResolvedAuth::Agent
                },
                source_label: alias.to_string(),
                host_key_name: resolve_host_key_name(
                    alias,
                    &host,
                    parsed.host_key_alias.as_deref(),
                ),
                known_hosts_paths: known_hosts_paths_from_option(
                    parsed.user_known_hosts_file.as_deref(),
                )?,
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
                    paths: vec![PathBuf::from(expand_tilde(
                        custom
                            .key_path
                            .as_deref()
                            .ok_or_else(|| "SSH key path is required".to_string())?,
                    ))],
                    allow_agent_fallback: false,
                },
            };
            Ok(ResolvedSshConfig {
                host: custom.host.clone(),
                port: custom.port,
                user: custom.user.clone(),
                auth,
                source_label: format!("{}@{}:{}", custom.user, custom.host, custom.port),
                host_key_name: custom.host.clone(),
                known_hosts_paths: known_hosts_paths_from_option(None)?,
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

fn verify_host_key(session: &Session, config: &ResolvedSshConfig) -> Result<(), String> {
    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| "The SSH server did not provide a host key".to_string())?;
    let mut known_hosts = session.known_hosts().map_err(|e| e.to_string())?;
    for path in &config.known_hosts_paths {
        if path.exists() {
            let _ = known_hosts.read_file(path, KnownHostFileKind::OpenSSH);
        }
    }
    match known_hosts.check_port(&config.host_key_name, config.port, key) {
        CheckResult::Match => Ok(()),
        CheckResult::NotFound => {
            let host_entry = if config.port == 22 {
                config.host_key_name.clone()
            } else {
                format!("[{}]:{}", config.host_key_name, config.port)
            };
            if let Some(path) = config.known_hosts_paths.first() {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                known_hosts
                    .add(&host_entry, key, &host_entry, key_type.into())
                    .map_err(|e| e.to_string())?;
                known_hosts
                    .write_file(path, KnownHostFileKind::OpenSSH)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        CheckResult::Mismatch => Err(format!(
            "Host key mismatch for {}:{}. Please inspect ~/.ssh/known_hosts before retrying.",
            config.host_key_name, config.port
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

fn authenticate_with_agent(session: &Session, config: &ResolvedSshConfig) -> Result<(), String> {
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
    if authenticated {
        Ok(())
    } else {
        Err(format!(
            "SSH agent authentication failed for '{}'. If this server requires a password, please create the tunnel with Custom SSH instead.",
            config.source_label
        ))
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
        ResolvedAuth::Key {
            paths,
            allow_agent_fallback,
        } => {
            let mut last_error = None;
            for path in paths {
                match session.userauth_pubkey_file(&config.user, None, path, None) {
                    Ok(_) => {
                        last_error = None;
                        break;
                    }
                    Err(error) => {
                        last_error = Some(format!(
                            "Public key authentication failed with {}: {}",
                            path.display(),
                            error
                        ));
                    }
                }
            }
            if !session.authenticated() {
                if *allow_agent_fallback {
                    if authenticate_with_agent(session, config).is_ok() {
                        last_error = None;
                    } else if last_error.is_none() {
                        last_error = Some(
                            "SSH public key authentication failed with all configured identities."
                                .to_string(),
                        );
                    }
                }
            }
            if let Some(error) = last_error {
                return Err(error);
            }
        }
        ResolvedAuth::Agent => authenticate_with_agent(session, config)?,
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
    set_session_timeout(&session, SSH_CONNECT_TIMEOUT);
    session.handshake().map_err(|e| e.to_string())?;
    verify_host_key(&session, config)?;
    authenticate_session(&session, config)?;
    set_session_timeout(&session, SSH_IO_TIMEOUT);
    session.set_keepalive(true, 30);
    Ok(session)
}

fn session_timeout_ms(timeout: Duration) -> u32 {
    timeout.as_millis().min(u128::from(u32::MAX)) as u32
}

fn set_session_timeout(session: &Session, timeout: Duration) {
    session.set_timeout(session_timeout_ms(timeout));
}

fn with_session_connect_timeout<T>(
    session: &Session,
    operation: impl FnOnce(&Session) -> Result<T, String>,
) -> Result<T, String> {
    set_session_timeout(session, SSH_CONNECT_TIMEOUT);
    let result = operation(session);
    set_session_timeout(session, SSH_IO_TIMEOUT);
    result
}

fn open_direct_tcpip_channel(
    session: &Session,
    target_host: &str,
    target_port: u16,
) -> Result<ssh2::Channel, String> {
    with_session_connect_timeout(session, |session| {
        session
            .channel_direct_tcpip(target_host, target_port, None)
            .map_err(|e| e.to_string())
    })
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
    let result = match forward.mode {
        SshTunnelForwardMode::Local => {
            let local_port = forward
                .local_port
                .ok_or_else(|| "Missing local port".to_string())?;
            ensure_local_port_available(local_port)?;
            let session = open_authenticated_session(resolved)?;
            let target_host = forward
                .target_host
                .as_deref()
                .ok_or_else(|| "Missing target host".to_string())?;
            let target_port = forward
                .target_port
                .ok_or_else(|| "Missing target port".to_string())?;
            let mut channel = open_direct_tcpip_channel(&session, target_host, target_port)
                .map_err(|e| {
                    format!(
                        "Target {}:{} is unreachable: {}",
                        target_host, target_port, e
                    )
                })?;
            let _ = channel.close();
            let _ = session.disconnect(None, "Probe completed", None);
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
            let (_listener, bound_port) = with_session_connect_timeout(&session, |session| {
                session
                    .channel_forward_listen(remote_port, Some(remote_host), Some(16))
                    .map_err(|e| {
                        format!(
                            "Failed to reserve remote port {}:{}: {}",
                            remote_host, remote_port, e
                        )
                    })
            })?;
            let _ = session.disconnect(None, "Probe completed", None);
            Ok(format!(
                "SSH login succeeded, remote port {}:{} is available, and local target {}:{} is reachable.",
                remote_host, bound_port, target_host, target_port
            ))
        }
        SshTunnelForwardMode::Dynamic => {
            let local_port = forward
                .local_port
                .ok_or_else(|| "Missing local port".to_string())?;
            ensure_local_port_available(local_port)?;
            let session = open_authenticated_session(resolved)?;
            let _ = session.disconnect(None, "Probe completed", None);
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
    };
    result
}

fn bind_local_listener(port: u16) -> Result<TcpListener, String> {
    let listener = TcpListener::bind((LOCAL_BIND_HOST, port))
        .map_err(|e| format!("Failed to bind local port {}: {}", port, e))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("Failed to set non-blocking listener on {}: {}", port, e))?;
    Ok(listener)
}

fn is_retryable_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn wait_before_io_retry(stop: &Arc<AtomicBool>) -> bool {
    if stop.load(Ordering::Relaxed) {
        return false;
    }
    thread::sleep(SSH_IO_RETRY_BACKOFF);
    !stop.load(Ordering::Relaxed)
}

fn write_all_channel(
    channel: &mut ssh2::Channel,
    stop: &Arc<AtomicBool>,
    data: &[u8],
) -> Result<(), String> {
    let mut offset = 0;
    while offset < data.len() {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match channel.write(&data[offset..]) {
            Ok(0) => return Err("SSH channel closed while forwarding data".to_string()),
            Ok(written) => offset += written,
            Err(error) if is_retryable_io_error(&error) => {
                if !wait_before_io_retry(stop) {
                    return Ok(());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn write_all_socket(
    stream: &mut TcpStream,
    stop: &Arc<AtomicBool>,
    data: &[u8],
) -> Result<(), String> {
    let mut offset = 0;
    while offset < data.len() {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match stream.write(&data[offset..]) {
            Ok(0) => return Err("Local socket closed while forwarding data".to_string()),
            Ok(written) => offset += written,
            Err(error) if is_retryable_io_error(&error) => {
                if !wait_before_io_retry(stop) {
                    return Ok(());
                }
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
                Err(error) if is_retryable_io_error(&error) => {
                    if !wait_before_io_retry(&stop_a) {
                        let _ = channel_write.send_eof();
                        return Ok(());
                    }
                }
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
                Ok(read) => write_all_socket(&mut socket_write, &stop_b, &buf[..read])?,
                Err(error) if is_retryable_io_error(&error) => {
                    if !wait_before_io_retry(&stop_b) {
                        let _ = socket_write.shutdown(Shutdown::Both);
                        return Ok(());
                    }
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    });

    let res_a = a
        .join()
        .map_err(|_| "Forwarding thread panicked".to_string())?;
    let res_b = b
        .join()
        .map_err(|_| "Forwarding thread panicked".to_string())?;
    let _ = channel_close.close();
    let _ = channel_close.wait_close();
    res_a?;
    res_b?;
    Ok(())
}

fn drain_written_prefix(buffer: &mut Vec<u8>, written: usize) {
    if written >= buffer.len() {
        buffer.clear();
    } else {
        buffer.drain(..written);
    }
}

fn bridge_streams_dedicated_session(
    mut socket: TcpStream,
    session: &Session,
    mut channel: ssh2::Channel,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    socket.set_nodelay(true).ok();
    socket
        .set_nonblocking(true)
        .map_err(|e| format!("Failed to set local socket non-blocking: {}", e))?;
    session.set_blocking(false);

    let result = (|| -> Result<(), String> {
        let mut socket_to_channel = Vec::with_capacity(8192);
        let mut channel_to_socket = Vec::with_capacity(8192);
        let mut socket_read_eof = false;
        let mut channel_read_eof = false;
        let mut buf = [0u8; 8192];

        loop {
            if stop.load(Ordering::Relaxed) {
                let _ = channel.send_eof();
                let _ = socket.shutdown(Shutdown::Both);
                return Ok(());
            }

            let mut progressed = false;

            if !socket_read_eof && socket_to_channel.is_empty() {
                match socket.read(&mut buf) {
                    Ok(0) => {
                        socket_read_eof = true;
                        let _ = channel.send_eof();
                        progressed = true;
                    }
                    Ok(read) => {
                        socket_to_channel.extend_from_slice(&buf[..read]);
                        progressed = true;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error.to_string()),
                }
            }

            while !socket_to_channel.is_empty() {
                match channel.write(&socket_to_channel) {
                    Ok(0) => return Err("SSH channel closed while forwarding data".to_string()),
                    Ok(written) => {
                        drain_written_prefix(&mut socket_to_channel, written);
                        progressed = true;
                    }
                    Err(error) if is_retryable_io_error(&error) => break,
                    Err(error) => return Err(error.to_string()),
                }
            }

            if !channel_read_eof && channel_to_socket.is_empty() {
                match channel.read(&mut buf) {
                    Ok(0) => {
                        channel_read_eof = true;
                        let _ = socket.shutdown(Shutdown::Write);
                        progressed = true;
                    }
                    Ok(read) => {
                        channel_to_socket.extend_from_slice(&buf[..read]);
                        progressed = true;
                    }
                    Err(error) if is_retryable_io_error(&error) => {}
                    Err(error) => return Err(error.to_string()),
                }
            }

            while !channel_to_socket.is_empty() {
                match socket.write(&channel_to_socket) {
                    Ok(0) => return Err("Local socket closed while forwarding data".to_string()),
                    Ok(written) => {
                        drain_written_prefix(&mut channel_to_socket, written);
                        progressed = true;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error.to_string()),
                }
            }

            if socket_read_eof
                && channel_read_eof
                && socket_to_channel.is_empty()
                && channel_to_socket.is_empty()
            {
                return Ok(());
            }

            if !progressed {
                thread::sleep(SSH_IO_RETRY_BACKOFF);
            }
        }
    })();

    session.set_blocking(true);
    set_session_timeout(session, SSH_IO_TIMEOUT);
    let _ = channel.close();
    let _ = channel.wait_close();
    result
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

    let initial_session = match open_authenticated_session(&resolved) {
        Ok(session) => session,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };

    if let Err(error) = open_direct_tcpip_channel(&initial_session, &target_host, target_port)
        .and_then(|mut channel| {
            let _ = channel.close();
            Ok(())
        })
    {
        let _ = startup.send(Err(error));
        return;
    }

    let session_pool = Arc::new(SessionPool::with_initial_session(
        resolved.clone(),
        initial_session,
    ));

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
                let app_for_client = app.clone();
                let tunnel_id = record.id.clone();
                let target_host_for_client = target_host.clone();
                let session_pool_for_client = session_pool.clone();
                active_clients.fetch_add(1, Ordering::Relaxed);
                let active_clients_for_worker = active_clients.clone();
                thread::spawn(move || {
                    let result = (|| -> Result<(), String> {
                        let session = session_pool_for_client.acquire()?;
                        let channel = open_direct_tcpip_channel(
                            &session,
                            &target_host_for_client,
                            target_port,
                        )?;
                        let result = bridge_streams_dedicated_session(
                            socket,
                            &session,
                            channel,
                            stop_for_client,
                        );
                        if result.is_ok() {
                            session_pool_for_client.release(session);
                        }
                        result
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
    socket
        .read_exact(&mut port_buf)
        .map_err(|e| e.to_string())?;
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
    session_pool: Arc<SessionPool>,
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
        socket.write_all(&[5, 0xff]).map_err(|e| e.to_string())?;
        return Err("SOCKS5 client does not support no-auth mode".to_string());
    }
    socket.write_all(&[5, 0]).map_err(|e| e.to_string())?;
    let (target_host, target_port) = read_socks_address(&mut socket)?;
    let session = session_pool.acquire()?;
    match open_direct_tcpip_channel(&session, &target_host, target_port) {
        Ok(channel) => {
            write_socks_success(&mut socket)?;
            let result = bridge_streams_dedicated_session(socket, &session, channel, stop);
            if result.is_ok() {
                session_pool.release(session);
            }
            result
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
    let initial_session = match open_authenticated_session(&resolved) {
        Ok(session) => session,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    let session_pool = Arc::new(SessionPool::with_initial_session(
        resolved.clone(),
        initial_session,
    ));
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
                let app_for_client = app.clone();
                let tunnel_id = record.id.clone();
                let session_pool_for_client = session_pool.clone();
                let active_clients_for_worker = active_clients.clone();
                active_clients_for_worker.fetch_add(1, Ordering::Relaxed);
                thread::spawn(move || {
                    let result =
                        handle_dynamic_client(socket, session_pool_for_client, stop_for_client);
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
    let (mut listener, bound_port) = match with_session_connect_timeout(&session, |session| {
        session
            .channel_forward_listen(remote_port, Some(&remote_host), Some(16))
            .map_err(|e| e.to_string())
    }) {
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
                            .map_err(|e| {
                                format!("Failed to connect to local target {}: {}", addr, e)
                            })?;
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
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = stop.clone();
    let initial_session = open_authenticated_session(&resolved)?;
    let session_pool = Arc::new(SessionPool::with_initial_session(resolved, initial_session));
    let session_pool_for_thread = session_pool.clone();
    let handle = thread::spawn(move || -> Result<(), String> {
        let (socket, _) = listener.accept().map_err(|e| e.to_string())?;
        handle_dynamic_client(socket, session_pool_for_thread, stop_for_thread)
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
    client
        .read_exact(&mut response)
        .map_err(|e| e.to_string())?;
    stop.store(true, Ordering::Relaxed);
    let thread_result = handle
        .join()
        .map_err(|_| "Dynamic probe thread panicked".to_string())?;
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

fn connect_internal(
    app: AppHandle,
    id: String,
    emit_failure_event: bool,
) -> Result<SshTunnelRuntimeView, String> {
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
pub fn ssh_tunnel_groups_list() -> Result<Vec<SshTunnelGroupView>, String> {
    let mut groups = load_state()?.groups;
    sort_groups(&mut groups);
    Ok(groups.iter().map(to_group_view).collect())
}

#[tauri::command]
pub fn ssh_tunnel_group_upsert(
    app: AppHandle,
    input: SshTunnelGroupUpsertInput,
) -> Result<SshTunnelGroupView, String> {
    let group = mutate_state(|state| {
        let editing_id = input.id.as_deref();
        if editing_id == Some(DEFAULT_TUNNEL_GROUP_ID) {
            return Err("The default environment group cannot be renamed".to_string());
        }
        let name = validate_group_name(&state.groups, &input.name, editing_id)?;
        let now = now_ts();
        if let Some(id) = editing_id {
            let group = state
                .groups
                .iter_mut()
                .find(|group| group.id == id)
                .ok_or_else(|| "Environment group not found".to_string())?;
            group.name = name;
            group.updated_at = now;
            return Ok(group.clone());
        }

        let group = SshTunnelGroupRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at: now,
            updated_at: now,
            is_default: false,
        };
        state.groups.push(group.clone());
        Ok(group)
    })?;
    emit_tunnels_updated(&app);
    Ok(to_group_view(&group))
}

#[tauri::command]
pub fn ssh_tunnel_group_delete(app: AppHandle, id: String) -> Result<(), String> {
    if id == DEFAULT_TUNNEL_GROUP_ID {
        return Err("The default environment group cannot be deleted".to_string());
    }
    mutate_state(|state| {
        let group_index = state
            .groups
            .iter()
            .position(|group| group.id == id)
            .ok_or_else(|| "Environment group not found".to_string())?;
        state.groups.remove(group_index);
        for tunnel in &mut state.tunnels {
            if tunnel.group_id == id {
                tunnel.group_id = DEFAULT_TUNNEL_GROUP_ID.to_string();
                tunnel.updated_at = now_ts();
            }
        }
        Ok(())
    })?;
    emit_tunnels_updated(&app);
    Ok(())
}

#[tauri::command]
pub fn ssh_tunnels_list() -> Result<Vec<SshTunnelView>, String> {
    let mut records = load_records()?;
    sort_tunnels(&mut records);
    Ok(records.iter().map(to_view).collect())
}

#[tauri::command]
pub async fn ssh_tunnel_upsert(
    app: AppHandle,
    input: SshTunnelUpsertInput,
) -> Result<SshTunnelView, String> {
    let state = load_state()?;
    let existing = input.id.as_ref().and_then(|id| {
        state
            .tunnels
            .iter()
            .find(|record| record.id == *id)
            .cloned()
    });
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
        group_id: normalize_group_id(input.group_id.as_deref(), &state.groups),
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
            remote_bind_host: input
                .forward
                .remote_bind_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string()),
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
        created_at: existing
            .as_ref()
            .map(|record| record.created_at)
            .unwrap_or(now),
        updated_at: now,
        last_connected_at: existing
            .as_ref()
            .and_then(|record| record.last_connected_at),
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
            crate::secrets::save_secret(app.clone(), secret_key.clone(), password.to_string())
                .await?;
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
pub fn ssh_tunnel_group_connect(
    app: AppHandle,
    group_id: String,
) -> Result<SshTunnelBatchOperationResult, String> {
    let state = load_state()?;
    let group = state
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "Environment group not found".to_string())?;

    let target_group_id = if group_id == DEFAULT_TUNNEL_GROUP_ID {
        DEFAULT_TUNNEL_GROUP_ID.to_string()
    } else {
        group_id.clone()
    };

    let tunnels: Vec<SshTunnelRecord> = state
        .tunnels
        .iter()
        .filter(|t| {
            let normalized = normalize_group_id(Some(&t.group_id), &state.groups);
            normalized == target_group_id
        })
        .cloned()
        .collect();

    let total_count = tunnels.len();
    let group_name = if group.is_default {
        DEFAULT_TUNNEL_GROUP_NAME.to_string()
    } else {
        group.name.clone()
    };

    let mut success_count = 0;
    let mut skipped_count = 0;
    let mut failures: Vec<SshTunnelBatchFailureDetail> = Vec::new();

    let manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    let running_ids: HashSet<String> = manager.keys().cloned().collect();
    drop(manager);

    for tunnel in tunnels {
        if running_ids.contains(&tunnel.id) {
            skipped_count += 1;
            continue;
        }

        match connect_internal(app.clone(), tunnel.id.clone(), false) {
            Ok(_) => success_count += 1,
            Err(error) => {
                failures.push(SshTunnelBatchFailureDetail {
                    tunnel_id: tunnel.id.clone(),
                    tunnel_name: tunnel.name.clone(),
                    error,
                });
            }
        }
    }

    emit_tunnels_updated(&app);

    Ok(SshTunnelBatchOperationResult {
        operation: "connect".to_string(),
        group_id,
        group_name,
        success_count,
        failed_count: failures.len(),
        skipped_count,
        total_count,
        failures,
    })
}

#[tauri::command]
pub fn ssh_tunnel_group_disconnect(
    app: AppHandle,
    group_id: String,
) -> Result<SshTunnelBatchOperationResult, String> {
    let state = load_state()?;
    let group = state
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "Environment group not found".to_string())?;

    let target_group_id = if group_id == DEFAULT_TUNNEL_GROUP_ID {
        DEFAULT_TUNNEL_GROUP_ID.to_string()
    } else {
        group_id.clone()
    };

    let tunnels: Vec<SshTunnelRecord> = state
        .tunnels
        .iter()
        .filter(|t| {
            let normalized = normalize_group_id(Some(&t.group_id), &state.groups);
            normalized == target_group_id
        })
        .cloned()
        .collect();

    let total_count = tunnels.len();
    let group_name = if group.is_default {
        DEFAULT_TUNNEL_GROUP_NAME.to_string()
    } else {
        group.name.clone()
    };

    let mut success_count = 0;
    let mut skipped_count = 0;
    let mut failures: Vec<SshTunnelBatchFailureDetail> = Vec::new();

    let manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    let running_ids: HashSet<String> = manager.keys().cloned().collect();
    drop(manager);

    for tunnel in tunnels {
        if !running_ids.contains(&tunnel.id) {
            skipped_count += 1;
            continue;
        }

        match disconnect_runtime(&tunnel.id) {
            Ok(_) => success_count += 1,
            Err(error) => {
                failures.push(SshTunnelBatchFailureDetail {
                    tunnel_id: tunnel.id.clone(),
                    tunnel_name: tunnel.name.clone(),
                    error,
                });
            }
        }
    }

    emit_tunnels_updated(&app);

    Ok(SshTunnelBatchOperationResult {
        operation: "disconnect".to_string(),
        group_id,
        group_name,
        success_count,
        failed_count: failures.len(),
        skipped_count,
        total_count,
        failures,
    })
}

#[tauri::command]
pub fn ssh_tunnel_probe_draft(
    input: SshTunnelProbeDraftInput,
) -> Result<SshTunnelProbeResult, String> {
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
    let mut records = load_records()?;
    sort_tunnels(&mut records);
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

fn snapshot_state() -> Result<SshTunnelsSnapshot, String> {
    let mut state = load_state()?;
    sort_groups(&mut state.groups);
    sort_tunnels(&mut state.tunnels);
    let runtime = ssh_tunnels_refresh_status()?;
    Ok(SshTunnelsSnapshot {
        groups: state.groups.iter().map(to_group_view).collect(),
        tunnels: state.tunnels.iter().map(to_view).collect(),
        runtime,
    })
}

#[tauri::command]
pub fn ssh_tunnels_snapshot() -> Result<SshTunnelsSnapshot, String> {
    snapshot_state()
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
    use std::time::Instant;

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

    fn sample_record(group_id: &str) -> SshTunnelRecord {
        SshTunnelRecord {
            id: "tunnel-1".to_string(),
            name: "Local tunnel".to_string(),
            group_id: group_id.to_string(),
            source_kind: SshTunnelSourceKind::SavedHost,
            saved_host_name: Some("dev".to_string()),
            custom: None,
            forward: local_forward(),
            auto_connect: false,
            created_at: 1,
            updated_at: 1,
            last_connected_at: None,
            last_error: None,
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
            group_id: None,
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
            group_id: None,
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

    #[test]
    fn resolve_host_key_name_prefers_alias_then_host_key_alias() {
        assert_eq!(
            resolve_host_key_name("dev-box", "10.1.3.2", None),
            "dev-box"
        );
        assert_eq!(
            resolve_host_key_name("dev-box", "10.1.3.2", Some("cluster-entry")),
            "cluster-entry"
        );
    }

    #[test]
    fn known_hosts_paths_option_supports_multiple_entries_and_none() {
        let paths = known_hosts_paths_from_option(Some("~/known_a ~/.ssh/known_b"))
            .expect("known_hosts paths should parse");
        assert_eq!(paths.len(), 2);
        assert!(paths[0].to_string_lossy().contains("known_a"));
        assert!(paths[1].to_string_lossy().contains(".ssh/known_b"));

        let none_paths =
            known_hosts_paths_from_option(Some("none")).expect("none should disable user paths");
        assert!(none_paths.is_empty());
    }

    #[test]
    fn resolved_key_paths_preserve_order() {
        let paths = resolved_key_paths(&["~/first_key".to_string(), "/tmp/second_key".to_string()]);
        assert_eq!(paths.len(), 2);
        assert!(paths[0].to_string_lossy().contains("first_key"));
        assert!(paths[1].to_string_lossy().ends_with("/tmp/second_key"));
    }

    #[test]
    fn normalize_state_injects_default_group() {
        let mut state = SshTunnelState {
            groups: vec![SshTunnelGroupRecord {
                id: "dev".to_string(),
                name: "Development".to_string(),
                created_at: 10,
                updated_at: 10,
                is_default: false,
            }],
            tunnels: vec![sample_record("")],
        };

        normalize_state(&mut state);

        assert!(state
            .groups
            .iter()
            .any(|group| group.id == DEFAULT_TUNNEL_GROUP_ID && group.is_default));
        assert_eq!(state.tunnels[0].group_id, DEFAULT_TUNNEL_GROUP_ID);
    }

    #[test]
    fn normalize_state_falls_back_invalid_group_ids() {
        let mut state = SshTunnelState {
            groups: vec![
                default_group_record(),
                SshTunnelGroupRecord {
                    id: "test".to_string(),
                    name: "Testing".to_string(),
                    created_at: 20,
                    updated_at: 20,
                    is_default: false,
                },
            ],
            tunnels: vec![sample_record("missing")],
        };

        normalize_state(&mut state);

        assert_eq!(state.tunnels[0].group_id, DEFAULT_TUNNEL_GROUP_ID);
    }

    #[test]
    fn parse_state_payload_rejects_encrypted_wrapper() {
        let wrapped = serde_json::json!({
            "is_encrypted": true,
            "data": "ciphertext",
        });

        let parsed = parse_state_payload(&wrapped.to_string());

        assert!(parsed.is_err());
    }

    #[test]
    fn parse_state_payload_accepts_structured_state_object() {
        let payload = serde_json::json!({
            "groups": [
                {
                    "id": DEFAULT_TUNNEL_GROUP_ID,
                    "name": DEFAULT_TUNNEL_GROUP_NAME,
                    "created_at": 1,
                    "updated_at": 1,
                    "is_default": true
                },
                {
                    "id": "dev",
                    "name": "Development",
                    "created_at": 2,
                    "updated_at": 2,
                    "is_default": false
                }
            ],
            "tunnels": [
                {
                    "id": "tunnel-1",
                    "name": "Local tunnel",
                    "group_id": "dev",
                    "source_kind": "saved_host",
                    "saved_host_name": "dev",
                    "forward": {
                        "mode": "local",
                        "local_bind_host": "127.0.0.1",
                        "local_port": 5432,
                        "remote_bind_host": "127.0.0.1",
                        "target_host": "127.0.0.1",
                        "target_port": 5432
                    },
                    "auto_connect": false,
                    "created_at": 1,
                    "updated_at": 1
                }
            ]
        });

        let parsed = parse_state_payload(&payload.to_string()).expect("state should parse");

        assert_eq!(parsed.groups.len(), 2);
        assert_eq!(parsed.tunnels.len(), 1);
        assert_eq!(parsed.tunnels[0].group_id, "dev");
    }

    #[test]
    fn retryable_io_errors_cover_would_block_and_timeout() {
        assert!(is_retryable_io_error(&io::Error::from(
            io::ErrorKind::WouldBlock
        )));
        assert!(is_retryable_io_error(&io::Error::from(
            io::ErrorKind::TimedOut
        )));
        assert!(!is_retryable_io_error(&io::Error::from(
            io::ErrorKind::ConnectionReset
        )));
    }

    #[test]
    fn wait_before_io_retry_obeys_stop_signal() {
        let stop = Arc::new(AtomicBool::new(false));
        let started_at = Instant::now();
        assert!(wait_before_io_retry(&stop));
        assert!(started_at.elapsed() >= SSH_IO_RETRY_BACKOFF);

        stop.store(true, Ordering::Relaxed);
        let stopped_at = Instant::now();
        assert!(!wait_before_io_retry(&stop));
        assert!(stopped_at.elapsed() < SSH_IO_RETRY_BACKOFF);
    }
}
