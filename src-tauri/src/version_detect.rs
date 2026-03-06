use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionInfo {
    pub tool: String,
    pub version: String,
    pub is_installed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigCompatibility {
    pub config_key: String,
    pub min_version: String,
    pub max_version: Option<String>,
    pub deprecated_in: Option<String>,
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
        "gemini" => "gemini",
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

/// 获取 Claude Code 的配置兼容性信息
fn get_claude_configs() -> Vec<ConfigCompatibility> {
    vec![
        ConfigCompatibility {
            config_key: "dangerously_skip_permissions".to_string(),
            min_version: "0.1.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "enable_mcp".to_string(),
            min_version: "0.5.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "enable_all_memory_features".to_string(),
            min_version: "0.6.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "allowed_tools".to_string(),
            min_version: "0.1.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "blocked_tools".to_string(),
            min_version: "0.1.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
    ]
}

/// 获取 Codex 的配置兼容性信息
fn get_codex_configs() -> Vec<ConfigCompatibility> {
    vec![
        ConfigCompatibility {
            config_key: "model_reasoning_effort".to_string(),
            min_version: "1.0.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "model_reasoning_summary".to_string(),
            min_version: "1.0.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "approval_policy".to_string(),
            min_version: "1.0.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "sandbox_mode".to_string(),
            min_version: "1.0.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
    ]
}

/// 获取 Gemini 的配置兼容性信息
fn get_gemini_configs() -> Vec<ConfigCompatibility> {
    vec![
        ConfigCompatibility {
            config_key: "theme".to_string(),
            min_version: "0.1.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "vim_mode".to_string(),
            min_version: "0.1.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "default_approval_mode".to_string(),
            min_version: "0.1.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
    ]
}

/// 获取 OpenCode 的配置兼容性信息
fn get_opencode_configs() -> Vec<ConfigCompatibility> {
    vec![
        ConfigCompatibility {
            config_key: "small_model".to_string(),
            min_version: "0.1.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "timeout".to_string(),
            min_version: "0.1.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
        ConfigCompatibility {
            config_key: "share_mode".to_string(),
            min_version: "0.1.0".to_string(),
            max_version: None,
            deprecated_in: None,
        },
    ]
}

/// 检查配置兼容性
#[tauri::command]
pub fn check_config_compatibility(
    tool: String,
    version: String,
    config_key: String,
) -> Result<bool, String> {
    let configs = match tool.as_str() {
        "claude" => get_claude_configs(),
        "codex" => get_codex_configs(),
        "gemini" => get_gemini_configs(),
        "opencode" => get_opencode_configs(),
        _ => return Err(format!("Unknown tool: {}", tool)),
    };

    if let Some(compat) = configs.iter().find(|c| c.config_key == config_key) {
        return Ok(is_version_compatible(
            &version,
            &compat.min_version,
            compat.max_version.as_ref().map(|s| s.as_str()),
        ));
    }

    Ok(true) // 未知配置默认兼容
}

/// 检查版本是否兼容
fn is_version_compatible(version: &str, min_version: &str, max_version: Option<&str>) -> bool {
    let current = parse_version(version);
    let min = parse_version(min_version);

    if current < min {
        return false;
    }

    if let Some(max) = max_version {
        let max = parse_version(max);
        if current > max {
            return false;
        }
    }

    true
}

/// 解析版本号字符串为可比较的元组
fn parse_version(version: &str) -> (i32, i32, i32) {
    let parts: Vec<&str> = version.split('.').take(3).collect();

    let major: i32 = parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: i32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch: i32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    (major, minor, patch)
}

/// 获取所有配置兼容性信息
#[tauri::command]
pub fn get_all_config_compatibility(tool: String) -> Result<Vec<ConfigCompatibility>, String> {
    let configs = match tool.as_str() {
        "claude" => Ok(get_claude_configs()),
        "codex" => Ok(get_codex_configs()),
        "gemini" => Ok(get_gemini_configs()),
        "opencode" => Ok(get_opencode_configs()),
        _ => Err(format!("Unknown tool: {}", tool)),
    }?;

    Ok(configs)
}
