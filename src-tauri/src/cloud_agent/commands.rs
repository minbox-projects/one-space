use crate::cloud_agent::gateway_sync::sync_gateway_to_cloud;
use crate::cloud_agent::settings::{load_settings, save_settings};
use crate::cloud_agent::ssh::{
    create_session, get_cloud_status, kill_session, list_sessions, list_tasks, list_worktrees,
};
use crate::cloud_agent::terminal::launch_cloud_terminal_session;
use crate::cloud_agent::types::{
    CloudAgentSettings, CloudAgentStatus, CloudAgentTask, CloudSession, CloudWorktree,
    GatewaySyncResult,
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
