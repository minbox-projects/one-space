use super::paths::quote_remote_path;
use serde::{Deserialize, Serialize};
use std::process::Command as StdCommand;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteFileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub permissions: String,
    pub modified: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SftpDirectoryResult {
    pub current_path: String,
    pub entries: Vec<RemoteFileEntry>,
}

/**
 * 解析 Linux `ls -la` 输出格式（首行为 pwd 解析的实际绝对路径）
 */
pub fn parse_ls_output(base_dir: &str, output: &str) -> SftpDirectoryResult {
    let mut resolved_dir = base_dir.to_string();
    let mut entries = Vec::new();
    let mut lines = output.lines().peekable();

    // 如果第一行是以 '/' 开头的绝对路径（由 pwd 输出）
    if let Some(&first_line) = lines.peek() {
        let trimmed = first_line.trim();
        if trimmed.starts_with('/') {
            resolved_dir = trimmed.to_string();
            lines.next();
        }
    }

    let normalized_base = if resolved_dir.ends_with('/') {
        resolved_dir.clone()
    } else {
        format!("{resolved_dir}/")
    };

    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with("total") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }

        let permissions = parts[0].to_string();
        let is_dir = permissions.starts_with('d');

        // 文件大小（第 5 列，索引 4）
        let size = parts[4].parse::<u64>().unwrap_or(0);

        // 文件名可能带有空格，截取最后部分
        let (modified, name) = if parts.len() >= 9 && parts[5].contains('-') {
            let mod_time = format!("{} {}", parts[5], parts[6]);
            let file_name = parts[7..].join(" ");
            (mod_time, file_name)
        } else if parts.len() >= 9 {
            let mod_time = format!("{} {} {}", parts[5], parts[6], parts[7]);
            let file_name = parts[8..].join(" ");
            (mod_time, file_name)
        } else {
            ("Unknown".to_string(), parts[parts.len() - 1].to_string())
        };

        if name == "." || name == ".." {
            continue;
        }

        let full_path = format!("{}{}", normalized_base, name);

        entries.push(RemoteFileEntry {
            name,
            is_dir,
            size,
            permissions,
            modified,
            path: full_path,
        });
    }

    // 目录优先，按名称排序
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    SftpDirectoryResult {
        current_path: resolved_dir,
        entries,
    }
}

#[tauri::command]
pub async fn sftp_list_directory(
    host: String,
    remote_path: String,
) -> Result<SftpDirectoryResult, String> {
    let clean_path = if remote_path.trim().is_empty() {
        "~".to_string()
    } else {
        remote_path.trim().to_string()
    };

    let target_quoted = quote_remote_path(&clean_path);

    // 先进入目录并获取真实 pwd 绝对路径，再执行 ls -la
    let remote_cmd = format!(
        "cd {} && pwd && (ls -la --time-style=long-iso . 2>/dev/null || ls -la .)",
        target_quoted
    );

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(&remote_cmd)
        .output()
        .map_err(|e| format!("Failed to execute SSH command: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ls_output(&clean_path, &stdout_str))
}

#[tauri::command]
pub async fn sftp_read_file(
    host: String,
    remote_path: String,
    max_bytes: Option<usize>,
) -> Result<String, String> {
    let limit = max_bytes.unwrap_or(256 * 1024); // 默认最多读 256KB 预览
    let path_quoted = quote_remote_path(&remote_path);
    let remote_cmd = format!("head -c {} {}", limit, path_quoted);

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(&remote_cmd)
        .output()
        .map_err(|e| format!("Failed to read file over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(err_msg.to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
