use super::paths::quote_remote_path;
use super::session::get_active_tailers;
use std::process::Stdio;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

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
