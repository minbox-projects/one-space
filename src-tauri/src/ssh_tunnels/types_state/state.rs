use crate::messages;
use crate::ssh_tunnels::{
    open_authenticated_session, prepare_session_for_reuse, snapshot_state, DEFAULT_TUNNEL_GROUP_ID,
    DEFAULT_TUNNEL_GROUP_NAME, LOCAL_BIND_HOST, PASSWORD_SECRET_PREFIX, REMOTE_BIND_HOST,
    SSH_SESSION_POOL_MAX_IDLE, SSH_TUNNELS_UPDATED_EVENT, SSH_TUNNEL_CONNECT_FAILED_EVENT,
};
use serde_json::json;
use ssh2::Session;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

use super::persistence::{load_records, mutate_records};
use super::types::{
    ResolvedSshConfig, RunningTunnel, RuntimeState, SshCommonPortRecord, SshCommonPortView,
    SshTunnelBatchFailureDetail, SshTunnelFailureEvent, SshTunnelForwardConfig,
    SshTunnelForwardMode, SshTunnelGroupRecord, SshTunnelGroupView, SshTunnelRecord,
    SshTunnelRuntimeView, SshTunnelState, SshTunnelStatus, SshTunnelView,
};

pub(in crate::ssh_tunnels) static RECORDS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
pub(in crate::ssh_tunnels) static RUNTIME_MANAGER: OnceLock<Mutex<HashMap<String, RunningTunnel>>> =
    OnceLock::new();
/// Process-memory set of tunnels the user wants running. Cleared on every stop
/// path so a manual disconnect is never resurrected by the watchdog.
pub(in crate::ssh_tunnels) static DESIRED_TUNNEL_IDS: OnceLock<Mutex<HashSet<String>>> =
    OnceLock::new();

pub(in crate::ssh_tunnels) fn records_lock() -> &'static Mutex<()> {
    RECORDS_LOCK.get_or_init(|| Mutex::new(()))
}

fn desired_tunnel_id_set() -> &'static Mutex<HashSet<String>> {
    DESIRED_TUNNEL_IDS.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(in crate::ssh_tunnels) fn mark_tunnel_desired(id: &str) {
    if let Ok(mut ids) = desired_tunnel_id_set().lock() {
        ids.insert(id.to_string());
    }
}

pub(in crate::ssh_tunnels) fn clear_tunnel_desired(id: &str) {
    if let Ok(mut ids) = desired_tunnel_id_set().lock() {
        ids.remove(id);
    }
}

pub(in crate::ssh_tunnels) fn desired_tunnel_ids() -> HashSet<String> {
    desired_tunnel_id_set()
        .lock()
        .map(|ids| ids.clone())
        .unwrap_or_default()
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

    let mut seen_port_ids = HashSet::new();
    let mut normalized_ports = Vec::new();
    for port in state.common_ports.drain(..) {
        let trimmed_id = port.id.trim();
        let trimmed_name = port.name.trim();
        let local_port = if port.local_port == 0 {
            port.port.unwrap_or(0)
        } else {
            port.local_port
        };
        let remote_port = if port.remote_port == 0 {
            port.port.unwrap_or(0)
        } else {
            port.remote_port
        };
        if trimmed_id.is_empty() || trimmed_name.is_empty() || local_port == 0 || remote_port == 0 {
            continue;
        }
        if !seen_port_ids.insert(trimmed_id.to_string()) {
            continue;
        }
        normalized_ports.push(SshCommonPortRecord {
            id: trimmed_id.to_string(),
            name: trimmed_name.to_string(),
            local_port,
            remote_port,
            port: None,
            description: port
                .description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
            created_at: port.created_at,
            updated_at: port.updated_at,
        });
    }
    sort_common_ports(&mut normalized_ports);
    state.common_ports = normalized_ports;
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
                common_ports: super::types::default_common_ports(),
            })
        }
        serde_json::Value::Object(ref map)
            if map.contains_key("groups") || map.contains_key("tunnels") || map.contains_key("common_ports") =>
        {
            serde_json::from_value::<SshTunnelState>(value).map_err(|e| e.to_string())
        }
        _ => Err("Unrecognized SSH tunnel state payload".to_string()),
    }
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
    let custom = record.custom.as_ref().map(|custom| super::types::SshTunnelCustomView {
        host: custom.host.clone(),
        port: custom.port,
        user: custom.user.clone(),
        auth_kind: custom.auth_kind.clone(),
        key_path: custom.key_path.clone(),
        has_password: matches!(custom.auth_kind, super::types::SshTunnelAuthKind::Password)
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

pub(in crate::ssh_tunnels) fn to_common_port_view(
    port: &SshCommonPortRecord,
) -> SshCommonPortView {
    SshCommonPortView {
        id: port.id.clone(),
        name: port.name.clone(),
        local_port: port.local_port,
        remote_port: port.remote_port,
        port: port.remote_port,
        description: port.description.clone(),
        created_at: port.created_at,
        updated_at: port.updated_at,
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

pub(in crate::ssh_tunnels) fn sort_common_ports(ports: &mut [SshCommonPortRecord]) {
    ports.sort_by(|a, b| {
        a.local_port
            .cmp(&b.local_port)
            .then_with(|| a.remote_port.cmp(&b.remote_port))
            .then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
}
