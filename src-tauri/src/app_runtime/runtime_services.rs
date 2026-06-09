use crate::app_store;
use std::time::Duration;
use tauri::Emitter;

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
