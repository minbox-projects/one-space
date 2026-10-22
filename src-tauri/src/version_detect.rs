use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionInfo {
    pub tool: String,
    pub version: String,
    pub is_installed: bool,
}

/// 检测 CLI 工具版本
#[tauri::command]
pub async fn detect_cli_version(tool: String) -> Result<VersionInfo, String> {
    tauri::async_runtime::spawn_blocking(move || detect_cli_version_impl(tool))
        .await
        .map_err(|e| format!("Failed to join version detection task: {}", e))?
}

fn detect_cli_version_impl(tool: String) -> Result<VersionInfo, String> {
    let cmd_name = match tool.as_str() {
        "claude" => "claude",
        "codex" => "codex",
        "antigravity" => "agy",
        "opencode" => "opencode",
        _ => return Err(format!("Unknown tool: {}", tool)),
    };

    let probe = crate::cli_probe::probe_cli_version(cmd_name);
    Ok(VersionInfo {
        tool,
        version: probe.version,
        is_installed: probe.installed,
    })
}

