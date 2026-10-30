use crate::cloud_agent::gateway_sync::sync_gateway_to_cloud;
use crate::cloud_agent::local_plans;
use crate::cloud_agent::plan_transfer;
use crate::cloud_agent::settings::{load_settings, save_settings};
use crate::cloud_agent::ssh::{
    cancel_plan, create_session, get_cloud_status, get_plan_capabilities, inspect_plan,
    kill_session, list_sessions, list_tasks, list_worktrees, submit_plan,
};
use crate::cloud_agent::terminal::{launch_cloud_remote_command, launch_cloud_terminal_session};
use crate::cloud_agent::types::{
    CloudAgentSettings, CloudAgentStatus, CloudAgentTask, CloudPlanCapabilities,
    CloudPlanSubmitInput, CloudPlanSubmitResult, CloudSession, CloudWorktree, GatewaySyncResult,
    LocalPlanContext, LocalPlanSummary,
};

pub(crate) fn resolve_host(host: Option<String>, settings: &CloudAgentSettings) -> Result<String, String> {
    let resolved = host
        .filter(|h| !h.trim().is_empty())
        .or_else(|| {
            if !settings.default_host.trim().is_empty() {
                Some(settings.default_host.clone())
            } else {
                None
            }
        });

    resolved.ok_or_else(|| "未指定且未配置云端主机，请先选择或配置主机".to_string())
}

#[tauri::command]
pub fn cloud_agent_get_settings() -> Result<CloudAgentSettings, String> {
    Ok(load_settings())
}

#[tauri::command]
pub fn cloud_agent_save_settings(settings: CloudAgentSettings) -> Result<(), String> {
    save_settings(&settings)
}

#[tauri::command]
pub async fn cloud_agent_get_status(host: Option<String>) -> Result<CloudAgentStatus, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    get_cloud_status(&effective_host, &settings.remote_agent_dir).await
}

#[tauri::command]
pub async fn cloud_agent_list_sessions(
    host: Option<String>,
) -> Result<Vec<CloudSession>, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    list_sessions(&effective_host).await
}

#[tauri::command]
pub async fn cloud_agent_list_worktrees(
    host: Option<String>,
) -> Result<Vec<CloudWorktree>, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    list_worktrees(&effective_host).await
}

#[tauri::command]
pub async fn cloud_agent_list_tasks(
    host: Option<String>,
) -> Result<Vec<CloudAgentTask>, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    list_tasks(&effective_host, &settings.remote_agent_dir).await
}

#[tauri::command]
pub async fn cloud_agent_create_session(
    host: Option<String>,
    repo: String,
    branch: Option<String>,
    use_yolo: Option<bool>,
    extra_args: Option<String>,
) -> Result<serde_json::Value, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    let effective_yolo = use_yolo.unwrap_or(settings.opencode_yolo_mode);
    let effective_extra = extra_args.unwrap_or(settings.opencode_extra_args);

    create_session(
        &effective_host,
        &repo,
        branch.as_deref(),
        effective_yolo,
        &effective_extra,
    )
    .await
}

#[tauri::command]
pub async fn cloud_agent_kill_session(
    host: Option<String>,
    session_name: String,
) -> Result<bool, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    kill_session(&effective_host, &session_name).await
}

#[tauri::command]
pub fn cloud_agent_launch_terminal(
    host: Option<String>,
    target: String,
    branch: Option<String>,
    use_yolo: Option<bool>,
    extra_args: Option<String>,
    terminal_app: Option<String>,
) -> Result<(), String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    let effective_yolo = use_yolo.unwrap_or(settings.opencode_yolo_mode);
    let effective_extra = extra_args.unwrap_or(settings.opencode_extra_args);
    let effective_term = terminal_app
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(settings.terminal_app);

    launch_cloud_terminal_session(
        &effective_host,
        &target,
        branch.as_deref(),
        effective_yolo,
        &effective_extra,
        Some(&effective_term),
    )
}

#[tauri::command]
pub async fn cloud_agent_sync_gateway(
    host: Option<String>,
) -> Result<GatewaySyncResult, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    sync_gateway_to_cloud(&effective_host, &settings.remote_agent_dir).await
}

#[tauri::command]
pub fn cloud_agent_plan_context(workspace_id: String) -> Result<LocalPlanContext, String> {
    local_plans::list_local_plans(&workspace_id)
}

#[tauri::command]
pub fn cloud_agent_plan_list(workspace_id: String) -> Result<Vec<LocalPlanSummary>, String> {
    Ok(local_plans::list_local_plans(&workspace_id)?.plans)
}

#[tauri::command]
pub async fn cloud_agent_plan_submit(
    input: CloudPlanSubmitInput,
) -> Result<CloudPlanSubmitResult, String> {
    let settings = load_settings();
    let effective_host = resolve_host(input.host.clone(), &settings)?;

    let (workspace_name, root_path) =
        local_plans::read_workspace_root(&input.workspace_id)?;
    let repository =
        local_plans::read_repository_info(&root_path, input.base_branch.as_deref())?;
    local_plans::validate_selected_plan(&root_path, &input.plan_directory)?;
    let payload =
        local_plans::collect_plan_payload(&root_path, &input.plan_directory)?;

    let request_id = if input.request_id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        input.request_id.trim().to_string()
    };

    let request = plan_transfer::build_plan_submit_request(
        &input.workspace_id,
        &workspace_name,
        &root_path,
        Some(repository),
        payload,
    );
    let stdin_data = serde_json::to_vec(&request)
        .map_err(|e| format!("序列化计划提交请求失败: {}", e))?;

    let mut result = submit_plan(&effective_host, &settings.remote_agent_dir, &stdin_data).await?;
    if result.request_id.trim().is_empty() {
        result.request_id = request_id;
    }
    Ok(result)
}

#[tauri::command]
pub async fn cloud_agent_plan_cancel(
    host: Option<String>,
    job_id: String,
) -> Result<CloudAgentTask, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    cancel_plan(&effective_host, &settings.remote_agent_dir, &job_id).await
}

#[tauri::command]
pub async fn cloud_agent_plan_attach(
    host: Option<String>,
    job_id: String,
    terminal_app: Option<String>,
) -> Result<(), String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    let task = inspect_plan(&effective_host, &settings.remote_agent_dir, &job_id).await?;

    let session_url = task
        .session_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "云端任务缺少实时会话地址 (session_url)，无法接管".to_string())?;
    let session_id = task
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "云端任务缺少实时会话 ID (session_id)，无法接管".to_string())?;
    let remote_root = task
        .remote_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "云端任务缺少云端工作目录 (remote_root)，无法接管".to_string())?;

    let command = plan_transfer::build_plan_attach_command(session_url, session_id, remote_root)?;
    let effective_term = terminal_app
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(settings.terminal_app);
    launch_cloud_remote_command(&effective_host, &command, Some(&effective_term))
}

#[tauri::command]
pub async fn cloud_agent_plan_capabilities(
    host: Option<String>,
) -> Result<CloudPlanCapabilities, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    get_plan_capabilities(&effective_host, &settings.remote_agent_dir).await
}
