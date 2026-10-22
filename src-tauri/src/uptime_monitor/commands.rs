use super::prober::execute_probe;
use super::scheduler::UptimeScheduler;
use super::storage::{delete_monitor, load_monitors, open_uptime_db, save_monitors, upsert_monitor};
use super::types::{MonitorStatus, MonitorSummary, MonitorTarget, ProbeTestResult};

#[tauri::command]
pub async fn uptime_list_monitors() -> Result<Vec<MonitorSummary>, String> {
    let scheduler = UptimeScheduler::global();
    scheduler.get_all_summaries()
}

#[tauri::command]
pub async fn uptime_save_monitor(mut target: MonitorTarget) -> Result<MonitorSummary, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if target.id.trim().is_empty() {
        target.id = uuid::Uuid::new_v4().to_string();
        target.created_at = now;
    }
    target.updated_at = now;

    let saved = upsert_monitor(target)?;
    let scheduler = UptimeScheduler::global();
    let summaries = scheduler.get_all_summaries()?;
    summaries
        .into_iter()
        .find(|s| s.target.id == saved.id)
        .ok_or_else(|| "Failed to locate saved monitor in summaries".to_string())
}

#[tauri::command]
pub async fn uptime_delete_monitor(id: String) -> Result<(), String> {
    delete_monitor(&id)?;
    let scheduler = UptimeScheduler::global();
    scheduler.state_machine().reset_monitor(&id);
    Ok(())
}

#[tauri::command]
pub async fn uptime_toggle_monitor(id: String, enabled: bool) -> Result<(), String> {
    let mut monitors = load_monitors()?;
    let mut found = false;
    for m in monitors.iter_mut() {
        if m.id == id {
            m.is_enabled = enabled;
            m.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            found = true;
            break;
        }
    }
    if found {
        save_monitors(&monitors)?;
        let scheduler = UptimeScheduler::global();
        if !enabled {
            scheduler.state_machine().reset_monitor(&id);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn uptime_test_probe(target: MonitorTarget) -> Result<ProbeTestResult, String> {
    let hb = execute_probe(&target).await;
    let is_ok = hb.status == MonitorStatus::Up || hb.status == MonitorStatus::Degraded;
    Ok(ProbeTestResult {
        success: is_ok,
        status: hb.status,
        response_time_ms: hb.response_time_ms,
        status_code: hb.status_code,
        message: hb.error_message.unwrap_or_else(|| "OK".to_string()),
        ssl_days_remaining: hb.ssl_days_remaining,
    })
}

#[tauri::command]
pub async fn uptime_clear_history(id: String) -> Result<(), String> {
    let conn = open_uptime_db()?;
    let _ = conn.execute(
        "DELETE FROM uptime_heartbeats WHERE monitor_id = ?1",
        rusqlite::params![id],
    );
    let _ = conn.execute(
        "DELETE FROM uptime_incidents WHERE monitor_id = ?1",
        rusqlite::params![id],
    );
    let scheduler = UptimeScheduler::global();
    scheduler.state_machine().reset_monitor(&id);
    Ok(())
}
