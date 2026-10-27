use super::paths::{expand_local_path, quote_remote_path};
use super::session::get_active_transfers;
use crate::config::get_config;
use dirs::{download_dir, home_dir};
use serde::{Deserialize, Serialize};
use std::fs::create_dir_all;
use std::process::{Command as StdCommand, Stdio};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::fs::{metadata, remove_file, rename, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

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
