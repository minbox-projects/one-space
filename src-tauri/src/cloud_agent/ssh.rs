use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::cloud_agent::plan_transfer;
use crate::cloud_agent::types::{
    CloudAgentStatus, CloudAgentTask, CloudPlanCapabilities, CloudPlanSubmitResult, CloudSession,
    CloudWorktree,
};

const SSH_TIMEOUT: Duration = Duration::from_secs(12);
const PLAN_UPLOAD_TIMEOUT: Duration = Duration::from_secs(600);

pub(super) async fn run_with_stdin(cmd: Command, stdin_data: &[u8]) -> Result<Output, String> {
    run_with_stdin_timeout(cmd, stdin_data, SSH_TIMEOUT).await
}

/// Spawn a command, stream `stdin_data`, and wait for its bounded completion.
///
/// The child is killed when the future is dropped, including on timeout.
pub async fn run_with_stdin_timeout(
    mut cmd: Command,
    stdin_data: &[u8],
    limit: Duration,
) -> Result<Output, String> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let operation = async move {
        let mut child = cmd.spawn().map_err(|e| format!("启动命令失败: {}", e))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "无法获取命令标准输入".to_string())?;

        let write_result: Result<(), String> = async {
            stdin
                .write_all(stdin_data)
                .await
                .map_err(|e| format!("写入标准输入失败: {}", e))?;
            stdin
                .shutdown()
                .await
                .map_err(|e| format!("关闭标准输入失败: {}", e))
        }
        .await;
        drop(stdin);

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| format!("等待命令结束失败: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                format!("远程命令执行失败，退出码: {:?}", output.status.code())
            } else {
                stderr
            });
        }

        write_result?;
        Ok(output)
    };

    match timeout(limit, operation).await {
        Ok(result) => result,
        Err(_) => Err(format!("命令执行超时 ({}秒)", limit.as_secs())),
    }
}

fn ssh_command(host: &str, command: &str) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.arg("-o")
        .arg("ConnectTimeout=5")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(host)
        .arg(command);
    cmd
}

pub async fn run_ssh_command(host: &str, command: &str) -> Result<String, String> {
    let child = ssh_command(host, command).output();
    let output = match timeout(SSH_TIMEOUT, child).await {
        Ok(res) => res.map_err(|e| format!("执行 SSH 命令失败: {}", e))?,
        Err(_) => return Err(format!("连接远程主机 {} 超时 ({}秒)", host, SSH_TIMEOUT.as_secs())),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("远程命令执行失败，退出码: {:?}", output.status.code())
        } else {
            stderr
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub async fn run_ssh_command_with_stdin(
    host: &str,
    command: &str,
    stdin_data: &[u8],
) -> Result<String, String> {
    run_ssh_command_with_stdin_deadline(host, command, stdin_data, SSH_TIMEOUT).await
}

/// Stream stdin to a remote command with an explicit transfer deadline.
pub async fn run_ssh_command_with_stdin_deadline(
    host: &str,
    command: &str,
    stdin_data: &[u8],
    limit: Duration,
) -> Result<String, String> {
    let cmd = ssh_command(host, command);
    let output = match run_with_stdin_timeout(cmd, stdin_data, limit).await {
        Ok(output) => output,
        Err(error) => {
            return Err(format!("连接远程主机 {} 失败或超时: {}", host, error));
        }
    };

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub async fn get_cloud_status(
    host: &str,
    remote_agent_dir: &str,
) -> Result<CloudAgentStatus, String> {
    let agent_py = format!("{}/agent.py", remote_agent_dir.trim_end_matches('/'));
    let cmd = format!("python3 {} status --json", agent_py);
    let raw = run_ssh_command(host, &cmd).await?;
    serde_json::from_str::<CloudAgentStatus>(&raw)
        .map_err(|e| format!("解析云端状态数据失败: {} (原始输出: {})", e, raw))
}

pub async fn list_sessions(host: &str) -> Result<Vec<CloudSession>, String> {
    let cmd = "cloud-session list --json";
    let raw = run_ssh_command(host, cmd).await?;
    serde_json::from_str::<Vec<CloudSession>>(&raw)
        .map_err(|e| format!("解析云端会话列表失败: {} (原始输出: {})", e, raw))
}

pub async fn list_worktrees(host: &str) -> Result<Vec<CloudWorktree>, String> {
    let cmd = "cloud-session worktrees --json";
    let raw = run_ssh_command(host, cmd).await?;
    serde_json::from_str::<Vec<CloudWorktree>>(&raw)
        .map_err(|e| format!("解析云端 Worktree 列表失败: {} (原始输出: {})", e, raw))
}

pub async fn list_tasks(host: &str, remote_agent_dir: &str) -> Result<Vec<CloudAgentTask>, String> {
    let agent_py = format!("{}/agent.py", remote_agent_dir.trim_end_matches('/'));
    let cmd = format!("python3 {} tasks --json", agent_py);
    let raw = run_ssh_command(host, &cmd).await?;
    serde_json::from_str::<Vec<CloudAgentTask>>(&raw)
        .map_err(|e| format!("解析自动流水线任务失败: {} (原始输出: {})", e, raw))
}

pub async fn create_session(
    host: &str,
    repo: &str,
    branch: Option<&str>,
    use_yolo: bool,
    extra_args: &str,
) -> Result<serde_json::Value, String> {
    let mut cmd = format!("cloud-session create {}", repo);
    if let Some(b) = branch.filter(|b| !b.is_empty()) {
        cmd.push(' ');
        cmd.push_str(b);
    }
    if !use_yolo {
        cmd.push_str(" --no-yolo");
    }
    if !extra_args.trim().is_empty() {
        cmd.push_str(" --extra-args ");
        cmd.push_str(&format!("'{}'", extra_args.trim().replace('\'', "'\\''")));
    }
    cmd.push_str(" --no-attach --json");

    let raw = run_ssh_command(host, &cmd).await?;
    serde_json::from_str::<serde_json::Value>(&raw)
        .map_err(|e| format!("创建远程会话失败: {} (原始输出: {})", e, raw))
}

pub async fn kill_session(host: &str, session_name: &str) -> Result<bool, String> {
    let cmd = format!("cloud-session kill {} --json", session_name);
    let raw = run_ssh_command(host, &cmd).await?;
    let val: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("终止远程会话失败: {} (原始输出: {})", e, raw))?;
    Ok(val.get("success").and_then(|v| v.as_bool()).unwrap_or(false))
}

fn agent_py_path(remote_agent_dir: &str) -> String {
    format!("{}/agent.py", remote_agent_dir.trim_end_matches('/'))
}

fn map_plan_transport_error(error: &str) -> String {
    plan_transfer::map_plan_command_error(false, "", error)
}

fn parse_plan_response(raw: &str) -> Result<serde_json::Value, String> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        plan_transfer::map_plan_command_error(
            false,
            raw,
            &format!("解析云端计划响应失败: {}", e),
        )
    })?;
    if value.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
        let error = value
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("云端拒绝了计划命令");
        return Err(plan_transfer::map_plan_command_error(false, error, ""));
    }
    Ok(value)
}

fn task_from_response(
    value: serde_json::Value,
    raw: &str,
) -> Result<CloudAgentTask, String> {
    let task_value = value.get("task").cloned().unwrap_or(value);
    serde_json::from_value(task_value)
        .map_err(|e| format!("解析云端任务失败: {} (原始输出: {})", e, raw))
}

pub async fn get_plan_capabilities(
    host: &str,
    remote_agent_dir: &str,
) -> Result<CloudPlanCapabilities, String> {
    let cmd = format!(
        "python3 {} plan capabilities --json",
        agent_py_path(remote_agent_dir)
    );
    let raw = run_ssh_command(host, &cmd)
        .await
        .map_err(|error| map_plan_transport_error(&error))?;
    let value = parse_plan_response(&raw)?;
    let capabilities = value
        .get("capabilities")
        .cloned()
        .unwrap_or_else(|| value.clone());
    serde_json::from_value(capabilities)
        .map_err(|e| format!("解析云端计划能力失败: {} (原始输出: {})", e, raw))
}

pub async fn submit_plan(
    host: &str,
    remote_agent_dir: &str,
    payload: &[u8],
) -> Result<CloudPlanSubmitResult, String> {
    let cmd = format!(
        "python3 {} plan submit --json",
        agent_py_path(remote_agent_dir)
    );
    let raw = run_ssh_command_with_stdin_deadline(host, &cmd, payload, PLAN_UPLOAD_TIMEOUT)
        .await
        .map_err(|error| map_plan_transport_error(&error))?;
    match plan_transfer::parse_plan_submit_response(&raw) {
        Ok(result) => Ok(result),
        Err(error) => Err(plan_transfer::map_plan_command_error(false, &raw, &error)),
    }
}

fn validate_job_id(job_id: &str) -> Result<(), String> {
    let job_id = job_id.trim();
    if job_id.is_empty() {
        return Err("云端任务 ID 不能为空".to_string());
    }
    if !job_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
    {
        return Err(format!("云端任务 ID 含非法字符: {}", job_id));
    }
    Ok(())
}

pub async fn cancel_plan(
    host: &str,
    remote_agent_dir: &str,
    job_id: &str,
) -> Result<CloudAgentTask, String> {
    let job_id = job_id.trim();
    validate_job_id(job_id)?;
    let cmd = format!(
        "python3 {} plan cancel {} --json",
        agent_py_path(remote_agent_dir),
        job_id
    );
    let raw = run_ssh_command(host, &cmd)
        .await
        .map_err(|error| map_plan_transport_error(&error))?;
    let value = parse_plan_response(&raw)?;
    task_from_response(value, &raw)
}

pub async fn inspect_plan(
    host: &str,
    remote_agent_dir: &str,
    job_id: &str,
) -> Result<CloudAgentTask, String> {
    let job_id = job_id.trim();
    validate_job_id(job_id)?;
    let cmd = format!(
        "python3 {} plan inspect {} --json",
        agent_py_path(remote_agent_dir),
        job_id
    );
    let raw = run_ssh_command(host, &cmd)
        .await
        .map_err(|error| map_plan_transport_error(&error))?;
    let value = parse_plan_response(&raw)?;
    task_from_response(value, &raw)
}
