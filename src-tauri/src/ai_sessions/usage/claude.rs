use super::json::{
    file_stem_session_id, json_files_recursive, json_nonempty_string, json_u64, modified_ms,
    total_or_sum, usage_file_may_overlap_window,
};
use super::model::{unavailable_scan, ToolScan, UsageRecord, UsageWindow};
use super::stats::{record_usage_parsed_entry, record_usage_source_read};
use crate::ai_sessions::{parse_rfc3339_millis, shared_usage_file_cache};
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub(super) fn collect_claude_usage_records(window: &UsageWindow) -> ToolScan {
    let Some(home) = dirs::home_dir() else {
        return unavailable_scan();
    };
    let projects_root = home.join(".claude").join("projects");
    if !projects_root.is_dir() {
        return unavailable_scan();
    }
    let mut scan = ToolScan {
        source_status: "available".to_string(),
        scanned_sessions: 0,
        transcript_calls: 0,
        records: Vec::new(),
        errors: Vec::new(),
    };
    for path in json_files_recursive(&projects_root, "jsonl") {
        if !usage_file_may_overlap_window(modified_ms(&path), window.start_ms) {
            continue;
        }
        scan.scanned_sessions += 1;
        match shared_usage_file_cache().get_or_parse(&path, || parse_claude_usage_file(&path)) {
            Ok(records) => scan.records.extend(records.iter().cloned()),
            Err(error) => scan.errors.push(format!("{}: {error}", path.display())),
        }
    }
    scan
}

pub(in crate::ai_sessions) fn parse_claude_usage_file(
    path: &Path,
) -> Result<Vec<UsageRecord>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    record_usage_source_read();
    let reader = BufReader::new(file);
    let fallback_session_id = file_stem_session_id(path);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if value.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let Some(usage) = value
            .get("message")
            .and_then(|message| message.get("usage"))
        else {
            continue;
        };
        let cache_read = json_u64(usage.get("cache_read_input_tokens"));
        let cache_create = json_u64(usage.get("cache_creation_input_tokens"));
        let input = json_u64(usage.get("input_tokens"));
        let output = json_u64(usage.get("output_tokens"));
        let cache = cache_read.saturating_add(cache_create);
        let total = total_or_sum(json_u64(usage.get("total_tokens")), input, output, cache);
        if input == 0 && output == 0 && cache == 0 && total == 0 {
            continue;
        }
        let timestamp_ms = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_rfc3339_millis)
            .unwrap_or_else(|| modified_ms(path));
        record_usage_parsed_entry();
        out.push(UsageRecord {
            session_id: value
                .get("sessionId")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&fallback_session_id)
                .to_string(),
            model: value
                .get("message")
                .and_then(|message| json_nonempty_string(message.get("model"))),
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
