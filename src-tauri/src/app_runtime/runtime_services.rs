use crate::app_store;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

/// Fixed grace after a detected system resume during which transport failures
/// are not counted toward mapping-row health (REQ-004).
const SYSTEM_RESUME_GRACE_SECS: u64 = 60;

/// Most recent detected system resume as Unix epoch seconds, or `0` when no
/// resume has been detected in this process. Process memory only; never
/// persisted.
static SYSTEM_RESUME_AT_SECS: AtomicI64 = AtomicI64::new(0);

/// Truncate a `SystemTime` to Unix epoch seconds. `None` (and any instant
/// before the epoch) maps to the `0` "no resume" sentinel.
fn epoch_secs(at: Option<SystemTime>) -> i64 {
    at.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

/// Record the most recent detected system resume (REQ-004). Called from the
/// SSH-tunnel sleep-gap heartbeat and the macOS wake observer before they
/// schedule their SSH reconnect work.
pub(crate) fn mark_system_resume() {
    SYSTEM_RESUME_AT_SECS.store(epoch_secs(Some(SystemTime::now())), Ordering::Relaxed);
}

/// Whether `now` falls inside the fixed grace measured from the most recent
/// detected resume, inclusive of its `T+60s` boundary. False when no resume has
/// ever been detected in this process.
pub(crate) fn system_resume_grace_active(now: SystemTime) -> bool {
    let resume = SYSTEM_RESUME_AT_SECS.load(Ordering::Relaxed);
    if resume == 0 {
        return false;
    }
    epoch_secs(Some(now)).saturating_sub(resume) <= SYSTEM_RESUME_GRACE_SECS as i64
}

/// Test-only seam that sets/clears the process-wide resume timestamp.
#[cfg(test)]
pub(crate) fn set_system_resume_at_for_tests(at: Option<SystemTime>) {
    SYSTEM_RESUME_AT_SECS.store(epoch_secs(at), Ordering::Relaxed);
}

pub(super) fn setup_proxy_monitor(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;

        loop {
            let interval_mins = crate::config::get_config()
                .ok()
                .and_then(|c| c.proxy.map(|p| p.check_interval))
                .unwrap_or(15);

            tokio::time::sleep(Duration::from_secs(interval_mins * 60)).await;

            if let Some(proxy_mgr) = crate::proxy::PROXY_MANAGER.get() {
                if proxy_mgr.is_enabled() {
                    match proxy_mgr.test_proxy().await {
                        Ok(status) => {
                            let _ = app.emit("proxy-status-update", &status);
                            if !status.is_available {
                                log::warn!("Proxy check failed: {}", status.message);
                            }
                        }
                        Err(e) => {
                            log::error!("Proxy test error: {}", e);
                        }
                    }
                }
            }
        }
    });
}

pub(super) fn setup_sessions_history_sync_service(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let _ = app_store::run_sessions_history_sync_pass(app.clone()).await;

        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.tick().await;
        loop {
            interval.tick().await;
            let _ = app_store::run_sessions_history_sync_pass(app.clone()).await;
        }
    });
}

#[tauri::command]
pub(super) async fn proxy_http_request(
    url: String,
    method: String,
    headers: Option<std::collections::HashMap<String, String>>,
    body: Option<String>,
) -> Result<String, String> {
    let proxy_mgr = crate::proxy::PROXY_MANAGER
        .get()
        .ok_or("Proxy manager not initialized")?;
    let client = proxy_mgr.get_client()?;

    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|e| format!("Invalid method: {}", e))?;

    let mut req = client.request(method, &url);

    if let Some(h) = headers {
        for (key, value) in h {
            req = req.header(&key, &value);
        }
    }

    if let Some(b) = body {
        req = req.body(b);
    }

    let res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status();
    let text = res.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), text));
    }

    Ok(text)
}
