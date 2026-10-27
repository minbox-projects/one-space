use super::json::{
    file_stem_session_id, json_files_recursive, json_millis, json_nonempty_string, json_u64,
    modified_ms, sqlite_table_exists, total_or_sum, usage_file_may_overlap_window,
};
use super::model::{unavailable_scan, ToolScan, UsageRecord, UsageWindow};
use super::protobuf::{parse_antigravity_gen_metadata, parse_antigravity_step_tokens};
use super::stats::{record_usage_parsed_entry, record_usage_source_read};
use crate::ai_sessions::{
    antigravity_brain_roots, antigravity_conversations_roots, antigravity_entry_timestamp_ms,
    find_antigravity_transcript, shared_usage_file_cache,
};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn read_antigravity_db_tokens(
    db_path: &Path,
    session_id: &str,
    window: &UsageWindow,
) -> Result<Vec<UsageRecord>, String> {
    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    record_usage_source_read();

    if !sqlite_table_exists(&conn, "steps").unwrap_or(false) {
        return Ok(Vec::new());
    }

    let file_mtime = modified_ms(db_path);
    let mut step_models = HashMap::<i64, String>::new();
    let mut fallback_model = "gemini-3.8-flash".to_string();

    if sqlite_table_exists(&conn, "gen_metadata").unwrap_or(false) {
        if let Ok(mut stmt) = conn.prepare("SELECT data FROM gen_metadata WHERE data IS NOT NULL ORDER BY idx ASC") {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0)) {
                for data in rows.flatten() {
                    if let Some((m, last_step_idx)) = parse_antigravity_gen_metadata(&data) {
                        fallback_model = m.clone();
                        if let Some(lsi) = last_step_idx {
                            step_models.insert(lsi + 1, m);
                        }
                    }
                }
            }
        }
    }

    let mut stmt = conn
        .prepare("SELECT idx, metadata FROM steps WHERE metadata IS NOT NULL")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let idx: i64 = row.get(0)?;
            let metadata: Vec<u8> = row.get(1)?;
            Ok((idx, metadata))
        })
        .map_err(|e| e.to_string())?;

    let mut records = Vec::new();
    for row in rows {
        let (idx, metadata) = match row {
            Ok(r) => r,
            Err(_) => continue,
        };
        if let Some(step_tokens) = parse_antigravity_step_tokens(&metadata) {
            let ts = if step_tokens.timestamp_ms > 0 {
                step_tokens.timestamp_ms
            } else {
                file_mtime
            };
            if ts < window.start_ms || ts >= window.end_ms {
                continue;
            }
            let model = step_models
                .get(&idx)
                .cloned()
                .unwrap_or_else(|| fallback_model.clone());
            let total = total_or_sum(
                0,
                step_tokens.input_tokens,
                step_tokens.output_tokens,
                step_tokens.cache_tokens,
            );
            record_usage_parsed_entry();
            records.push(UsageRecord {
                session_id: session_id.to_string(),
                model: Some(model),
                timestamp_ms: ts,
                input_tokens: step_tokens.input_tokens,
                output_tokens: step_tokens.output_tokens,
                cache_tokens: step_tokens.cache_tokens,
                cache_read_tokens: step_tokens.cache_tokens,
                total_tokens: total,
            });
        }
    }
    Ok(records)
}

fn collect_antigravity_db_usage(
    home: &Path,
    window: &UsageWindow,
    scan: &mut ToolScan,
    seen_session_ids: &mut HashSet<String>,
) {
    for conv_root in antigravity_conversations_roots(home) {
        let Ok(entries) = fs::read_dir(&conv_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("db") {
                continue;
            }
            let session_id = file_stem_session_id(&path);
            scan.source_status = "available".to_string();
            seen_session_ids.insert(session_id.clone());
            if !usage_file_may_overlap_window(modified_ms(&path), window.start_ms) {
                continue;
            }
            scan.scanned_sessions += 1;
            match read_antigravity_db_tokens(&path, &session_id, window) {
                Ok(records) => {
                    scan.records.extend(records);
                }
                Err(error) => scan.errors.push(format!("{}: {error}", path.display())),
            }
        }
    }
}

pub(super) fn collect_antigravity_usage_records(window: &UsageWindow) -> ToolScan {
    let Some(home) = usage_home_dir() else {
        return unavailable_scan();
    };
    let tmp_root = home.join(".gemini").join("tmp");
    let has_brain_root = antigravity_brain_roots(&home).iter().any(|p| p.is_dir());
    let has_conversations_root = antigravity_conversations_roots(&home).iter().any(|p| p.is_dir());
    let mut scan = ToolScan {
        source_status: if tmp_root.is_dir() || has_brain_root || has_conversations_root {
            "available".to_string()
        } else {
            "unavailable".to_string()
        },
        scanned_sessions: 0,
        transcript_calls: 0,
        records: Vec::new(),
        errors: Vec::new(),
    };
    let mut seen_session_ids = HashSet::new();
    collect_antigravity_db_usage(&home, window, &mut scan, &mut seen_session_ids);

    for path in antigravity_session_files(&tmp_root) {
        if !usage_file_may_overlap_window(modified_ms(&path), window.start_ms) {
            continue;
        }
        scan.scanned_sessions += 1;
        let is_jsonl = path.extension().and_then(|extension| extension.to_str()) == Some("jsonl");
        let parsed = shared_usage_file_cache().get_or_parse(&path, || {
            if is_jsonl {
                parse_antigravity_jsonl_usage_file(&path)
            } else {
                parse_antigravity_json_usage_file(&path)
            }
        });
        match parsed {
            Ok(records) => {
                for r in records.iter() {
                    seen_session_ids.insert(r.session_id.clone());
                }
                scan.records.extend(records.iter().cloned());
            }
            Err(error) => scan.errors.push(format!("{}: {error}", path.display())),
        }
    }
    collect_antigravity_transcript_usage(&home, window, &mut scan, &seen_session_ids);
    scan
}

/// Counts in-window `USER_INPUT` rows from brain-root transcripts that are not
/// already covered by conversation databases or tmp session files.
fn collect_antigravity_transcript_usage(
    home: &Path,
    window: &UsageWindow,
    scan: &mut ToolScan,
    seen_session_ids: &HashSet<String>,
) {
    for brain_root in antigravity_brain_roots(home) {
        let Ok(entries) = fs::read_dir(&brain_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let conversation_dir = entry.path();
            if !conversation_dir.is_dir() {
                continue;
            }
            let session_id = file_stem_session_id(&conversation_dir);
            if seen_session_ids.contains(&session_id) {
                continue;
            }
            let Some(transcript) = find_antigravity_transcript(&conversation_dir) else {
                continue;
            };
            // A discovered transcript makes the source available even when all
            // of its rows fall outside the window.
            scan.source_status = "available".to_string();
            if !usage_file_may_overlap_window(modified_ms(&transcript), window.start_ms) {
                continue;
            }
            match parse_antigravity_transcript_calls(&transcript, window.start_ms, window.end_ms) {
                Ok(calls) if calls > 0 => {
                    scan.scanned_sessions += 1;
                    scan.transcript_calls = scan.transcript_calls.saturating_add(calls);
                }
                Ok(_) => {}
                Err(error) => scan.errors.push(format!("{}: {error}", transcript.display())),
            }
        }
    }
}

fn parse_antigravity_transcript_calls(
    path: &Path,
    start_ms: i64,
    end_ms: i64,
) -> Result<u64, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    record_usage_source_read();
    let reader = BufReader::new(file);
    let fallback_ms = modified_ms(path);
    let mut calls = 0_u64;
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Corrupt lines are skipped; a transcript that cannot be read at all
        // surfaces as a single error from the caller.
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if !value
            .get("type")
            .and_then(Value::as_str)
            .map(|kind| kind.eq_ignore_ascii_case("USER_INPUT"))
            .unwrap_or(false)
        {
            continue;
        }
        let timestamp_ms = antigravity_entry_timestamp_ms(&value).unwrap_or(fallback_ms);
        if timestamp_ms >= start_ms && timestamp_ms < end_ms {
            record_usage_parsed_entry();
            calls += 1;
        }
    }
    Ok(calls)
}

/// Resolves HOME for usage scans so tests can redirect it via the thread-local
/// override without mutating the process environment.
pub(super) fn usage_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        if let Some(home) = crate::config::test_home::test_home_override() {
            return Some(home);
        }
    }
    dirs::home_dir()
}

fn antigravity_session_files(tmp_root: &Path) -> Vec<PathBuf> {
    let mut out = json_files_recursive(tmp_root, ".json");
    out.extend(json_files_recursive(tmp_root, ".jsonl"));
    out.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("session-"))
            .unwrap_or(false)
    });
    out
}

fn parse_antigravity_json_usage_file(path: &Path) -> Result<Vec<UsageRecord>, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    record_usage_source_read();
    let value: Value = serde_json::from_str(&content).map_err(|error| error.to_string())?;
    let session_id = json_nonempty_string(value.get("sessionId"))
        .unwrap_or_else(|| file_stem_session_id(path));
    let file_model = json_nonempty_string(value.get("model"))
        .or_else(|| json_nonempty_string(value.get("modelName")));
    let mut out = Vec::new();
    let Some(messages) = value.get("messages").and_then(Value::as_array) else {
        return Ok(out);
    };
    for message in messages {
        let Some(tokens) = message.get("tokens") else { continue; };
        let input = json_u64(tokens.get("input"));
        let output = json_u64(tokens.get("output"));
        let cache = json_u64(tokens.get("cached")).saturating_add(json_u64(tokens.get("cache")));
        let total = total_or_sum(json_u64(tokens.get("total")), input, output, cache);
        if input == 0 && output == 0 && cache == 0 && total == 0 {
            continue;
        }
        let timestamp_ms = json_millis(message.get("timestamp"))
            .or_else(|| json_millis(message.get("time")))
            .or_else(|| json_millis(value.get("lastUpdated")))
            .or_else(|| json_millis(value.get("startTime")))
            .unwrap_or_else(|| modified_ms(path));
        record_usage_parsed_entry();
        out.push(UsageRecord {
            session_id: session_id.clone(),
            model: json_nonempty_string(message.get("model"))
                .or_else(|| json_nonempty_string(message.get("modelName")))
                .or_else(|| {
                    message
                        .get("metadata")
                        .and_then(|metadata| json_nonempty_string(metadata.get("model")))
                })
                .or_else(|| file_model.clone()),
            timestamp_ms,
            input_tokens: input,
            output_tokens: output,
            cache_tokens: cache,
            cache_read_tokens: 0,
            total_tokens: total,
        });
    }
    Ok(out)
}

fn parse_antigravity_jsonl_usage_file(path: &Path) -> Result<Vec<UsageRecord>, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    record_usage_source_read();
    let reader = BufReader::new(file);
    let fallback_session_id = file_stem_session_id(path);
    let mut session_id = String::new();
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|error| error.to_string())?;
        if value.get("$set").is_some() {
            continue;
        }
        if session_id.is_empty() {
            if let Some(id) = json_nonempty_string(value.get("sessionId")) {
                session_id = id;
            }
        }
        let Some(tokens) = value.get("tokens") else {
            continue;
        };
        let input = json_u64(tokens.get("input"));
        let output = json_u64(tokens.get("output"));
        let cached = json_u64(tokens.get("cached"));
        let total = total_or_sum(json_u64(tokens.get("total")), input, output, cached);
        if input == 0 && output == 0 && cached == 0 && total == 0 {
            continue;
        }
        let timestamp_ms = json_millis(value.get("timestamp"))
            .or_else(|| json_millis(value.get("time")))
            .unwrap_or_else(|| modified_ms(path));
        record_usage_parsed_entry();
        out.push(UsageRecord {
            session_id: if session_id.is_empty() {
                fallback_session_id.clone()
            } else {
                session_id.clone()
            },
            model: json_nonempty_string(value.get("model"))
                .or_else(|| json_nonempty_string(value.get("modelName"))),
            timestamp_ms,
            input_tokens: input,
            output_tokens: output,
            cache_tokens: cached,
            cache_read_tokens: 0,
            total_tokens: total,
        });
    }
    Ok(out)
}
