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
    String::new()
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CloudGatewayModel {
    pub id: String,
    #[serde(default)]
    pub upstream_model: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub reasoning_efforts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CloudGatewayProvider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub models: Vec<CloudGatewayModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayHealth {
    pub status: String,
    pub port: Option<u16>,
    pub providers_count: Option<usize>,
    pub model_pools: Option<Vec<String>>,
    #[serde(default)]
    pub providers: Option<Vec<CloudGatewayProvider>>,
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
    #[serde(default)]
    pub issue_number: Option<u64>,
    pub title: String,
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_seconds: Option<u64>,
    pub branch: Option<String>,
    pub pr_url: Option<String>,
    pub error_message: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub trigger_type: Option<String>,
    #[serde(default)]
    pub attempts: Option<u32>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub diff_stat: Option<String>,
    #[serde(default)]
    pub step: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub plan_title: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub base_branch: Option<String>,
    #[serde(default)]
    pub base_commit: Option<String>,
    #[serde(default)]
    pub final_commit: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub session_url: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub remote_root: Option<String>,
    #[serde(default)]
    pub local_root: Option<String>,
    #[serde(default)]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudPlanRepository {
    pub remote_url: String,
    pub canonical: String,
    pub host: String,
    pub branch: String,
    pub base_branch: String,
    pub source: String,
    pub head_sha: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalPlanSummary {
    pub plan_id: String,
    pub title: String,
    pub directory: String,
    pub valid: bool,
    pub error: Option<String>,
    pub has_tasks: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalPlanContext {
    pub workspace_id: String,
    pub workspace_name: String,
    pub root_path: String,
    pub repository: Option<CloudPlanRepository>,
    pub repository_error: Option<String>,
    pub plans: Vec<LocalPlanSummary>,
    pub plans_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanFilePayload {
    pub path: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalPlanPayload {
    pub plan_id: String,
    pub plan_title: String,
    pub directory: String,
    pub files: Vec<PlanFilePayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudPlanSubmitInput {
    pub workspace_id: String,
    pub plan_directory: String,
    #[serde(default)]
    pub base_branch: Option<String>,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudPlanSubmitResult {
    pub task: CloudAgentTask,
    pub reused: bool,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudPlanCapabilities {
    #[serde(default)]
    pub plan_submit: bool,
    #[serde(default)]
    pub plan_list: bool,
    #[serde(default)]
    pub plan_cancel: bool,
    #[serde(default)]
    pub plan_attach: bool,
    #[serde(default = "default_plan_schema_version")]
    pub schema_version: u32,
}

fn default_plan_schema_version() -> u32 {
    1
}

impl Default for CloudPlanCapabilities {
    fn default() -> Self {
        Self {
            plan_submit: false,
            plan_list: false,
            plan_cancel: false,
            plan_attach: false,
            schema_version: default_plan_schema_version(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySyncResult {
    pub success: bool,
    pub providers_synced: usize,
    pub model_pools_synced: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudAgentInstallationState {
    Installed,
    Incomplete,
    NotInstalled,
    ProbeError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudAgentInstallationInfo {
    pub host: String,
    pub state: CloudAgentInstallationState,
    pub local_latest_version: String,
    #[serde(default)]
    pub remote_version: Option<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
    #[serde(default)]
    pub metadata_reason: Option<String>,
    #[serde(default)]
    pub configuration_error: Option<String>,
    #[serde(default)]
    pub service_active: Option<bool>,
    #[serde(default)]
    pub probe_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudAgentDeployPhase {
    Platform,
    Dependencies,
    Upload,
    Config,
    Publish,
    Service,
    Metadata,
    Cleanup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudAgentDeployPhaseStatus {
    Start,
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudAgentDeploymentProgress {
    pub host: String,
    pub operation_id: String,
    pub phase: CloudAgentDeployPhase,
    pub status: CloudAgentDeployPhaseStatus,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudAgentDeploymentResult {
    pub host: String,
    pub operation_id: String,
    pub success: bool,
    pub phase: CloudAgentDeployPhase,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub remote_version: Option<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
}
