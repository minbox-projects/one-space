use crate::ai_sessions::{parse_rfc3339_millis, system_time_to_epoch_millis};
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn total_or_sum(total: u64, input: u64, output: u64, cache: u64) -> u64 {
    if total > 0 {
        total
    } else {
        input.saturating_add(output).saturating_add(cache)
    }
}

pub(super) fn json_u64(value: Option<&Value>) -> u64 {
    match value {
        Some(Value::Number(number)) => number.as_u64().unwrap_or_else(|| {
            number
                .as_i64()
                .filter(|value| *value > 0)
                .map(|value| value as u64)
                .unwrap_or(0)
        }),
        Some(Value::String(text)) => text.trim().parse::<u64>().unwrap_or(0),
        _ => 0,
    }
}

pub(super) fn json_nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn file_stem_session_id(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub(super) fn modified_ms(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_to_epoch_millis)
        .unwrap_or(0)
}

pub(super) fn usage_file_may_overlap_window(modified_ms: i64, window_start_ms: i64) -> bool {
    // Unknown metadata must fall back to parsing so optimization never drops usage.
    modified_ms == 0 || modified_ms >= window_start_ms
}

pub(super) fn json_millis(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            parse_rfc3339_millis(text).or_else(|| text.parse::<i64>().ok())
        }
        _ => None,
    }
}

pub(super) fn sqlite_table_exists(conn: &Connection, table: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )
}

pub(super) fn json_files_recursive(root: &Path, suffix: &str) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(suffix))
                .unwrap_or(false)
            {
                out.push(path);
            }
        }
    }
    out
}
