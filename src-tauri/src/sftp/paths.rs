use dirs::home_dir;
use std::path::PathBuf;

/**
 * 远程路径安全转义，支持 Shell 波浪号（~）扩展与防命令注入
 */
pub fn quote_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "~" {
        "\"$HOME\"".to_string()
    } else if trimmed == "~/" {
        "\"$HOME/\"".to_string()
    } else if let Some(sub) = trimmed.strip_prefix("~/") {
        let escaped = sub.replace('\'', "'\\''");
        format!("\"$HOME/\"'{}'", escaped)
    } else {
        let escaped = trimmed.replace('\'', "'\\''");
        format!("'{}'", escaped)
    }
}

/**
 * 本地路径波浪号（~）安全展开
 */
pub fn expand_local_path(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return PathBuf::new();
    }
    if trimmed == "~" {
        home_dir().unwrap_or_else(|| PathBuf::from("~"))
    } else if let Some(sub) = trimmed.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            home.join(sub)
        } else {
            PathBuf::from(trimmed)
        }
    } else {
        PathBuf::from(trimmed)
    }
}
