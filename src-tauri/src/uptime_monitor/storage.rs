use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use rusqlite::{params, Connection};

use super::types::{
    HeartbeatBarPoint, HeartbeatRecord, MonitorIncident, MonitorStatus, MonitorTarget,
};

static MONITORS_STORAGE_LOCK: Mutex<()> = Mutex::new(());

fn get_storage_dir() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    Ok(dir)
}

fn get_monitors_file_path() -> Result<PathBuf, String> {
    let dir = get_storage_dir()?;
    Ok(dir.join("uptime_monitors.json"))
}

pub fn get_db_file_path() -> Result<PathBuf, String> {
    let dir = get_storage_dir()?;
    Ok(dir.join("uptime_heartbeats.db"))
}

pub fn open_uptime_db() -> Result<Connection, String> {
    let path = get_db_file_path()?;
    let conn = Connection::open(path).map_err(|e| format!("Failed to open uptime db: {}", e))?;
    init_db(&conn).map_err(|e| format!("Failed to init uptime db schema: {}", e))?;
    Ok(conn)
}

pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS uptime_heartbeats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            monitor_id TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            status TEXT NOT NULL,
            response_time_ms INTEGER NOT NULL,
            status_code INTEGER,
            error_message TEXT,
            ssl_days_remaining INTEGER
        );

        CREATE INDEX IF NOT EXISTS idx_uptime_hb_monitor_time
            ON uptime_heartbeats(monitor_id, timestamp DESC);

        CREATE TABLE IF NOT EXISTS uptime_incidents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            monitor_id TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            resolved_at INTEGER,
            duration_sec INTEGER,
            cause TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_uptime_incidents_monitor
            ON uptime_incidents(monitor_id, started_at DESC);
        ",
    )?;
    Ok(())
}

pub fn load_monitors() -> Result<Vec<MonitorTarget>, String> {
    let _guard = MONITORS_STORAGE_LOCK.lock().unwrap();
    let path = get_monitors_file_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read monitors json: {}", e))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse monitors json: {}", e))
}

pub fn save_monitors(monitors: &[MonitorTarget]) -> Result<(), String> {
    let _guard = MONITORS_STORAGE_LOCK.lock().unwrap();
    let path = get_monitors_file_path()?;
    let json_bytes = serde_json::to_vec_pretty(monitors)
        .map_err(|e| format!("Failed to serialize monitors: {}", e))?;

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, json_bytes)
        .map_err(|e| format!("Failed to write tmp monitors file: {}", e))?;
    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename monitors file: {}", e))?;
    Ok(())
}

pub fn upsert_monitor(target: MonitorTarget) -> Result<MonitorTarget, String> {
    let mut monitors = load_monitors()?;
    let mut found = false;
    for item in monitors.iter_mut() {
        if item.id == target.id {
            *item = target.clone();
            found = true;
            break;
        }
    }
    if !found {
        monitors.push(target.clone());
    }
    save_monitors(&monitors)?;
    Ok(target)
}

pub fn delete_monitor(id: &str) -> Result<(), String> {
    let mut monitors = load_monitors()?;
    let len_before = monitors.len();
    monitors.retain(|m| m.id != id);
    if monitors.len() != len_before {
        save_monitors(&monitors)?;
    }
    // 清理数据库中对应 monitor 的心跳与事件记录
    if let Ok(conn) = open_uptime_db() {
        let _ = conn.execute("DELETE FROM uptime_heartbeats WHERE monitor_id = ?1", params![id]);
        let _ = conn.execute("DELETE FROM uptime_incidents WHERE monitor_id = ?1", params![id]);
    }
    Ok(())
}

pub fn insert_heartbeat(conn: &Connection, record: &HeartbeatRecord) -> rusqlite::Result<i64> {
    let status_str = match record.status {
        MonitorStatus::Up => "up",
        MonitorStatus::Down => "down",
        MonitorStatus::Degraded => "degraded",
        MonitorStatus::Paused => "paused",
        MonitorStatus::Unknown => "unknown",
    };
    conn.execute(
        "INSERT INTO uptime_heartbeats (monitor_id, timestamp, status, response_time_ms, status_code, error_message, ssl_days_remaining)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            record.monitor_id,
            record.timestamp,
            status_str,
            record.response_time_ms,
            record.status_code,
            record.error_message,
            record.ssl_days_remaining,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_recent_heartbeats(
    conn: &Connection,
    monitor_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<HeartbeatBarPoint>> {
    let mut stmt = conn.prepare(
        "SELECT timestamp, status, response_time_ms, status_code, error_message
         FROM uptime_heartbeats
         WHERE monitor_id = ?1
         ORDER BY timestamp DESC
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(params![monitor_id, limit as i64], |row| {
        let status_str: String = row.get(1)?;
        let status = match status_str.as_str() {
            "up" => MonitorStatus::Up,
            "down" => MonitorStatus::Down,
            "degraded" => MonitorStatus::Degraded,
            "paused" => MonitorStatus::Paused,
            _ => MonitorStatus::Unknown,
        };
        Ok(HeartbeatBarPoint {
            timestamp: row.get(0)?,
            status,
            response_time_ms: row.get(2)?,
            status_code: row.get(3)?,
            error_message: row.get(4)?,
        })
    })?;

    let mut points: Vec<HeartbeatBarPoint> = Vec::new();
    for r in rows {
        points.push(r?);
    }
    // 返回正序排列（从早到晚，便于前端左->右渲染心跳柱）
    points.reverse();
    Ok(points)
}

pub fn get_uptime_metrics(
    conn: &Connection,
    monitor_id: &str,
    window_sec: i64,
    now: i64,
) -> rusqlite::Result<(f64, u32)> {
    let start_time = now - window_sec;
    let mut stmt = conn.prepare(
        "SELECT status, response_time_ms
         FROM uptime_heartbeats
         WHERE monitor_id = ?1 AND timestamp >= ?2",
    )?;

    let rows = stmt.query_map(params![monitor_id, start_time], |row| {
        let status_str: String = row.get(0)?;
        let rt: u32 = row.get(1)?;
        Ok((status_str, rt))
    })?;

    let mut total_count = 0usize;
    let mut up_count = 0usize;
    let mut total_rt = 0u64;

    for r in rows {
        let (st, rt) = r?;
        total_count += 1;
        if st == "up" || st == "degraded" {
            up_count += 1;
        }
        total_rt += rt as u64;
    }

    if total_count == 0 {
        return Ok((100.0, 0));
    }

    let uptime_ratio = (up_count as f64 / total_count as f64) * 100.0;
    let avg_rt = (total_rt / total_count as u64) as u32;
    Ok(((uptime_ratio * 100.0).round() / 100.0, avg_rt))
}

pub fn get_active_incident(
    conn: &Connection,
    monitor_id: &str,
) -> rusqlite::Result<Option<MonitorIncident>> {
    let mut stmt = conn.prepare(
        "SELECT id, monitor_id, started_at, resolved_at, duration_sec, cause
         FROM uptime_incidents
         WHERE monitor_id = ?1 AND resolved_at IS NULL
         ORDER BY started_at DESC
         LIMIT 1",
    )?;

    let mut rows = stmt.query(params![monitor_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(MonitorIncident {
            id: row.get(0)?,
            monitor_id: row.get(1)?,
            started_at: row.get(2)?,
            resolved_at: row.get(3)?,
            duration_sec: row.get(4)?,
            cause: row.get(5)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn create_incident(
    conn: &Connection,
    monitor_id: &str,
    now: i64,
    cause: &str,
) -> rusqlite::Result<MonitorIncident> {
    if let Some(existing) = get_active_incident(conn, monitor_id)? {
        return Ok(existing);
    }
    conn.execute(
        "INSERT INTO uptime_incidents (monitor_id, started_at, resolved_at, duration_sec, cause)
         VALUES (?1, ?2, NULL, NULL, ?3)",
        params![monitor_id, now, cause],
    )?;
    let id = conn.last_insert_rowid();
    Ok(MonitorIncident {
        id,
        monitor_id: monitor_id.to_string(),
        started_at: now,
        resolved_at: None,
        duration_sec: None,
        cause: cause.to_string(),
    })
}

pub fn resolve_incident(
    conn: &Connection,
    monitor_id: &str,
    now: i64,
) -> rusqlite::Result<Option<MonitorIncident>> {
    if let Some(mut incident) = get_active_incident(conn, monitor_id)? {
        let duration = (now - incident.started_at).max(0);
        conn.execute(
            "UPDATE uptime_incidents
             SET resolved_at = ?1, duration_sec = ?2
             WHERE id = ?3",
            params![now, duration, incident.id],
        )?;
        incident.resolved_at = Some(now);
        incident.duration_sec = Some(duration);
        Ok(Some(incident))
    } else {
        Ok(None)
    }
}

pub fn prune_old_records(conn: &Connection, keep_days: i64, now: i64) -> rusqlite::Result<usize> {
    let cutoff = now - (keep_days * 86400);
    let deleted_hb = conn.execute(
        "DELETE FROM uptime_heartbeats WHERE timestamp < ?1",
        params![cutoff],
    )?;
    let _ = conn.execute(
        "DELETE FROM uptime_incidents WHERE resolved_at IS NOT NULL AND resolved_at < ?1",
        params![cutoff],
    );
    Ok(deleted_hb)
}
