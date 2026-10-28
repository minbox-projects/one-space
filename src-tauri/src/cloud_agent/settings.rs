use std::fs;
use std::path::PathBuf;

use crate::cloud_agent::types::CloudAgentSettings;

pub fn settings_file_path() -> Result<PathBuf, String> {
    let app_dir = crate::config::get_app_dir().map_err(|e| e.to_string())?;
    Ok(app_dir.join("cloud_agent_settings.json"))
}

pub fn load_settings() -> CloudAgentSettings {
    let Ok(path) = settings_file_path() else {
        return CloudAgentSettings::default();
    };

    if !path.exists() {
        return CloudAgentSettings::default();
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => CloudAgentSettings::default(),
    }
}

pub fn save_settings(settings: &CloudAgentSettings) -> Result<(), String> {
    let path = settings_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let json_bytes = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, json_bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
    Ok(())
}
