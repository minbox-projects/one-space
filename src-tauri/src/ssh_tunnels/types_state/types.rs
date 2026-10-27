use crate::ssh_tunnels::{DEFAULT_TUNNEL_GROUP_ID, DEFAULT_TUNNEL_GROUP_NAME};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

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
    pub(in crate::ssh_tunnels) is_encrypted: bool,
    pub(in crate::ssh_tunnels) data: String,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SshCommonPortRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub local_port: u16,
    #[serde(default)]
    pub remote_port: u16,
    #[serde(default, skip_serializing)]
    pub port: Option<u16>,
    #[serde(default)]
    pub description: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(in crate::ssh_tunnels) struct SshTunnelState {
    #[serde(default)]
    pub groups: Vec<SshTunnelGroupRecord>,
    #[serde(default)]
    pub tunnels: Vec<SshTunnelRecord>,
    #[serde(default = "default_common_ports")]
    pub common_ports: Vec<SshCommonPortRecord>,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SshTunnelGroupView {
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_default: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SshCommonPortView {
    pub id: String,
    pub name: String,
    #[serde(rename = "localPort")]
    pub local_port: u16,
    #[serde(rename = "remotePort")]
    pub remote_port: u16,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub description: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
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
    #[serde(default)]
    pub common_ports: Vec<SshCommonPortView>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshCommonPortUpsertInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "localPort", default)]
    pub local_port: u16,
    #[serde(rename = "remotePort", default)]
    pub remote_port: u16,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub description: Option<String>,
}

pub type SshTunnelProbeDraftInput = SshTunnelUpsertInput;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(in crate::ssh_tunnels) struct SshTunnelFailureEvent {
    pub id: String,
    pub name: String,
    pub error: String,
    pub auto_connect: bool,
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

pub(in crate::ssh_tunnels) fn default_common_ports() -> Vec<SshCommonPortRecord> {
    vec![
        SshCommonPortRecord {
            id: "ssh".to_string(),
            name: "SSH".to_string(),
            local_port: 22,
            remote_port: 22,
            port: None,
            description: Some("Secure Shell".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "http".to_string(),
            name: "HTTP".to_string(),
            local_port: 80,
            remote_port: 80,
            port: None,
            description: Some("Hypertext Transfer Protocol".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "https".to_string(),
            name: "HTTPS".to_string(),
            local_port: 443,
            remote_port: 443,
            port: None,
            description: Some("Hypertext Transfer Protocol Secure".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "web-dev".to_string(),
            name: "Web Dev".to_string(),
            local_port: 3000,
            remote_port: 3000,
            port: None,
            description: Some("React / Next.js / Vite dev server".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "mysql".to_string(),
            name: "MySQL".to_string(),
            local_port: 3306,
            remote_port: 3306,
            port: None,
            description: Some("MySQL / MariaDB database".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "postgres".to_string(),
            name: "PostgreSQL".to_string(),
            local_port: 5432,
            remote_port: 5432,
            port: None,
            description: Some("PostgreSQL database".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "redis".to_string(),
            name: "Redis".to_string(),
            local_port: 6379,
            remote_port: 6379,
            port: None,
            description: Some("Redis key-value store".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "http-alt".to_string(),
            name: "HTTP Alt".to_string(),
            local_port: 8080,
            remote_port: 8080,
            port: None,
            description: Some("Tomcat / Spring Boot / HTTP alternate".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "clickhouse".to_string(),
            name: "ClickHouse".to_string(),
            local_port: 8123,
            remote_port: 8123,
            port: None,
            description: Some("ClickHouse HTTP interface".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "https-alt".to_string(),
            name: "HTTPS Alt".to_string(),
            local_port: 8443,
            remote_port: 8443,
            port: None,
            description: Some("HTTPS alternate".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "minio".to_string(),
            name: "MinIO".to_string(),
            local_port: 9000,
            remote_port: 9000,
            port: None,
            description: Some("MinIO S3 API / PHP-FPM".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "kafka".to_string(),
            name: "Kafka".to_string(),
            local_port: 9092,
            remote_port: 9092,
            port: None,
            description: Some("Apache Kafka broker".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "elasticsearch".to_string(),
            name: "Elasticsearch".to_string(),
            local_port: 9200,
            remote_port: 9200,
            port: None,
            description: Some("Elasticsearch REST API".to_string()),
            created_at: 0,
            updated_at: 0,
        },
        SshCommonPortRecord {
            id: "mongodb".to_string(),
            name: "MongoDB".to_string(),
            local_port: 27017,
            remote_port: 27017,
            port: None,
            description: Some("MongoDB document database".to_string()),
            created_at: 0,
            updated_at: 0,
        },
    ]
}
