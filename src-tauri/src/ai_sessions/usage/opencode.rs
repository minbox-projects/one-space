use super::json::{
    json_files_recursive, json_nonempty_string, json_u64, modified_ms, sqlite_table_exists,
    total_or_sum,
};
use super::model::{ToolScan, UsageRecord, UsageWindow};
use super::stats::{record_usage_parsed_entry, record_usage_source_read};
use crate::ai_sessions::{candidate_opencode_storage_paths, shared_usage_file_cache};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn collect_opencode_usage_records(
    window: &UsageWindow,
    _include_model_breakdown: bool,
) -> ToolScan {
    let db_path = dirs::home_dir()
        .map(|home| {
            home.join(".local")
                .join("share")
                .join("opencode")
                .join("opencode.db")
        })
        .unwrap_or_default();
    let storage_roots = candidate_opencode_storage_paths()
        .into_iter()
        .filter_map(|paths| paths.sessions_root.parent().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    collect_opencode_usage_records_from_sources(
        &db_path,
        &storage_roots,
        window.start_ms,
        window.end_ms,
    )
}

#[derive(Debug, Default)]
struct OpenCodeUsageSource {
    available: bool,
    session_ids: HashSet<String>,
    blocking_session_ids: HashSet<String>,
    records: Vec<UsageRecord>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum OpenCodeDbUsageSchema {
    V2,
    V1,
}

impl OpenCodeDbUsageSchema {
    fn session_table(self) -> &'static str {
        match self {
            Self::V2 => "session_v2",
            Self::V1 => "session",
        }
    }

    fn message_table(self) -> &'static str {
        match self {
            Self::V2 => "session_message",
            Self::V1 => "message",
        }
    }
}

pub(in crate::ai_sessions) fn collect_opencode_usage_records_from_sources(
    db_path: &Path,
    storage_roots: &[PathBuf],
    start_ms: i64,
    end_ms: i64,
) -> ToolScan {
    let mut sources = Vec::<OpenCodeUsageSource>::new();
    let mut source_errors = Vec::<String>::new();

    if db_path.is_file() {
        match Connection::open(db_path) {
            Ok(conn) => {
                record_usage_source_read();
                let mut found_supported_schema = false;
                for schema in [OpenCodeDbUsageSchema::V2, OpenCodeDbUsageSchema::V1] {
                    match opencode_db_schema_presence(&conn, schema) {
                        Ok((false, false)) => {}
                        Ok((true, true)) => {
                            found_supported_schema = true;
                            match read_opencode_usage_source_from_db(
                                &conn, db_path, schema, start_ms, end_ms,
                            ) {
                                Ok(source) => sources.push(source),
                                Err(error) => source_errors.push(error),
                            }
                        }
                        Ok((has_session, has_message)) => {
                            found_supported_schema = true;
                            source_errors.push(format!(
                                "{}: incomplete OpenCode usage schema ({}={}, {}={})",
                                db_path.display(),
                                schema.session_table(),
                                has_session,
                                schema.message_table(),
                                has_message
                            ));
                        }
                        Err(error) => {
                            found_supported_schema = true;
                            source_errors.push(format!("{}: {error}", db_path.display()));
                        }
                    }
                }
                if !found_supported_schema {
                    source_errors.push(format!(
                        "{}: no supported OpenCode usage tables",
                        db_path.display()
                    ));
                }
            }
            Err(error) => source_errors.push(format!("{}: {error}", db_path.display())),
        }
    }

    for storage_root in storage_roots {
        let source = read_opencode_usage_source_from_storage_root(storage_root, start_ms, end_ms);
        if source.available || !source.errors.is_empty() {
            sources.push(source);
        }
    }

    let mut available = false;
    let mut claimed_session_ids = HashSet::<String>::new();
    let mut blocked_session_ids = HashSet::<String>::new();
    let mut records = Vec::<UsageRecord>::new();
    let mut errors = source_errors;
    for source in sources {
        errors.extend(source.errors);
        if !source.available {
            continue;
        }
        available = true;
        let newly_claimed = source
            .session_ids
            .into_iter()
            .filter(|session_id| !blocked_session_ids.contains(session_id))
            .filter(|session_id| claimed_session_ids.insert(session_id.clone()))
            .collect::<HashSet<_>>();
        records.extend(
            source
                .records
                .into_iter()
                .filter(|record| newly_claimed.contains(&record.session_id)),
        );
        blocked_session_ids.extend(source.blocking_session_ids);
    }

    ToolScan {
        source_status: if available {
            "available"
        } else if errors.is_empty() {
            "unavailable"
        } else {
            "error"
        }
        .to_string(),
        scanned_sessions: claimed_session_ids.len() as u64,
        transcript_calls: 0,
        records,
        errors,
    }
}

fn opencode_db_schema_presence(
    conn: &Connection,
    schema: OpenCodeDbUsageSchema,
) -> Result<(bool, bool), rusqlite::Error> {
    Ok((
        sqlite_table_exists(conn, schema.session_table())?,
        sqlite_table_exists(conn, schema.message_table())?,
    ))
}

fn read_trimmed_session_ids(
    conn: &Connection,
    query: &str,
    db_path: &Path,
    errors: &mut Vec<String>,
) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare(query)
        .map_err(|error| format!("{}: {error}", db_path.display()))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("{}: {error}", db_path.display()))?;

    let mut out = HashSet::new();
    for row in rows {
        match row {
            Ok(session_id) if !session_id.trim().is_empty() => {
                out.insert(session_id.trim().to_string());
            }
            Ok(_) => {}
            Err(error) => errors.push(format!("{}: {error}", db_path.display())),
        }
    }
    Ok(out)
}

fn opencode_model_name(value: &Value) -> Option<String> {
    let model = value
        .get("modelID")
        .or_else(|| value.get("model"))
        .or_else(|| value.get("data").and_then(|data| data.get("modelID")))?;
    json_nonempty_string(Some(model)).or_else(|| {
        model
            .get("id")
            .and_then(|id| json_nonempty_string(Some(id)))
    })
}

fn opencode_v2_model_name(value: &Value) -> Option<String> {
    value
        .get("model")
        .and_then(|model| model.get("id"))
        .or_else(|| {
            value
                .get("data")
                .and_then(|data| data.get("model"))
                .and_then(|model| model.get("id"))
        })
        .and_then(|id| json_nonempty_string(Some(id)))
        .or_else(|| json_nonempty_string(value.get("modelID")))
        .or_else(|| {
            value
                .get("data")
                .and_then(|data| json_nonempty_string(data.get("modelID")))
        })
        .or_else(|| {
            value
                .get("model")
                .and_then(|model| json_nonempty_string(Some(model)))
        })
}

fn read_opencode_usage_source_from_db(
    conn: &Connection,
    db_path: &Path,
    schema: OpenCodeDbUsageSchema,
    start_ms: i64,
    end_ms: i64,
) -> Result<OpenCodeUsageSource, String> {
    let session_query = match schema {
        OpenCodeDbUsageSchema::V2 => "SELECT id FROM session_v2 WHERE time_archived IS NULL",
        OpenCodeDbUsageSchema::V1 => "SELECT id FROM session WHERE time_archived IS NULL",
    };
    let blocking_session_query = match schema {
        OpenCodeDbUsageSchema::V2 => "SELECT id FROM session_v2",
        OpenCodeDbUsageSchema::V1 => "SELECT id FROM session",
    };
    let message_query = match schema {
        OpenCodeDbUsageSchema::V2 => {
            r#"
            SELECT session_id, time_created, data
            FROM session_message
            WHERE time_created >= ?1
              AND time_created < ?2
            ORDER BY time_created ASC
            "#
        }
        OpenCodeDbUsageSchema::V1 => {
            r#"
            SELECT session_id, time_created, data
            FROM message
            WHERE time_created >= ?1
              AND time_created < ?2
            ORDER BY time_created ASC
            "#
        }
    };

    let mut source = OpenCodeUsageSource {
        available: true,
        ..OpenCodeUsageSource::default()
    };
    source.session_ids =
        read_trimmed_session_ids(conn, session_query, db_path, &mut source.errors)?;
    source.blocking_session_ids =
        read_trimmed_session_ids(conn, blocking_session_query, db_path, &mut source.errors)?;

    let mut message_stmt = conn
        .prepare(message_query)
        .map_err(|error| format!("{}: {error}", db_path.display()))?;
    let rows = message_stmt
        .query_map(params![start_ms, end_ms], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("{}: {error}", db_path.display()))?;
    for row in rows {
        match row {
            Ok((session_id, timestamp_ms, data)) => {
                let session_id = session_id.trim();
                if session_id.is_empty() || !source.session_ids.contains(session_id) {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&data) else {
                    source.errors.push(format!(
                        "{}: invalid OpenCode usage JSON for session {}",
                        db_path.display(),
                        session_id
                    ));
                    continue;
                };
                let tokens = value
                    .get("tokens")
                    .or_else(|| value.get("data").and_then(|data| data.get("tokens")));
                if let Some((input, output, cache, cache_read, total)) =
                    parse_opencode_tokens_value(tokens)
                {
                    record_usage_parsed_entry();
                    source.records.push(UsageRecord {
                        session_id: session_id.to_string(),
                        model: match schema {
                            OpenCodeDbUsageSchema::V2 => opencode_v2_model_name(&value),
                            OpenCodeDbUsageSchema::V1 => opencode_model_name(&value),
                        },
                        timestamp_ms,
                        input_tokens: input,
                        output_tokens: output,
                        cache_tokens: cache,
                        cache_read_tokens: cache_read,
                        total_tokens: total,
                    });
                }
            }
            Err(error) => source
                .errors
                .push(format!("{}: {error}", db_path.display())),
        }
    }
    Ok(source)
}

fn read_opencode_usage_source_from_storage_root(
    storage_root: &Path,
    start_ms: i64,
    end_ms: i64,
) -> OpenCodeUsageSource {
    let sessions_root = storage_root.join("session");
    let messages_root = storage_root.join("message");
    if !sessions_root.is_dir() && !messages_root.is_dir() {
        return OpenCodeUsageSource::default();
    }
    let mut source = OpenCodeUsageSource {
        available: true,
        ..OpenCodeUsageSource::default()
    };
    source.session_ids = opencode_json_session_ids(&sessions_root)
        .into_iter()
        .collect();
    source.blocking_session_ids = source.session_ids.clone();
    for session_id in &source.session_ids {
        let messages_dir = messages_root.join(session_id);
        let (records, errors) = parse_opencode_message_usage_dir(&messages_dir, session_id);
        source.records.extend(
            records
                .into_iter()
                .filter(|record| record.timestamp_ms >= start_ms && record.timestamp_ms < end_ms),
        );
        source.errors.extend(errors);
    }
    source
}

#[cfg(test)]
pub(super) fn read_opencode_message_tokens_from_db(
    conn: &Connection,
    db_path: &Path,
    window: &UsageWindow,
) -> ToolScan {
    match read_opencode_usage_source_from_db(
        conn,
        db_path,
        OpenCodeDbUsageSchema::V1,
        window.start_ms,
        window.end_ms,
    ) {
        Ok(source) => ToolScan {
            source_status: "available".to_string(),
            scanned_sessions: source.session_ids.len() as u64,
            transcript_calls: 0,
            records: source.records,
            errors: source.errors,
        },
        Err(error) => ToolScan {
            source_status: "error".to_string(),
            scanned_sessions: 0,
            transcript_calls: 0,
            records: Vec::new(),
            errors: vec![error],
        },
    }
}

pub(in crate::ai_sessions) fn parse_opencode_message_usage_dir(
    messages_dir: &Path,
    session_id: &str,
) -> (Vec<UsageRecord>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    if !messages_dir.is_dir() {
        return (out, errors);
    }
    for path in json_files_recursive(messages_dir, "json") {
        match shared_usage_file_cache()
            .get_or_parse(&path, || parse_opencode_usage_message_file(&path, session_id))
        {
            Ok(records) => out.extend(records.iter().cloned()),
            Err(error) => errors.push(error),
        }
    }
    (out, errors)
}

/// Parses one OpenCode storage message file into its optional usage record.
/// Errors carry the file path, matching the directory-level contract.
fn parse_opencode_usage_message_file(
    path: &Path,
    session_id: &str,
) -> Result<Vec<UsageRecord>, String> {
    let content =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    record_usage_source_read();
    let value: Value =
        serde_json::from_str(&content).map_err(|error| format!("{}: {error}", path.display()))?;
    let Some((input, output, cache, cache_read, total)) = parse_opencode_tokens_value(
        value
            .get("tokens")
            .or_else(|| value.get("data").and_then(|data| data.get("tokens"))),
    ) else {
        return Ok(Vec::new());
    };
    let timestamp_ms = value
        .get("time")
        .and_then(|time| time.get("created"))
        .and_then(|v| v.as_i64())
        .or_else(|| value.get("time_created").and_then(|v| v.as_i64()))
        .unwrap_or_else(|| modified_ms(path));
    record_usage_parsed_entry();
    Ok(vec![UsageRecord {
        session_id: session_id.to_string(),
        model: opencode_model_name(&value),
        timestamp_ms,
        input_tokens: input,
        output_tokens: output,
        cache_tokens: cache,
        cache_read_tokens: cache_read,
        total_tokens: total,
    }])
}

fn parse_opencode_tokens_value(tokens: Option<&Value>) -> Option<(u64, u64, u64, u64, u64)> {
    let tokens = tokens?;
    let input = json_u64(tokens.get("input")).saturating_add(json_u64(tokens.get("input_tokens")));
    let output =
        json_u64(tokens.get("output")).saturating_add(json_u64(tokens.get("output_tokens")));
    let cache_read = json_u64(tokens.get("cache_read"))
        .saturating_add(json_u64(tokens.get("cached")))
        .saturating_add(json_u64(tokens.get("cache_read_tokens")))
        .saturating_add(json_u64(
            tokens.get("cache").and_then(|cache| cache.get("read")),
        ));
    let cache_write = json_u64(tokens.get("cache_write"))
        .saturating_add(json_u64(tokens.get("cache_write_tokens")))
        .saturating_add(json_u64(tokens.get("cache")))
        .saturating_add(json_u64(
            tokens.get("cache").and_then(|cache| cache.get("write")),
        ));
    let cache = cache_read.saturating_add(cache_write);
    let total = total_or_sum(
        json_u64(tokens.get("total")).saturating_add(json_u64(tokens.get("total_tokens"))),
        input,
        output,
        cache,
    );
    if input == 0 && output == 0 && cache == 0 && total == 0 {
        None
    } else {
        Some((input, output, cache, cache_read, total))
    }
}

fn opencode_json_session_ids(sessions_root: &Path) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in json_files_recursive(sessions_root, ".json") {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        record_usage_source_read();
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let Some(id) = value
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if seen.insert(id.to_string()) {
            out.push(id.to_string());
        }
    }
    out
}
