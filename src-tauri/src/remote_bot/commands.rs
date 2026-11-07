use std::collections::HashSet;
use std::path::PathBuf;

use crate::cli_probe::{augmented_path, extract_semver};
use super::client::TelegramClient;
use super::storage::{load_config, save_config};
use super::types::{OpenCodeInstalledVersion, RemoteBotConfig, RemoteBotStatus, TelegramUser};
use super::RemoteBotManager;

#[tauri::command]
pub async fn remote_bot_get_config() -> Result<RemoteBotConfig, String> {
    load_config()
}

#[tauri::command]
pub async fn remote_bot_save_config(mut config: RemoteBotConfig) -> Result<(), String> {
    if config.enabled {
        let has_bin = config.opencode_bin_path.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);
        if !has_bin {
            return Err("必须选择一个具体的 OpenCode 版本后才能启用 Telegram 远程工作台".to_string());
        }
    }

    // 若指定了 opencode_bin_path，自动检测并填充版本信息
    if let Some(ref bin_path) = config.opencode_bin_path {
        let trimmed = bin_path.trim();
        if !trimmed.is_empty() {
            let resolved = resolve_executable_path(trimmed);
            if let Ok(output) = std::process::Command::new(&resolved).arg("--version").output() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let raw = if !stdout.is_empty() { stdout } else { stderr };
                if let Some(ver) = extract_semver(&raw) {
                    let major = ver.split('.').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(1);
                    config.opencode_version = Some(ver);
                    config.opencode_version_type = Some(if major >= 2 { "v2".to_string() } else { "v1".to_string() });
                } else if !raw.is_empty() {
                    config.opencode_version = Some(raw);
                    config.opencode_version_type = Some("unknown".to_string());
                }
            }
        }
    }

    save_config(&config)?;
    // 保存配置后，自动热重载或重启后台轮询任务
    if let Some(manager) = RemoteBotManager::global() {
        manager.reload().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn remote_bot_test_connection(bot_token: String) -> Result<TelegramUser, String> {
    let token = if !bot_token.trim().is_empty() {
        bot_token.trim().to_string()
    } else if let Ok(env_token) = std::env::var("TELEGRAM_BOT_TOKEN") {
        env_token.trim().to_string()
    } else {
        return Err("Bot Token 不能为空".to_string());
    };

    if token.is_empty() {
        return Err("Bot Token 不能为空".to_string());
    }

    let client = TelegramClient::new(token);
    client.get_me().await
}

#[tauri::command]
pub async fn remote_bot_get_status() -> Result<RemoteBotStatus, String> {
    if let Some(manager) = RemoteBotManager::global() {
        Ok(manager.get_status().await)
    } else {
        Ok(RemoteBotStatus::default())
    }
}

#[tauri::command]
pub async fn remote_bot_restart() -> Result<RemoteBotStatus, String> {
    if let Some(manager) = RemoteBotManager::global() {
        manager.reload().await;
        Ok(manager.get_status().await)
    } else {
        Err("RemoteBotManager 未初始化".to_string())
    }
}

#[tauri::command]
pub async fn remote_bot_probe_opencode(bin_path: Option<String>) -> Result<String, String> {
    let raw_bin = bin_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("opencode");

    let bin = resolve_executable_path(raw_bin);

    let output = tokio::process::Command::new(&bin)
        .arg("--version")
        .output()
        .await
        .map_err(|e| format!("无法执行 [{}]: {}。请检查路径或确认是否已安装 OpenCode CLI", bin, e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() {
            Ok(stdout)
        } else {
            Ok(format!("OpenCode CLI 执行成功 ({})", bin))
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("OpenCode 探针非零退出: {}", if stderr.is_empty() { "未知错误".to_string() } else { stderr }))
    }
}

#[tauri::command]
pub async fn remote_bot_get_running_opencode_sessions() -> Result<Vec<super::types::ExternalOpenCodeSessionInfo>, String> {
    Ok(super::opencode_inspector::discover_external_opencode_sessions().await)
}

#[tauri::command]
pub async fn remote_bot_get_hosted_sessions() -> Result<Vec<super::types::HostedSessionInfo>, String> {
    if let Some(manager) = RemoteBotManager::global() {
        if let Some(router) = manager.get_router().await {
            return Ok(router.supervisor.list_hosted_sessions(None).await);
        }
    }
    Ok(Vec::new())
}

#[tauri::command]
pub async fn remote_bot_list_installed_opencode() -> Result<Vec<OpenCodeInstalledVersion>, String> {
    Ok(tokio::task::spawn_blocking(scan_installed_opencode_versions).await.unwrap_or_default())
}

/// 扫描本机所有候选路径，发现并解析已安装的 OpenCode 版本
pub fn scan_installed_opencode_versions() -> Vec<OpenCodeInstalledVersion> {
    let mut candidates = Vec::<PathBuf>::new();
    let mut seen_canonical = HashSet::<PathBuf>::new();

    // 1. 如果已有配置中的路径，优先加入候选
    if let Ok(config) = load_config() {
        if let Some(ref path) = config.opencode_bin_path {
            let resolved = resolve_executable_path(path);
            let pb = PathBuf::from(&resolved);
            if pb.is_file() {
                candidates.push(pb);
            }
        }
    }

    // 2. 遍历 augmented_path 中的目录
    if let Some(paths_os) = augmented_path() {
        for dir in std::env::split_paths(&paths_os) {
            let candidate = dir.join("opencode");
            if candidate.is_file() {
                candidates.push(candidate);
            }
        }
    }

    // 3. 常见默认安装路径（作为保底）
    if let Some(home) = dirs::home_dir() {
        let extras = [
            home.join(".opencode").join("bin").join("opencode"),
            home.join(".local").join("bin").join("opencode"),
            home.join(".bun").join("bin").join("opencode"),
            home.join(".npm-global").join("bin").join("opencode"),
            PathBuf::from("/opt/homebrew/bin/opencode"),
            PathBuf::from("/usr/local/bin/opencode"),
        ];
        for extra in extras {
            if extra.is_file() {
                candidates.push(extra);
            }
        }
    }

    let active_path_opt = load_config().ok().and_then(|c| c.opencode_bin_path);
    let resolved_active = active_path_opt.as_deref().map(resolve_executable_path);

    let mut results = Vec::new();

    for candidate in candidates {
        let canonical_key = candidate.canonicalize().unwrap_or_else(|_| candidate.clone());
        if !seen_canonical.insert(canonical_key.clone()) {
            continue;
        }

        let mut cmd = std::process::Command::new(&candidate);
        cmd.arg("--version");
        if let Some(p) = augmented_path() {
            cmd.env("PATH", p);
        }

        let Ok(output) = cmd.output() else {
            continue;
        };

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let raw = if !stdout.is_empty() { stdout } else { stderr };
        if raw.is_empty() {
            continue;
        }

        let version_str = extract_semver(&raw).unwrap_or_else(|| raw.clone());
        let major = version_str
            .split('.')
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or_else(|| {
                if version_str.contains("2.") { 2 } else { 1 }
            });

        let version_type = if major >= 2 { "v2".to_string() } else { "v1".to_string() };
        let path_str = candidate.to_string_lossy().to_string();

        let is_active = resolved_active.as_deref().map(|act| {
            act == path_str || act == canonical_key.to_string_lossy()
        }).unwrap_or(false);

        results.push(OpenCodeInstalledVersion {
            path: path_str,
            version: version_str,
            major_version: major,
            version_type,
            is_active,
        });
    }

    // 排序：先按 major_version 降序（v2 优先），再按 version 降序
    results.sort_by(|a, b| {
        b.major_version
            .cmp(&a.major_version)
            .then_with(|| b.version.cmp(&a.version))
            .then_with(|| a.path.cmp(&b.path))
    });

    results
}

/// 解析可能带有波浪号 `~` 的可执行路径
pub fn resolve_executable_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&trimmed[2..]).to_string_lossy().to_string();
        }
    }
    trimmed.to_string()
}
