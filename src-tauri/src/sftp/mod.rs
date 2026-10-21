use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

fn get_active_tailers() -> &'static Mutex<HashMap<String, Child>> {
    static TAILERS: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    TAILERS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteFileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub permissions: String,
    pub modified: String,
    pub path: String,
}

/**
 * 解析 Linux `ls -la` 输出格式
 */
pub fn parse_ls_output(base_dir: &str, output: &str) -> Vec<RemoteFileEntry> {
    let mut entries = Vec::new();
    let normalized_base = if base_dir.ends_with('/') {
        base_dir.to_string()
    } else {
        format!("{base_dir}/")
    };

    for line in output.lines() {
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
        // long-iso: drwxr-xr-x 2 root root 4096 2026-10-21 14:30 name
        // 传统: drwxr-xr-x 2 root root 4096 Oct 21 14:30 name
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

    entries
}

#[tauri::command]
pub async fn sftp_list_directory(
    host: String,
    remote_path: String,
) -> Result<Vec<RemoteFileEntry>, String> {
    let clean_path = if remote_path.trim().is_empty() {
        "~".to_string()
    } else {
        remote_path.trim().to_string()
    };

    // 兼容 Linux 各类发行版与 macOS 的 ls 格式
    let remote_cmd = format!(
        "ls -la --time-style=long-iso '{}' 2>/dev/null || ls -la '{}'",
        clean_path, clean_path
    );

    let output = std::process::Command::new("ssh")
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
    let remote_cmd = format!("head -c {} '{}'", limit, remote_path);

    let output = std::process::Command::new("ssh")
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

#[tauri::command]
pub async fn sftp_download_file(
    host: String,
    remote_path: String,
    local_path: String,
) -> Result<(), String> {
    let scp_source = format!("{}:{}", host, remote_path);
    let output = std::process::Command::new("scp")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg(&scp_source)
        .arg(&local_path)
        .output()
        .map_err(|e| format!("Failed to execute scp download: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(err_msg.to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn sftp_upload_file(
    host: String,
    local_path: String,
    remote_path: String,
) -> Result<(), String> {
    let scp_dest = format!("{}:{}", host, remote_path);
    let output = std::process::Command::new("scp")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg(&local_path)
        .arg(&scp_dest)
        .output()
        .map_err(|e| format!("Failed to execute scp upload: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(err_msg.to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn sftp_start_log_tail(
    app: AppHandle,
    host: String,
    remote_path: String,
    tail_id: String,
) -> Result<(), String> {
    // 先停止已存在的相同 tail_id
    sftp_stop_log_tail(tail_id.clone()).await.ok();

    let remote_cmd = format!("tail -n 100 -f '{}'", remote_path);
    let mut child = Command::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(&remote_cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn tail process: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture tail stdout".to_string())?;

    let tail_id_clone = tail_id.clone();
    let app_clone = app.clone();

    // 在 Tokio 异步任务中流式分发日志行
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        let event_name = format!("sftp-log-line-{}", tail_id_clone);

        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_clone.emit(&event_name, line);
        }
    });

    {
        let mut lock = get_active_tailers().lock().map_err(|e| e.to_string())?;
        lock.insert(tail_id, child);
    }

    Ok(())
}

#[tauri::command]
pub async fn sftp_stop_log_tail(tail_id: String) -> Result<(), String> {
    let maybe_child = {
        let mut lock = get_active_tailers().lock().map_err(|e| e.to_string())?;
        lock.remove(&tail_id)
    };

    if let Some(mut child) = maybe_child {
        let _ = child.kill().await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ls_output() {
        let sample_output = r#"
total 24
drwxr-xr-x  3 root root 4096 2026-10-21 14:30 .
drwxr-xr-x 10 root root 4096 2026-10-20 09:12 ..
drwxr-xr-x  2 root root 4096 2026-10-21 14:30 nginx
-rw-r--r--  1 root root 1024 2026-10-21 15:20 app.log
-rw-r--r--  1 root root  512 2026-10-20 11:00 config.yml
"#;

        let entries = parse_ls_output("/var/log", sample_output);
        assert_eq!(entries.len(), 3);

        // 目录在前
        assert_eq!(entries[0].name, "nginx");
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].path, "/var/log/nginx");

        // 文件在后
        assert_eq!(entries[1].name, "app.log");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].size, 1024);
        assert_eq!(entries[1].path, "/var/log/app.log");

        assert_eq!(entries[2].name, "config.yml");
    }
}
