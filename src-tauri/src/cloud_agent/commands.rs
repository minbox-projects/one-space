use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};

use crate::cloud_agent::deployment::{
    inspect_installation, read_install_metadata, run_deploy_process_async, validate_local_payload,
    DeploymentRegistry, SshRemoteShell,
};
use crate::cloud_agent::gateway_sync::sync_gateway_to_cloud;
use crate::cloud_agent::settings::{load_settings, save_settings};
use crate::cloud_agent::ssh::{
    create_session, get_cloud_status, kill_session, list_sessions, list_tasks, list_worktrees,
};
use crate::cloud_agent::terminal::launch_cloud_terminal_session;
use crate::cloud_agent::types::{
    CloudAgentDeploymentResult, CloudAgentInstallationInfo, CloudAgentSettings, CloudAgentStatus,
    CloudAgentTask, CloudSession, CloudWorktree, GatewaySyncResult,
};

const DEPLOY_TIMEOUT: Duration = Duration::from_secs(15 * 60);

fn deployment_registry() -> &'static DeploymentRegistry {
    static REGISTRY: OnceLock<DeploymentRegistry> = OnceLock::new();
    REGISTRY.get_or_init(DeploymentRegistry::new)
}

fn resolve_payload_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .resolve("cloud-agent", BaseDirectory::Resource)
        .map_err(|error| format!("无法定位 Cloud Agent 部署资源: {}", error))
}

fn require_host(host: &str) -> Result<String, String> {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return Err("未指定云端主机，请选择一个 SSH 主机".to_string());
    }
    Ok(trimmed.to_string())
}

fn require_agent_dir(agent_dir: &str) -> Result<String, String> {
    let trimmed = agent_dir.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return Err("远程 Agent 目录无效".to_string());
    }
    Ok(trimmed.to_string())
}

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
pub async fn cloud_agent_get_installation_info(
    app: AppHandle,
    host: Option<String>,
) -> Result<CloudAgentInstallationInfo, String> {
    let settings = load_settings();
    let effective_host = resolve_host(host, &settings)?;
    let payload_dir = resolve_payload_dir(&app)?;
    let local_version = validate_local_payload(&payload_dir)?;
    let agent_dir = settings.remote_agent_dir;
    tauri::async_runtime::spawn_blocking(move || {
        let shell = SshRemoteShell;
        inspect_installation(&effective_host, &agent_dir, &local_version, &shell)
    })
    .await
    .map_err(|error| format!("Cloud Agent 安装检查任务失败: {}", error))
}

#[tauri::command]
pub async fn cloud_agent_deploy(
    app: AppHandle,
    host: String,
    operation_id: String,
) -> Result<CloudAgentDeploymentResult, String> {
    let host = require_host(&host)?;
    let operation_id = operation_id.trim().to_string();
    if operation_id.is_empty() {
        return Err("缺少部署操作标识".to_string());
    }

    let settings = load_settings();
    let agent_dir = require_agent_dir(&settings.remote_agent_dir)?;
    let payload_dir = resolve_payload_dir(&app)?;
    let local_version = validate_local_payload(&payload_dir)?;
    let deployer = payload_dir.join("deploy.sh");
    let shell = SshRemoteShell;

    let inspection = inspect_installation(&host, &agent_dir, &local_version, &shell);
    let registry = deployment_registry();
    registry.may_deploy(&host, &agent_dir, inspection.state)?;
    registry.begin(host.clone(), agent_dir.clone(), operation_id.clone())?;

    let outcome = run_deploy_process_async(
        &deployer,
        &local_version,
        &host,
        &agent_dir,
        &operation_id,
        DEPLOY_TIMEOUT,
        |progress| {
            let _ = app.emit("cloud_agent_deploy_progress", progress);
        },
    )
    .await;
    registry.finish(&operation_id);

    match outcome {
        Ok(mut result) => {
            let (_remote_version, installed_at, _reason) =
                read_install_metadata(&host, &agent_dir, &local_version, &shell);
            result.installed_at = installed_at;
            Ok(result)
        }
        Err(error) => {
            registry.record_failed(host, agent_dir);
            Err(error)
        }
    }
}
