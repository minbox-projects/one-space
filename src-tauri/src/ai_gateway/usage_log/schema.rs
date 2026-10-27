//! Usage-log database schema, migrations, inserts and write-side test seams.

use super::clock::now_millis;
use super::range::DAY_MS;
use super::records::{normalize_retention_days, UsageAccounting, UsageLogRecord};
use super::super::migration;
use rusqlite::Connection;
use std::fs;
use std::path::Path;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS usage_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ms INTEGER NOT NULL,
    local_model TEXT NOT NULL,
    upstream_model TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    provider_name TEXT NOT NULL,
    result TEXT NOT NULL,
    status INTEGER NOT NULL,
    input_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,
    amount REAL,
    duration_ms INTEGER NOT NULL,
    error_message TEXT,
    terminal INTEGER NOT NULL DEFAULT 1,
    reasoning_effort TEXT,
    cost_breakdown TEXT,
    first_token_ms REAL,
    avg_tps REAL,
    resolved_reasoning_effort TEXT,
    native_reasoning TEXT
);
CREATE INDEX IF NOT EXISTS idx_usage_logs_timestamp ON usage_logs(timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_usage_logs_local_model ON usage_logs(local_model);
";

/// Column list and placeholders shared by every usage-log insert path. New rows
/// always carry the canonical semantics marker; presence and accounting
/// validity vary per row.
pub(super) const INSERT_SQL: &str = "
INSERT INTO usage_logs (
    timestamp_ms, local_model, upstream_model, provider_id, provider_name,
    result, status, input_tokens, cache_read_tokens, cache_write_tokens,
    output_tokens, total_tokens, amount, duration_ms, error_message, terminal,
    reasoning_effort, usage_semantics, usage_present, cache_accounting_valid,
    cost_breakdown, first_token_ms, avg_tps, resolved_reasoning_effort, native_reasoning
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
";

/// Semantics marker persisted for every newly written row (REQ-004).
pub(in crate::ai_gateway) const CANONICAL_USAGE_SEMANTICS: &str = "canonical_v1";

/// Column list of every record-producing `SELECT`, in [`record_from_row`] order.
pub(super) const RECORD_COLUMNS: &str = "timestamp_ms, local_model, upstream_model, provider_id, provider_name, result, status, input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, total_tokens, amount, duration_ms, error_message, terminal, reasoning_effort, cost_breakdown, first_token_ms, avg_tps, resolved_reasoning_effort, native_reasoning";

/// Run one additive `ALTER TABLE ... ADD COLUMN`, treating a concurrent
/// `duplicate column name` failure as success: two openers can both observe the
/// column as missing and race the same ALTER, and the first to commit wins. Every
/// other error is returned unchanged.
fn add_usage_log_column(connection: &Connection, sql: &str) -> Result<(), String> {
    match connection.execute(sql, []) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().to_ascii_lowercase().contains("duplicate column name") => {
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

/// Add the attempt/terminal columns to a database written before this change
/// (REQ-005). The `PRAGMA table_info` inspection makes the migration idempotent:
/// present columns are left alone, existing rows default to terminal with no
/// message. A concurrent opener may still win the race between the inspection
/// and an `ALTER TABLE`, so a `duplicate column name` failure from any additive
/// statement is accepted as success; any other failure is returned so the caller
/// swallows it like any other log-write error and a later open retries.
fn migrate_usage_logs(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(usage_logs)")
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    let has_column = |name: &str| columns.iter().any(|column| column == name);
    if !has_column("error_message") {
        add_usage_log_column(connection, "ALTER TABLE usage_logs ADD COLUMN error_message TEXT")?;
    }
    if !has_column("terminal") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN terminal INTEGER NOT NULL DEFAULT 1",
        )?;
    }
    if !has_column("reasoning_effort") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN reasoning_effort TEXT",
        )?;
    }
    // Nullable and never backfilled: historical rates cannot be reconstructed.
    if !has_column("cost_breakdown") {
        add_usage_log_column(connection, "ALTER TABLE usage_logs ADD COLUMN cost_breakdown TEXT")?;
    }
    // Additive usage-accounting columns (REQ-004). Pre-existing rows default to
    // legacy semantics with no present/valid usage and stay excluded from the
    // new cache numerator/denominator; original tokens, totals, amounts and log
    // fields are never rewritten.
    if !has_column("usage_semantics") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN usage_semantics TEXT NOT NULL DEFAULT 'legacy'",
        )?;
    }
    if !has_column("usage_present") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN usage_present INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    if !has_column("cache_accounting_valid") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN cache_accounting_valid INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    // Nullable stream-timing measurements (REQ-002/REQ-003). Added additively and
    // never backfilled: pre-existing rows stay unknown (null) and are never
    // treated as an observed zero.
    if !has_column("first_token_ms") {
        add_usage_log_column(connection, "ALTER TABLE usage_logs ADD COLUMN first_token_ms REAL")?;
    }
    if !has_column("avg_tps") {
        add_usage_log_column(connection, "ALTER TABLE usage_logs ADD COLUMN avg_tps REAL")?;
    }
    // Nullable pool-only evidence columns (REQ-006). Added additively and never
    // backfilled: historical rows stay null and are never inferred.
    if !has_column("resolved_reasoning_effort") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN resolved_reasoning_effort TEXT",
        )?;
    }
    if !has_column("native_reasoning") {
        add_usage_log_column(
            connection,
            "ALTER TABLE usage_logs ADD COLUMN native_reasoning TEXT",
        )?;
    }
    Ok(())
}

/// Bind one record and its classification into a prepared [`INSERT_SQL`].
pub(super) fn insert_record(
    statement: &mut rusqlite::Statement<'_>,
    record: &UsageLogRecord,
    accounting: UsageAccounting,
) -> Result<(), String> {
    let cost_breakdown = record
        .cost_breakdown
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| error.to_string())?;
    let native_reasoning = record
        .native_reasoning
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| error.to_string())?;
    statement
        .execute(rusqlite::params![
            record.timestamp_ms,
            record.local_model,
            record.upstream_model,
            record.provider_id,
            record.provider_name,
            record.result.as_str(),
            record.status as i64,
            record.input_tokens as i64,
            record.cache_read_tokens as i64,
            record.cache_write_tokens as i64,
            record.output_tokens as i64,
            record.total_tokens as i64,
            record.amount,
            record.duration_ms as i64,
            record.error_message,
            record.terminal,
            record.reasoning_effort,
            CANONICAL_USAGE_SEMANTICS,
            accounting.present as i64,
            accounting.valid as i64,
            cost_breakdown,
            record.first_token_ms,
            record.avg_tps,
            record.resolved_reasoning_effort,
            native_reasoning,
        ])
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Permanently delete rows older than the retention window (REQ-009).
pub(super) fn apply_retention(connection: &Connection, retention_days: u32) {
    let cutoff = now_millis() - normalize_retention_days(retention_days) as i64 * DAY_MS;
    let _ = connection.execute("DELETE FROM usage_logs WHERE timestamp_ms < ?", [cutoff]);
}

// ---------------------------------------------------------------------------
// Benchmark-only usage-log write counters (REQ-005/AC-005 baseline).
//
// Compiled only into test builds; the ignored `core_workflows_perf` measurement
// tests reset and read them. `db_opens` counts every `UsageLogStore` connection
// opened for a write, `transactions` counts each committed write unit (one per
// single/batch append; at baseline these are SQLite autocommits, and the later
// per-request transaction increments the same counter once), `rows_written`
// counts successful row inserts and `batch_writes` counts batch append calls.
// They change no behavior.
// ---------------------------------------------------------------------------

/// Counters describing one or more usage-log write passes.
#[cfg(test)]
#[allow(dead_code)] // consumed by the Test-owned `core_workflows_perf` measurement tests
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ai_gateway) struct UsageLogWriteStats {
    pub db_opens: u64,
    pub transactions: u64,
    pub rows_written: u64,
    pub batch_writes: u64,
}

#[cfg(test)]
static USAGE_LOG_DB_OPENS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static USAGE_LOG_TRANSACTIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static USAGE_LOG_ROWS_WRITTEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static USAGE_LOG_BATCH_WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
fn record_usage_log_db_open() {
    USAGE_LOG_DB_OPENS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
fn record_usage_log_db_open() {}

#[cfg(test)]
pub(super) fn record_usage_log_transaction() {
    USAGE_LOG_TRANSACTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
pub(super) fn record_usage_log_transaction() {}

#[cfg(test)]
pub(super) fn record_usage_log_row_written() {
    USAGE_LOG_ROWS_WRITTEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
pub(super) fn record_usage_log_row_written() {}

#[cfg(test)]
pub(super) fn record_usage_log_batch_write() {
    USAGE_LOG_BATCH_WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
pub(super) fn record_usage_log_batch_write() {}

/// Snapshot of the benchmark-only usage-log write counters.
#[cfg(test)]
#[allow(dead_code)] // consumed by the Test-owned `core_workflows_perf` measurement tests
pub(in crate::ai_gateway) fn usage_log_write_stats() -> UsageLogWriteStats {
    UsageLogWriteStats {
        db_opens: USAGE_LOG_DB_OPENS.load(std::sync::atomic::Ordering::Relaxed),
        transactions: USAGE_LOG_TRANSACTIONS.load(std::sync::atomic::Ordering::Relaxed),
        rows_written: USAGE_LOG_ROWS_WRITTEN.load(std::sync::atomic::Ordering::Relaxed),
        batch_writes: USAGE_LOG_BATCH_WRITES.load(std::sync::atomic::Ordering::Relaxed),
    }
}

/// Zero the benchmark-only usage-log write counters.
#[cfg(test)]
#[allow(dead_code)] // consumed by the Test-owned `core_workflows_perf` measurement tests
pub(in crate::ai_gateway) fn reset_usage_log_write_stats() {
    USAGE_LOG_DB_OPENS.store(0, std::sync::atomic::Ordering::Relaxed);
    USAGE_LOG_TRANSACTIONS.store(0, std::sync::atomic::Ordering::Relaxed);
    USAGE_LOG_ROWS_WRITTEN.store(0, std::sync::atomic::Ordering::Relaxed);
    USAGE_LOG_BATCH_WRITES.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// Opens a usage-log database and brings it to the current schema. Shared by
/// the separate query connection and the path-scoped write connection so both
/// initialize and migrate the actual database exactly once per open.
pub(super) fn initialize_connection(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut connection = Connection::open(path).map_err(|error| error.to_string())?;
    record_usage_log_db_open();
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(SCHEMA)
        .map_err(|error| error.to_string())?;
    migrate_usage_logs(&connection)?;
    // The version-gated one-time cleanup and summary backfill live in the
    // permanent migration module; a later open finds the marker advanced and
    // changes nothing.
    migration::migrate_usage_database(&mut connection)?;
    Ok(connection)
}

// ---------------------------------------------------------------------------
// Test-only deterministic write-fault seam (REQ-005/AC-005).
//
// The batch insert path consults this seam at each 0-based insert index. The
// Test-owned atomic-batch test arms it so the batch fails mid-request, then
// asserts no partial batch remains and the transaction counter did not move.
// It is compiled only into test builds and never affects production.
// ---------------------------------------------------------------------------

#[cfg(test)]
static USAGE_LOG_FAIL_AT_INSERT: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(-1);

/// Arms a one-shot fault that fails the batch when it reaches the 0-based
/// `insert_index`. Cleared automatically when it fires.
#[cfg(test)]
pub(in crate::ai_gateway) fn inject_usage_log_insert_failure_at(insert_index: usize) {
    USAGE_LOG_FAIL_AT_INSERT.store(insert_index as i64, std::sync::atomic::Ordering::SeqCst);
}

/// Disarms any pending write fault.
#[cfg(test)]
pub(in crate::ai_gateway) fn clear_usage_log_insert_failure() {
    USAGE_LOG_FAIL_AT_INSERT.store(-1, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
pub(super) fn usage_log_write_fault_should_fail(insert_index: usize) -> bool {
    let armed = USAGE_LOG_FAIL_AT_INSERT.load(std::sync::atomic::Ordering::SeqCst);
    if armed < 0 || armed != insert_index as i64 {
        return false;
    }
    USAGE_LOG_FAIL_AT_INSERT.store(-1, std::sync::atomic::Ordering::SeqCst);
    true
}
#[cfg(not(test))]
pub(super) fn usage_log_write_fault_should_fail(_insert_index: usize) -> bool {
    false
}
