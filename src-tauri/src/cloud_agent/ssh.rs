use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::cloud_agent::types::{
    CloudAgentStatus, CloudAgentTask, CloudSession, CloudWorktree,
};

const SSH_TIMEOUT: Duration = Duration::from_secs(12);

pub async fn run_ssh_command(host: &str, command: &str) -> Result<String, String> {
    let mut cmd = Command::new("ssh");
    cmd.arg("-o")
        .arg("ConnectTimeout=5")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(host)
        .arg(command);

    let child = cmd.output();
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
