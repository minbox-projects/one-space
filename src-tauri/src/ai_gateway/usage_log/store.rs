//! SQLite-backed usage-log storage and its append/read boundaries.

use super::records::{UsageAccounting, UsageLogEntry, UsageLogRecord};
use super::schema::{
    apply_retention, initialize_connection, insert_record, record_usage_log_batch_write,
    record_usage_log_row_written, record_usage_log_transaction, usage_log_write_fault_should_fail,
    INSERT_SQL,
};
#[cfg(test)]
use super::schema::RECORD_COLUMNS;
#[cfg(test)]
use super::stats::record_from_row;
use super::super::migration;
use super::super::provider_usage;
use super::super::usage_store::{with_query_connection, with_write_connection, QueryConnection};
#[cfg(test)]
use rusqlite::Connection;
use std::path::PathBuf;

/// File name of the dedicated usage-log database under `get_app_dir()`.
pub(in crate::ai_gateway) const USAGE_DB_FILE: &str = "ai_gateway_usage.db";

/// SQLite-backed usage-log storage bound to one explicit database path.
#[derive(Debug, Clone)]
pub(in crate::ai_gateway) struct UsageLogStore {
    path: PathBuf,
}

impl UsageLogStore {
    /// Injected base path, used by tests to point at a temp directory.
    pub(in crate::ai_gateway) fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Default store under `get_app_dir()/ai_gateway_usage.db`.
    pub(in crate::ai_gateway) fn default_store() -> Result<Self, String> {
        let dir = crate::config::get_app_dir()?;
        let path = dir.join(USAGE_DB_FILE);
        migration::migrate_legacy_files();
        Ok(Self::at(path))
    }

    /// Opens and migrates a fresh connection. Public reads reuse the
    /// path-scoped query connection instead; this remains the explicit fixture
    /// and inspection opener.
    #[cfg(test)]
    pub(in crate::ai_gateway) fn open(&self) -> Result<Connection, String> {
        initialize_connection(&self.path)
    }

    /// Borrows the path-scoped reusable query connection, so schema and
    /// migration initialization run only when the actual database is opened or
    /// reopened. Public read methods use this boundary; written rows remain
    /// visible because no query result is cached.
    pub(in crate::ai_gateway) fn query_connection<T>(
        &self,
        use_connection: impl FnOnce(&QueryConnection<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        with_query_connection(&self.path, initialize_connection, use_connection)
    }

    /// Insert one record with the canonical request-log classification
    /// (`canonical_v1`, usage present and accounting valid), then permanently
    /// delete records older than the retention window (REQ-004/REQ-009).
    #[cfg(test)]
    pub(in crate::ai_gateway) fn append(
        &self,
        record: &UsageLogRecord,
        retention_days: u32,
    ) -> Result<(), String> {
        self.append_with_accounting(record, UsageAccounting::CANONICAL, retention_days)
    }

    /// Insert one record carrying the parser's real usage classification. Used
    /// by the request runtime so missing or invalid usage is persisted as such.
    pub(in crate::ai_gateway) fn append_with_accounting(
        &self,
        record: &UsageLogRecord,
        accounting: UsageAccounting,
        retention_days: u32,
    ) -> Result<(), String> {
        with_write_connection(
            &self.path,
            initialize_connection,
            |connection| {
                {
                    let transaction = connection
                        .transaction()
                        .map_err(|error| error.to_string())?;
                    {
                        let mut statement = transaction
                            .prepare(INSERT_SQL)
                            .map_err(|error| error.to_string())?;
                        insert_record(&mut statement, record, accounting)?;
                    }
                    let entry = UsageLogEntry {
                        record: record.clone(),
                        accounting,
                    };
                    provider_usage::apply_summary_contributions(
                        &transaction,
                        std::slice::from_ref(&entry),
                    )?;
                    transaction.commit().map_err(|error| error.to_string())?;
                }
                record_usage_log_row_written();
                record_usage_log_transaction();
                apply_retention(connection, retention_days);
                Ok(())
            },
        )
    }

    /// Insert every record of one request through a single connection, in slice
    /// order, then apply the same retention cleanup as [`Self::append`]
    /// (REQ-005). Each record carries the canonical request-log classification.
    /// An empty slice writes nothing.
    #[cfg(test)]
    pub(in crate::ai_gateway) fn append_batch(
        &self,
        records: &[UsageLogRecord],
        retention_days: u32,
    ) -> Result<(), String> {
        let entries: Vec<UsageLogEntry> = records
            .iter()
            .cloned()
            .map(|record| UsageLogEntry {
                record,
                accounting: UsageAccounting::CANONICAL,
            })
            .collect();
        self.append_batch_with_accounting(&entries, retention_days)
    }

    /// Insert every entry of one request through one explicit transaction on the
    /// path-scoped write connection, in slice order, committing only after every
    /// insert succeeded, then apply the same retention cleanup as [`Self::append`].
    /// A failed insert rolls the whole batch back, leaving no partial
    /// request. An empty slice writes nothing.
    pub(in crate::ai_gateway) fn append_batch_with_accounting(
        &self,
        entries: &[UsageLogEntry],
        retention_days: u32,
    ) -> Result<(), String> {
        with_write_connection(
            &self.path,
            initialize_connection,
            |connection| {
                if !entries.is_empty() {
                    {
                        let transaction = connection
                            .transaction()
                            .map_err(|error| error.to_string())?;
                        {
                            let mut statement = transaction
                                .prepare(INSERT_SQL)
                                .map_err(|error| error.to_string())?;
                            for (index, entry) in entries.iter().enumerate() {
                                if usage_log_write_fault_should_fail(index) {
                                    return Err("injected usage log insert failure".to_string());
                                }
                                insert_record(&mut statement, &entry.record, entry.accounting)?;
                            }
                        }
                        // Detail rows and their eligible provider/day contributions
                        // commit as one unit; a summary-update fault rolls the
                        // details back too.
                        provider_usage::apply_summary_contributions(
                            &transaction,
                            entries,
                        )?;
                        transaction.commit().map_err(|error| error.to_string())?;
                    }
                    record_usage_log_batch_write();
                    record_usage_log_transaction();
                    for _ in entries {
                        record_usage_log_row_written();
                    }
                }
                apply_retention(connection, retention_days);
                Ok(())
            },
        )
    }

    /// Test-only inspection helper: total row count.
    #[cfg(test)]
    pub(in crate::ai_gateway) fn count(&self) -> Result<u32, String> {
        self.query_connection(|query| {
            let count: i64 = query
                .prepare("SELECT COUNT(*) FROM usage_logs")?
                .query_row([], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            Ok(count as u32)
        })
    }

    /// Test-only inspection helper: every row, newest first.
    #[cfg(test)]
    pub(in crate::ai_gateway) fn all_records(&self) -> Result<Vec<UsageLogRecord>, String> {
        self.query_connection(|query| {
            let mut statement = query.prepare(&format!(
                "SELECT {RECORD_COLUMNS}
                 FROM usage_logs ORDER BY timestamp_ms DESC, id DESC"
            ))?;
            let rows = statement
                .query_map([], record_from_row)
                .map_err(|error| error.to_string())?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())
        })
    }
}
