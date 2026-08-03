use rusqlite::{ffi::ErrorCode, Connection, OpenFlags};
use std::{
    env, fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

mod migrations;

#[cfg(test)]
pub(crate) use migrations::AI_ROUTING_GATEWAY_SUBSYSTEM;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const BUSY_RETRY_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SharedSqliteError {
    HomeDirectoryUnavailable,
    DirectoryCreationFailed,
    DatabaseUnavailable,
    MigrationFailed,
}

impl std::fmt::Display for SharedSqliteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::HomeDirectoryUnavailable => "shared_sqlite_home_unavailable",
            Self::DirectoryCreationFailed => "shared_sqlite_directory_creation_failed",
            Self::DatabaseUnavailable => "shared_sqlite_database_unavailable",
            Self::MigrationFailed => "shared_sqlite_migration_failed",
        })
    }
}

impl std::error::Error for SharedSqliteError {}

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

pub(crate) fn open_at(path: &Path) -> Result<Connection, SharedSqliteError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| SharedSqliteError::DirectoryCreationFailed)?;
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|_| SharedSqliteError::DatabaseUnavailable)?;
    configure_connection(&connection)?;
    migrations::apply(&connection, migrations::MIGRATIONS)?;
    Ok(connection)
}

fn configure_connection(connection: &Connection) -> Result<(), SharedSqliteError> {
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|_| SharedSqliteError::DatabaseUnavailable)?;
    enable_wal(&connection)?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|_| SharedSqliteError::DatabaseUnavailable)?;
    Ok(())
}

fn enable_wal(connection: &Connection) -> Result<(), SharedSqliteError> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        match connection.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(error) if is_busy_or_locked(&error) && Instant::now() < deadline => {
                thread::sleep(BUSY_RETRY_INTERVAL);
            }
            Err(_) => return Err(SharedSqliteError::DatabaseUnavailable),
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
                "SELECT COUNT(*) FROM app_schema_migrations WHERE subsystem = ?1 AND version = 1",
                [AI_ROUTING_GATEWAY_SUBSYSTEM],
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
        let failing = [migrations::Migration {
            version: 42,
            sql: "CREATE TABLE must_roll_back (id INTEGER PRIMARY KEY); INSERT INTO missing_table VALUES (1);",
        }];
        assert_eq!(
            migrations::apply(&connection, &failing),
            Err(SharedSqliteError::MigrationFailed)
        );
        let table: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'must_roll_back'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query rolled back table");
        let migration_table: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'app_schema_migrations'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query migration table");
        assert!(table.is_none());
        assert!(migration_table.is_none());
        remove_database(&path);
    }

    #[test]
    fn forward_migrations_apply_in_version_order() {
        let path = temporary_database("upgrade");
        let connection = Connection::open(&path).expect("open test database");
        configure_connection(&connection).expect("configure test database");
        let first = [migrations::Migration {
            version: 1,
            sql: "CREATE TABLE upgrade_probe (value INTEGER NOT NULL); INSERT INTO upgrade_probe VALUES (1);",
        }];
        migrations::apply(&connection, &first).expect("apply first migration");
        let upgraded = [
            first[0],
            migrations::Migration {
                version: 2,
                sql: "ALTER TABLE upgrade_probe ADD COLUMN label TEXT; UPDATE upgrade_probe SET label = 'upgraded';",
            },
        ];
        migrations::apply(&connection, &upgraded).expect("apply upgrade");
        let row: (i64, String) = connection
            .query_row("SELECT value, label FROM upgrade_probe", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .expect("read upgraded row");
        assert_eq!(row, (1, "upgraded".to_string()));
        remove_database(&path);
    }

    #[test]
    fn attempt_limit_upgrade_preserves_v1_rows_and_allows_oauth_refresh_attempts() {
        let path = temporary_database("attempt-limit-upgrade");
        let connection = Connection::open(&path).expect("open test database");
        configure_connection(&connection).expect("configure test database");
        migrations::apply(&connection, &migrations::MIGRATIONS[..1]).expect("apply gateway v1");
        connection
            .execute_batch(
                "INSERT INTO ai_gateway_request_logs (id, request_id, started_at, local_date, timezone_name, endpoint, public_model_id, status) VALUES ('log-upgrade', 'req-upgrade', CURRENT_TIMESTAMP, '2026-08-01', 'UTC', 'responses', 'gpt-5.6-sol', 'failed');
                 INSERT INTO ai_gateway_request_attempts (id, request_log_id, attempt_number, account_name_snapshot, started_at, status) VALUES ('attempt-existing', 'log-upgrade', 1, 'Account', CURRENT_TIMESTAMP, 'failed');",
            )
            .expect("seed v1 attempt");

        migrations::apply(&connection, migrations::MIGRATIONS).expect("upgrade gateway schema");
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
                "SELECT COUNT(*) FROM app_schema_migrations WHERE subsystem = ?1 AND version = 2",
                [AI_ROUTING_GATEWAY_SUBSYSTEM],
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
        migrations::apply(&connection, &migrations::MIGRATIONS[..2]).expect("apply v1 and v2");
        connection
            .execute_batch(
                "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-migrate', 'oauth', 'Account', 'default');
                 INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_hash, hash_salt) VALUES ('key-migrate', 'Key', 'osk_migrate', X'11', X'12');
                 INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version, metadata_json) VALUES ('account-migrate', 'oauth_token_bundle', X'01', zeroblob(12), 1, '{\"client_secret\":\"legacy-secret\",\"token_endpoint\":\"https://issuer.example/token\"}');
                 INSERT INTO ai_gateway_request_logs (id, request_id, started_at, local_date, timezone_name, endpoint, public_model_id, api_key_id, api_key_name_snapshot, account_id, account_name_snapshot, status) VALUES ('log-migrate', 'request-migrate', CURRENT_TIMESTAMP, '2026-08-01', 'UTC', '/v1/responses', 'gpt-5.6-sol', 'key-migrate', 'Key', 'account-migrate', 'Account', 'failed');",
            )
            .expect("seed pre-v3 records");

        migrations::apply(&connection, migrations::MIGRATIONS).expect("apply v3");
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
        migrations::apply(&connection, &migrations::MIGRATIONS[..3]).expect("apply through v3");
        connection
            .execute(
                "INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_hash, hash_salt) VALUES ('legacy-key', 'Legacy', 'osk_legacy12', X'11', X'12')",
                [],
            )
            .expect("seed legacy gateway key");

        migrations::apply(&connection, migrations::MIGRATIONS).expect("apply v4");

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
                "SELECT COUNT(*) FROM app_schema_migrations WHERE subsystem = ?1 AND version = 4",
                [AI_ROUTING_GATEWAY_SUBSYSTEM],
                |row| row.get(0),
            )
            .expect("read v4 migration record");
        assert_eq!(migration_count, 1);
        remove_database(&path);
    }

    #[test]
    fn v2_oauth_upgrade_preserves_refresh_endpoint_and_requires_reauthorization_for_legacy_secret()
    {
        let path = temporary_database("oauth-refresh-upgrade");
        let connection = Connection::open(&path).expect("open oauth upgrade database");
        configure_connection(&connection).expect("configure oauth upgrade database");
        migrations::apply(&connection, &migrations::MIGRATIONS[..2]).expect("apply v1 and v2");
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

        migrations::apply(&connection, migrations::MIGRATIONS).expect("apply v3");
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
