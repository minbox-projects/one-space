use super::client::TelegramClient;
use super::storage::{load_config, save_config};
use super::types::{RemoteBotConfig, RemoteBotStatus, TelegramUser};
use super::RemoteBotManager;

#[tauri::command]
pub async fn remote_bot_get_config() -> Result<RemoteBotConfig, String> {
    load_config()
}

#[tauri::command]
pub async fn remote_bot_save_config(config: RemoteBotConfig) -> Result<(), String> {
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
