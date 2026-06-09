use super::{
    open_authenticated_session, prepare_session_for_reuse, snapshot_state, DEFAULT_TUNNEL_GROUP_ID,
    DEFAULT_TUNNEL_GROUP_NAME, LOCAL_BIND_HOST, PASSWORD_SECRET_PREFIX, REMOTE_BIND_HOST,
    SSH_SESSION_POOL_MAX_IDLE, SSH_TUNNELS_UPDATED_EVENT, SSH_TUNNEL_CONNECT_FAILED_EVENT,
};
use crate::{crypto, get_data_dir, messages};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ssh2::Session;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

pub(in crate::ssh_tunnels) static RECORDS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
pub(in crate::ssh_tunnels) static RUNTIME_MANAGER: OnceLock<Mutex<HashMap<String, RunningTunnel>>> =
    OnceLock::new();
pub(in crate::ssh_tunnels) static RECONNECT_RECONCILE_RUNNING: AtomicBool = AtomicBool::new(false);
pub(in crate::ssh_tunnels) static LAST_RECONNECT_RECONCILE_AT: AtomicU64 = AtomicU64::new(0);

pub(in crate::ssh_tunnels) fn records_lock() -> &'static Mutex<()> {
    RECORDS_LOCK.get_or_init(|| Mutex::new(()))
}

pub(in crate::ssh_tunnels) fn runtime_manager() -> &'static Mutex<HashMap<String, RunningTunnel>> {
    RUNTIME_MANAGER.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(in crate::ssh_tunnels) fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(in crate::ssh_tunnels) fn default_group_name() -> String {
    DEFAULT_TUNNEL_GROUP_NAME.to_string()
}

pub(in crate::ssh_tunnels) fn default_group_id() -> String {
    DEFAULT_TUNNEL_GROUP_ID.to_string()
}

pub(in crate::ssh_tunnels) fn default_auto_reconnect() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub(in crate::ssh_tunnels) struct EncryptedBlob {
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
    Reconnecting,
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
pub(in crate::ssh_tunnels) struct SshTunnelRecord {
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
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub last_connected_at: Option<u64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(in crate::ssh_tunnels) struct SshTunnelGroupRecord {
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
pub(in crate::ssh_tunnels) struct SshTunnelState {
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
    pub auto_reconnect: bool,
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
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshTunnelGroupUpsertInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
}

pub type SshTunnelProbeDraftInput = SshTunnelUpsertInput;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(in crate::ssh_tunnels) struct SshTunnelFailureEvent {
    pub id: String,
    pub name: String,
    pub error: String,
    pub auto_connect: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(in crate::ssh_tunnels) struct SshTunnelWindowReconnectDoneEvent {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
}

#[derive(Debug, Clone)]
pub(in crate::ssh_tunnels) struct RuntimeState {
    pub(in crate::ssh_tunnels) status: SshTunnelStatus,
    pub(in crate::ssh_tunnels) mode: SshTunnelForwardMode,
    pub(in crate::ssh_tunnels) summary: String,
    pub(in crate::ssh_tunnels) resolved_server_host: Option<String>,
    pub(in crate::ssh_tunnels) listening_addr: Option<String>,
    pub(in crate::ssh_tunnels) last_error: Option<String>,
}

pub(in crate::ssh_tunnels) struct RunningTunnel {
    pub(in crate::ssh_tunnels) stop: Arc<AtomicBool>,
    pub(in crate::ssh_tunnels) active_clients: Arc<AtomicUsize>,
    pub(in crate::ssh_tunnels) state: Arc<Mutex<RuntimeState>>,
    pub(in crate::ssh_tunnels) join: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub(in crate::ssh_tunnels) enum ResolvedAuth {
    Password(String),
    Key {
        paths: Vec<PathBuf>,
        allow_agent_fallback: bool,
    },
    Agent,
}

#[derive(Debug, Clone)]
pub(in crate::ssh_tunnels) struct ResolvedSshConfig {
    pub(in crate::ssh_tunnels) host: String,
    pub(in crate::ssh_tunnels) port: u16,
    pub(in crate::ssh_tunnels) user: String,
    pub(in crate::ssh_tunnels) auth: ResolvedAuth,
    pub(in crate::ssh_tunnels) source_label: String,
    pub(in crate::ssh_tunnels) host_key_name: String,
    pub(in crate::ssh_tunnels) known_hosts_paths: Vec<PathBuf>,
}

#[derive(Debug, Default, Clone)]
pub(in crate::ssh_tunnels) struct ParsedSshAlias {
    pub(in crate::ssh_tunnels) host_name: Option<String>,
    pub(in crate::ssh_tunnels) user: Option<String>,
    pub(in crate::ssh_tunnels) port: Option<u16>,
    pub(in crate::ssh_tunnels) identity_files: Vec<String>,
    pub(in crate::ssh_tunnels) identities_only: Option<bool>,
    pub(in crate::ssh_tunnels) host_key_alias: Option<String>,
    pub(in crate::ssh_tunnels) user_known_hosts_file: Option<String>,
    pub(in crate::ssh_tunnels) proxy_command: Option<String>,
    pub(in crate::ssh_tunnels) proxy_jump: Option<String>,
}

#[derive(Debug, Clone)]
pub(in crate::ssh_tunnels) struct StartupSuccess {
    pub(in crate::ssh_tunnels) listening_addr: Option<String>,
    pub(in crate::ssh_tunnels) resolved_server_host: String,
}

/// Result of a tunnel startup attempt, sent over the startup channel.
#[derive(Debug)]
pub(in crate::ssh_tunnels) enum StartupResult {
    Connected(StartupSuccess),
    Failed(String),
}

pub(in crate::ssh_tunnels) struct SessionPool {
    resolved: ResolvedSshConfig,
    idle: Mutex<Vec<Session>>,
    max_idle: usize,
}

impl SessionPool {
    pub(in crate::ssh_tunnels) fn with_initial_session(
        resolved: ResolvedSshConfig,
        session: Session,
    ) -> Self {
        Self {
            resolved,
            idle: Mutex::new(vec![session]),
            max_idle: SSH_SESSION_POOL_MAX_IDLE,
        }
    }

    pub(in crate::ssh_tunnels) fn acquire(&self) -> Result<Session, String> {
        loop {
            let idle_session = {
                let mut idle = self.idle.lock().map_err(|e| e.to_string())?;
                idle.pop()
            };

            match idle_session {
                Some(session) if prepare_session_for_reuse(&session).is_ok() => {
                    return Ok(session);
                }
                Some(_) => continue,
                None => return open_authenticated_session(&self.resolved),
            }
        }
    }

    pub(in crate::ssh_tunnels) fn health_check(&self) -> Result<(), String> {
        let session = self.acquire()?;
        self.release(session);
        Ok(())
    }

    pub(in crate::ssh_tunnels) fn release(&self, session: Session) {
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

pub(in crate::ssh_tunnels) fn get_tunnels_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    let dir = data_dir.join("data").join("ssh_tunnels");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.enc.json"))
}

pub(in crate::ssh_tunnels) fn default_group_record() -> SshTunnelGroupRecord {
    let now = now_ts();
    SshTunnelGroupRecord {
        id: DEFAULT_TUNNEL_GROUP_ID.to_string(),
        name: DEFAULT_TUNNEL_GROUP_NAME.to_string(),
        created_at: now,
        updated_at: now,
        is_default: true,
    }
}

pub(in crate::ssh_tunnels) fn is_reserved_default_group_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "default group" | "默认分组"
    )
}

pub(in crate::ssh_tunnels) fn canonical_group_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

pub(in crate::ssh_tunnels) fn normalize_group_id(
    group_id: Option<&str>,
    groups: &[SshTunnelGroupRecord],
) -> String {
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

pub(in crate::ssh_tunnels) fn normalize_state(state: &mut SshTunnelState) {
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

pub(in crate::ssh_tunnels) fn parse_state_payload(content: &str) -> Result<SshTunnelState, String> {
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

pub(in crate::ssh_tunnels) fn load_state_unlocked() -> Result<SshTunnelState, String> {
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

pub(in crate::ssh_tunnels) fn write_state_unlocked(state: &SshTunnelState) -> Result<(), String> {
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

pub(in crate::ssh_tunnels) fn load_state() -> Result<SshTunnelState, String> {
    let _guard = records_lock().lock().map_err(|e| e.to_string())?;
    load_state_unlocked()
}

pub(in crate::ssh_tunnels) fn mutate_state<T>(
    mutator: impl FnOnce(&mut SshTunnelState) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = records_lock().lock().map_err(|e| e.to_string())?;
    let mut state = load_state_unlocked()?;
    let result = mutator(&mut state)?;
    write_state_unlocked(&state)?;
    Ok(result)
}

pub(in crate::ssh_tunnels) fn load_records() -> Result<Vec<SshTunnelRecord>, String> {
    Ok(load_state()?.tunnels)
}

pub(in crate::ssh_tunnels) fn mutate_records<T>(
    mutator: impl FnOnce(&mut Vec<SshTunnelRecord>) -> Result<T, String>,
) -> Result<T, String> {
    mutate_state(|state| mutator(&mut state.tunnels))
}

pub(in crate::ssh_tunnels) fn secret_key_for_tunnel(id: &str) -> String {
    format!("{PASSWORD_SECRET_PREFIX}{id}")
}

pub(in crate::ssh_tunnels) fn password_exists(id: &str) -> bool {
    crate::secrets::get_secret(&secret_key_for_tunnel(id))
        .ok()
        .flatten()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

pub(in crate::ssh_tunnels) fn password_for_tunnel(id: &str) -> Result<Option<String>, String> {
    crate::secrets::get_secret(&secret_key_for_tunnel(id))
}

pub(in crate::ssh_tunnels) fn to_view(record: &SshTunnelRecord) -> SshTunnelView {
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
        auto_reconnect: record.auto_reconnect,
        created_at: record.created_at,
        updated_at: record.updated_at,
        last_connected_at: record.last_connected_at,
        last_error: record.last_error.clone(),
    }
}

pub(in crate::ssh_tunnels) fn to_group_view(group: &SshTunnelGroupRecord) -> SshTunnelGroupView {
    SshTunnelGroupView {
        id: group.id.clone(),
        name: group.name.clone(),
        created_at: group.created_at,
        updated_at: group.updated_at,
        is_default: group.is_default,
    }
}

pub(in crate::ssh_tunnels) fn tunnel_summary(forward: &SshTunnelForwardConfig) -> String {
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

pub(in crate::ssh_tunnels) fn default_runtime_view(
    record: &SshTunnelRecord,
) -> SshTunnelRuntimeView {
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

pub(in crate::ssh_tunnels) fn runtime_view(
    record: &SshTunnelRecord,
    running: Option<&RunningTunnel>,
) -> SshTunnelRuntimeView {
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

pub(in crate::ssh_tunnels) fn update_runtime_state(
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

pub(in crate::ssh_tunnels) fn emit_tunnels_updated(app: &AppHandle) {
    if let Ok(snapshot) = snapshot_state() {
        let _ = app.emit(SSH_TUNNELS_UPDATED_EVENT, snapshot);
    } else {
        let _ = app.emit(SSH_TUNNELS_UPDATED_EVENT, ());
    }
}

pub(in crate::ssh_tunnels) fn emit_connect_failed(
    app: &AppHandle,
    record: &SshTunnelRecord,
    error: &str,
) {
    record_tunnel_failure(app, record, error, "auto-connect");
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

pub(in crate::ssh_tunnels) fn load_record_by_id(
    id: &str,
) -> Result<Option<SshTunnelRecord>, String> {
    Ok(load_records()?.into_iter().find(|record| record.id == id))
}

pub(in crate::ssh_tunnels) fn record_tunnel_failure(
    app: &AppHandle,
    record: &SshTunnelRecord,
    error: &str,
    category: &str,
) {
    let title = match category {
        "health-check" => {
            messages::localized("SSH 隧道健康检查失败", "SSH tunnel health check failed")
        }
        "auto-reconnect" => {
            messages::localized("SSH 隧道自动重连失败", "SSH tunnel auto-reconnect failed")
        }
        _ => messages::localized("SSH 隧道自动连接失败", "SSH tunnel auto-connect failed"),
    };
    messages::record_message_silent(
        app,
        messages::MessageCreateInput {
            source: "ssh_tunnels".to_string(),
            category: category.to_string(),
            severity: "error".to_string(),
            title,
            summary: Some(format!("{}: {}", record.name, error)),
            detail: Some(error.to_string()),
            dedupe_key: Some(format!("ssh-tunnels:{}:{}", category, record.id)),
            target: Some(messages::MessageTarget {
                tab: "ssh-tunnels".to_string(),
                section: None,
                entity_id: Some(record.id.clone()),
            }),
            metadata: Some(json!({
                "tunnel_id": record.id,
                "tunnel_name": record.name,
                "auto_connect": record.auto_connect,
                "auto_reconnect": record.auto_reconnect,
                "category": category,
            })),
        },
    );
}

pub(in crate::ssh_tunnels) fn record_group_operation_failure(
    app: &AppHandle,
    group_id: &str,
    group_name: &str,
    operation: &str,
    total_count: usize,
    failures: &[SshTunnelBatchFailureDetail],
) {
    if failures.is_empty() {
        return;
    }
    let is_zh = messages::current_language_is_zh();
    let op_label = if operation == "disconnect" {
        if is_zh {
            "断开"
        } else {
            "disconnect"
        }
    } else if is_zh {
        "连接"
    } else {
        "connect"
    };
    let detail = failures
        .iter()
        .map(|failure| {
            format!(
                "{} ({}): {}",
                failure.tunnel_name, failure.tunnel_id, failure.error
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    messages::record_message_silent(
        app,
        messages::MessageCreateInput {
            source: "ssh_tunnels".to_string(),
            category: format!("group_{}", operation),
            severity: "error".to_string(),
            title: if operation == "disconnect" {
                messages::localized(
                    "SSH 隧道分组断开部分失败",
                    "SSH tunnel group disconnect partially failed",
                )
            } else {
                messages::localized(
                    "SSH 隧道分组连接部分失败",
                    "SSH tunnel group connect partially failed",
                )
            },
            summary: Some(if is_zh {
                format!(
                    "{}: {}/{} 个隧道{}失败",
                    group_name,
                    failures.len(),
                    total_count,
                    op_label
                )
            } else {
                format!(
                    "{}: {}/{} tunnel(s) failed to {}",
                    group_name,
                    failures.len(),
                    total_count,
                    op_label
                )
            }),
            detail: Some(detail),
            dedupe_key: Some(format!("ssh-tunnels:group-{}:{}", operation, group_id)),
            target: Some(messages::MessageTarget {
                tab: "ssh-tunnels".to_string(),
                section: None,
                entity_id: Some(group_id.to_string()),
            }),
            metadata: Some(json!({
                "group_id": group_id,
                "group_name": group_name,
                "operation": operation,
                "total_count": total_count,
                "failed_count": failures.len(),
                "failures": failures,
            })),
        },
    );
}

pub(in crate::ssh_tunnels) fn update_record_connection_success(id: &str) -> Result<(), String> {
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

pub(in crate::ssh_tunnels) fn update_record_error(id: &str, error: &str) -> Result<(), String> {
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

pub(in crate::ssh_tunnels) fn clear_record_error(id: &str) -> Result<(), String> {
    mutate_records(|records| {
        let record = records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| "Tunnel not found".to_string())?;
        record.last_error = None;
        Ok(())
    })
}

pub(in crate::ssh_tunnels) fn validate_group_name(
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

pub(in crate::ssh_tunnels) fn sort_groups(groups: &mut [SshTunnelGroupRecord]) {
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

pub(in crate::ssh_tunnels) fn sort_tunnels(tunnels: &mut [SshTunnelRecord]) {
    tunnels.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
}
