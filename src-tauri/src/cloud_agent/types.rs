use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudAgentSettings {
    #[serde(default = "default_host")]
    pub default_host: String,
    #[serde(default = "default_workspace_root")]
    pub remote_workspace_root: String,
    #[serde(default = "default_agent_dir")]
    pub remote_agent_dir: String,
    #[serde(default = "default_true")]
    pub opencode_yolo_mode: bool,
    #[serde(default)]
    pub opencode_extra_args: String,
    #[serde(default = "default_refresh_interval")]
    pub auto_refresh_interval_seconds: u32,
    #[serde(default = "default_disk_alert_threshold")]
    pub disk_alert_threshold_percent: f64,
    #[serde(default = "default_terminal_app")]
    pub terminal_app: String,
}

fn default_host() -> String {
    "dmit-server".to_string()
}

fn default_workspace_root() -> String {
    "/root/cloud-agent-workspaces".to_string()
}

fn default_agent_dir() -> String {
    "/root/onespace-cloud-agent".to_string()
}

fn default_true() -> bool {
    true
}

fn default_refresh_interval() -> u32 {
    10
}

fn default_disk_alert_threshold() -> f64 {
    80.0
}

fn default_terminal_app() -> String {
    "auto".to_string()
}

impl Default for CloudAgentSettings {
    fn default() -> Self {
        Self {
            default_host: default_host(),
            remote_workspace_root: default_workspace_root(),
            remote_agent_dir: default_agent_dir(),
            opencode_yolo_mode: default_true(),
            opencode_extra_args: String::new(),
            auto_refresh_interval_seconds: default_refresh_interval(),
            disk_alert_threshold_percent: default_disk_alert_threshold(),
            terminal_app: default_terminal_app(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSession {
    pub session_name: String,
    pub repo_name: String,
    pub branch: String,
    pub working_dir: String,
    pub created_at: i64,
    pub windows_count: u32,
    pub pane_pid: u32,
    pub is_active: bool,
    #[serde(default)]
    pub worktree_name: Option<String>,
    #[serde(default)]
    pub opencode_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastCommit {
    pub hash: String,
    pub author: String,
    pub timestamp: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudWorktree {
    pub repo_name: String,
    pub branch: String,
    pub worktree_path: String,
    pub has_active_session: bool,
    pub session_name: String,
    pub last_commit: Option<LastCommit>,
    pub last_modified: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiskUsage {
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub use_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayHealth {
    pub status: String,
    pub port: Option<u16>,
    pub providers_count: Option<usize>,
    pub model_pools: Option<Vec<String>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudAgentStatus {
    pub service_active: bool,
    pub disk: DiskUsage,
    pub gateway: GatewayHealth,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudAgentTask {
    pub id: String,
    pub repo: String,
    pub issue_number: u64,
    pub title: String,
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_seconds: Option<u64>,
    pub branch: Option<String>,
    pub pr_url: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySyncResult {
    pub success: bool,
    pub providers_synced: usize,
    pub model_pools_synced: usize,
    pub message: String,
}
