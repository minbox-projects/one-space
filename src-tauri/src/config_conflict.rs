use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use std::time::SystemTime;

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigConflict {
    pub file_path: String,
    pub file_modified_at: u64,
    pub last_applied_at: u64,
    pub has_conflict: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConflictCheckResult {
    pub has_conflicts: bool,
    pub conflicts: Vec<ConfigConflict>,
}

/// 获取配置文件的最后修改时间
fn get_file_mtime(path: &PathBuf) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|t| t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis() as u64)
}

/// 检测配置文件冲突
#[tauri::command]
pub fn check_config_conflicts(
    tool: String,
    last_applied_timestamp: Option<u64>,
) -> Result<ConflictCheckResult, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    
    let config_paths: Vec<(&str, PathBuf)> = match tool.as_str() {
        "claude" => vec![
            ("claude_settings", home_dir.join(".claude").join("settings.json")),
            ("claude_main", home_dir.join(".claude.json")),
        ],
        "codex" => vec![
            ("codex_config", home_dir.join(".codex").join("config.toml")),
        ],
        "gemini" => vec![
            ("gemini_settings", home_dir.join(".gemini").join("settings.json")),
        ],
        "opencode" => vec![
            ("opencode_config", home_dir.join(".config").join("opencode").join("opencode.json")),
        ],
        _ => return Err(format!("Unknown tool: {}", tool)),
    };
    
    let mut conflicts = vec![];
    
    for (name, path) in config_paths {
        if !path.exists() {
            continue;
        }
        
        if let Some(mtime) = get_file_mtime(&path) {
            if let Some(last_applied) = last_applied_timestamp {
                // 如果文件在最后一次应用后被修改过，则存在冲突
                let has_conflict = mtime > last_applied;
                
                if has_conflict {
                    conflicts.push(ConfigConflict {
                        file_path: path.to_string_lossy().to_string(),
                        file_modified_at: mtime,
                        last_applied_at: last_applied,
                        has_conflict,
                    });
                }
            }
        }
    }
    
    Ok(ConflictCheckResult {
        has_conflicts: !conflicts.is_empty(),
        conflicts,
    })
}

/// 强制应用配置（忽略冲突）
#[tauri::command]
pub async fn apply_ai_environment_force(
    provider: crate::ai_env::AiProvider,
) -> Result<(), String> {
    // 直接调用原来的 apply_ai_environment 函数
    crate::ai_env::apply_ai_environment(provider).await
}
