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
