const SCHEMA_VERSION: u32 = 1;
const ALLOWED_TOOLS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];
const LAUNCH_SCOPE_SHARED: &str = "shared";
const LAUNCH_SCOPE_STRICT: &str = "strict";
const PROMPT_STATUS_APPLIED: &str = "applied";
const PROMPT_STATUS_MANUAL: &str = "manual";
const DEP_MODE_SHARED_GLOBAL: &str = "shared-global";
const DEP_MODE_STRICT_LOCAL: &str = "strict-local";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiMeta {
    pub schema_version: u32,
    pub revision: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiOk<T> {
    pub ok: bool,
    pub data: T,
    pub meta: ApiMeta,
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn api_ok<T: Serialize>(data: T) -> Result<ApiOk<T>, String> {
    Ok(ApiOk {
        ok: true,
        data,
        meta: ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision: now_ts(),
        },
    })
}

fn normalize_tool(tool: &str) -> String {
    let t = tool.trim().to_lowercase();
    if ALLOWED_TOOLS.contains(&t.as_str()) {
        t
    } else {
        "claude".to_string()
    }
}

fn dedup_non_empty(items: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_string();
        if seen.insert(key.clone()) {
            out.push(key);
        }
    }
    out
}

fn normalize_launch_scope(scope: Option<&str>) -> String {
    let value = scope.unwrap_or("").trim().to_lowercase();
    if value == LAUNCH_SCOPE_STRICT {
        LAUNCH_SCOPE_STRICT.to_string()
    } else {
        LAUNCH_SCOPE_SHARED.to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowPreset {
    pub id: String,
    pub name: String,
    pub tool: String,
    pub working_dir: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub required_skill_ids: Vec<String>,
    #[serde(default)]
    pub launch_prompt: Option<String>,
    #[serde(default = "default_launch_scope")]
    pub launch_scope: String,
    pub created_at: u64,
    pub updated_at: u64,
}

fn default_launch_scope() -> String {
    LAUNCH_SCOPE_SHARED.to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowPresetInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub required_skill_ids: Vec<String>,
    #[serde(default)]
    pub launch_prompt: Option<String>,
    #[serde(default)]
    pub launch_scope: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowRun {
    pub id: String,
    pub preset_id: String,
    pub preset_name: String,
    pub tool: String,
    pub working_dir: String,
    #[serde(default)]
    pub launch_prompt: Option<String>,
    #[serde(default)]
    pub launch_scope: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tool_session_id: Option<String>,
    #[serde(default)]
    pub runtime_mode: String,
    #[serde(default)]
    pub runtime_profile_id: Option<String>,
    #[serde(default)]
    pub prompt_apply_status: String,
    #[serde(default)]
    pub dependency_apply_mode: String,
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    pub started_at: u64,
    #[serde(default)]
    pub ended_at: Option<u64>,
    #[serde(default)]
    pub replay_of_run_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowRunUpdateInput {
    pub run_id: String,
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowRunDeleteInput {
    pub run_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowLaunchInput {
    pub preset_id: String,
    #[serde(default)]
    pub session_name: Option<String>,
    #[serde(default)]
    pub override_working_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowReplayInput {
    pub run_id: String,
    #[serde(default)]
    pub session_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowRunListInput {
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct WorkflowDependencyState {
    #[serde(default)]
    pub active_provider_id: Option<String>,
    #[serde(default)]
    pub active_provider_name: Option<String>,
    #[serde(default)]
    pub missing_mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub missing_mcp_names: Vec<String>,
    #[serde(default)]
    pub inactive_mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub inactive_mcp_names: Vec<String>,
    #[serde(default)]
    pub missing_skill_ids: Vec<String>,
    #[serde(default)]
    pub missing_skill_names: Vec<String>,
    #[serde(default)]
    pub installable_skill_ids: Vec<String>,
    #[serde(default)]
    pub unresolved_skill_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowDependencyApplyResult {
    pub preset_id: String,
    pub linked_mcp_count: usize,
    pub enabled_mcp_switch_count: usize,
    pub installed_skill_count: usize,
    pub failed_skill_installs: Vec<String>,
    pub dependencies_after: WorkflowDependencyState,
}
