use crate::app_store::{ApiErr, ApiMeta, ApiOk, SessionInput};
use crate::{ai_sessions, atomic_write_string, get_data_dir, open_path_with_system, workspaces};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const AI_FLOW_REPO_URL: &str = "https://github.com/hengboy/ai-flow.git";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiFlowInstallStatus {
    pub repo_url: String,
    pub cache_dir: String,
    pub installed: bool,
    pub commit: Option<String>,
    pub branch: Option<String>,
    pub log: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowHealthItem {
    pub id: String,
    pub label: String,
    pub ok: bool,
    pub status: String,
    pub path: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowHealthCheck {
    pub installed: bool,
    pub repo_commit: Option<String>,
    pub repo_branch: Option<String>,
    pub items: Vec<AiFlowHealthItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowProjectSummary {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub ai_flow_dir: String,
    pub from_workspace: bool,
    pub has_ai_flow: bool,
    pub plan_count: usize,
    pub pending_count: usize,
    pub failed_count: usize,
    pub done_count: usize,
    pub invalid_state_count: usize,
    pub queue_count: usize,
    pub group_count: usize,
    pub html_status_path: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiFlowPlanTransition {
    pub seq: Option<i64>,
    pub at: Option<String>,
    pub event: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub actor: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowPlanState {
    pub slug: String,
    pub title: String,
    pub current_status: String,
    pub plan_file: Option<String>,
    pub plan_path: Option<String>,
    pub review_files: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub transitions: Vec<AiFlowPlanTransition>,
    pub raw_state_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowPlanGroupState {
    pub slug: String,
    pub title: Option<String>,
    pub current_status: Option<String>,
    pub current_child: Option<String>,
    pub children: Vec<Value>,
    pub dependencies: Vec<Value>,
    pub raw_state_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowQueueState {
    pub slug: String,
    pub title: Option<String>,
    pub current_status: Option<String>,
    pub items: Vec<Value>,
    pub raw_state_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowInvalidState {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowConfigSummary {
    pub global_setting_exists: bool,
    pub project_setting_exists: bool,
    pub project_rule_exists: bool,
    pub effective_setting: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowProjectStatus {
    pub project: AiFlowProjectSummary,
    pub plans: Vec<AiFlowPlanState>,
    pub groups: Vec<AiFlowPlanGroupState>,
    pub queues: Vec<AiFlowQueueState>,
    pub invalid_states: Vec<AiFlowInvalidState>,
    pub config_summary: AiFlowConfigSummary,
    pub html_status_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowConfigDocument {
    pub scope: String,
    pub format: String,
    pub path: String,
    pub exists: bool,
    pub content: String,
    pub parsed: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowConfigSaveResult {
    pub path: String,
    pub backup_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowLaunchAction {
    pub tool: String,
    pub action: String,
    pub slug: String,
    pub project_root: String,
    pub session_id: Option<String>,
    pub permission_mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowLaunchPreview {
    pub tool: String,
    pub permission_confirmation_required: bool,
    pub prompt: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiFlowQueueCreateInput {
    pub project_root: String,
    pub queue_slug: String,
    #[serde(default)]
    pub plan_slugs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiFlowQueueCreateResult {
    pub queue_slug: String,
    pub state_path: String,
    pub log: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiFlowConfigSaveInput {
    pub scope: String,
    #[serde(default)]
    pub project_root: Option<String>,
    pub format: String,
    pub content: String,
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn revision_now() -> u64 {
    now_ts()
}

fn ok<T: Serialize>(data: T) -> Result<ApiOk<T>, ApiErr> {
    Ok(ApiOk {
        ok: true,
        data,
        meta: ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision: revision_now(),
        },
    })
}

fn api_error(code: &str, message: impl Into<String>) -> ApiErr {
    ApiErr {
        ok: false,
        code: code.to_string(),
        message: message.into(),
        details: None,
    }
}

fn home_dir() -> Result<PathBuf, ApiErr> {
    dirs::home_dir().ok_or_else(|| api_error("io_error", "home directory not found"))
}

fn ai_flow_home() -> PathBuf {
    std::env::var_os("AI_FLOW_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".config").join("ai-flow")))
        .unwrap_or_else(|| PathBuf::from(".ai-flow-home"))
}

fn cache_repo_dir() -> Result<PathBuf, ApiErr> {
    Ok(get_data_dir()
        .map_err(|e| api_error("io_error", e))?
        .join("cache")
        .join("ai-flow")
        .join("repo"))
}

fn run_command(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<String, String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().map_err(|e| e.to_string())?;
    let mut log = String::new();
    if !output.stdout.is_empty() {
        log.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        if !log.is_empty() && !log.ends_with('\n') {
            log.push('\n');
        }
        log.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if output.status.success() {
        Ok(log)
    } else {
        Err(log)
    }
}

fn git_output(repo: &Path, args: &[&str]) -> Option<String> {
    run_command("git", args, Some(repo))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn install_status_for_repo(repo: &Path, log: String) -> AiFlowInstallStatus {
    AiFlowInstallStatus {
        repo_url: AI_FLOW_REPO_URL.to_string(),
        cache_dir: repo.to_string_lossy().to_string(),
        installed: repo.join("install.sh").is_file(),
        commit: git_output(repo, &["rev-parse", "HEAD"]),
        branch: git_output(repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
        log,
    }
}

#[tauri::command]
pub async fn ai_flow_install_latest() -> Result<ApiOk<AiFlowInstallStatus>, ApiErr> {
    let repo = cache_repo_dir()?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut log = String::new();
        if repo.exists() {
            log.push_str(
                &run_command("git", &["fetch", "--prune", "origin"], Some(&repo))
                    .map_err(|e| api_error("AI_FLOW_INSTALL_FAILED", e))?,
            );
        } else {
            let parent = repo
                .parent()
                .ok_or_else(|| api_error("io_error", "invalid cache repo path"))?;
            fs::create_dir_all(parent).map_err(|e| api_error("io_error", e.to_string()))?;
            let repo_arg = repo.to_string_lossy().to_string();
            log.push_str(
                &run_command("git", &["clone", AI_FLOW_REPO_URL, repo_arg.as_str()], None)
                    .map_err(|e| api_error("AI_FLOW_INSTALL_FAILED", e))?,
            );
        }

        log.push_str(
            &run_command(
                "git",
                &["checkout", "-B", "main", "origin/main"],
                Some(&repo),
            )
            .map_err(|e| api_error("AI_FLOW_INSTALL_FAILED", e))?,
        );
        log.push_str(
            &run_command("git", &["reset", "--hard", "origin/main"], Some(&repo))
                .map_err(|e| api_error("AI_FLOW_INSTALL_FAILED", e))?,
        );
        log.push_str(
            &run_command("bash", &["install.sh"], Some(&repo))
                .map_err(|e| api_error("AI_FLOW_INSTALL_FAILED", e))?,
        );
        ok(install_status_for_repo(&repo, log))
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?
}

fn health_item(id: &str, label: &str, path: PathBuf, required_dir: bool) -> AiFlowHealthItem {
    let exists = if required_dir {
        path.is_dir()
    } else {
        path.exists()
    };
    AiFlowHealthItem {
        id: id.to_string(),
        label: label.to_string(),
        ok: exists,
        status: if exists { "ok" } else { "missing" }.to_string(),
        path: Some(path.to_string_lossy().to_string()),
        detail: None,
    }
}

fn executable_item(id: &str, label: &str, path: PathBuf) -> AiFlowHealthItem {
    let exists = path.is_file();
    let executable = exists
        && fs::metadata(&path)
            .map(|meta| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    meta.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
            .unwrap_or(false);
    AiFlowHealthItem {
        id: id.to_string(),
        label: label.to_string(),
        ok: executable,
        status: if executable {
            "ok"
        } else if exists {
            "not_executable"
        } else {
            "missing"
        }
        .to_string(),
        path: Some(path.to_string_lossy().to_string()),
        detail: None,
    }
}

fn git_freshness_item(repo: &Path) -> AiFlowHealthItem {
    if !repo.join(".git").is_dir() {
        return AiFlowHealthItem {
            id: "git_freshness".to_string(),
            label: "Git checkout freshness".to_string(),
            ok: false,
            status: "missing".to_string(),
            path: Some(repo.to_string_lossy().to_string()),
            detail: Some("AI Flow cache repository is not cloned.".to_string()),
        };
    }
    let head = git_output(repo, &["rev-parse", "HEAD"]);
    let remote = git_output(repo, &["rev-parse", "origin/main"]);
    let ok = head.is_some() && head == remote;
    AiFlowHealthItem {
        id: "git_freshness".to_string(),
        label: "Git checkout freshness".to_string(),
        ok,
        status: if ok { "ok" } else { "outdated" }.to_string(),
        path: Some(repo.to_string_lossy().to_string()),
        detail: Some(format!(
            "HEAD={}, origin/main={}",
            head.as_deref().unwrap_or("unknown"),
            remote.as_deref().unwrap_or("unknown")
        )),
    }
}

#[tauri::command]
pub async fn ai_flow_health_check() -> Result<ApiOk<AiFlowHealthCheck>, ApiErr> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = home_dir()?;
        let repo = cache_repo_dir()?;
        let installed = repo.join("install.sh").is_file() || ai_flow_home().is_dir();
        let mut items = Vec::new();
        items.push(health_item(
            "runtime_home",
            "AI Flow runtime",
            ai_flow_home(),
            true,
        ));
        items.push(health_item(
            "claude_skills",
            "Claude skills",
            home.join(".claude").join("skills"),
            true,
        ));
        items.push(health_item(
            "claude_agents",
            "Claude agents",
            home.join(".claude").join("agents"),
            true,
        ));
        items.push(health_item(
            "codex_skills",
            "Codex skills",
            home.join(".codex").join("skills"),
            true,
        ));
        items.push(health_item(
            "codex_agents",
            "Codex agents",
            home.join(".codex").join("agents"),
            true,
        ));
        items.push(health_item(
            "onespace_skills",
            "OneSpace skills",
            home.join(".config").join("onespace").join("skills"),
            true,
        ));
        items.push(health_item(
            "onespace_subagents",
            "OneSpace subagents",
            home.join(".config").join("onespace").join("subagents"),
            true,
        ));
        items.push(health_item(
            "gemini_skills",
            "Gemini skills",
            home.join(".gemini").join("skills"),
            true,
        ));
        items.push(health_item(
            "opencode_skills",
            "OpenCode skills",
            home.join(".opencode").join("skills"),
            true,
        ));
        items.push(executable_item(
            "install_script",
            "install.sh",
            repo.join("install.sh"),
        ));
        items.push(executable_item(
            "flow_state_script",
            "flow-state.sh",
            ai_flow_home().join("scripts").join("flow-state.sh"),
        ));
        items.push(executable_item(
            "flow_orchestrate_script",
            "flow-plan-orchestrate.sh",
            ai_flow_home()
                .join("scripts")
                .join("flow-plan-orchestrate.sh"),
        ));
        items.push(executable_item(
            "flow_group_script",
            "flow-plan-group.sh",
            ai_flow_home().join("scripts").join("flow-plan-group.sh"),
        ));
        items.push(git_freshness_item(&repo));
        ok(AiFlowHealthCheck {
            installed,
            repo_commit: git_output(&repo, &["rev-parse", "HEAD"]),
            repo_branch: git_output(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
            items,
        })
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_string()
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
}

fn value_array(value: &Value, key: &str) -> Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn parse_transition(value: &Value) -> AiFlowPlanTransition {
    AiFlowPlanTransition {
        seq: value.get("seq").and_then(Value::as_i64),
        at: value_string(value, "at"),
        event: value_string(value, "event"),
        from: value_string(value, "from"),
        to: value_string(value, "to"),
        actor: value_string(value, "actor"),
        note: value_string(value, "note"),
    }
}

fn collect_review_files(ai_flow_dir: &Path, slug: &str) -> Vec<String> {
    let reports = ai_flow_dir.join("reports");
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(reports) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            if name.contains(slug) && (name.ends_with(".md") || name.ends_with(".json")) {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out
}

fn parse_plan_state(ai_flow_dir: &Path, path: &Path, value: &Value) -> AiFlowPlanState {
    let slug = value_string(value, "slug").unwrap_or_else(|| file_stem(path));
    let plan_file = value_string(value, "plan_file");
    let plan_path = plan_file.as_ref().map(|file| {
        if Path::new(file).is_absolute() {
            PathBuf::from(file)
        } else {
            ai_flow_dir.parent().unwrap_or(ai_flow_dir).join(file)
        }
        .to_string_lossy()
        .to_string()
    });
    let transitions = value
        .get("transitions")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(parse_transition).collect())
        .unwrap_or_default();
    AiFlowPlanState {
        slug: slug.clone(),
        title: value_string(value, "title").unwrap_or_else(|| slug.clone()),
        current_status: value_string(value, "current_status")
            .or_else(|| value_string(value, "status"))
            .unwrap_or_else(|| "UNKNOWN".to_string()),
        plan_file,
        plan_path,
        review_files: collect_review_files(ai_flow_dir, &slug),
        created_at: value_string(value, "created_at"),
        updated_at: value_string(value, "updated_at"),
        transitions,
        raw_state_path: path.to_string_lossy().to_string(),
    }
}

fn parse_group_state(path: &Path, value: &Value) -> AiFlowPlanGroupState {
    AiFlowPlanGroupState {
        slug: value_string(value, "group_slug")
            .or_else(|| value_string(value, "slug"))
            .unwrap_or_else(|| file_stem(path)),
        title: value_string(value, "title"),
        current_status: value_string(value, "current_status")
            .or_else(|| value_string(value, "status")),
        current_child: value_string(value, "current_child_id")
            .or_else(|| value_string(value, "current_child"))
            .or_else(|| value_string(value, "current_child_slug")),
        children: value_array(value, "children"),
        dependencies: value_array(value, "dependencies"),
        raw_state_path: path.to_string_lossy().to_string(),
    }
}

fn parse_queue_state(path: &Path, value: &Value) -> AiFlowQueueState {
    AiFlowQueueState {
        slug: value_string(value, "queue_slug")
            .or_else(|| value_string(value, "slug"))
            .unwrap_or_else(|| file_stem(path)),
        title: value_string(value, "title"),
        current_status: value_string(value, "current_status")
            .or_else(|| value_string(value, "status")),
        items: value_array(value, "items"),
        raw_state_path: path.to_string_lossy().to_string(),
    }
}

fn read_json_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) == Some("json") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn merge_json_values(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                if let Some(existing) = base_map.get_mut(&key) {
                    merge_json_values(existing, value);
                } else {
                    base_map.insert(key, value);
                }
            }
        }
        (base_slot, overlay_value) => {
            *base_slot = overlay_value;
        }
    }
}

fn read_json_value_if_exists(path: &Path) -> Option<Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
}

fn effective_setting_for_project(ai_flow_dir: &Path) -> Option<Value> {
    let mut merged = read_json_value_if_exists(&ai_flow_home().join("setting.json"))
        .unwrap_or_else(|| Value::Object(Default::default()));
    if let Some(project) = read_json_value_if_exists(&ai_flow_dir.join("setting.json")) {
        merge_json_values(&mut merged, project);
    }
    if merged
        .as_object()
        .map(|map| map.is_empty())
        .unwrap_or(false)
    {
        None
    } else {
        Some(merged)
    }
}

fn parse_project(root: &Path, from_workspace: bool) -> AiFlowProjectStatus {
    let ai_flow_dir = root.join(".ai-flow");
    let mut plans = Vec::new();
    let mut invalid_states = Vec::new();
    for path in read_json_files(&ai_flow_dir.join("state")) {
        match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|content| serde_json::from_str::<Value>(&content).map_err(|e| e.to_string()))
        {
            Ok(value) => plans.push(parse_plan_state(&ai_flow_dir, &path, &value)),
            Err(error) => invalid_states.push(AiFlowInvalidState {
                path: path.to_string_lossy().to_string(),
                error,
            }),
        }
    }

    let mut groups = Vec::new();
    for dir in [
        ai_flow_dir.join("plan-groups").join("state"),
        ai_flow_dir.join("orchestrations").join("groups"),
        ai_flow_dir
            .join("orchestrations")
            .join("state")
            .join("groups"),
    ] {
        for path in read_json_files(&dir) {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(value) = serde_json::from_str::<Value>(&content) {
                    groups.push(parse_group_state(&path, &value));
                }
            }
        }
    }

    let mut queues = Vec::new();
    for dir in [
        ai_flow_dir.join("queues").join("state"),
        ai_flow_dir.join("orchestrations").join("state"),
        ai_flow_dir.join("orchestrations").join("queues"),
    ] {
        for path in read_json_files(&dir) {
            if groups
                .iter()
                .any(|group| group.raw_state_path == path.to_string_lossy())
            {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(value) = serde_json::from_str::<Value>(&content) {
                    queues.push(parse_queue_state(&path, &value));
                }
            }
        }
    }

    plans.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.slug.cmp(&b.slug)));
    let pending_count = plans
        .iter()
        .filter(|plan| {
            let status = plan.current_status.to_ascii_uppercase();
            status.contains("AWAITING")
                || status.contains("PENDING")
                || status.contains("IN_PROGRESS")
        })
        .count();
    let failed_count = plans
        .iter()
        .filter(|plan| plan.current_status.to_ascii_uppercase().contains("FAILED"))
        .count();
    let done_count = plans
        .iter()
        .filter(|plan| plan.current_status.eq_ignore_ascii_case("DONE"))
        .count();
    let updated_at = plans
        .iter()
        .filter_map(|plan| plan.updated_at.clone())
        .max();
    let html_status_path = ai_flow_dir
        .join("html")
        .join("index.html")
        .is_file()
        .then(|| {
            ai_flow_dir
                .join("html")
                .join("index.html")
                .to_string_lossy()
                .to_string()
        });
    let name = root
        .file_name()
        .and_then(|v| v.to_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());
    let project = AiFlowProjectSummary {
        id: root.to_string_lossy().to_string(),
        name,
        root_path: root.to_string_lossy().to_string(),
        ai_flow_dir: ai_flow_dir.to_string_lossy().to_string(),
        from_workspace,
        has_ai_flow: ai_flow_dir.is_dir(),
        plan_count: plans.len(),
        pending_count,
        failed_count,
        done_count,
        invalid_state_count: invalid_states.len(),
        queue_count: queues.len(),
        group_count: groups.len(),
        html_status_path: html_status_path.clone(),
        updated_at,
    };
    AiFlowProjectStatus {
        project,
        plans,
        groups,
        queues,
        invalid_states,
        config_summary: AiFlowConfigSummary {
            global_setting_exists: ai_flow_home().join("setting.json").is_file(),
            project_setting_exists: ai_flow_dir.join("setting.json").is_file(),
            project_rule_exists: ai_flow_dir.join("rule.yaml").is_file(),
            effective_setting: effective_setting_for_project(&ai_flow_dir),
        },
        html_status_path,
    }
}

fn project_roots(extra_path: Option<String>) -> Vec<(PathBuf, bool)> {
    let mut roots = Vec::<(PathBuf, bool)>::new();
    let mut seen = BTreeMap::<String, bool>::new();
    if let Ok(workspace_roots) = workspaces::workspace_roots() {
        for root in workspace_roots {
            let normalized = ai_sessions::normalize_working_dir_for_terminal(&root);
            if seen.insert(normalized.clone(), true).is_none() {
                roots.push((PathBuf::from(normalized), true));
            }
        }
    }
    if let Some(path) = extra_path {
        let normalized = ai_sessions::normalize_working_dir_for_terminal(&path);
        if seen.insert(normalized.clone(), false).is_none() {
            roots.push((PathBuf::from(normalized), false));
        }
    }
    roots
}

#[tauri::command]
pub async fn ai_flow_projects_list(
    extra_path: Option<String>,
) -> Result<ApiOk<Vec<AiFlowProjectSummary>>, ApiErr> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut summaries = Vec::new();
        for (root, from_workspace) in project_roots(extra_path) {
            if !root.is_dir() {
                continue;
            }
            let status = parse_project(&root, from_workspace);
            if status.project.has_ai_flow || from_workspace {
                summaries.push(status.project);
            }
        }
        summaries.sort_by(|a, b| {
            b.has_ai_flow
                .cmp(&a.has_ai_flow)
                .then(b.updated_at.cmp(&a.updated_at))
                .then(a.name.cmp(&b.name))
        });
        ok(summaries)
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?
}

#[tauri::command]
pub async fn ai_flow_project_status(
    project_root: String,
) -> Result<ApiOk<AiFlowProjectStatus>, ApiErr> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(ai_sessions::normalize_working_dir_for_terminal(
            &project_root,
        ));
        if !root.join(".ai-flow").is_dir() {
            return Err(api_error(
                "AI_FLOW_PROJECT_NOT_FOUND",
                "project .ai-flow directory not found",
            ));
        }
        ok(parse_project(&root, false))
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?
}

fn config_path(
    scope: &str,
    project_root: Option<&str>,
    format: Option<&str>,
) -> Result<(PathBuf, String), ApiErr> {
    match scope {
        "global_setting" => Ok((ai_flow_home().join("setting.json"), "json".to_string())),
        "project_setting" => {
            let root = project_root.ok_or_else(|| {
                api_error("AI_FLOW_PROJECT_NOT_FOUND", "project_root is required")
            })?;
            Ok((
                PathBuf::from(ai_sessions::normalize_working_dir_for_terminal(root))
                    .join(".ai-flow")
                    .join("setting.json"),
                "json".to_string(),
            ))
        }
        "project_rule" => {
            let root = project_root.ok_or_else(|| {
                api_error("AI_FLOW_PROJECT_NOT_FOUND", "project_root is required")
            })?;
            Ok((
                PathBuf::from(ai_sessions::normalize_working_dir_for_terminal(root))
                    .join(".ai-flow")
                    .join("rule.yaml"),
                "yaml".to_string(),
            ))
        }
        _ => {
            let fmt = format.unwrap_or("json").to_string();
            Err(api_error(
                "invalid_payload",
                format!("unsupported config scope for format {fmt}: {scope}"),
            ))
        }
    }
}

fn parse_config_content(format: &str, content: &str) -> Result<Option<Value>, ApiErr> {
    if content.trim().is_empty() {
        return Ok(None);
    }
    match format {
        "json" => serde_json::from_str::<Value>(content)
            .map(Some)
            .map_err(|e| api_error("AI_FLOW_CONFIG_INVALID", e.to_string())),
        "yaml" | "yml" => serde_yaml::from_str::<Value>(content)
            .map(Some)
            .map_err(|e| api_error("AI_FLOW_CONFIG_INVALID", e.to_string())),
        other => Err(api_error(
            "AI_FLOW_CONFIG_INVALID",
            format!("unsupported config format: {other}"),
        )),
    }
}

#[tauri::command]
pub async fn ai_flow_config_get(
    scope: String,
    project_root: Option<String>,
) -> Result<ApiOk<AiFlowConfigDocument>, ApiErr> {
    tauri::async_runtime::spawn_blocking(move || {
        let (path, format) = config_path(&scope, project_root.as_deref(), None)?;
        let exists = path.is_file();
        let content = if exists {
            fs::read_to_string(&path).map_err(|e| api_error("io_error", e.to_string()))?
        } else {
            String::new()
        };
        let parsed = parse_config_content(&format, &content)?;
        ok(AiFlowConfigDocument {
            scope,
            format,
            path: path.to_string_lossy().to_string(),
            exists,
            content,
            parsed,
        })
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?
}

#[tauri::command]
pub async fn ai_flow_config_save(
    input: AiFlowConfigSaveInput,
) -> Result<ApiOk<AiFlowConfigSaveResult>, ApiErr> {
    tauri::async_runtime::spawn_blocking(move || {
        let (path, expected_format) = config_path(
            &input.scope,
            input.project_root.as_deref(),
            Some(&input.format),
        )?;
        let format = input.format.trim().to_lowercase();
        if format != expected_format {
            return Err(api_error(
                "AI_FLOW_CONFIG_INVALID",
                format!("{} expects {} content", input.scope, expected_format),
            ));
        }
        parse_config_content(&format, &input.content)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| api_error("io_error", e.to_string()))?;
        }
        let backup_path = if path.exists() {
            let backup = path.with_extension(format!(
                "{}.bak.{}",
                path.extension().and_then(|v| v.to_str()).unwrap_or("txt"),
                now_ts()
            ));
            fs::copy(&path, &backup).map_err(|e| api_error("io_error", e.to_string()))?;
            Some(backup.to_string_lossy().to_string())
        } else {
            None
        };
        atomic_write_string(&path, &input.content).map_err(|e| api_error("io_error", e))?;
        ok(AiFlowConfigSaveResult {
            path: path.to_string_lossy().to_string(),
            backup_path,
        })
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?
}

fn command_for_action(action: &str) -> Option<&'static str> {
    match action {
        "plan-review" => Some("/ai-flow-plan-review"),
        "coding" => Some("/ai-flow-plan-coding"),
        "review" => Some("/ai-flow-plan-coding-review"),
        "resume" => Some("/ai-flow-plan-orchestrate --resume"),
        "status" => Some("/ai-flow-plan-orchestrate --status"),
        "reopen-current" => Some("/ai-flow-plan-orchestrate --reopen-current"),
        "group-review" => Some("/ai-flow-plan-review"),
        "group-final-review" => Some("/ai-flow-plan-review"),
        _ => None,
    }
}

fn prompt_for_action(action: &str, slug: &str) -> Result<String, ApiErr> {
    let command = command_for_action(action)
        .ok_or_else(|| api_error("invalid_payload", "unsupported AI Flow action"))?;
    Ok(format!("{command} {slug}"))
}

#[tauri::command]
pub fn ai_flow_launch_preview(
    input: AiFlowLaunchAction,
) -> Result<ApiOk<AiFlowLaunchPreview>, ApiErr> {
    let tool = input.tool.trim().to_lowercase();
    if tool != "claude" && tool != "codex" {
        return Err(api_error(
            "AI_FLOW_UNSUPPORTED_TOOL",
            "AI Flow launch actions support Claude Code and Codex only",
        ));
    }
    let slug = input.slug.trim();
    if slug.is_empty() {
        return Err(api_error("AI_FLOW_SLUG_REQUIRED", "slug is required"));
    }
    let prompt = prompt_for_action(input.action.trim(), slug)?;
    let config_perm_mode = crate::app_store::resolve_permission_mode_for_tool(&tool);
    ok(AiFlowLaunchPreview {
        tool,
        permission_confirmation_required: config_perm_mode
            == ai_sessions::TerminalPermissionMode::FullAccess
            && input.permission_mode.as_deref() != Some("full_access"),
        prompt,
    })
}

#[tauri::command]
pub async fn ai_flow_launch_action(
    app: tauri::AppHandle,
    input: AiFlowLaunchAction,
) -> Result<ApiOk<Value>, ApiErr> {
    let tool = input.tool.trim().to_lowercase();
    if tool != "claude" && tool != "codex" {
        return Err(api_error(
            "AI_FLOW_UNSUPPORTED_TOOL",
            "AI Flow launch actions support Claude Code and Codex only",
        ));
    }
    let slug = input.slug.trim();
    if slug.is_empty() {
        return Err(api_error("AI_FLOW_SLUG_REQUIRED", "slug is required"));
    }
    let initial_prompt = prompt_for_action(input.action.trim(), slug)?;
    let project_root = ai_sessions::normalize_working_dir_for_terminal(&input.project_root);
    if !Path::new(&project_root).join(".ai-flow").is_dir() {
        return Err(api_error(
            "AI_FLOW_PROJECT_NOT_FOUND",
            "project .ai-flow directory not found",
        ));
    }
    if let Some(session_id) = input
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return crate::app_store::sessions_launch_impl(
            app,
            session_id.to_string(),
            input.permission_mode.clone(),
            Some(initial_prompt),
        )
        .await;
    }
    let session_input = SessionInput {
        id: None,
        name: format!("AI Flow {} {}", input.action.trim(), slug),
        working_dir: project_root,
        tool,
        tool_session_id: input.session_id,
        runtime_mode: None,
        runtime_profile_id: None,
        preset_id: None,
        status: None,
        provider_id: None,
        initial_prompt: Some(initial_prompt),
        permission_mode: input.permission_mode,
    };
    crate::app_store::sessions_create(app, session_input).await
}

#[tauri::command]
pub async fn ai_flow_queue_create(
    input: AiFlowQueueCreateInput,
) -> Result<ApiOk<AiFlowQueueCreateResult>, ApiErr> {
    tauri::async_runtime::spawn_blocking(move || {
        let project_root = PathBuf::from(ai_sessions::normalize_working_dir_for_terminal(
            &input.project_root,
        ));
        if !project_root.join(".ai-flow").join("state").is_dir() {
            return Err(api_error(
                "AI_FLOW_PROJECT_NOT_FOUND",
                "project .ai-flow/state directory not found",
            ));
        }
        let queue_slug = input.queue_slug.trim();
        if queue_slug.is_empty() {
            return Err(api_error("AI_FLOW_SLUG_REQUIRED", "queue slug is required"));
        }
        let plan_slugs = input
            .plan_slugs
            .iter()
            .map(|slug| slug.trim().to_string())
            .filter(|slug| !slug.is_empty())
            .collect::<Vec<_>>();
        if plan_slugs.is_empty() {
            return Err(api_error(
                "AI_FLOW_SLUG_REQUIRED",
                "at least one plan slug is required",
            ));
        }
        let script = ai_flow_home()
            .join("scripts")
            .join("flow-plan-orchestrate.sh");
        if !script.is_file() {
            return Err(api_error(
                "AI_FLOW_NOT_INSTALLED",
                format!(
                    "missing flow-plan-orchestrate.sh: {}",
                    script.to_string_lossy()
                ),
            ));
        }
        let mut args = vec!["--queue".to_string(), queue_slug.to_string()];
        args.extend(plan_slugs);
        let output = Command::new("bash")
            .arg(script)
            .args(&args)
            .current_dir(&project_root)
            .output()
            .map_err(|e| api_error("AI_FLOW_INSTALL_FAILED", e.to_string()))?;
        let mut log = String::new();
        if !output.stdout.is_empty() {
            log.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            if !log.is_empty() && !log.ends_with('\n') {
                log.push('\n');
            }
            log.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        if !output.status.success() {
            return Err(api_error("AI_FLOW_INSTALL_FAILED", log));
        }
        let state_path = project_root
            .join(".ai-flow")
            .join("orchestrations")
            .join("state")
            .join(format!("{queue_slug}.json"));
        ok(AiFlowQueueCreateResult {
            queue_slug: queue_slug.to_string(),
            state_path: state_path.to_string_lossy().to_string(),
            log,
        })
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?
}

#[tauri::command]
pub async fn ai_flow_open_path(path: String) -> Result<ApiOk<Value>, ApiErr> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(api_error("invalid_payload", "path is required"));
    }
    open_path_with_system(trimmed).map_err(|e| api_error("io_error", e))?;
    ok(json!({ "opened": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("onespace-ai-flow-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join(".ai-flow").join("state")).unwrap();
        root
    }

    #[test]
    fn parse_project_keeps_invalid_state_from_blocking_valid_plans() {
        let root = temp_project("mixed-state");
        fs::write(
            root.join(".ai-flow").join("state").join("ok.json"),
            r#"{"slug":"ok","title":"OK","current_status":"DONE","updated_at":"2026-06-01T00:00:00+08:00"}"#,
        )
        .unwrap();
        fs::write(root.join(".ai-flow").join("state").join("bad.json"), "{").unwrap();
        let status = parse_project(&root, true);
        assert_eq!(status.plans.len(), 1);
        assert_eq!(status.invalid_states.len(), 1);
        assert_eq!(status.project.done_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parse_project_reads_official_queue_and_group_locations() {
        let root = temp_project("queue-group-state");
        fs::create_dir_all(root.join(".ai-flow").join("orchestrations").join("state")).unwrap();
        fs::create_dir_all(root.join(".ai-flow").join("plan-groups").join("state")).unwrap();
        fs::write(
            root.join(".ai-flow")
                .join("orchestrations")
                .join("state")
                .join("queue-a.json"),
            r#"{"schema_version":1,"queue_slug":"queue-a","current_status":"READY","active_index":0,"items":[]}"#,
        )
        .unwrap();
        fs::write(
            root.join(".ai-flow")
                .join("plan-groups")
                .join("state")
                .join("group-a.json"),
            r#"{"schema_version":1,"group_slug":"group-a","title":"Group A","current_status":"AWAITING_GROUP_REVIEW","current_child_id":null,"children":[],"transitions":[]}"#,
        )
        .unwrap();

        let status = parse_project(&root, true);

        assert_eq!(status.queues.len(), 1);
        assert_eq!(status.queues[0].slug, "queue-a");
        assert_eq!(status.groups.len(), 1);
        assert_eq!(status.groups[0].slug, "group-a");
        assert_eq!(status.project.queue_count, 1);
        assert_eq!(status.project.group_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn yaml_validation_rejects_invalid_content() {
        let parsed = parse_config_content("yaml", "version: [");
        assert!(parsed.is_err());
        assert_eq!(parsed.unwrap_err().code, "AI_FLOW_CONFIG_INVALID");
    }

    #[test]
    fn merge_json_values_deep_merges_objects_and_overlays_scalars() {
        let mut base = serde_json::json!({
            "engine_mode": "auto",
            "orchestration": {
                "tool": "auto",
                "launcher": "auto"
            }
        });
        let overlay = serde_json::json!({
            "orchestration": {
                "tool": "codex"
            },
            "new_key": true
        });

        merge_json_values(&mut base, overlay);

        assert_eq!(base["engine_mode"], "auto");
        assert_eq!(base["orchestration"]["tool"], "codex");
        assert_eq!(base["orchestration"]["launcher"], "auto");
        assert_eq!(base["new_key"], true);
    }

    #[test]
    fn action_command_requires_known_action() {
        assert_eq!(command_for_action("coding"), Some("/ai-flow-plan-coding"));
        assert_eq!(
            prompt_for_action("review", "20260609-plan").unwrap(),
            "/ai-flow-plan-coding-review 20260609-plan"
        );
        assert_eq!(
            prompt_for_action("resume", "queue-a").unwrap(),
            "/ai-flow-plan-orchestrate --resume queue-a"
        );
        assert_eq!(command_for_action("unknown"), None);
    }
}
