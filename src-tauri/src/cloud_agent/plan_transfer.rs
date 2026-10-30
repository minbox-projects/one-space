use serde_json::{json, Value};

use crate::cloud_agent::types::{
    CloudPlanRepository, CloudPlanSubmitResult, LocalPlanPayload,
};

/// Build the stdin envelope for a plan submission.
///
/// The complete payload, including file bytes, stays inside the returned JSON
/// so callers can stream it to the remote command without shell interpolation.
pub fn build_plan_submit_request(
    workspace_id: &str,
    workspace_name: &str,
    root_path: &str,
    repository: Option<CloudPlanRepository>,
    payload: LocalPlanPayload,
) -> Value {
    let files = payload
        .files
        .into_iter()
        .map(|file| {
            json!({
                "path": file.path,
                "content_base64": file.content_base64,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "schema_version": 1,
        "workspace": {
            "id": workspace_id,
            "name": workspace_name,
            "root_path": root_path,
        },
        "repository": repository,
        "plan": {
            "plan_id": payload.plan_id,
            "title": payload.plan_title,
            "directory": payload.directory,
            "files": files,
        },
        "policy": {
            "auto_permission_once": true,
            "auto_recommended_questions": true,
            "auto_merge": false,
        },
    })
}

/// Parse a structured `agent.py plan submit --json` response.
pub fn parse_plan_submit_response(raw: &str) -> Result<CloudPlanSubmitResult, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|e| format!("解析云端计划提交响应失败: {} (原始输出: {})", e, raw))?;

    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("云端拒绝了计划提交")
            .to_string());
    }

    serde_json::from_value(value)
        .map_err(|e| format!("解析云端计划提交结果失败: {} (原始输出: {})", e, raw))
}

/// Map a failed plan transport command to an actionable message.
///
/// Unknown or invalid `plan` subcommands indicate an older cloud agent that
/// needs an upgrade. Otherwise the original stderr/stdout is preserved.
pub fn map_plan_command_error(status_success: bool, stdout: &str, stderr: &str) -> String {
    if status_success {
        return String::new();
    }

    let stderr = stderr.trim();
    let stdout = stdout.trim();
    let primary = if !stderr.is_empty() { stderr } else { stdout };
    let lower = primary.to_lowercase();

    let looks_like_unknown_plan_command = [
        "unknown command",
        "unknown subcommand",
        "unknown sub-command",
        "invalid choice",
        "no such command",
        "unrecognized",
        "invalid subcommand",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        && (lower.contains("plan")
            || lower.contains("invalid choice")
            || lower.contains("no such command")
            || lower.contains("unknown command"));

    if looks_like_unknown_plan_command {
        return format!(
            "云端 Agent 版本过旧，不支持 plan 子命令，请先升级云端 OneSpace Cloud Agent (upgrade)。原始错误: {}",
            primary
        );
    }

    if primary.is_empty() {
        "云端计划命令执行失败，请检查云端 Agent 版本或日志".to_string()
    } else {
        primary.to_string()
    }
}

/// Build the exact remote `opencode attach` command.
///
/// Values are single-quoted when any value contains characters outside the
/// conservative safe set, with embedded single quotes escaped for POSIX shells.
pub fn build_plan_attach_command(
    session_url: &str,
    session_id: &str,
    cloud_root: &str,
) -> Result<String, String> {
    let session_url = session_url.trim();
    let session_id = session_id.trim();
    let cloud_root = cloud_root.trim();

    if session_url.is_empty() {
        return Err("会话地址 (session_url) 不能为空".to_string());
    }
    if session_id.is_empty() {
        return Err("会话 ID (session_id) 不能为空".to_string());
    }
    if cloud_root.is_empty() {
        return Err("云端工作目录 (cloud_root) 不能为空".to_string());
    }

    let needs_quoting = [session_url, session_id, cloud_root]
        .iter()
        .any(|value| !value.chars().all(is_shell_safe_char));

    let quote = |value: &str| {
        if needs_quoting {
            format!("'{}'", value.replace('\'', "'\\''"))
        } else {
            value.to_string()
        }
    };

    Ok(format!(
        "opencode attach {} --session {} --dir {}",
        quote(session_url),
        quote(session_id),
        quote(cloud_root)
    ))
}

fn is_shell_safe_char(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(
            character,
            '_' | '.' | '/' | ':' | '@' | '%' | '+' | '=' | ',' | '-'
        )
}
