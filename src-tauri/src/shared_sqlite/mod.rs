use rusqlite::{ffi::ErrorCode, Connection, OpenFlags};
use std::{
    env, fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

mod migrations;

pub(crate) use migrations::MigrationDiagnostic;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const BUSY_RETRY_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SharedSqliteError {
    HomeDirectoryUnavailable,
    DirectoryCreationFailed,
    DatabaseUnavailable,
    MigrationStateInvalid,
    MigrationFailed,
}

impl std::fmt::Display for SharedSqliteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::HomeDirectoryUnavailable => "shared_sqlite_home_unavailable",
            Self::DirectoryCreationFailed => "shared_sqlite_directory_creation_failed",
            Self::DatabaseUnavailable => "shared_sqlite_database_unavailable",
            Self::MigrationStateInvalid => "shared_sqlite_migration_state_invalid",
            Self::MigrationFailed => "shared_sqlite_migration_failed",
        })
    }
}

impl std::error::Error for SharedSqliteError {}

#[derive(Debug, Clone, Copy)]
struct SqliteCauseCode {
    code: ErrorCode,
    extended_code: i32,
}

fn sqlite_cause_code(source: &rusqlite::Error) -> Option<SqliteCauseCode> {
    source.sqlite_error().map(|error| SqliteCauseCode {
        code: error.code,
        extended_code: error.extended_code,
    })
}

#[derive(Debug, Clone, Copy)]
enum BootstrapStage {
    Open,
    ConnectionConfiguration,
}

impl std::fmt::Display for BootstrapStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Open => "open",
            Self::ConnectionConfiguration => "connection_configuration",
        })
    }
}

#[derive(Debug)]
pub(crate) struct StorageFailure {
    stage: BootstrapStage,
    operation: &'static str,
    source: SharedSqliteError,
    io_kind: Option<std::io::ErrorKind>,
    raw_os_error: Option<i32>,
    sqlite_code: Option<SqliteCauseCode>,
}

impl StorageFailure {
    fn new(stage: BootstrapStage, operation: &'static str, source: SharedSqliteError) -> Self {
        Self {
            stage,
            operation,
            source,
            io_kind: None,
            raw_os_error: None,
            sqlite_code: None,
        }
    }

    fn from_io(stage: BootstrapStage, operation: &'static str, source: &std::io::Error) -> Self {
        Self {
            stage,
            operation,
            source: SharedSqliteError::DirectoryCreationFailed,
            io_kind: Some(source.kind()),
            raw_os_error: source.raw_os_error(),
            sqlite_code: None,
        }
    }

    fn from_sqlite(
        stage: BootstrapStage,
        operation: &'static str,
        source: &rusqlite::Error,
    ) -> Self {
        Self {
            stage,
            operation,
            source: SharedSqliteError::DatabaseUnavailable,
            io_kind: None,
            raw_os_error: None,
            sqlite_code: sqlite_cause_code(source),
        }
    }
}

impl std::fmt::Display for StorageFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}:context={},operation={}",
            self.source, self.stage, self.operation
        )?;
        if let Some(sqlite_code) = self.sqlite_code {
            write!(
                formatter,
                ",sqlite_code={:?},sqlite_extended_code={}",
                sqlite_code.code, sqlite_code.extended_code
            )?;
        }
        if let Some(io_kind) = self.io_kind {
            write!(formatter, ",io_kind={io_kind:?}")?;
        }
        if let Some(raw_os_error) = self.raw_os_error {
            write!(formatter, ",raw_os_error={raw_os_error}")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum BootstrapError {
    Storage {
        path: Option<PathBuf>,
        failure: StorageFailure,
    },
    Migration(MigrationDiagnostic),
}

impl std::fmt::Display for BootstrapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage { path, failure } => write!(
                formatter,
                "shared database bootstrap failed: stage={}, path={}, identified_version=unknown, target_version={}, cause={failure}",
                failure.stage,
                path.as_deref()
                    .map_or_else(|| "unknown".to_owned(), |path| path.display().to_string()),
                migrations::LATEST_VERSION
            ),
            Self::Migration(diagnostic) => diagnostic.fmt(formatter),
        }
    }
}

impl std::error::Error for BootstrapError {}

pub(crate) fn database_path() -> Result<PathBuf, SharedSqliteError> {
    let home = env::var_os("HOME").ok_or(SharedSqliteError::HomeDirectoryUnavailable)?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("onespace")
        .join("data")
        .join("onespace.sqlite3"))
}

pub(crate) fn open() -> Result<Connection, SharedSqliteError> {
    open_at(&database_path()?)
}

pub(crate) fn bootstrap() -> Result<(), BootstrapError> {
    let path = database_path().map_err(|source| BootstrapError::Storage {
        path: None,
        failure: StorageFailure::new(BootstrapStage::Open, "database_path", source),
    })?;
    bootstrap_at(&path)
}

fn bootstrap_at(path: &Path) -> Result<(), BootstrapError> {
    let connection = open_configured_at(path).map_err(|source| BootstrapError::Storage {
        path: Some(path.to_owned()),
        failure: source,
    })?;
    migrations::apply_with_diagnostics(&connection, path).map_err(BootstrapError::Migration)
}

pub(crate) fn open_at(path: &Path) -> Result<Connection, SharedSqliteError> {
    let connection = open_configured_at(path).map_err(|failure| failure.source)?;
    migrations::apply(&connection)?;
    Ok(connection)
}

fn open_configured_at(path: &Path) -> Result<Connection, StorageFailure> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| {
            StorageFailure::from_io(BootstrapStage::Open, "directory_creation", &source)
        })?;
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|source| {
        StorageFailure::from_sqlite(BootstrapStage::Open, "database_open", &source)
    })?;
    configure_connection(&connection)?;
    Ok(connection)
}

fn configure_connection(connection: &Connection) -> Result<(), StorageFailure> {
    connection.busy_timeout(BUSY_TIMEOUT).map_err(|source| {
        StorageFailure::from_sqlite(
            BootstrapStage::ConnectionConfiguration,
            "busy_timeout",
            &source,
        )
    })?;
    enable_wal(&connection)?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|source| {
            StorageFailure::from_sqlite(
                BootstrapStage::ConnectionConfiguration,
                "foreign_keys",
                &source,
            )
        })?;
    Ok(())
}

fn enable_wal(connection: &Connection) -> Result<(), StorageFailure> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        match connection.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(error) if is_busy_or_locked(&error) && Instant::now() < deadline => {
                thread::sleep(BUSY_RETRY_INTERVAL);
            }
            Err(error) => {
                return Err(StorageFailure::from_sqlite(
                    BootstrapStage::ConnectionConfiguration,
                    "journal_mode_wal",
                    &error,
                ));
            }
        }
    }
}

fn is_busy_or_locked(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(failure.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_routing_gateway::{
        accounts::{load_oauth_refresh_material, OAuthTokenBundle},
        security::{encrypt_credential, RootKey},
    };
    use rusqlite::OptionalExtension;
    use std::sync::{Arc, Barrier};

    fn temporary_database(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "onespace-shared-sqlite-{name}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ))
    }

    fn remove_database(path: &Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    fn seed_legacy_oauth_metadata(connection: &Connection, metadata_json: &str) {
        connection
            .execute(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('migration-account', 'oauth', 'Migration Account', 'default')",
                [],
            )
            .expect("insert legacy oauth account");
        connection
            .execute(
                "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version, metadata_json) VALUES ('migration-account', 'oauth_token_bundle', X'01', zeroblob(12), 1, ?1)",
                [metadata_json],
            )
            .expect("insert legacy oauth metadata");
    }

    #[test]
    fn database_path_is_fixed_under_dot_config() {
        let _guard = crate::lock_test_home_env();
        let original = env::var_os("HOME");
        env::set_var("HOME", "/tmp/onespace-fixed-home");
        assert_eq!(
            database_path().expect("resolve database path"),
            PathBuf::from("/tmp/onespace-fixed-home/.config/onespace/data/onespace.sqlite3")
        );
        if let Some(home) = original {
            env::set_var("HOME", home);
        } else {
            env::remove_var("HOME");
        }
    }

    #[test]
    fn bootstrap_sets_pragmas_and_complete_defaults() {
        let path = temporary_database("pragmas");
        let connection = open_at(&path).expect("bootstrap database");
        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read journal mode");
        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("read foreign keys");
        let busy_timeout: i64 = connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .expect("read busy timeout");
        assert_eq!(journal_mode, "wal");
        assert_eq!(foreign_keys, 1);
        assert_eq!(busy_timeout, 5_000);

        let settings: (i64, i64, i64, i64) = connection
            .query_row(
                "SELECT port, global_quota_threshold_percent, log_retention_days, run_enabled FROM ai_gateway_settings WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("read default settings");
        assert_eq!(settings, (17_688, 10, 90, 1));
        let default_groups: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_groups WHERE is_default = 1",
                [],
                |row| row.get(0),
            )
            .expect("count default groups");
        assert_eq!(default_groups, 1);
        let default_key_display_groups: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_key_display_groups WHERE is_default = 1",
                [],
                |row| row.get(0),
            )
            .expect("count default key display groups");
        assert_eq!(default_key_display_groups, 1);
        remove_database(&path);
    }

    #[test]
    fn default_group_cannot_be_deleted_or_unset() {
        let path = temporary_database("default-group-invariant");
        let connection = open_at(&path).expect("bootstrap database");
        connection
            .execute(
                "INSERT INTO ai_gateway_groups (id, name, is_default) VALUES ('secondary', 'Secondary', 0)",
                [],
            )
            .expect("insert non-default group");

        assert!(connection
            .execute("DELETE FROM ai_gateway_groups WHERE id = 'default'", [])
            .is_err());
        assert!(connection
            .execute(
                "UPDATE ai_gateway_groups SET is_default = 0 WHERE id = 'default'",
                [],
            )
            .is_err());

        let default_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_groups WHERE is_default = 1",
                [],
                |row| row.get(0),
            )
            .expect("count default groups");
        assert_eq!(default_count, 1);
        remove_database(&path);
    }

    #[test]
    fn repeated_bootstrap_preserves_data_and_unknown_future_tables() {
        let path = temporary_database("repeat");
        {
            let connection = open_at(&path).expect("first bootstrap");
            connection
                .execute(
                    "CREATE TABLE future_subsystem_data (value TEXT NOT NULL)",
                    [],
                )
                .expect("create unknown table");
            connection
                .execute(
                    "INSERT INTO future_subsystem_data (value) VALUES ('preserved')",
                    [],
                )
                .expect("seed unknown table");
            connection
                .execute(
                    "UPDATE ai_gateway_settings SET port = 18000 WHERE id = 1",
                    [],
                )
                .expect("change setting");
        }
        let connection = open_at(&path).expect("repeat bootstrap");
        let port: i64 = connection
            .query_row(
                "SELECT port FROM ai_gateway_settings WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("read preserved setting");
        let future: String = connection
            .query_row("SELECT value FROM future_subsystem_data", [], |row| {
                row.get(0)
            })
            .expect("read unknown table");
        assert_eq!(port, 18_000);
        assert_eq!(future, "preserved");
        remove_database(&path);
    }

    #[test]
    fn schema_contains_every_planned_table_and_preserves_history_on_deletion() {
        let path = temporary_database("schema-contract");
        let connection = open_at(&path).expect("bootstrap database");
        let expected_tables = [
            "ai_gateway_account_model_mappings",
            "ai_gateway_account_tags",
            "ai_gateway_accounts",
            "ai_gateway_api_key_groups",
            "ai_gateway_api_key_models",
            "ai_gateway_api_keys",
            "ai_gateway_credentials",
            "ai_gateway_daily_aggregates",
            "ai_gateway_groups",
            "ai_gateway_key_display_groups",
            "ai_gateway_key_provider_conversions",
            "ai_gateway_model_prices",
            "ai_gateway_models",
            "ai_gateway_quota_windows",
            "ai_gateway_request_attempts",
            "ai_gateway_request_logs",
            "ai_gateway_settings",
            "ai_gateway_tags",
        ];
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'ai_gateway_%' ORDER BY name")
            .expect("prepare schema query");
        let actual_tables: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .expect("query schema tables")
            .collect::<Result<_, _>>()
            .expect("collect schema tables");
        assert_eq!(actual_tables, expected_tables);
        drop(statement);

        connection
            .execute_batch(
                "INSERT INTO ai_gateway_models (id, display_name) VALUES ('model-1', 'Model 1');
                 INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-1', 'api_key', 'Account 1', 'default');
                 INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version) VALUES ('account-1', 'api_key', X'0102', zeroblob(12), 1);
                 INSERT INTO ai_gateway_account_model_mappings (account_id, public_model_id, upstream_model_id) VALUES ('account-1', 'model-1', 'upstream-1');
                 INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_hash, hash_salt) VALUES ('key-1', 'Key 1', 'osk_test', X'01', X'02');
                 INSERT INTO ai_gateway_request_logs (id, request_id, started_at, local_date, timezone_name, endpoint, public_model_id, api_key_id, api_key_id_snapshot, api_key_name_snapshot, account_id, account_id_snapshot, account_name_snapshot, status) VALUES ('log-1', 'request-1', CURRENT_TIMESTAMP, '2026-08-01', 'UTC', '/v1/responses', 'model-1', 'key-1', 'key-1', 'Key 1', 'account-1', 'account-1', 'Account 1', 'succeeded');
                 INSERT INTO ai_gateway_request_attempts (id, request_log_id, attempt_number, account_id, account_name_snapshot, started_at, status) VALUES ('attempt-1', 'log-1', 1, 'account-1', 'Account 1', CURRENT_TIMESTAMP, 'succeeded');
                 DELETE FROM ai_gateway_accounts WHERE id = 'account-1';
                 DELETE FROM ai_gateway_api_keys WHERE id = 'key-1';",
            )
            .expect("exercise deletion semantics");
        let credential_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM ai_gateway_credentials", [], |row| {
                row.get(0)
            })
            .expect("count cascaded credentials");
        let mapping_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_account_model_mappings",
                [],
                |row| row.get(0),
            )
            .expect("count cascaded mappings");
        let log_snapshot: (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
        ) = connection
            .query_row(
                "SELECT account_id, api_key_id, account_id_snapshot, api_key_id_snapshot, account_name_snapshot, api_key_name_snapshot FROM ai_gateway_request_logs WHERE id = 'log-1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .expect("read preserved logical request");
        let attempt_snapshot: (Option<String>, String) = connection
            .query_row(
                "SELECT account_id, account_name_snapshot FROM ai_gateway_request_attempts WHERE id = 'attempt-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read preserved request attempt");
        assert_eq!(credential_count, 0);
        assert_eq!(mapping_count, 0);
        assert_eq!(
            log_snapshot,
            (
                None,
                None,
                Some("account-1".to_string()),
                Some("key-1".to_string()),
                "Account 1".to_string(),
                "Key 1".to_string()
            )
        );
        assert_eq!(attempt_snapshot, (None, "Account 1".to_string()));
        remove_database(&path);
    }

    #[test]
    fn request_log_upstream_model_index_orders_snapshot_and_time() {
        let path = temporary_database("request-log-upstream-index");
        let connection = open_at(&path).expect("bootstrap database");
        let mut statement = connection
            .prepare("PRAGMA index_xinfo('ai_gateway_request_logs_upstream_model_time')")
            .expect("prepare request log index query");
        let entries: Vec<(Option<String>, i64, i64)> = statement
            .query_map([], |row| Ok((row.get(2)?, row.get(3)?, row.get(5)?)))
            .expect("query request log index")
            .collect::<Result<_, _>>()
            .expect("collect request log index");
        let key_columns: Vec<(Option<String>, i64)> = entries
            .into_iter()
            .filter(|(_, _, is_key)| *is_key == 1)
            .map(|(name, descending, _)| (name, descending))
            .collect();
        assert_eq!(
            key_columns,
            vec![
                (Some("upstream_model_id_snapshot".to_string()), 0),
                (Some("started_at".to_string()), 1),
                (Some("id".to_string()), 1),
            ]
        );
        drop(statement);
        remove_database(&path);
    }

    #[test]
    fn concurrent_bootstrap_records_each_version_once() {
        let path = temporary_database("concurrent");
        const CONCURRENT_CONNECTIONS: usize = 12;
        let barrier = Arc::new(Barrier::new(CONCURRENT_CONNECTIONS + 1));
        let mut threads = Vec::new();
        for _ in 0..CONCURRENT_CONNECTIONS {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                open_at(&path).map(|_| ())
            }));
        }
        barrier.wait();
        for thread in threads {
            thread
                .join()
                .expect("join bootstrap thread")
                .expect("bootstrap concurrently");
        }
        let connection = open_at(&path).expect("inspect database");
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM refinery_schema_history WHERE version = 1",
                [],
                |row| row.get(0),
            )
            .expect("count migration records");
        assert_eq!(count, 1);
        remove_database(&path);
    }

    #[test]
    fn migration_failure_rolls_back_schema_and_version() {
        let path = temporary_database("rollback");
        let connection = Connection::open(&path).expect("open test database");
        configure_connection(&connection).expect("configure test database");
        migrations::install_legacy_fixture(&connection, 2, true);
        connection
            .execute(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('preserved', 'api_key', 'Preserved', 'default')",
                [],
            )
            .expect("seed legacy data");
        assert_eq!(
            migrations::apply_with_failure_after_baseline(&connection),
            Err(SharedSqliteError::MigrationFailed)
        );
        let refinery_table: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'refinery_schema_history'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query rolled back refinery table");
        let v3_column: Option<String> = connection
            .query_row(
                "SELECT name FROM pragma_table_info('ai_gateway_request_logs') WHERE name = 'api_key_id_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query rolled back schema");
        let account_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_accounts WHERE id = 'preserved'",
                [],
                |row| row.get(0),
            )
            .expect("query preserved data");
        assert!(refinery_table.is_none());
        assert!(v3_column.is_none());
        assert_eq!(account_count, 1);
        remove_database(&path);
    }

    #[test]
    fn migration_diagnostic_reports_stage_path_versions_and_redacted_cause() {
        let path = temporary_database("diagnostic");
        let connection = Connection::open(&path).expect("open diagnostic database");
        configure_connection(&connection).expect("configure diagnostic database");
        migrations::install_legacy_fixture(&connection, 4, true);
        connection
            .execute("DELETE FROM app_schema_migrations WHERE version = 2", [])
            .expect("create history gap");

        let diagnostic = migrations::apply_with_diagnostics(&connection, &path)
            .expect_err("reject invalid migration state");
        let rendered = diagnostic.to_string();
        assert_eq!(diagnostic.stage(), migrations::MigrationStage::Check);
        assert_eq!(diagnostic.identified_version(), Some(4));
        assert!(rendered.contains(&format!("path={}", path.display())));
        assert!(rendered.contains("identified_version=4"));
        assert!(rendered.contains("target_version=5"));
        assert!(rendered.contains("cause=shared_sqlite_migration_state_invalid"));
        for secret in ["token", "Bearer", "client_secret", "business-record"] {
            assert!(!rendered.contains(secret));
        }
        remove_database(&path);
    }

    #[test]
    fn incomplete_defaults_cannot_be_compensated_by_prefixed_price() {
        let path = temporary_database("fingerprint-exact-defaults");
        let connection = Connection::open(&path).expect("open fingerprint database");
        configure_connection(&connection).expect("configure fingerprint database");
        migrations::install_legacy_fixture(&connection, 2, false);
        connection
            .execute(
                "DELETE FROM ai_gateway_model_prices WHERE id = ?1",
                ["official-openai-api-pricing-2026-08-01-r1-gpt-5.6-luna"],
            )
            .expect("remove exact default price");
        connection
            .execute(
                "INSERT INTO ai_gateway_model_prices (id, public_model_id, account_id, input_per_million_usd, output_per_million_usd, cache_read_per_million_usd, cache_write_per_million_usd, source, effective_at) VALUES (?1, 'gpt-5.6-sol', NULL, '1', '6', '0.1', '1.25', 'official', '2026-08-01T00:00:00Z')",
                ["official-openai-api-pricing-2026-08-01-r1-extra"],
            )
            .expect("insert compensating prefixed price");

        assert_eq!(
            migrations::apply(&connection),
            Err(SharedSqliteError::MigrationStateInvalid)
        );
        let refinery_table: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_schema WHERE type = 'table' AND name = 'refinery_schema_history'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query rejected fingerprint side effects");
        assert!(refinery_table.is_none());
        remove_database(&path);
    }

    #[test]
    fn renamed_default_group_is_rejected_without_history_side_effects() {
        let path = temporary_database("fingerprint-default-group-id");
        let connection = Connection::open(&path).expect("open default group fingerprint database");
        configure_connection(&connection).expect("configure default group fingerprint database");
        migrations::install_legacy_fixture(&connection, 2, false);
        connection
            .execute(
                "UPDATE ai_gateway_groups SET id = 'renamed-default' WHERE id = 'default'",
                [],
            )
            .expect("rename default group in fixture");

        assert_eq!(
            migrations::apply(&connection),
            Err(SharedSqliteError::MigrationStateInvalid)
        );
        let refinery_table: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_schema WHERE type = 'table' AND name = 'refinery_schema_history'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query rejected default group fingerprint side effects");
        assert!(refinery_table.is_none());
        remove_database(&path);
    }

    #[test]
    fn no_history_requires_canonical_default_group() {
        let path = temporary_database("fingerprint-missing-default-group");
        let connection = Connection::open(&path).expect("open missing default group fixture");
        configure_connection(&connection).expect("configure missing default group fixture");
        migrations::install_legacy_fixture(&connection, 2, false);
        connection
            .execute_batch(
                "DROP TRIGGER ai_gateway_groups_prevent_default_unset;
                 UPDATE ai_gateway_groups SET is_default = 0 WHERE id = 'default';
                 CREATE TRIGGER ai_gateway_groups_prevent_default_unset
                 BEFORE UPDATE OF is_default ON ai_gateway_groups
                 WHEN OLD.is_default = 1 AND NEW.is_default = 0
                 BEGIN
                     SELECT RAISE(ABORT, 'ai_gateway_groups_default_required');
                 END;",
            )
            .expect("remove canonical default group");

        assert_eq!(
            migrations::apply(&connection),
            Err(SharedSqliteError::MigrationStateInvalid)
        );
        let refinery_table: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_schema WHERE type = 'table' AND name = 'refinery_schema_history'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query missing default group side effects");
        assert!(refinery_table.is_none());
        remove_database(&path);
    }

    #[test]
    fn no_history_requires_unique_default_group() {
        let path = temporary_database("fingerprint-multiple-default-groups");
        let connection = Connection::open(&path).expect("open multiple default group fixture");
        configure_connection(&connection).expect("configure multiple default group fixture");
        migrations::install_legacy_fixture(&connection, 2, false);
        connection
            .execute("DROP INDEX ai_gateway_groups_one_default", [])
            .expect("remove default uniqueness index");
        connection
            .execute(
                "INSERT INTO ai_gateway_groups (id, name, is_default) VALUES ('extra-default', 'Extra', 1)",
                [],
            )
            .expect("insert second default group");
        connection
            .execute(
                "CREATE INDEX ai_gateway_groups_one_default ON ai_gateway_groups (is_default) WHERE is_default = 1",
                [],
            )
            .expect("restore default index fixture");
        connection
            .execute_batch(
                "PRAGMA writable_schema = ON;
                 UPDATE sqlite_schema
                 SET sql = 'CREATE UNIQUE INDEX ai_gateway_groups_one_default ON ai_gateway_groups (is_default) WHERE is_default = 1'
                 WHERE name = 'ai_gateway_groups_one_default';
                 PRAGMA writable_schema = OFF;",
            )
            .expect("restore default index fingerprint");

        assert_eq!(
            migrations::apply(&connection),
            Err(SharedSqliteError::MigrationStateInvalid)
        );
        let refinery_table: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_schema WHERE type = 'table' AND name = 'refinery_schema_history'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query multiple default group side effects");
        assert!(refinery_table.is_none());
        remove_database(&path);
    }

    #[test]
    fn managed_database_reopens_after_public_oauth_metadata_is_written() {
        let path = temporary_database("managed-public-oauth-metadata");
        {
            let connection = open_at(&path).expect("bootstrap managed database");
            seed_legacy_oauth_metadata(
                &connection,
                r#"{"issuer":"public-issuer","authorization_endpoint":"https://issuer.example/authorize"}"#,
            );
        }
        let connection = open_at(&path).expect("reopen managed database");
        let stored: String = connection
            .query_row(
                "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'migration-account'",
                [],
                |row| row.get(0),
            )
            .expect("read managed public oauth metadata");
        assert_eq!(
            stored,
            r#"{"issuer":"public-issuer","authorization_endpoint":"https://issuer.example/authorize"}"#
        );
        drop(connection);
        let connection = open_at(&path).expect("reopen managed database again");
        drop(connection);
        remove_database(&path);
    }

    #[test]
    fn legacy_v1_v2_oauth_metadata_cleanup_preserves_public_fields_and_is_idempotent() {
        let metadata = r#"{"issuer":"public-issuer","authorization_endpoint":"https://issuer.example/authorize","token_endpoint":"https://issuer.example/token","jwks_uri":"https://issuer.example/jwks","nested":{"display_name":"public","client_secret":"legacy-secret","deep":{"audience":"public-audience","refresh_token":"legacy-refresh"}}}"#;
        for version in [1, 2] {
            let path = temporary_database(&format!("legacy-public-metadata-v{version}"));
            let connection = Connection::open(&path).expect("open legacy metadata fixture");
            configure_connection(&connection).expect("configure legacy metadata fixture");
            migrations::install_legacy_fixture(&connection, version, true);
            seed_legacy_oauth_metadata(&connection, metadata);

            migrations::apply(&connection).expect("upgrade legacy metadata fixture");
            let stored: String = connection
                .query_row(
                    "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'migration-account'",
                    [],
                    |row| row.get(0),
                )
                .expect("read cleaned legacy metadata");
            let parsed: serde_json::Value =
                serde_json::from_str(&stored).expect("parse cleaned legacy metadata");
            let object = parsed.as_object().expect("cleaned metadata object");
            assert_eq!(
                object.get("issuer").and_then(|value| value.as_str()),
                Some("public-issuer")
            );
            assert_eq!(
                object
                    .get("authorization_endpoint")
                    .and_then(|value| value.as_str()),
                Some("https://issuer.example/authorize")
            );
            assert_eq!(
                object.get("jwks_uri").and_then(|value| value.as_str()),
                Some("https://issuer.example/jwks")
            );
            let nested = object
                .get("nested")
                .and_then(|value| value.as_object())
                .expect("preserved nested public metadata");
            assert_eq!(
                nested.get("display_name").and_then(|value| value.as_str()),
                Some("public")
            );
            assert!(!nested.contains_key("client_secret"));
            let deep = nested
                .get("deep")
                .and_then(|value| value.as_object())
                .expect("preserved deeply nested public metadata");
            assert_eq!(
                deep.get("audience").and_then(|value| value.as_str()),
                Some("public-audience")
            );
            assert!(!deep.contains_key("refresh_token"));

            drop(connection);
            let connection = open_at(&path).expect("reopen cleaned legacy metadata fixture");
            let reopened: String = connection
                .query_row(
                    "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'migration-account'",
                    [],
                    |row| row.get(0),
                )
                .expect("read reopened legacy metadata");
            assert_eq!(reopened, stored, "legacy version {version}");
            drop(connection);
            remove_database(&path);
        }
    }

    #[test]
    fn legacy_v1_v2_nested_array_metadata_cleanup_visits_every_element() {
        let metadata = r#"{"issuer":"public-issuer","token_endpoint":"https://issuer.example/token","providers":[{"name":"first","token":"SAFE_FIXTURE_TOKEN_FIRST","nested":[{"audience":"first-public","client_secret":"SAFE_FIXTURE_CLIENT_FIRST"}]},{"name":"second","nested":[{"audience":"second-public","refresh_token":"SAFE_FIXTURE_REFRESH_SECOND"},{"audience":"second-deep-public","deep":[{"public":"second-deep-public","client_secret":"SAFE_FIXTURE_CLIENT_SECOND"}]}]},{"name":"third","authorization":"SAFE_FIXTURE_AUTH_THIRD","nested":[{"audience":"third-public","children":[{"token":"SAFE_FIXTURE_TOKEN_THIRD"}]},{"audience":"third-tail-public","refresh_token":"SAFE_FIXTURE_REFRESH_THIRD"}]}],"after_providers":{"label":"after","credential":"SAFE_FIXTURE_CREDENTIAL_AFTER","public":"after-public"},"tail":[{"label":"tail-first","api_key":"SAFE_FIXTURE_API_KEY_TAIL"},{"label":"tail-second","children":[{"password":"SAFE_FIXTURE_PASSWORD_TAIL","public":"tail-public"}]}],"final":"public-final"}"#;

        for version in [1, 2] {
            let path = temporary_database(&format!("nested-array-metadata-v{version}"));
            {
                let connection = Connection::open(&path).expect("open nested array fixture");
                configure_connection(&connection).expect("configure nested array fixture");
                migrations::install_legacy_fixture(&connection, version, true);
                seed_legacy_oauth_metadata(&connection, metadata);
            }

            bootstrap_at(&path).expect("upgrade nested array fixture");

            let assert_metadata = |connection: &Connection| {
                let stored: String = connection
                    .query_row(
                        "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'migration-account'",
                        [],
                        |row| row.get(0),
                    )
                    .expect("read cleaned nested array metadata");
                let parsed: serde_json::Value =
                    serde_json::from_str(&stored).expect("parse cleaned nested array metadata");
                assert!(
                    crate::ai_routing_gateway::accounts::is_safe_public_metadata_object(&parsed),
                    "legacy version {version} retained sensitive metadata: {stored}"
                );
                let object = parsed.as_object().expect("cleaned metadata object");
                assert_eq!(
                    object.get("issuer").and_then(|value| value.as_str()),
                    Some("public-issuer")
                );
                assert_eq!(
                    object.get("final").and_then(|value| value.as_str()),
                    Some("public-final")
                );

                let providers = object
                    .get("providers")
                    .and_then(|value| value.as_array())
                    .expect("preserved provider metadata array");
                assert_eq!(providers.len(), 3);
                assert_eq!(providers[0]["name"], "first");
                assert_eq!(providers[0]["nested"][0]["audience"], "first-public");
                assert_eq!(providers[1]["name"], "second");
                assert_eq!(providers[1]["nested"][0]["audience"], "second-public");
                assert_eq!(
                    providers[1]["nested"][1]["deep"][0]["public"],
                    "second-deep-public"
                );
                assert_eq!(providers[2]["name"], "third");
                assert_eq!(providers[2]["nested"][0]["audience"], "third-public");
                assert_eq!(providers[2]["nested"][1]["audience"], "third-tail-public");

                assert_eq!(object["after_providers"]["public"], "after-public");
                assert_eq!(object["tail"][0]["label"], "tail-first");
                assert_eq!(object["tail"][1]["children"][0]["public"], "tail-public");
            };

            let connection = open_at(&path).expect("open migrated nested array fixture");
            assert_metadata(&connection);
            drop(connection);

            bootstrap_at(&path).expect("repeat bootstrap nested array fixture");
            let connection = open_at(&path).expect("repeat open nested array fixture");
            assert_metadata(&connection);

            let history: (i64, String) = connection
                .query_row(
                    "SELECT COUNT(*), group_concat(version, ',') FROM refinery_schema_history ORDER BY version",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("read nested array migration history");
            assert_eq!(history, (5, "1,2,3,4,5".to_owned()));
            drop(connection);
            remove_database(&path);
        }
    }

    #[test]
    fn legacy_v1_metadata_bridge_runs_after_v2_and_preserves_public_fields() {
        let path = temporary_database("legacy-v1-metadata-order");
        let connection = Connection::open(&path).expect("open legacy order fixture");
        configure_connection(&connection).expect("configure legacy order fixture");
        migrations::install_legacy_fixture(&connection, 1, true);
        seed_legacy_oauth_metadata(
            &connection,
            r#"{"issuer":"public-issuer","nested":{"audience":"public-audience","client_secret":"legacy-secret"}}"#,
        );
        connection
            .execute_batch(
                "CREATE TRIGGER legacy_metadata_requires_v2
                 BEFORE UPDATE OF metadata_json ON ai_gateway_credentials
                 WHEN EXISTS (
                     SELECT 1
                     FROM sqlite_schema
                     WHERE type = 'table'
                       AND name = 'ai_gateway_request_attempts'
                       AND sql LIKE '%BETWEEN 1 AND 3%'
                 )
                 BEGIN
                     SELECT RAISE(ABORT, 'metadata_cleanup_before_v2');
                 END;",
            )
            .expect("install migration order trigger");

        migrations::apply(&connection).expect("upgrade legacy v1 order fixture");
        let attempt_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'ai_gateway_request_attempts'",
                [],
                |row| row.get(0),
            )
            .expect("read v2 attempt schema");
        assert!(attempt_sql.contains("BETWEEN 1 AND 6"));
        let metadata: String = connection
            .query_row(
                "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'migration-account'",
                [],
                |row| row.get(0),
            )
            .expect("read bridged metadata");
        let parsed: serde_json::Value =
            serde_json::from_str(&metadata).expect("parse bridged metadata");
        assert_eq!(
            parsed.get("issuer").and_then(serde_json::Value::as_str),
            Some("public-issuer")
        );
        assert_eq!(
            parsed
                .get("nested")
                .and_then(serde_json::Value::as_object)
                .and_then(|nested| nested.get("audience"))
                .and_then(serde_json::Value::as_str),
            Some("public-audience")
        );
        assert!(!metadata.contains("client_secret"));
        let versions: String = connection
            .query_row(
                "SELECT group_concat(version, ',') FROM refinery_schema_history ORDER BY version",
                [],
                |row| row.get(0),
            )
            .expect("read ordered migration history");
        assert_eq!(versions, "1,2,3,4,5");
        remove_database(&path);
    }

    #[test]
    fn historical_refinery_v3_checksum_bootstraps_but_any_checksum_mismatch_is_rejected() {
        for mismatched_version in [None, Some(3), Some(4)] {
            let path = temporary_database(&format!(
                "historical-refinery-checksum-{}",
                mismatched_version
                    .map_or_else(|| "valid".to_owned(), |version| version.to_string())
            ));
            let connection = Connection::open(&path).expect("open historical checksum fixture");
            configure_connection(&connection).expect("configure historical checksum fixture");
            migrations::install_legacy_fixture(&connection, 4, false);
            migrations::install_refinery_history_fixture(&connection, 4, mismatched_version);
            drop(connection);

            let result = bootstrap_at(&path);
            if mismatched_version.is_none() {
                result.expect("bootstrap historical refinery checksum fixture");
            } else {
                assert!(matches!(result, Err(BootstrapError::Migration(_))));
            }
            remove_database(&path);
        }
    }

    #[test]
    fn managed_metadata_safety_contract_rejects_unsafe_json_without_side_effects() {
        for (name, metadata) in [
            (
                "nested-sensitive",
                r#"{"issuer":"public-issuer","nested":{"token":"secret"}}"#,
            ),
            ("invalid-json", "not-json"),
            ("non-object", r#"["public"]"#),
        ] {
            let path = temporary_database(&format!("managed-metadata-{name}"));
            let connection = open_at(&path).expect("bootstrap managed metadata fixture");
            seed_legacy_oauth_metadata(&connection, metadata);
            let history_before: i64 = connection
                .query_row("SELECT COUNT(*) FROM refinery_schema_history", [], |row| {
                    row.get(0)
                })
                .expect("count managed migration history");
            let health_before: (String, Option<String>) = connection
                .query_row(
                    "SELECT health_status, health_reason_code FROM ai_gateway_accounts WHERE id = 'migration-account'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("read managed account health");

            assert_eq!(
                migrations::apply(&connection),
                Err(SharedSqliteError::MigrationStateInvalid),
                "metadata case {name}"
            );

            let history_after: i64 = connection
                .query_row("SELECT COUNT(*) FROM refinery_schema_history", [], |row| {
                    row.get(0)
                })
                .expect("count managed history after rejection");
            let metadata_after: String = connection
                .query_row(
                    "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'migration-account'",
                    [],
                    |row| row.get(0),
                )
                .expect("read metadata after rejection");
            let health_after: (String, Option<String>) = connection
                .query_row(
                    "SELECT health_status, health_reason_code FROM ai_gateway_accounts WHERE id = 'migration-account'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("read health after rejection");
            assert_eq!(history_after, history_before, "metadata case {name}");
            assert_eq!(metadata_after, metadata, "metadata case {name}");
            assert_eq!(health_after, health_before, "metadata case {name}");
            remove_database(&path);
        }
    }

    #[test]
    fn trusted_v3_v4_history_preserves_public_oauth_metadata() {
        let metadata = r#"{"issuer":"public-issuer","authorization_endpoint":"https://issuer.example/authorize","token_endpoint":"https://issuer.example/token"}"#;
        for version in [3, 4] {
            let path = temporary_database(&format!("public-oauth-v{version}"));
            let connection = Connection::open(&path).expect("open public oauth fixture");
            configure_connection(&connection).expect("configure public oauth fixture");
            migrations::install_legacy_fixture(&connection, version, true);
            seed_legacy_oauth_metadata(&connection, metadata);

            migrations::apply(&connection).expect("upgrade trusted public oauth fixture");
            let stored: String = connection
                .query_row(
                    "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'migration-account'",
                    [],
                    |row| row.get(0),
                )
                .expect("read preserved public oauth metadata");
            assert_eq!(stored, metadata, "legacy version {version}");
            remove_database(&path);
        }
    }

    #[test]
    fn migration_diagnostics_preserve_distinct_stage_causes_without_secrets() {
        let check_path = temporary_database("diagnostic-check-busy");
        let holder = Connection::open(&check_path).expect("open lock holder");
        holder
            .execute_batch("BEGIN IMMEDIATE")
            .expect("hold migration lock");
        let contender = Connection::open(&check_path).expect("open lock contender");
        let check = migrations::apply_with_diagnostics(&contender, &check_path)
            .expect_err("reject locked migration database");
        let check_rendered = check.to_string();
        assert_eq!(check.stage(), migrations::MigrationStage::Check);
        assert!(check_rendered.contains("sqlite_code=DatabaseBusy"));
        assert!(check_rendered.contains("sqlite_extended_code=5"));
        assert!(!check_rendered.contains("check-secret"));
        holder
            .execute_batch("ROLLBACK")
            .expect("release migration lock");
        drop(contender);
        drop(holder);
        remove_database(&check_path);

        let baseline_path = temporary_database("diagnostic-baseline");
        let baseline_connection = Connection::open(&baseline_path).expect("open baseline database");
        configure_connection(&baseline_connection).expect("configure baseline database");
        migrations::install_legacy_fixture(&baseline_connection, 2, true);
        baseline_connection
            .execute_batch(
                "CREATE TABLE refinery_schema_history(
                    version INT4 PRIMARY KEY,
                    name VARCHAR(255),
                    applied_on VARCHAR(255),
                    checksum VARCHAR(255)
                );
                 CREATE TRIGGER baseline_failure BEFORE INSERT ON refinery_schema_history
                 BEGIN
                     SELECT RAISE(ABORT, 'baseline-secret');
                 END;",
            )
            .expect("install baseline failure fixture");
        let baseline = migrations::apply_with_diagnostics(&baseline_connection, &baseline_path)
            .expect_err("reject failed baseline");
        let baseline_rendered = baseline.to_string();
        assert_eq!(baseline.stage(), migrations::MigrationStage::Baseline);
        assert!(baseline_rendered.contains("sqlite_code=ConstraintViolation"));
        assert!(!baseline_rendered.contains("baseline-secret"));
        remove_database(&baseline_path);

        let execute_path = temporary_database("diagnostic-execute");
        let execute_connection = Connection::open(&execute_path).expect("open execute database");
        configure_connection(&execute_connection).expect("configure execute database");
        migrations::install_legacy_fixture(&execute_connection, 2, true);
        seed_legacy_oauth_metadata(
            &execute_connection,
            r#"{"token_endpoint":"https://issuer.example/token","client_secret":"execute-secret"}"#,
        );
        execute_connection
            .execute_batch(
                "CREATE TRIGGER execute_failure BEFORE UPDATE OF health_status ON ai_gateway_accounts
                 WHEN NEW.health_reason_code = 'oauth_reauthorization_required'
                 BEGIN
                     SELECT RAISE(ABORT, 'execute-secret');
                 END;",
            )
            .expect("install execute failure fixture");
        let execute = migrations::apply_with_diagnostics(&execute_connection, &execute_path)
            .expect_err("reject failed migration execution");
        let execute_rendered = execute.to_string();
        assert_eq!(execute.stage(), migrations::MigrationStage::Execute);
        assert!(execute_rendered.contains("sqlite_code=ConstraintViolation"));
        assert!(!execute_rendered.contains("execute-secret"));
        remove_database(&execute_path);

        let commit_path = temporary_database("diagnostic-commit");
        let commit_connection = Connection::open(&commit_path).expect("open commit database");
        configure_connection(&commit_connection).expect("configure commit database");
        migrations::install_legacy_fixture(&commit_connection, 2, true);
        seed_legacy_oauth_metadata(
            &commit_connection,
            r#"{"token_endpoint":"https://issuer.example/token","client_secret":"commit-secret"}"#,
        );
        commit_connection
            .execute_batch(
                "CREATE TABLE commit_deferred_parent (id TEXT PRIMARY KEY);
                 CREATE TABLE commit_deferred_child (
                     id TEXT PRIMARY KEY,
                     parent_id TEXT REFERENCES commit_deferred_parent(id) DEFERRABLE INITIALLY DEFERRED
                 );
                 CREATE TRIGGER commit_failure AFTER UPDATE OF health_status ON ai_gateway_accounts
                 WHEN NEW.health_reason_code = 'oauth_reauthorization_required'
                 BEGIN
                     INSERT INTO commit_deferred_child(id, parent_id) VALUES ('commit-child', 'missing-parent');
                 END;",
            )
            .expect("install commit failure fixture");
        let commit = migrations::apply_with_diagnostics(&commit_connection, &commit_path)
            .expect_err("reject failed migration commit");
        let commit_rendered = commit.to_string();
        assert_eq!(commit.stage(), migrations::MigrationStage::Commit);
        assert_eq!(commit.identified_version(), Some(2));
        assert!(commit_rendered.contains("identified_version=2"));
        assert!(commit_rendered.contains("target_version=5"));
        assert!(!commit_rendered.contains("identified_version=5"));
        assert!(commit_rendered.contains("foreign_key_check"));
        assert!(!commit_rendered.contains("commit-secret"));
        remove_database(&commit_path);
    }

    #[test]
    fn bootstrap_open_diagnostic_keeps_path_context_and_redacted_sqlite_cause() {
        let path = temporary_database("bootstrap-open-diagnostic");
        fs::create_dir_all(&path).expect("create directory at database path");

        let diagnostic = bootstrap_at(&path).expect_err("reject directory as database file");
        let rendered = diagnostic.to_string();
        assert!(rendered.contains("stage=open"));
        assert!(rendered.contains(&format!("path={}", path.display())));
        assert!(rendered.contains("context=open"));
        assert!(rendered.contains("sqlite_code=CannotOpen"));
        assert!(rendered.contains("sqlite_extended_code=14"));
        for secret in ["SELECT", "client_secret", "business-value"] {
            assert!(!rendered.contains(secret));
        }

        fs::remove_dir_all(&path).expect("remove database path fixture");
    }

    #[test]
    fn bootstrap_directory_creation_diagnostic_keeps_safe_io_classification() {
        let blocker = temporary_database("bootstrap-directory-blocker");
        fs::write(&blocker, "directory-creation-secret").expect("create directory blocker");
        let path = blocker.join("nested").join("onespace.sqlite3");

        let diagnostic = bootstrap_at(&path).expect_err("reject blocked database directory");
        let rendered = diagnostic.to_string();
        assert!(rendered.contains("stage=open"));
        assert!(rendered.contains(&format!("path={}", path.display())));
        assert!(rendered.contains("context=open"));
        assert!(rendered.contains("operation=directory_creation"));
        assert!(
            [
                "io_kind=PermissionDenied",
                "io_kind=NotADirectory",
                "io_kind=AlreadyExists"
            ]
            .into_iter()
            .any(|kind| rendered.contains(kind)),
            "missing safe io classification: {rendered}"
        );
        assert!(!rendered.contains("directory-creation-secret"));
        assert!(!rendered.contains("Not a directory"));

        fs::remove_file(&blocker).expect("remove directory blocker");
    }

    #[test]
    fn failed_real_migration_rolls_back_ddl_data_and_refinery_history() {
        let path = temporary_database("real-migration-rollback");
        let connection = Connection::open(&path).expect("open rollback database");
        configure_connection(&connection).expect("configure rollback database");
        migrations::install_legacy_fixture(&connection, 2, true);
        seed_legacy_oauth_metadata(
            &connection,
            r#"{"token_endpoint":"https://issuer.example/token","client_secret":"rollback-secret"}"#,
        );
        connection
            .execute(
                "INSERT INTO ai_gateway_request_logs (id, request_id, started_at, local_date, timezone_name, endpoint, public_model_id, account_id, status) VALUES ('rollback-log', 'rollback-request', CURRENT_TIMESTAMP, '2026-08-01', 'UTC', '/v1/responses', 'gpt-5.6-sol', 'migration-account', 'failed')",
                [],
            )
            .expect("seed rollback request log");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_after_real_updates BEFORE UPDATE OF health_status ON ai_gateway_accounts
                 WHEN NEW.health_reason_code = 'oauth_reauthorization_required'
                 BEGIN
                     SELECT RAISE(ABORT, 'rollback-secret');
                 END;",
            )
            .expect("install real migration failure fixture");

        let diagnostic = migrations::apply_with_diagnostics(&connection, &path)
            .expect_err("reject failed real migration");
        assert_eq!(diagnostic.stage(), migrations::MigrationStage::Execute);

        let refinery_table: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_schema WHERE type = 'table' AND name = 'refinery_schema_history'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query rolled back refinery history");
        let snapshot_column: Option<String> = connection
            .query_row(
                "SELECT name FROM pragma_table_info('ai_gateway_request_logs') WHERE name = 'account_id_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query rolled back migration schema");
        let account_state: (String, Option<String>, Option<String>) = connection
            .query_row(
                "SELECT health_status, health_reason_code, (SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'migration-account') FROM ai_gateway_accounts WHERE id = 'migration-account'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("query rolled back business data");
        let request_account: Option<String> = connection
            .query_row(
                "SELECT account_id FROM ai_gateway_request_logs WHERE id = 'rollback-log'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query preserved business row");
        assert!(refinery_table.is_none());
        assert!(snapshot_column.is_none());
        assert_eq!(
            account_state,
            (
                "unknown".to_owned(),
                None,
                Some(r#"{"token_endpoint":"https://issuer.example/token","client_secret":"rollback-secret"}"#.to_owned())
            )
        );
        assert_eq!(request_account.as_deref(), Some("migration-account"));
        remove_database(&path);
    }

    #[test]
    fn no_history_complete_v2_schema_is_baselined_and_upgraded() {
        let path = temporary_database("fingerprint-upgrade");
        let connection = Connection::open(&path).expect("open test database");
        configure_connection(&connection).expect("configure test database");
        migrations::install_legacy_fixture(&connection, 2, false);
        migrations::apply(&connection).expect("bridge fingerprinted fixture");
        let versions: String = connection
            .query_row(
                "SELECT group_concat(version, ',') FROM refinery_schema_history ORDER BY version",
                [],
                |row| row.get(0),
            )
            .expect("read refinery history");
        assert_eq!(versions, "1,2,3,4,5");
        remove_database(&path);
    }

    #[test]
    fn every_real_legacy_version_runs_only_missing_migrations() {
        for version in 1..=5 {
            for with_history in [false, true] {
                let path = temporary_database(&format!("legacy-v{version}-{with_history}"));
                let connection = Connection::open(&path).expect("open legacy fixture");
                configure_connection(&connection).expect("configure legacy fixture");
                migrations::install_legacy_fixture(&connection, version, with_history);
                connection
                    .execute(
                        "UPDATE ai_gateway_settings SET port = ?1 WHERE id = 1",
                        [18_000 + version],
                    )
                    .expect("seed legacy setting");

                migrations::apply(&connection).expect("bridge legacy fixture");
                migrations::apply(&connection).expect("repeat bridged migration");

                let port: u32 = connection
                    .query_row(
                        "SELECT port FROM ai_gateway_settings WHERE id = 1",
                        [],
                        |row| row.get(0),
                    )
                    .expect("read preserved setting");
                let versions: String = connection
                    .query_row(
                        "SELECT group_concat(version, ',') FROM refinery_schema_history ORDER BY version",
                        [],
                        |row| row.get(0),
                    )
                    .expect("read refinery history");
                assert_eq!(port, 18_000 + version);
                assert_eq!(versions, "1,2,3,4,5");
                remove_database(&path);
            }
        }
    }

    #[test]
    fn invalid_legacy_states_are_rejected_without_refinery_side_effects() {
        enum Corruption {
            MissingColumn,
            WrongIndex,
            HistoryGap,
            FutureVersion,
            WrongSubsystem,
        }

        for (name, corruption) in [
            ("missing-column", Corruption::MissingColumn),
            ("wrong-index", Corruption::WrongIndex),
            ("history-gap", Corruption::HistoryGap),
            ("future-version", Corruption::FutureVersion),
            ("wrong-subsystem", Corruption::WrongSubsystem),
        ] {
            let path = temporary_database(name);
            let connection = Connection::open(&path).expect("open invalid fixture");
            configure_connection(&connection).expect("configure invalid fixture");
            migrations::install_legacy_fixture(&connection, 4, true);
            match corruption {
                Corruption::MissingColumn => {
                    connection
                        .execute_batch(
                            "ALTER TABLE ai_gateway_settings RENAME TO ai_gateway_settings_full;
                             CREATE TABLE ai_gateway_settings (
                                 id INTEGER PRIMARY KEY CHECK (id = 1),
                                 port INTEGER NOT NULL DEFAULT 17688
                             );
                             INSERT INTO ai_gateway_settings(id, port)
                                 SELECT id, port FROM ai_gateway_settings_full;
                             DROP TABLE ai_gateway_settings_full;",
                        )
                        .expect("remove required columns");
                }
                Corruption::WrongIndex => {
                    connection
                        .execute_batch(
                            "DROP INDEX ai_gateway_request_logs_upstream_model_time;
                             CREATE INDEX ai_gateway_request_logs_upstream_model_time
                                 ON ai_gateway_request_logs(started_at, id);",
                        )
                        .expect("replace required index");
                }
                Corruption::HistoryGap => {
                    connection
                        .execute(
                            "DELETE FROM app_schema_migrations WHERE subsystem = ?1 AND version = 2",
                            [migrations::AI_ROUTING_GATEWAY_SUBSYSTEM],
                        )
                        .expect("create legacy history gap");
                }
                Corruption::FutureVersion => {
                    connection
                        .execute(
                            "INSERT INTO app_schema_migrations(subsystem, version) VALUES (?1, 5)",
                            [migrations::AI_ROUTING_GATEWAY_SUBSYSTEM],
                        )
                        .expect("create future legacy version");
                }
                Corruption::WrongSubsystem => {
                    connection
                        .execute(
                            "UPDATE app_schema_migrations SET subsystem = 'other_subsystem'",
                            [],
                        )
                        .expect("confuse legacy subsystem");
                }
            }
            let legacy_rows_before: i64 = connection
                .query_row("SELECT COUNT(*) FROM app_schema_migrations", [], |row| {
                    row.get(0)
                })
                .expect("count legacy history before rejection");

            assert_eq!(
                migrations::apply(&connection),
                Err(SharedSqliteError::MigrationStateInvalid),
                "corruption case {name}"
            );

            let refinery_table: Option<String> = connection
                .query_row(
                    "SELECT name FROM sqlite_schema WHERE type = 'table' AND name = 'refinery_schema_history'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .expect("query refinery side effects");
            let legacy_rows_after: i64 = connection
                .query_row("SELECT COUNT(*) FROM app_schema_migrations", [], |row| {
                    row.get(0)
                })
                .expect("count legacy history after rejection");
            assert!(refinery_table.is_none(), "corruption case {name}");
            assert_eq!(
                legacy_rows_after, legacy_rows_before,
                "corruption case {name}"
            );
            remove_database(&path);
        }
    }

    #[test]
    fn contradictory_refinery_history_is_rejected_without_schema_changes() {
        let path = temporary_database("refinery-history-gap");
        let connection = open_at(&path).expect("bootstrap refinery fixture");
        connection
            .execute("DELETE FROM refinery_schema_history WHERE version = 2", [])
            .expect("create refinery history gap");
        let schema_before: String = connection
            .query_row(
                "SELECT group_concat(name, ',') FROM (
                    SELECT name FROM sqlite_schema WHERE name GLOB 'ai_gateway_*' ORDER BY name
                )",
                [],
                |row| row.get(0),
            )
            .expect("snapshot schema names");

        assert_eq!(
            migrations::apply(&connection),
            Err(SharedSqliteError::MigrationStateInvalid)
        );

        let schema_after: String = connection
            .query_row(
                "SELECT group_concat(name, ',') FROM (
                    SELECT name FROM sqlite_schema WHERE name GLOB 'ai_gateway_*' ORDER BY name
                )",
                [],
                |row| row.get(0),
            )
            .expect("read schema names after rejection");
        let history_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM refinery_schema_history", [], |row| {
                row.get(0)
            })
            .expect("count contradictory refinery history");
        assert_eq!(schema_after, schema_before);
        assert_eq!(history_count, 4);
        remove_database(&path);
    }

    #[test]
    fn refinery_history_failures_keep_unique_schema_version_in_diagnostics() {
        for (name, mutation) in [
            (
                "gap",
                (|connection: &Connection| {
                    connection
                        .execute("DELETE FROM refinery_schema_history WHERE version = 2", [])
                        .expect("create refinery history gap");
                }) as fn(&Connection),
            ),
            (
                "checksum",
                (|connection: &Connection| {
                    connection
                        .execute(
                            "UPDATE refinery_schema_history SET checksum = 'mismatch' WHERE version = 4",
                            [],
                        )
                        .expect("create refinery checksum mismatch");
                }) as fn(&Connection),
            ),
        ] {
            let path = temporary_database(&format!("refinery-diagnostic-{name}"));
            let connection = open_at(&path).expect("bootstrap refinery diagnostic fixture");
            mutation(&connection);

            let diagnostic = migrations::apply_with_diagnostics(&connection, &path)
                .expect_err("reject invalid refinery history");
            assert_eq!(diagnostic.stage(), migrations::MigrationStage::Check);
            assert_eq!(diagnostic.identified_version(), Some(5), "refinery {name}");
            let rendered = diagnostic.to_string();
            assert!(rendered.contains("identified_version=5"));
            assert!(rendered.contains("cause=shared_sqlite_migration_state_invalid"));
            assert!(!rendered.contains("'mismatch'"));
            remove_database(&path);
        }
    }

    #[test]
    fn attempt_limit_upgrade_preserves_v1_rows_and_allows_oauth_refresh_attempts() {
        let path = temporary_database("attempt-limit-upgrade");
        let connection = Connection::open(&path).expect("open test database");
        configure_connection(&connection).expect("configure test database");
        migrations::install_legacy_fixture(&connection, 1, true);
        connection
            .execute_batch(
                "INSERT INTO ai_gateway_request_logs (id, request_id, started_at, local_date, timezone_name, endpoint, public_model_id, status) VALUES ('log-upgrade', 'req-upgrade', CURRENT_TIMESTAMP, '2026-08-01', 'UTC', 'responses', 'gpt-5.6-sol', 'failed');
                 INSERT INTO ai_gateway_request_attempts (id, request_log_id, attempt_number, account_name_snapshot, started_at, status) VALUES ('attempt-existing', 'log-upgrade', 1, 'Account', CURRENT_TIMESTAMP, 'failed');",
            )
            .expect("seed v1 attempt");

        migrations::apply(&connection).expect("upgrade gateway schema");
        let preserved: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_request_attempts WHERE id = 'attempt-existing'",
                [],
                |row| row.get(0),
            )
            .expect("read preserved attempt");
        assert_eq!(preserved, 1);
        connection
            .execute(
                "INSERT INTO ai_gateway_request_attempts (id, request_log_id, attempt_number, account_name_snapshot, started_at, status) VALUES ('attempt-refresh', 'log-upgrade', 4, 'Account', CURRENT_TIMESTAMP, 'succeeded')",
                [],
            )
            .expect("insert post-upgrade refresh attempt");
        let migration_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM refinery_schema_history WHERE version = 2",
                [],
                |row| row.get(0),
            )
            .expect("read migration record");
        assert_eq!(migration_count, 1);
        remove_database(&path);
    }

    #[test]
    fn task_four_migration_cleans_metadata_and_backfills_request_id_snapshots() {
        let path = temporary_database("task-four-migration");
        let connection = Connection::open(&path).expect("open migration database");
        configure_connection(&connection).expect("configure migration database");
        migrations::install_legacy_fixture(&connection, 2, true);
        connection
            .execute_batch(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-migrate', 'oauth', 'Account', 'default');
                 INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_hash, hash_salt) VALUES ('key-migrate', 'Key', 'osk_migrate', X'11', X'12');
                 INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version, metadata_json) VALUES ('account-migrate', 'oauth_token_bundle', X'01', zeroblob(12), 1, '{\"client_secret\":\"legacy-secret\",\"token_endpoint\":\"https://issuer.example/token\"}');
                 INSERT INTO ai_gateway_request_logs (id, request_id, started_at, local_date, timezone_name, endpoint, public_model_id, api_key_id, api_key_name_snapshot, account_id, account_name_snapshot, status) VALUES ('log-migrate', 'request-migrate', CURRENT_TIMESTAMP, '2026-08-01', 'UTC', '/v1/responses', 'gpt-5.6-sol', 'key-migrate', 'Key', 'account-migrate', 'Account', 'failed');",
            )
            .expect("seed pre-v3 records");

        migrations::apply(&connection).expect("apply v3");
        let migrated: (
            Option<String>,
            Option<String>,
            Option<String>,
        ) = connection
            .query_row(
                "SELECT account_id_snapshot, api_key_id_snapshot, metadata_json FROM ai_gateway_request_logs LEFT JOIN ai_gateway_credentials ON ai_gateway_credentials.account_id = 'account-migrate' WHERE ai_gateway_request_logs.id = 'log-migrate'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read migrated records");
        assert_eq!(
            migrated,
            (
                Some("account-migrate".to_string()),
                Some("key-migrate".to_string()),
                Some("{\"token_endpoint\":\"https://issuer.example/token\"}".to_string())
            )
        );
        let health: (String, Option<String>) = connection
            .query_row(
                "SELECT health_status, health_reason_code FROM ai_gateway_accounts WHERE id = 'account-migrate'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read migration reauthorization state");
        assert_eq!(
            health,
            (
                "authorization_invalid".to_string(),
                Some("oauth_reauthorization_required".to_string())
            )
        );
        let sensitive_metadata: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_credentials WHERE lower(COALESCE(metadata_json, '')) LIKE '%client_secret%' OR lower(COALESCE(metadata_json, '')) LIKE '%refresh_token%' OR lower(COALESCE(metadata_json, '')) LIKE '%access_token%'",
                [],
                |row| row.get(0),
            )
            .expect("scan migrated metadata");
        assert_eq!(sensitive_metadata, 0);
        remove_database(&path);
    }

    #[test]
    fn gateway_key_encryption_upgrade_keeps_legacy_keys_explicitly_uncopyable() {
        let path = temporary_database("gateway-key-encryption-upgrade");
        let connection = Connection::open(&path).expect("open migration database");
        configure_connection(&connection).expect("configure migration database");
        migrations::install_legacy_fixture(&connection, 3, true);
        connection
            .execute(
                "INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_hash, hash_salt) VALUES ('legacy-key', 'Legacy', 'osk_legacy12', X'11', X'12')",
                [],
            )
            .expect("seed legacy gateway key");

        migrations::apply(&connection).expect("apply v4");

        let encrypted: (Option<String>, Option<Vec<u8>>, Option<Vec<u8>>, Option<i64>) = connection
            .query_row(
                "SELECT key_suffix, ciphertext, nonce, cipher_version FROM ai_gateway_api_keys WHERE id = 'legacy-key'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("read upgraded legacy key");
        assert_eq!(encrypted, (None, None, None, None));
        let migration_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM refinery_schema_history WHERE version = 4",
                [],
                |row| row.get(0),
            )
            .expect("read v4 migration record");
        assert_eq!(migration_count, 1);
        remove_database(&path);
    }

    #[test]
    fn gateway_key_display_group_v5_migrates_v4_data_and_enforces_relations() {
        let path = temporary_database("gateway-key-display-group-v5");
        let connection = Connection::open(&path).expect("open migration database");
        configure_connection(&connection).expect("configure migration database");
        migrations::install_legacy_fixture(&connection, 4, true);
        connection
            .execute(
                "INSERT INTO ai_gateway_api_keys
                    (id, name, key_prefix, key_hash, hash_salt)
                 VALUES ('legacy-key-v5', 'Legacy', 'osk_legacyv5', X'11', X'12')",
                [],
            )
            .expect("seed v4 gateway key");

        migrations::apply(&connection).expect("apply v5");
        migrations::apply(&connection).expect("repeat v5");

        let defaults: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_key_display_groups WHERE is_default = 1",
                [],
                |row| row.get(0),
            )
            .expect("count display defaults");
        let group_id: String = connection
            .query_row(
                "SELECT display_group_id FROM ai_gateway_api_keys WHERE id = 'legacy-key-v5'",
                [],
                |row| row.get(0),
            )
            .expect("read migrated display group");
        let foreign_key_violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("check foreign keys");
        assert_eq!(defaults, 1);
        assert_eq!(group_id, "gateway-key-default");
        assert_eq!(foreign_key_violations, 0);
        assert!(connection
            .execute(
                "DELETE FROM ai_gateway_key_display_groups WHERE id = 'gateway-key-default'",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO ai_gateway_key_display_groups (id, name, is_default)
                 VALUES ('duplicate-default', 'Duplicate', 1)",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "UPDATE ai_gateway_api_keys SET display_group_id = 'missing' WHERE id = 'legacy-key-v5'",
                [],
            )
            .is_err());
        connection
            .execute(
                "INSERT INTO ai_gateway_key_provider_conversions
                    (gateway_key_id, tool, service_provider_id)
                 VALUES ('legacy-key-v5', 'claude', 'provider-1')",
                [],
            )
            .expect("insert conversion");
        assert!(connection
            .execute(
                "INSERT INTO ai_gateway_key_provider_conversions
                    (gateway_key_id, tool, service_provider_id)
                 VALUES ('legacy-key-v5', 'claude', 'provider-2')",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO ai_gateway_key_provider_conversions
                    (gateway_key_id, tool, service_provider_id)
                 VALUES ('legacy-key-v5', 'unknown', 'provider-3')",
                [],
            )
            .is_err());

        let version_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM refinery_schema_history WHERE version = 5",
                [],
                |row| row.get(0),
            )
            .expect("count v5 history");
        assert_eq!(version_count, 1);
        drop(connection);

        let reopened = open_at(&path).expect("restart migrated v4 database");
        let restarted_defaults: i64 = reopened
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_key_display_groups WHERE is_default = 1",
                [],
                |row| row.get(0),
            )
            .expect("count defaults after restart");
        let restarted_group: String = reopened
            .query_row(
                "SELECT display_group_id FROM ai_gateway_api_keys WHERE id = 'legacy-key-v5'",
                [],
                |row| row.get(0),
            )
            .expect("read migrated group after restart");
        let restarted_violations: i64 = reopened
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("check foreign keys after restart");
        assert_eq!(restarted_defaults, 1);
        assert_eq!(restarted_group, "gateway-key-default");
        assert_eq!(restarted_violations, 0);
        drop(reopened);
        remove_database(&path);
    }

    #[test]
    fn gateway_key_v5_empty_database_restarts_with_one_valid_default_group() {
        let path = temporary_database("gateway-key-v5-empty-restart");
        for _ in 0..2 {
            let connection = open_at(&path).expect("bootstrap empty gateway database");
            let defaults: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM ai_gateway_key_display_groups WHERE is_default = 1",
                    [],
                    |row| row.get(0),
                )
                .expect("count empty database defaults");
            let unassigned: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM ai_gateway_api_keys WHERE display_group_id IS NULL",
                    [],
                    |row| row.get(0),
                )
                .expect("count unassigned keys");
            let violations: i64 = connection
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })
                .expect("check empty database foreign keys");
            let version_count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM refinery_schema_history WHERE version = 5",
                    [],
                    |row| row.get(0),
                )
                .expect("count empty database v5 history");
            assert_eq!(
                (defaults, unassigned, violations, version_count),
                (1, 0, 0, 1)
            );
        }
        remove_database(&path);
    }

    #[test]
    fn v2_oauth_upgrade_preserves_refresh_endpoint_and_requires_reauthorization_for_legacy_secret()
    {
        let path = temporary_database("oauth-refresh-upgrade");
        let connection = Connection::open(&path).expect("open oauth upgrade database");
        configure_connection(&connection).expect("configure oauth upgrade database");
        migrations::install_legacy_fixture(&connection, 2, true);
        let root_key = RootKey::try_from(vec![71; 32]).expect("construct migration root key");
        let token_bundle = OAuthTokenBundle {
            access_token: "legacy-access".into(),
            refresh_token: "legacy-refresh".into(),
            expires_at: None,
            token_type: "Bearer".into(),
            scope: "fixture".into(),
        };
        for (account_id, metadata_json) in [
            (
                "account-safe-migrate",
                r#"{"token_endpoint":"http://127.0.0.1:19191/oauth/token"}"#,
            ),
            (
                "account-private-migrate",
                r#"{"token_endpoint":"http://127.0.0.1:19192/oauth/token","client_secret":"legacy-secret"}"#,
            ),
            ("account-invalid-migrate", "not-json"),
        ] {
            connection
                .execute(
                    "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES (?1, 'oauth', ?1, 'default')",
                    [account_id],
                )
                .expect("insert legacy oauth account");
            let plaintext = serde_json::to_vec(&token_bundle).expect("serialize token bundle");
            let encrypted =
                encrypt_credential(&root_key, "oauth_token_bundle", account_id, &plaintext)
                    .expect("encrypt legacy token bundle");
            connection
                .execute(
                    "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version, metadata_json) VALUES (?1, 'oauth_token_bundle', ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        account_id,
                        encrypted.ciphertext,
                        encrypted.nonce.as_slice(),
                        encrypted.cipher_version,
                        metadata_json,
                    ],
                )
                .expect("insert legacy oauth credential");
        }

        migrations::apply(&connection).expect("apply v3");
        let safe_material =
            load_oauth_refresh_material(&connection, &root_key, "account-safe-migrate")
                .expect("load migrated public refresh endpoint");
        assert_eq!(
            safe_material.token_endpoint.as_deref(),
            Some("http://127.0.0.1:19191/oauth/token")
        );
        assert_eq!(safe_material.client_secret, None);
        assert_eq!(
            load_oauth_refresh_material(&connection, &root_key, "account-private-migrate")
                .expect_err("legacy private metadata requires reauthorization")
                .category(),
            crate::ai_routing_gateway::error::GatewayErrorCategory::OAuthReauthorizationRequired
        );
        let private_metadata: Option<String> = connection
            .query_row(
                "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'account-private-migrate'",
                [],
                |row| row.get(0),
            )
            .expect("read cleaned legacy metadata");
        assert_eq!(
            private_metadata.as_deref(),
            Some("{\"token_endpoint\":\"http://127.0.0.1:19192/oauth/token\"}")
        );
        assert!(!private_metadata
            .as_deref()
            .unwrap_or_default()
            .contains("legacy-secret"));
        assert_eq!(
            load_oauth_refresh_material(&connection, &root_key, "account-invalid-migrate")
                .expect_err("invalid legacy metadata requires reauthorization")
                .category(),
            crate::ai_routing_gateway::error::GatewayErrorCategory::OAuthReauthorizationRequired
        );
        let invalid_metadata: Option<String> = connection
            .query_row(
                "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = 'account-invalid-migrate'",
                [],
                |row| row.get(0),
            )
            .expect("read cleaned invalid legacy metadata");
        assert_eq!(invalid_metadata, None);
        remove_database(&path);
    }
}
