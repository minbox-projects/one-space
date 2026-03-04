use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackupEntry {
    pub id: String,
    pub tool: String,
    pub file_path: String,
    pub backup_path: String,
    pub file_content_hash: String,
    pub created_at: DateTime<Utc>,
    pub file_size: u64,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct BackupHistory {
    pub entries: Vec<BackupEntry>,
    pub retention_days: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupBlob {
    #[serde(default)]
    is_encrypted: bool,
    data: String,
}

// 备份目标列表
const BACKUP_TARGETS: &[(&str, &str)] = &[
    ("claude", "~/.claude.json"),
    ("claude", "~/.claude/settings.json"),
    ("claude", "~/.claude/settings.local.json"),
    ("codex", "~/.codex/config.toml"),
    ("codex", "~/.codex/auth.json"),
    ("gemini", "~/.gemini/settings.json"),
    ("gemini", "~/.gemini/.env"),
    ("opencode", "~/.config/opencode/opencode.json"),
];

fn get_backup_dir() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let backup_dir = data_dir.join("backups");
    fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    Ok(backup_dir)
}

fn get_backup_history_path() -> Result<PathBuf, String> {
    let backup_dir = get_backup_dir()?;
    Ok(backup_dir.join("backup_history.json"))
}

/// 扩展 home 目录路径
fn _expand_home_dir(path: &str) -> Result<PathBuf, String> {
    if let Some(home) = dirs::home_dir() {
        if path == "~" {
            return Ok(home);
        }
        if path.starts_with("~/") {
            return Ok(home.join(&path[2..]));
        }
    }
    Ok(PathBuf::from(path))
}

/// 加载备份历史
fn load_backup_history() -> Result<BackupHistory, String> {
    let path = get_backup_history_path()?;

    if !path.exists() {
        return Ok(BackupHistory {
            entries: vec![],
            retention_days: 30,
        });
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let history: BackupHistory = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    Ok(history)
}

/// 保存备份历史
fn save_backup_history(history: &BackupHistory) -> Result<(), String> {
    let path = get_backup_history_path()?;
    let content = serde_json::to_string_pretty(history).unwrap();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut file = File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 创建备份
#[tauri::command]
pub fn create_backup(tool: String, reason: Option<String>) -> Result<Vec<BackupEntry>, String> {
    let backup_dir = get_backup_dir()?;
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let mut entries = vec![];

    // 备份该工具相关的所有文件
    for (t, file_path) in BACKUP_TARGETS.iter() {
        if *t != tool {
            continue;
        }

        let expanded_path = expand_home_dir_path(file_path)?;
        if !expanded_path.exists() {
            continue;
        }

        // 读取文件内容
        let content = fs::read_to_string(&expanded_path).map_err(|e| e.to_string())?;
        let file_hash = format!("{:x}", Sha256::digest(content.as_bytes()));

        // 创建备份目录
        let tool_backup_dir = backup_dir.join(&tool).join(&timestamp);
        fs::create_dir_all(&tool_backup_dir).map_err(|e| e.to_string())?;

        // 保存备份文件（主密码加密）
        let master_pass = crate::crypto::get_or_init_master_password()?;
        let encrypted_content = crate::crypto::encrypt(&content, &master_pass)?;
        let blob = BackupBlob {
            is_encrypted: true,
            data: encrypted_content,
        };
        let blob_json = serde_json::to_string_pretty(&blob).map_err(|e| e.to_string())?;

        let backup_file_name = expanded_path
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or("unknown");
        let backup_path = tool_backup_dir.join(format!("{}.backup", backup_file_name));

        let mut file = File::create(&backup_path).map_err(|e| e.to_string())?;
        file.write_all(blob_json.as_bytes())
            .map_err(|e| e.to_string())?;

        let entry = BackupEntry {
            id: format!("backup-{}", uuid::Uuid::new_v4()),
            tool: tool.clone(),
            file_path: file_path.to_string(),
            backup_path: backup_path.to_string_lossy().to_string(),
            file_content_hash: file_hash,
            created_at: Utc::now(),
            file_size: content.len() as u64,
            reason: reason.clone(),
        };

        entries.push(entry);
    }

    // 更新备份历史
    if !entries.is_empty() {
        let mut history = load_backup_history()?;
        history.entries.extend(entries.clone());
        save_backup_history(&history)?;
    }

    Ok(entries)
}

/// 列出备份
#[tauri::command]
pub fn list_backups(tool: Option<String>) -> Result<Vec<BackupEntry>, String> {
    let history = load_backup_history()?;

    let mut entries = history.entries;

    if let Some(tool_filter) = tool {
        entries.retain(|e| e.tool == tool_filter);
    }

    // 按时间排序（最新的在前）
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(entries)
}

/// 恢复备份
#[tauri::command]
pub fn restore_backup(entry_id: String) -> Result<(), String> {
    let history = load_backup_history()?;
    let entry = history
        .entries
        .iter()
        .find(|e| e.id == entry_id)
        .ok_or("Backup entry not found")?
        .clone();

    // 读取备份文件（兼容历史明文备份）
    let raw_content = fs::read_to_string(&entry.backup_path).map_err(|e| e.to_string())?;
    let content = if let Ok(blob) = serde_json::from_str::<BackupBlob>(&raw_content) {
        if blob.is_encrypted {
            let master_pass = crate::crypto::get_or_init_master_password()?;
            crate::crypto::decrypt(&blob.data, &master_pass)?
        } else {
            blob.data
        }
    } else {
        raw_content
    };

    // 恢复到原始位置
    let target_path = expand_home_dir_path(&entry.file_path)?;

    // 在恢复前创建当前版本的备份
    let _ = create_backup(entry.tool.clone(), Some("pre_restore".to_string()));

    // 恢复文件
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&target_path, content).map_err(|e| e.to_string())?;

    Ok(())
}

/// 清理过期备份
#[tauri::command]
pub fn cleanup_old_backups(retention_days: Option<u32>) -> Result<usize, String> {
    let mut history = load_backup_history()?;
    let retention = retention_days.unwrap_or(history.retention_days);
    let cutoff = Utc::now() - chrono::Duration::days(retention as i64);

    let _old_count = history.entries.len();

    // 过滤掉过期条目
    let expired_entries: Vec<_> = history
        .entries
        .drain(..)
        .filter(|e| e.created_at <= cutoff)
        .collect();

    // 删除过期文件
    for entry in &expired_entries {
        let _ = fs::remove_file(&entry.backup_path);
    }

    save_backup_history(&history)?;

    Ok(expired_entries.len())
}

/// 删除备份
#[tauri::command]
pub fn delete_backup(entry_id: String) -> Result<(), String> {
    let mut history = load_backup_history()?;

    if let Some(entry) = history.entries.iter().find(|e| e.id == entry_id) {
        // 删除备份文件
        let _ = fs::remove_file(&entry.backup_path);
    }

    history.entries.retain(|e| e.id != entry_id);
    save_backup_history(&history)?;

    Ok(())
}

/// 辅助函数：扩展 home 目录路径（内部使用）
fn expand_home_dir_path(path: &str) -> Result<PathBuf, String> {
    if let Some(home) = dirs::home_dir() {
        if path == "~" {
            return Ok(home);
        }
        if path.starts_with("~/") {
            return Ok(home.join(&path[2..]));
        }
    }
    Ok(PathBuf::from(path))
}

pub fn rotate_backup_password(old_pass: &str, new_pass: &str) -> Result<(), String> {
    let backup_dir = get_backup_dir()?;
    if !backup_dir.exists() {
        return Ok(());
    }

    fn walk(dir: &PathBuf, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files)?;
            } else if path
                .extension()
                .and_then(|v| v.to_str())
                .map(|v| v == "backup")
                .unwrap_or(false)
            {
                files.push(path);
            }
        }
        Ok(())
    }

    let mut targets = Vec::new();
    walk(&backup_dir, &mut targets)?;

    for path in targets {
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if let Ok(blob) = serde_json::from_str::<BackupBlob>(&raw) {
            if blob.is_encrypted {
                let plain = crate::crypto::decrypt(&blob.data, old_pass)
                    .unwrap_or_else(|_| blob.data.clone());
                let rotated = BackupBlob {
                    is_encrypted: true,
                    data: crate::crypto::encrypt(&plain, new_pass)?,
                };
                let out = serde_json::to_string_pretty(&rotated).map_err(|e| e.to_string())?;
                fs::write(&path, out).map_err(|e| e.to_string())?;
            } else {
                let rotated = BackupBlob {
                    is_encrypted: true,
                    data: crate::crypto::encrypt(&blob.data, new_pass)?,
                };
                let out = serde_json::to_string_pretty(&rotated).map_err(|e| e.to_string())?;
                fs::write(&path, out).map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}
