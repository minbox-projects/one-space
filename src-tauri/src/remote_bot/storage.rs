use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use super::types::RemoteBotConfig;

static BOT_STORAGE_LOCK: Mutex<()> = Mutex::new(());

fn get_storage_dir() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    Ok(dir)
}

pub fn get_bot_config_path() -> Result<PathBuf, String> {
    let dir = get_storage_dir()?;
    Ok(dir.join("remote_bot.json"))
}

pub fn load_config() -> Result<RemoteBotConfig, String> {
    let _guard = BOT_STORAGE_LOCK.lock().map_err(|e| e.to_string())?;
    let path = get_bot_config_path()?;
    if !path.exists() {
        return Ok(RemoteBotConfig::default());
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read remote bot config: {}", e))?;
    let config: RemoteBotConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse remote bot config: {}", e))?;
    Ok(config)
}

pub fn save_config(config: &RemoteBotConfig) -> Result<(), String> {
    let _guard = BOT_STORAGE_LOCK.lock().map_err(|e| e.to_string())?;
    let path = get_bot_config_path()?;
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize remote bot config: {}", e))?;

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, content)
        .map_err(|e| format!("Failed to write tmp config: {}", e))?;
    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to replace remote bot config: {}", e))?;
    Ok(())
}
