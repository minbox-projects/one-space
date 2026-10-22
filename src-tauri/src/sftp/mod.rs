use std::collections::HashMap;
use std::fs::create_dir_all;
use std::path::PathBuf;
use std::process::{Command as StdCommand, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use dirs::{download_dir, home_dir};
use serde::{Deserialize, Serialize};
use crate::config::get_config;
use tauri::{AppHandle, Emitter};
use tokio::fs::{metadata, remove_file, rename, File};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

fn get_active_tailers() -> &'static Mutex<HashMap<String, Child>> {
    static TAILERS: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    TAILERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_active_transfers() -> &'static Mutex<HashMap<String, Child>> {
    static TRANSFERS: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    TRANSFERS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SftpTransferProgressPayload {
    pub task_id: String,
    pub transfer_type: String, // "upload" | "download"
    pub file_name: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub percentage: f64,
    pub bytes_per_second: u64,
    pub eta_seconds: Option<u64>,
    pub status: String, // "transferring" | "completed" | "error" | "cancelled"
    pub error: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SftpDirectoryResult {
    pub current_path: String,
    pub entries: Vec<RemoteFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SftpDeleteResult {
    pub original_path: String,
    pub trash_path: Option<String>,
    pub is_permanent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SftpTrashItem {
    pub id: String,
    pub name: String,
    pub original_path: String,
    pub trash_path: String,
    pub size: u64,
    pub is_dir: bool,
    pub deleted_at: String,
}

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

#[tauri::command]
pub async fn sftp_delete_entry(
    host: String,
    remote_path: String,
    permanent: Option<bool>,
) -> Result<SftpDeleteResult, String> {
    let trimmed = remote_path.trim();
    if trimmed.is_empty()
        || trimmed == "/"
        || trimmed == "~"
        || trimmed == "$HOME"
        || trimmed == "."
        || trimmed == ".."
    {
        return Err("Refusing to delete root or home directory".to_string());
    }

    let is_permanent = permanent.unwrap_or(false);

    if is_permanent {
        let path_quoted = quote_remote_path(trimmed);
        let remote_cmd = format!("rm -rf -- {}", path_quoted);

        let output = StdCommand::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=6")
            .arg(&host)
            .arg(&remote_cmd)
            .output()
            .map_err(|e| format!("Failed to delete entry over SSH: {e}"))?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(if err_msg.is_empty() {
                format!("SSH exit code {}", output.status.code().unwrap_or(-1))
            } else {
                err_msg.to_string()
            });
        }

        return Ok(SftpDeleteResult {
            original_path: trimmed.to_string(),
            trash_path: None,
            is_permanent: true,
        });
    }

    // 默认移入回收站
    let file_name = trimmed.rsplit('/').next().unwrap_or(trimmed);
    let now_ts = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    };
    let trash_name = format!("{}_{}", now_ts, file_name);
    let trash_item_rel = format!("~/.onespace_trash/{}", trash_name);
    let trash_item_quoted = quote_remote_path(&trash_item_rel);
    let path_quoted = quote_remote_path(trimmed);
    let trash_name_quoted = quote_remote_path(&trash_name);

    let remote_cmd = format!(
        "mkdir -p \"$HOME/.onespace_trash\" 2>/dev/null && mv -- {} {} && printf '%s\\t%s\\t%s\\n' {} {} '{}' >> \"$HOME/.onespace_trash/.manifest\"",
        path_quoted, trash_item_quoted, trash_name_quoted, path_quoted, now_ts
    );

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(&remote_cmd)
        .output()
        .map_err(|e| format!("Failed to move file to trash over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    Ok(SftpDeleteResult {
        original_path: trimmed.to_string(),
        trash_path: Some(trash_item_rel),
        is_permanent: false,
    })
}

pub fn parse_trash_output(stdout: &str) -> Vec<SftpTrashItem> {
    let mut manifest_map: HashMap<String, (String, String)> = HashMap::new();
    let mut items = Vec::new();

    let parts: Vec<&str> = stdout.split("===LS===").collect();
    let manifest_part = if parts.len() > 1 {
        parts[0].replace("===MANIFEST===", "")
    } else {
        String::new()
    };
    let ls_part = if parts.len() > 1 { parts[1] } else { stdout };

    for line in manifest_part.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields: Vec<&str> = trimmed.split('\t').collect();
        if fields.len() >= 2 {
            let id = fields[0].trim().to_string();
            let orig = fields[1].trim().to_string();
            let date = if fields.len() >= 3 {
                fields[2].trim().to_string()
            } else {
                "Unknown".to_string()
            };
            manifest_map.insert(id, (orig, date));
        }
    }

    for line in ls_part.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("total ") {
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        if cols.len() < 8 {
            continue;
        }

        let is_dir = cols[0].starts_with('d');
        let size: u64 = cols[4].parse().unwrap_or(0);
        let (modified, file_name) = if cols.len() >= 9 {
            let mod_time = format!("{} {} {}", cols[5], cols[6], cols[7]);
            let name = cols[8..].join(" ");
            (mod_time, name)
        } else {
            ("Unknown".to_string(), cols[cols.len() - 1].to_string())
        };

        if file_name == "." || file_name == ".." || file_name.starts_with(".manifest") {
            continue;
        }

        let (orig_path, deleted_time, orig_name) = if let Some((orig, d_time)) = manifest_map.get(&file_name) {
            let name = orig.rsplit('/').next().unwrap_or(&file_name).to_string();
            (orig.clone(), d_time.clone(), name)
        } else {
            let clean_name = if let Some(idx) = file_name.find('_') {
                if file_name[..idx].chars().all(|c| c.is_ascii_digit()) {
                    file_name[idx + 1..].to_string()
                } else {
                    file_name.clone()
                }
            } else {
                file_name.clone()
            };
            (format!("~/{}", clean_name), modified.clone(), clean_name)
        };

        items.push(SftpTrashItem {
            id: file_name.clone(),
            name: orig_name,
            original_path: orig_path,
            trash_path: format!("~/.onespace_trash/{}", file_name),
            size,
            is_dir,
            deleted_at: deleted_time,
        });
    }

    items.sort_by(|a, b| b.id.cmp(&a.id));
    items
}

#[tauri::command]
pub async fn sftp_list_trash(host: String) -> Result<Vec<SftpTrashItem>, String> {
    let remote_cmd = "TRASH_DIR=\"$HOME/.onespace_trash\"; if [ -d \"$TRASH_DIR\" ]; then echo '===MANIFEST==='; cat \"$TRASH_DIR/.manifest\" 2>/dev/null; echo '===LS==='; (ls -la --time-style=long-iso \"$TRASH_DIR\" 2>/dev/null || ls -la \"$TRASH_DIR\"); fi";

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(remote_cmd)
        .output()
        .map_err(|e| format!("Failed to list trash over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    Ok(parse_trash_output(&stdout_str))
}

#[tauri::command]
pub async fn sftp_restore_trash_item(
    host: String,
    trash_name: String,
) -> Result<String, String> {
    let clean_id = trash_name.trim();
    if clean_id.is_empty() || clean_id.contains('/') || clean_id.contains("..") {
        return Err("Invalid trash item identifier".to_string());
    }

    let id_quoted = quote_remote_path(clean_id);
    let remote_cmd = format!(
        "TRASH_DIR=\"$HOME/.onespace_trash\"; MANIFEST=\"$TRASH_DIR/.manifest\"; TARGET_FILE=\"$TRASH_DIR/\"{}; ORIG=$(grep -F -m 1 {} \"$MANIFEST\" 2>/dev/null | cut -f2); if [ -z \"$ORIG\" ]; then CLEAN=$(echo {} | sed 's/^[0-9]*_//'); ORIG=\"$HOME/$CLEAN\"; fi; mkdir -p \"$(dirname \"$ORIG\")\" 2>/dev/null && mv -- \"$TARGET_FILE\" \"$ORIG\" && if [ -f \"$MANIFEST\" ]; then grep -v -F {} \"$MANIFEST\" > \"$MANIFEST.tmp\" 2>/dev/null && mv \"$MANIFEST.tmp\" \"$MANIFEST\"; fi; echo \"$ORIG\"",
        id_quoted, id_quoted, id_quoted, id_quoted
    );

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(&remote_cmd)
        .output()
        .map_err(|e| format!("Failed to restore trash item over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let restored_path = stdout_str.lines().last().unwrap_or("").trim().to_string();
    if restored_path.is_empty() {
        return Err("Failed to resolve restored path".to_string());
    }

    Ok(restored_path)
}

#[tauri::command]
pub async fn sftp_clear_trash(host: String) -> Result<(), String> {
    let remote_cmd = "TRASH_DIR=\"$HOME/.onespace_trash\"; if [ -d \"$TRASH_DIR\" ]; then rm -rf \"$TRASH_DIR\"/* \"$TRASH_DIR\"/.* 2>/dev/null; mkdir -p \"$TRASH_DIR\" 2>/dev/null; fi";

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(remote_cmd)
        .output()
        .map_err(|e| format!("Failed to clear trash over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn sftp_restore_entry(
    host: String,
    trash_path: String,
    original_path: String,
) -> Result<(), String> {
    let clean_trash = trash_path.trim();
    let clean_orig = original_path.trim();
    if clean_trash.is_empty() || clean_orig.is_empty() {
        return Err("Trash path and original path cannot be empty".to_string());
    }

    let trash_quoted = quote_remote_path(clean_trash);
    let orig_quoted = quote_remote_path(clean_orig);

    let remote_cmd = format!(
        "mkdir -p \"$(dirname {})\" 2>/dev/null && mv -- {} {}",
        orig_quoted, trash_quoted, orig_quoted
    );

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(&remote_cmd)
        .output()
        .map_err(|e| format!("Failed to restore file from trash over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    Ok(())
}

pub fn calculate_progress_metrics(
    transferred: u64,
    total: u64,
    start_time: Instant,
    last_emit_bytes: u64,
    last_emit_time: Instant,
) -> (u64, Option<u64>, f64) {
    let recent_elapsed = last_emit_time.elapsed().as_secs_f64();
    let recent_bytes = transferred.saturating_sub(last_emit_bytes);
    let speed = if recent_elapsed > 0.05 {
        (recent_bytes as f64 / recent_elapsed) as u64
    } else {
        let total_elapsed = start_time.elapsed().as_secs_f64();
        if total_elapsed > 0.05 {
            (transferred as f64 / total_elapsed) as u64
        } else {
            0
        }
    };

    let percentage = if total > 0 {
        ((transferred as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let eta = if speed > 0 && total > transferred {
        Some((total - transferred) / speed)
    } else if total > 0 && transferred >= total {
        Some(0)
    } else {
        None
    };

    (speed, eta, percentage)
}

#[tauri::command]
pub async fn sftp_get_default_download_dir() -> Result<String, String> {
    if let Ok(cfg) = get_config() {
        if let Some(custom) = cfg.default_sftp_download_dir {
            let trimmed = custom.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }

    if let Some(dir) = download_dir() {
        return Ok(dir.to_string_lossy().to_string());
    }

    if let Some(home) = home_dir() {
        return Ok(home.to_string_lossy().to_string());
    }

    Ok(".".to_string())
}

#[tauri::command]
pub async fn sftp_cancel_transfer(app: AppHandle, task_id: String) -> Result<(), String> {
    let maybe_child = {
        let mut lock = get_active_transfers().lock().map_err(|e| e.to_string())?;
        lock.remove(&task_id)
    };

    if let Some(mut child) = maybe_child {
        let _ = child.kill().await;
    }

    let payload = SftpTransferProgressPayload {
        task_id: task_id.clone(),
        transfer_type: "unknown".to_string(),
        file_name: "".to_string(),
        transferred_bytes: 0,
        total_bytes: 0,
        percentage: 0.0,
        bytes_per_second: 0,
        eta_seconds: None,
        status: "cancelled".to_string(),
        error: None,
    };
    let _ = app.emit("sftp-transfer-progress", payload);

    Ok(())
}

#[tauri::command]
pub async fn sftp_download_file(
    app: AppHandle,
    task_id: String,
    host: String,
    remote_path: String,
    local_path: String,
    file_size: Option<u64>,
) -> Result<(), String> {
    let expanded_local = expand_local_path(&local_path);
    if let Some(parent) = expanded_local.parent() {
        if !parent.as_os_str().is_empty() {
            create_dir_all(parent).map_err(|e| format!("Failed to create local directory: {e}"))?;
        }
    }

    let file_name = expanded_local
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "download_file".to_string());

    let mut total_bytes = file_size.unwrap_or(0);
    let path_quoted = quote_remote_path(&remote_path);

    // 如果未提供总字节数，尝试快速通过 SSH 获取文件大小
    if total_bytes == 0 {
        let stat_cmd = format!("wc -c < {}", path_quoted);
        if let Ok(output) = StdCommand::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(&host)
            .arg(&stat_cmd)
            .output()
        {
            if output.status.success() {
                let size_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(parsed) = size_str.parse::<u64>() {
                    total_bytes = parsed;
                }
            }
        }
    }

    let temp_local_path = format!("{}.part_{}", expanded_local.display(), task_id);
    let mut local_file = File::create(&temp_local_path)
        .await
        .map_err(|e| format!("Failed to create local file: {e}"))?;

    let remote_cmd = format!("cat {}", path_quoted);
    let mut child = Command::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg(&host)
        .arg(&remote_cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start SSH download process: {e}"))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture download stdout".to_string())?;

    {
        let mut lock = get_active_transfers().lock().map_err(|e| e.to_string())?;
        lock.insert(task_id.clone(), child);
    }

    let start_time = Instant::now();
    let mut last_emit_time = Instant::now();
    let mut last_emit_bytes = 0u64;
    let mut transferred_bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read_result = stdout.read(&mut buffer).await;
        match read_result {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = local_file.write_all(&buffer[..n]).await {
                    let _ = remove_file(&temp_local_path).await;
                    let _ = {
                        let lock = get_active_transfers().lock();
                        if let Ok(mut l) = lock {
                            l.remove(&task_id);
                        }
                    };
                    let err_msg = format!("Failed to write local file chunk: {e}");
                    let payload = SftpTransferProgressPayload {
                        task_id: task_id.clone(),
                        transfer_type: "download".to_string(),
                        file_name: file_name.clone(),
                        transferred_bytes,
                        total_bytes,
                        percentage: 0.0,
                        bytes_per_second: 0,
                        eta_seconds: None,
                        status: "error".to_string(),
                        error: Some(err_msg.clone()),
                    };
                    let _ = app.emit("sftp-transfer-progress", payload);
                    return Err(err_msg);
                }
                transferred_bytes += n as u64;

                if last_emit_time.elapsed() >= Duration::from_millis(100)
                    || (total_bytes > 0 && transferred_bytes >= total_bytes)
                {
                    let (speed, eta, pct) = calculate_progress_metrics(
                        transferred_bytes,
                        total_bytes,
                        start_time,
                        last_emit_bytes,
                        last_emit_time,
                    );
                    let payload = SftpTransferProgressPayload {
                        task_id: task_id.clone(),
                        transfer_type: "download".to_string(),
                        file_name: file_name.clone(),
                        transferred_bytes,
                        total_bytes,
                        percentage: pct,
                        bytes_per_second: speed,
                        eta_seconds: eta,
                        status: "transferring".to_string(),
                        error: None,
                    };
                    let _ = app.emit("sftp-transfer-progress", payload);
                    last_emit_time = Instant::now();
                    last_emit_bytes = transferred_bytes;
                }
            }
            Err(e) => {
                let _ = remove_file(&temp_local_path).await;
                let _ = {
                    let lock = get_active_transfers().lock();
                    if let Ok(mut l) = lock {
                        l.remove(&task_id);
                    }
                };
                let err_msg = format!("SSH read stream error: {e}");
                let payload = SftpTransferProgressPayload {
                    task_id: task_id.clone(),
                    transfer_type: "download".to_string(),
                    file_name: file_name.clone(),
                    transferred_bytes,
                    total_bytes,
                    percentage: 0.0,
                    bytes_per_second: 0,
                    eta_seconds: None,
                    status: "error".to_string(),
                    error: Some(err_msg.clone()),
                };
                let _ = app.emit("sftp-transfer-progress", payload);
                return Err(err_msg);
            }
        }
    }

    // 确保写入持久化
    let _ = local_file.flush().await;
    drop(local_file);

    let maybe_child = {
        let mut lock = get_active_transfers().lock().map_err(|e| e.to_string())?;
        lock.remove(&task_id)
    };

    if let Some(mut child) = maybe_child {
        let exit_status = child.wait().await.map_err(|e| e.to_string())?;
        if !exit_status.success() {
            let _ = remove_file(&temp_local_path).await;
            let stderr_output = if let Some(mut stderr) = child.stderr.take() {
                let mut buf = Vec::new();
                let _ = stderr.read_to_end(&mut buf).await;
                String::from_utf8_lossy(&buf).trim().to_string()
            } else {
                String::new()
            };
            let err_msg = if !stderr_output.is_empty() {
                stderr_output
            } else {
                format!("SSH download process exited with code {}", exit_status.code().unwrap_or(-1))
            };
            let payload = SftpTransferProgressPayload {
                task_id: task_id.clone(),
                transfer_type: "download".to_string(),
                file_name: file_name.clone(),
                transferred_bytes,
                total_bytes,
                percentage: 0.0,
                bytes_per_second: 0,
                eta_seconds: None,
                status: "error".to_string(),
                error: Some(err_msg.clone()),
            };
            let _ = app.emit("sftp-transfer-progress", payload);
            return Err(err_msg);
        }
    }

    // 重命名临时文件为最终文件
    rename(&temp_local_path, &expanded_local)
        .await
        .map_err(|e| format!("Failed to finalize downloaded file: {e}"))?;

    let effective_total = if total_bytes == 0 { transferred_bytes } else { total_bytes };
    let final_payload = SftpTransferProgressPayload {
        task_id: task_id.clone(),
        transfer_type: "download".to_string(),
        file_name,
        transferred_bytes: effective_total,
        total_bytes: effective_total,
        percentage: 100.0,
        bytes_per_second: 0,
        eta_seconds: Some(0),
        status: "completed".to_string(),
        error: None,
    };
    let _ = app.emit("sftp-transfer-progress", final_payload);

    Ok(())
}

#[tauri::command]
pub async fn sftp_upload_file(
    app: AppHandle,
    task_id: String,
    host: String,
    local_path: String,
    remote_path: String,
) -> Result<(), String> {
    let expanded_local = expand_local_path(&local_path);
    if !expanded_local.is_file() {
        return Err(format!(
            "Local file does not exist or is a directory: {}",
            expanded_local.display()
        ));
    }

    let file_name = expanded_local
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "upload_file".to_string());

    let file_meta = metadata(&expanded_local)
        .await
        .map_err(|e| format!("Failed to read local file metadata: {e}"))?;
    let total_bytes = file_meta.len();

    let clean_remote = remote_path.trim();
    let target_remote_path = if clean_remote.is_empty() || clean_remote == "~" {
        format!("~/{}", file_name)
    } else if clean_remote.ends_with('/') {
        format!("{}{}", clean_remote, file_name)
    } else {
        format!("{}/{}", clean_remote, file_name)
    };

    let path_quoted = quote_remote_path(&target_remote_path);
    let escaped_file_name = file_name.replace('\'', "'\\''");
    // 关键：若远程目标是目录（如 /root），自动在目录下写入该文件，并自动确保父目录存在
    let remote_cmd = format!(
        "TARGET={}; if [ -d \"$TARGET\" ]; then TARGET=\"$TARGET\"/'{}'; fi; mkdir -p \"$(dirname \"$TARGET\")\" 2>/dev/null; cat > \"$TARGET\"",
        path_quoted, escaped_file_name
    );

    let mut child = Command::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg(&host)
        .arg(&remote_cmd)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start SSH upload process: {e}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to capture upload stdin".to_string())?;

    {
        let mut lock = get_active_transfers().lock().map_err(|e| e.to_string())?;
        lock.insert(task_id.clone(), child);
    }

    let mut local_file = File::open(&expanded_local)
        .await
        .map_err(|e| format!("Failed to open local file for upload: {e}"))?;

    let start_time = Instant::now();
    let mut last_emit_time = Instant::now();
    let mut last_emit_bytes = 0u64;
    let mut transferred_bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read_result = local_file.read(&mut buffer).await;
        match read_result {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = stdin.write_all(&buffer[..n]).await {
                    let _ = {
                        let lock = get_active_transfers().lock();
                        if let Ok(mut l) = lock {
                            l.remove(&task_id);
                        }
                    };
                    let err_msg = format!("Failed to write chunk to remote SSH stream: {e}");
                    let payload = SftpTransferProgressPayload {
                        task_id: task_id.clone(),
                        transfer_type: "upload".to_string(),
                        file_name: file_name.clone(),
                        transferred_bytes,
                        total_bytes,
                        percentage: 0.0,
                        bytes_per_second: 0,
                        eta_seconds: None,
                        status: "error".to_string(),
                        error: Some(err_msg.clone()),
                    };
                    let _ = app.emit("sftp-transfer-progress", payload);
                    return Err(err_msg);
                }
                transferred_bytes += n as u64;

                if last_emit_time.elapsed() >= Duration::from_millis(100)
                    || (total_bytes > 0 && transferred_bytes >= total_bytes)
                {
                    let (speed, eta, pct) = calculate_progress_metrics(
                        transferred_bytes,
                        total_bytes,
                        start_time,
                        last_emit_bytes,
                        last_emit_time,
                    );
                    let payload = SftpTransferProgressPayload {
                        task_id: task_id.clone(),
                        transfer_type: "upload".to_string(),
                        file_name: file_name.clone(),
                        transferred_bytes,
                        total_bytes,
                        percentage: pct,
                        bytes_per_second: speed,
                        eta_seconds: eta,
                        status: "transferring".to_string(),
                        error: None,
                    };
                    let _ = app.emit("sftp-transfer-progress", payload);
                    last_emit_time = Instant::now();
                    last_emit_bytes = transferred_bytes;
                }
            }
            Err(e) => {
                let _ = {
                    let lock = get_active_transfers().lock();
                    if let Ok(mut l) = lock {
                        l.remove(&task_id);
                    }
                };
                let err_msg = format!("Failed to read local file chunk: {e}");
                let payload = SftpTransferProgressPayload {
                    task_id: task_id.clone(),
                    transfer_type: "upload".to_string(),
                    file_name: file_name.clone(),
                    transferred_bytes,
                    total_bytes,
                    percentage: 0.0,
                    bytes_per_second: 0,
                    eta_seconds: None,
                    status: "error".to_string(),
                    error: Some(err_msg.clone()),
                };
                let _ = app.emit("sftp-transfer-progress", payload);
                return Err(err_msg);
            }
        }
    }

    // 关键：关闭 stdin 发送 EOF 信号，远程 cat 进程才能正常结束写入
    let _ = stdin.flush().await;
    drop(stdin);

    let maybe_child = {
        let mut lock = get_active_transfers().lock().map_err(|e| e.to_string())?;
        lock.remove(&task_id)
    };

    if let Some(mut child) = maybe_child {
        let exit_status = child.wait().await.map_err(|e| e.to_string())?;
        if !exit_status.success() {
            let stderr_output = if let Some(mut stderr) = child.stderr.take() {
                let mut buf = Vec::new();
                let _ = stderr.read_to_end(&mut buf).await;
                String::from_utf8_lossy(&buf).trim().to_string()
            } else {
                String::new()
            };
            let err_msg = if !stderr_output.is_empty() {
                stderr_output
            } else {
                format!("SSH upload process exited with code {}", exit_status.code().unwrap_or(-1))
            };
            let payload = SftpTransferProgressPayload {
                task_id: task_id.clone(),
                transfer_type: "upload".to_string(),
                file_name: file_name.clone(),
                transferred_bytes,
                total_bytes,
                percentage: 0.0,
                bytes_per_second: 0,
                eta_seconds: None,
                status: "error".to_string(),
                error: Some(err_msg.clone()),
            };
            let _ = app.emit("sftp-transfer-progress", payload);
            return Err(err_msg);
        }
    }

    let final_payload = SftpTransferProgressPayload {
        task_id: task_id.clone(),
        transfer_type: "upload".to_string(),
        file_name,
        transferred_bytes: total_bytes,
        total_bytes,
        percentage: 100.0,
        bytes_per_second: 0,
        eta_seconds: Some(0),
        status: "completed".to_string(),
        error: None,
    };
    let _ = app.emit("sftp-transfer-progress", final_payload);

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

    let path_quoted = quote_remote_path(&remote_path);
    let remote_cmd = format!("tail -n 100 -f {}", path_quoted);
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
    fn test_quote_remote_path() {
        assert_eq!(quote_remote_path("~"), "\"$HOME\"");
        assert_eq!(quote_remote_path("~/"), "\"$HOME/\"");
        assert_eq!(quote_remote_path("~/logs"), "\"$HOME/\"'logs'");
        assert_eq!(quote_remote_path("/var/log"), "'/var/log'");
        assert_eq!(quote_remote_path("dir's name"), "'dir'\\''s name'");
    }

    #[test]
    fn test_parse_ls_output() {
        let sample_output = r#"/var/log
total 24
drwxr-xr-x  3 root root 4096 2026-10-21 14:30 .
drwxr-xr-x 10 root root 4096 2026-10-20 09:12 ..
drwxr-xr-x  2 root root 4096 2026-10-21 14:30 nginx
-rw-r--r--  1 root root 1024 2026-10-21 15:20 app.log
-rw-r--r--  1 root root  512 2026-10-20 11:00 config.yml
"#;

        let res = parse_ls_output("~", sample_output);
        assert_eq!(res.current_path, "/var/log");
        assert_eq!(res.entries.len(), 3);

        // 目录在前
        assert_eq!(res.entries[0].name, "nginx");
        assert!(res.entries[0].is_dir);
        assert_eq!(res.entries[0].path, "/var/log/nginx");

        // 文件在后
        assert_eq!(res.entries[1].name, "app.log");
        assert!(!res.entries[1].is_dir);
        assert_eq!(res.entries[1].size, 1024);
        assert_eq!(res.entries[1].path, "/var/log/app.log");

        assert_eq!(res.entries[2].name, "config.yml");
    }

    #[test]
    fn test_calculate_progress_metrics() {
        let now = Instant::now();
        // 初始传输 50MB / 100MB
        let (speed, eta, pct) = calculate_progress_metrics(
            50 * 1024 * 1024,
            100 * 1024 * 1024,
            now - Duration::from_secs(5),
            0,
            now - Duration::from_secs(5),
        );
        assert!((pct - 50.0).abs() < 0.1);
        assert!(speed > 0);
        assert!(eta.is_some());
        // 传输完毕 100MB / 100MB
        let (_, eta_done, pct_done) = calculate_progress_metrics(
            100 * 1024 * 1024,
            100 * 1024 * 1024,
            now - Duration::from_secs(10),
            90 * 1024 * 1024,
            now - Duration::from_secs(1),
        );
        assert_eq!(pct_done, 100.0);
        assert_eq!(eta_done, Some(0));
    }

    #[test]
    fn test_expand_local_path() {
        let p = expand_local_path("/var/log");
        assert_eq!(p, PathBuf::from("/var/log"));

        let p_home = expand_local_path("~/Downloads/pelican-cycling.html");
        assert!(!p_home.to_string_lossy().starts_with("~"));
        assert!(p_home.to_string_lossy().ends_with("Downloads/pelican-cycling.html"));
    }

    #[tokio::test]
    async fn test_delete_entry_safety_checks() {
        assert!(sftp_delete_entry("host".to_string(), "/".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), "~".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), "$HOME".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), ".".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), "..".to_string(), None).await.is_err());
        assert!(sftp_delete_entry("host".to_string(), "   ".to_string(), None).await.is_err());
    }

    #[tokio::test]
    async fn test_restore_entry_validation() {
        assert!(sftp_restore_entry("host".to_string(), "".to_string(), "/var/log".to_string()).await.is_err());
        assert!(sftp_restore_entry("host".to_string(), "~/.onespace_trash/1_a.log".to_string(), "".to_string()).await.is_err());
    }

    #[test]
    fn test_parse_trash_output() {
        let sample_trash = r#"===MANIFEST===
1729587400_app.log	/var/log/nginx/app.log	1729587400
1729587500_data	/data/backup/data	1729587500
===LS===
total 16
drwxr-xr-x 2 root root 4096 2026-10-22 09:10 .
drwxr-xr-x 5 root root 4096 2026-10-22 09:00 ..
-rw-r--r-- 1 root root  120 2026-10-22 09:12 .manifest
-rw-r--r-- 1 root root 8192 2026-10-22 09:10 1729587400_app.log
drwxr-xr-x 2 root root 4096 2026-10-22 09:11 1729587500_data
-rw-r--r-- 1 root root 1024 2026-10-22 09:15 1729587600_orphan.txt
"#;
        let items = parse_trash_output(sample_trash);
        assert_eq!(items.len(), 3);

        // 第一个（倒序排序）：1729587600_orphan.txt（无 manifest 兜底）
        assert_eq!(items[0].id, "1729587600_orphan.txt");
        assert_eq!(items[0].name, "orphan.txt");
        assert_eq!(items[0].original_path, "~/orphan.txt");
        assert_eq!(items[0].size, 1024);
        assert!(!items[0].is_dir);

        // 第二个：1729587500_data（来自 manifest，目录）
        assert_eq!(items[1].id, "1729587500_data");
        assert_eq!(items[1].name, "data");
        assert_eq!(items[1].original_path, "/data/backup/data");
        assert!(items[1].is_dir);

        // 第三个：1729587400_app.log（来自 manifest，文件）
        assert_eq!(items[2].id, "1729587400_app.log");
        assert_eq!(items[2].name, "app.log");
        assert_eq!(items[2].original_path, "/var/log/nginx/app.log");
        assert_eq!(items[2].size, 8192);
        assert!(!items[2].is_dir);
    }

    #[tokio::test]
    async fn test_restore_trash_item_validation() {
        assert!(sftp_restore_trash_item("host".to_string(), "".to_string()).await.is_err());
        assert!(sftp_restore_trash_item("host".to_string(), "../escape".to_string()).await.is_err());
        assert!(sftp_restore_trash_item("host".to_string(), "dir/sub".to_string()).await.is_err());
    }
}
