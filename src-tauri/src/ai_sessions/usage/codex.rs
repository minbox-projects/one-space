use super::json::{
    file_stem_session_id, json_nonempty_string, json_u64, modified_ms,
    usage_file_may_overlap_window,
};
use super::model::{ToolScan, UsageRecord, UsageWindow};
use super::stats::{record_usage_parsed_entry, record_usage_source_read};
use crate::ai_sessions::{candidate_home_dirs, collect_codex_session_files, parse_rfc3339_millis};
use crate::ai_sessions::shared_usage_file_cache;
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub(super) fn collect_codex_usage_records(window: &UsageWindow) -> ToolScan {
    let mut scan = ToolScan {
        source_status: "unavailable".to_string(),
        scanned_sessions: 0,
        transcript_calls: 0,
        records: Vec::new(),
        errors: Vec::new(),
    };
    for home in candidate_home_dirs(None) {
        for root in [
            home.join(".codex").join("sessions"),
            home.join(".codex").join("archived_sessions"),
        ] {
            if !root.is_dir() {
                continue;
            }
            scan.source_status = "available".to_string();
            for (path, modified_ms) in collect_codex_session_files(&root, usize::MAX) {
                if !usage_file_may_overlap_window(modified_ms, window.start_ms) {
                    break;
                }
                scan.scanned_sessions += 1;
                match shared_usage_file_cache().get_or_parse(&path, || parse_codex_usage_file(&path)) {
                    Ok(records) => scan.records.extend(records.iter().cloned()),
                    Err(error) => scan.errors.push(format!("{}: {error}", path.display())),
                }
            }
        }
    }
    scan
}

pub(in crate::ai_sessions) fn parse_codex_usage_file(
    path: &Path,
) -> Result<Vec<UsageRecord>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    record_usage_source_read();
    let reader = BufReader::new(file);
    let fallback_session_id = file_stem_session_id(path);
    let mut session_id = String::new();
    let mut current_model = None;
    let mut records_pending_model: Vec<usize> = Vec::new();
    let mut out: Vec<UsageRecord> = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if value.get("type").and_then(|v| v.as_str()) == Some("session_meta") {
            if session_id.is_empty() {
                session_id = value
                    .get("payload")
                    .and_then(|payload| payload.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
            }
            if current_model.is_none() {
                current_model = value
                    .get("payload")
                    .and_then(|payload| json_nonempty_string(payload.get("model")));
            }
            continue;
        }
        if value.get("type").and_then(|v| v.as_str()) == Some("turn_context") {
            let next_model = value
                .get("payload")
                .and_then(|payload| json_nonempty_string(payload.get("model")));
            if current_model.is_none() {
                if let Some(model) = next_model.as_ref() {
                    for index in records_pending_model.drain(..) {
                        out[index].model = Some(model.clone());
                    }
                }
            }
            current_model = next_model.or(current_model);
            continue;
        }
        if value.get("type").and_then(|v| v.as_str()) != Some("event_msg") {
            continue;
        }
        let Some(payload) = value.get("payload") else {
            continue;
        };
        if payload.get("type").and_then(|v| v.as_str()) != Some("token_count") {
            continue;
        }
        let Some(usage) = payload
            .get("info")
            .and_then(|info| info.get("last_token_usage"))
        else {
            continue;
        };
        let cache_read = json_u64(usage.get("cached_input_tokens"));
        let input = json_u64(usage.get("input_tokens"));
        let output = json_u64(usage.get("output_tokens"));
        let cache = cache_read;
        // Codex reports cached input as part of input_tokens (see
        // cache_hit_rate_percent), so the no-total fallback must not add the
        // cache tier on top of the input tier.
        let total = match json_u64(usage.get("total_tokens")) {
            0 => input.saturating_add(output),
            total => total,
        };
        if input == 0 && output == 0 && cache == 0 && total == 0 {
            continue;
        }
        let timestamp_ms = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .or_else(|| payload.get("timestamp").and_then(|v| v.as_str()))
            .and_then(parse_rfc3339_millis)
            .unwrap_or_else(|| modified_ms(path));
        let model = payload
            .get("info")
            .and_then(|info| json_nonempty_string(info.get("model")))
            .or_else(|| current_model.clone());
        if model.is_none() {
            records_pending_model.push(out.len());
        }
        record_usage_parsed_entry();
        out.push(UsageRecord {
            session_id: if session_id.is_empty() {
                fallback_session_id.clone()
            } else {
                session_id.clone()
            },
            model,
            timestamp_ms,
            input_tokens: input,
            output_tokens: output,
            cache_tokens: cache,
            cache_read_tokens: cache_read,
            total_tokens: total,
        });
    }
    Ok(out)
}
