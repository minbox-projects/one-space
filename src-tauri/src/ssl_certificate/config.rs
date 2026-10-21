use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

static CONFIG_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SslConfig {
    #[serde(default = "default_auto_check_enabled")]
    pub auto_check_enabled: bool,
    #[serde(default = "default_check_times")]
    pub check_times: Vec<String>,
    #[serde(default = "default_alert_days_threshold")]
    pub alert_days_threshold: u32,
}

fn default_auto_check_enabled() -> bool {
    true
}

fn default_check_times() -> Vec<String> {
    vec!["09:00".to_string()]
}

fn default_alert_days_threshold() -> u32 {
    30
}

impl Default for SslConfig {
    fn default() -> Self {
        Self {
            auto_check_enabled: default_auto_check_enabled(),
            check_times: default_check_times(),
            alert_days_threshold: default_alert_days_threshold(),
        }
    }
}

pub fn is_valid_hh_mm(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    let (h_str, m_str) = (parts[0], parts[1]);
    if h_str.len() != 2 || m_str.len() != 2 {
        return false;
    }
    let Ok(hour) = h_str.parse::<u32>() else {
        return false;
    };
    let Ok(minute) = m_str.parse::<u32>() else {
        return false;
    };
    hour < 24 && minute < 60
}

pub fn normalize_config(mut config: SslConfig) -> SslConfig {
    let mut valid_set = BTreeSet::new();
    for time_str in &config.check_times {
        let trimmed = time_str.trim();
        if is_valid_hh_mm(trimmed) {
            valid_set.insert(trimmed.to_string());
        }
    }

    if valid_set.is_empty() {
        config.check_times = default_check_times();
    } else {
        config.check_times = valid_set.into_iter().collect();
    }

    if config.alert_days_threshold == 0 {
        config.alert_days_threshold = default_alert_days_threshold();
    } else if config.alert_days_threshold > 365 {
        config.alert_days_threshold = 365;
    }

    config
}

fn get_config_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    Ok(dir.join("ssl_config.json"))
}

pub fn load_config() -> Result<SslConfig, String> {
    let _guard = CONFIG_LOCK.lock().map_err(|e| e.to_string())?;
    let path = get_config_path()?;
    if !path.exists() {
        return Ok(SslConfig::default());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read ssl_config.json: {}", e))?;
    if content.trim().is_empty() {
        return Ok(SslConfig::default());
    }
    let config: SslConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse ssl_config.json: {}", e))?;
    Ok(normalize_config(config))
}

pub fn save_config(config: &SslConfig) -> Result<SslConfig, String> {
    let _guard = CONFIG_LOCK.lock().map_err(|e| e.to_string())?;
    let normalized = normalize_config(config.clone());
    let path = get_config_path()?;
    let json_bytes = serde_json::to_vec_pretty(&normalized)
        .map_err(|e| format!("Failed to serialize ssl config: {}", e))?;

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, json_bytes)
        .map_err(|e| format!("Failed to write tmp ssl config file: {}", e))?;
    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename ssl config file: {}", e))?;
    Ok(normalized)
}
