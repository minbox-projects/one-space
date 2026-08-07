use super::{sqlite_cause_code, SharedSqliteError, SqliteCauseCode};
use crate::ai_routing_gateway::accounts::{
    is_safe_public_metadata_object, strip_sensitive_metadata_keys,
};
use refinery::{embed_migrations, Migration, Target};
use refinery_core::traits::sync::{Migrate, Query, Transaction};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(crate) const AI_ROUTING_GATEWAY_SUBSYSTEM: &str = "ai_routing_gateway";
const LEGACY_HISTORY_TABLE: &str = "app_schema_migrations";
const REFINERY_HISTORY_TABLE: &str = "refinery_schema_history";
pub(super) const LATEST_VERSION: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MigrationStage {
    Check,
    Baseline,
    Execute,
    Commit,
}

impl std::fmt::Display for MigrationStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Check => "check",
            Self::Baseline => "baseline",
            Self::Execute => "execute",
            Self::Commit => "commit",
        })
    }
}

#[derive(Debug)]
pub(crate) struct MigrationDiagnostic {
    stage: MigrationStage,
    path: PathBuf,
    identified_version: Option<u32>,
    cause: String,
}

impl MigrationDiagnostic {
    fn new(
        stage: MigrationStage,
        path: &Path,
        identified_version: Option<u32>,
        cause: impl Into<String>,
    ) -> Self {
        Self {
            stage,
            path: path.to_owned(),
            identified_version,
            cause: cause.into(),
        }
    }

    pub(crate) fn stage(&self) -> MigrationStage {
        self.stage
    }

    pub(crate) fn identified_version(&self) -> Option<u32> {
        self.identified_version
    }
}

impl std::fmt::Display for MigrationDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "shared database migration failed: stage={}, path={}, identified_version={}, target_version={}, cause={}",
            self.stage,
            self.path.display(),
            self.identified_version
                .map_or_else(|| "unknown".to_owned(), |version| version.to_string()),
            LATEST_VERSION,
            self.cause
        )
    }
}

impl std::error::Error for MigrationDiagnostic {}

struct MigrationFailure {
    stage: MigrationStage,
    identified_version: Option<u32>,
    cause: MigrationCause,
}

impl MigrationFailure {
    fn new(stage: MigrationStage, identified_version: Option<u32>, cause: MigrationCause) -> Self {
        Self {
            stage,
            identified_version,
            cause,
        }
    }
}

#[derive(Debug)]
struct MigrationCause {
    error: SharedSqliteError,
    context: &'static str,
    sqlite_code: Option<SqliteCauseCode>,
}

impl MigrationCause {
    fn state(context: &'static str) -> Self {
        Self {
            error: SharedSqliteError::MigrationStateInvalid,
            context,
            sqlite_code: None,
        }
    }

    fn failed(context: &'static str) -> Self {
        Self {
            error: SharedSqliteError::MigrationFailed,
            context,
            sqlite_code: None,
        }
    }

    fn from_sqlite(
        error: SharedSqliteError,
        context: &'static str,
        source: &rusqlite::Error,
    ) -> Self {
        Self {
            error,
            context,
            sqlite_code: sqlite_cause_code(source),
        }
    }

    fn from_refinery(
        error: SharedSqliteError,
        context: &'static str,
        source: &refinery_core::Error,
    ) -> Self {
        Self {
            error,
            context,
            sqlite_code: sqlite_cause_code_from_error(source),
        }
    }
}

impl std::fmt::Display for MigrationCause {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:context={}", self.error, self.context)?;
        if let Some(sqlite_code) = self.sqlite_code {
            write!(
                formatter,
                ",sqlite_code={:?},sqlite_extended_code={}",
                sqlite_code.code, sqlite_code.extended_code
            )?;
        }
        Ok(())
    }
}

fn sqlite_cause_code_from_error(
    source: &(dyn std::error::Error + 'static),
) -> Option<SqliteCauseCode> {
    let mut current = Some(source);
    while let Some(error) = current {
        if let Some(sqlite_error) = error.downcast_ref::<rusqlite::Error>() {
            return sqlite_cause_code(sqlite_error);
        }
        current = error.source();
    }
    None
}

mod embedded {
    use super::embed_migrations;

    embed_migrations!("./src/shared_sqlite/migrations");
}

struct AtomicRefineryConnection<'a> {
    connection: &'a Connection,
}

impl Transaction for AtomicRefineryConnection<'_> {
    type Error = rusqlite::Error;

    fn execute(&mut self, queries: &[&str]) -> Result<usize, Self::Error> {
        for query in queries {
            self.connection.execute_batch(query)?;
        }
        Ok(queries.len())
    }
}

impl Query<Vec<Migration>> for AtomicRefineryConnection<'_> {
    fn query(&mut self, query: &str) -> Result<Vec<Migration>, Self::Error> {
        let mut temporary = Connection::open_in_memory()?;
        temporary.execute_batch(
            "CREATE TABLE refinery_schema_history(
                version INT4 PRIMARY KEY,
                name VARCHAR(255),
                applied_on VARCHAR(255),
                checksum VARCHAR(255)
            );",
        )?;
        let mut statement = self
            .connection
            .prepare("SELECT version, name, applied_on, checksum FROM refinery_schema_history")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (version, name, applied_on, checksum) = row?;
            temporary.execute(
                "INSERT INTO refinery_schema_history(version, name, applied_on, checksum)
                 VALUES (?1, ?2, ?3, ?4)",
                params![version, name, applied_on, checksum],
            )?;
        }
        <Connection as Query<Vec<Migration>>>::query(&mut temporary, query)
    }
}

impl Migrate for AtomicRefineryConnection<'_> {}

pub(super) fn apply(connection: &Connection) -> Result<(), SharedSqliteError> {
    apply_inner(connection, false).map_err(|failure| failure.cause.error)
}

fn apply_inner(connection: &Connection, fail_after_baseline: bool) -> Result<(), MigrationFailure> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|source| {
            MigrationFailure::new(
                MigrationStage::Check,
                None,
                MigrationCause::from_sqlite(
                    SharedSqliteError::MigrationFailed,
                    "begin_transaction",
                    &source,
                ),
            )
        })?;
    let result = migrate_in_transaction(connection, fail_after_baseline);
    match result {
        Ok(identified_version) => match connection.execute_batch("COMMIT") {
            Ok(()) => Ok(()),
            Err(source) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(MigrationFailure::new(
                    MigrationStage::Commit,
                    (identified_version > 0).then_some(identified_version),
                    MigrationCause::from_sqlite(
                        SharedSqliteError::MigrationFailed,
                        "commit_transaction",
                        &source,
                    ),
                ))
            }
        },
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn migrate_in_transaction(
    connection: &Connection,
    fail_after_baseline: bool,
) -> Result<u32, MigrationFailure> {
    let schema_version = identify_schema_version(connection, false)
        .map_err(|cause| MigrationFailure::new(MigrationStage::Check, None, cause))?;
    let identified_version = (schema_version > 0).then_some(schema_version);
    let refinery_version = read_refinery_version(connection)
        .map_err(|cause| MigrationFailure::new(MigrationStage::Check, identified_version, cause))?;
    if schema_version >= 3 {
        let metadata_safe = metadata_safety_contract_holds(connection).map_err(|cause| {
            MigrationFailure::new(MigrationStage::Check, identified_version, cause)
        })?;
        if !metadata_safe {
            return Err(MigrationFailure::new(
                MigrationStage::Check,
                identified_version,
                MigrationCause::state("metadata_safety_contract"),
            ));
        }
    }
    if refinery_version.unwrap_or(0) == 0 && schema_version > 0 {
        let contract_holds =
            migration_data_contract_holds(connection, schema_version).map_err(|cause| {
                MigrationFailure::new(MigrationStage::Check, identified_version, cause)
            })?;
        if !contract_holds {
            return Err(MigrationFailure::new(
                MigrationStage::Check,
                identified_version,
                MigrationCause::state("migration_data_contract"),
            ));
        }
    }
    let legacy_version = read_legacy_version(connection, schema_version > 0).map_err(|source| {
        MigrationFailure::new(MigrationStage::Check, identified_version, source)
    })?;

    let baseline = match refinery_version {
        Some(version) if version > 0 => {
            if schema_version != version || legacy_version.is_some_and(|legacy| legacy > version) {
                return Err(MigrationFailure::new(
                    MigrationStage::Check,
                    identified_version,
                    MigrationCause::state("refinery_legacy_version_mismatch"),
                ));
            }
            version
        }
        _ => match legacy_version {
            Some(version) if schema_version == version => version,
            Some(_) => {
                return Err(MigrationFailure::new(
                    MigrationStage::Check,
                    identified_version,
                    MigrationCause::state("legacy_version_mismatch"),
                ))
            }
            None => schema_version,
        },
    };

    let mut adapter = AtomicRefineryConnection { connection };
    if refinery_version.unwrap_or(0) == 0 && baseline > 0 {
        embedded::migrations::runner()
            .set_target(Target::FakeVersion(baseline))
            .set_grouped(true)
            .run(&mut adapter)
            .map_err(|source| {
                MigrationFailure::new(
                    MigrationStage::Baseline,
                    identified_version,
                    MigrationCause::from_refinery(
                        SharedSqliteError::MigrationFailed,
                        "refinery_baseline",
                        &source,
                    ),
                )
            })?;
    }
    if schema_version > 0 && baseline < 3 {
        sanitize_legacy_metadata(connection).map_err(|cause| {
            MigrationFailure::new(MigrationStage::Execute, identified_version, cause)
        })?;
    }
    if fail_after_baseline {
        return Err(MigrationFailure::new(
            MigrationStage::Baseline,
            identified_version,
            MigrationCause::failed("test_after_baseline"),
        ));
    }
    embedded::migrations::runner()
        .set_grouped(true)
        .run(&mut adapter)
        .map_err(|source| {
            MigrationFailure::new(
                MigrationStage::Execute,
                identified_version,
                MigrationCause::from_refinery(
                    SharedSqliteError::MigrationFailed,
                    "refinery_execute",
                    &source,
                ),
            )
        })?;
    Ok(schema_version)
}

pub(super) fn apply_with_diagnostics(
    connection: &Connection,
    path: &Path,
) -> Result<(), MigrationDiagnostic> {
    apply_inner(connection, false).map_err(|failure| {
        MigrationDiagnostic::new(
            failure.stage,
            path,
            failure.identified_version,
            failure.cause.to_string(),
        )
    })
}

fn identify_schema_version(
    connection: &Connection,
    require_migration_data_contract: bool,
) -> Result<u32, MigrationCause> {
    let actual = gateway_schema(connection)?;
    if actual.is_empty() {
        return Ok(0);
    }
    let mut matches = Vec::new();
    for version in 1..=LATEST_VERSION {
        if actual == expected_schema(version)?
            && (!require_migration_data_contract
                || migration_data_contract_holds(connection, version)?)
        {
            matches.push(version);
        }
    }
    match matches.as_slice() {
        [version] => Ok(*version),
        _ => Err(MigrationCause::state("schema_fingerprint_not_unique")),
    }
}

type SchemaObject = (String, String, String, String);

fn gateway_schema(connection: &Connection) -> Result<Vec<SchemaObject>, MigrationCause> {
    let mut statement = connection
        .prepare(
            "SELECT type, name, tbl_name, sql
             FROM sqlite_schema
             WHERE type IN ('table', 'index', 'trigger')
               AND name GLOB 'ai_gateway_*'
               AND sql IS NOT NULL
             ORDER BY type, name",
        )
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "schema_query",
                &source,
            )
        })?;
    let objects = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "schema_query_rows",
                &source,
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "schema_row_decode",
                &source,
            )
        })?;
    Ok(objects)
}

fn expected_schema(version: u32) -> Result<Vec<SchemaObject>, MigrationCause> {
    let connection = Connection::open_in_memory().map_err(|source| {
        MigrationCause::from_sqlite(
            SharedSqliteError::MigrationFailed,
            "expected_schema_open",
            &source,
        )
    })?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationFailed,
                "expected_schema_pragma",
                &source,
            )
        })?;
    let mut migrations = embedded::migrations::runner().get_migrations().clone();
    migrations.sort();
    for migration in &migrations {
        if migration.version() > version {
            break;
        }
        let sql = migration
            .sql()
            .ok_or_else(|| MigrationCause::failed("expected_schema_embedded_sql"))?;
        connection.execute_batch(sql).map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationFailed,
                "expected_schema_sql",
                &source,
            )
        })?;
    }
    gateway_schema(&connection)
}

fn metadata_safety_contract_holds(connection: &Connection) -> Result<bool, MigrationCause> {
    let mut statement = connection
        .prepare(
            "SELECT metadata_json
             FROM ai_gateway_credentials
             WHERE metadata_json IS NOT NULL",
        )
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "metadata_safety_prepare",
                &source,
            )
        })?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "metadata_safety_query",
                &source,
            )
        })?;
    for row in rows {
        let metadata_json = row.map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "metadata_safety_row",
                &source,
            )
        })?;
        let metadata = match serde_json::from_str::<Value>(&metadata_json) {
            Ok(metadata) => metadata,
            Err(_) => return Ok(false),
        };
        if !is_safe_public_metadata_object(&metadata) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn sanitize_legacy_metadata(connection: &Connection) -> Result<(), MigrationCause> {
    let mut statement = connection
        .prepare(
            "SELECT account_id, record_type, metadata_json
             FROM ai_gateway_credentials
             WHERE metadata_json IS NOT NULL",
        )
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationFailed,
                "oauth_metadata_cleanup_prepare",
                &source,
            )
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationFailed,
                "oauth_metadata_cleanup_query",
                &source,
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationFailed,
                "oauth_metadata_cleanup_row",
                &source,
            )
        })?;
    drop(statement);

    for (account_id, record_type, metadata_json) in rows {
        let metadata_update: Option<Option<String>> =
            match serde_json::from_str::<Value>(&metadata_json) {
                Ok(mut metadata) if metadata.is_object() => {
                    if strip_sensitive_metadata_keys(&mut metadata) {
                        let sanitized = serde_json::to_string(&metadata)
                            .map_err(|_| MigrationCause::failed("oauth_metadata_cleanup_encode"))?;
                        Some(Some(sanitized))
                    } else {
                        None
                    }
                }
                _ => Some(None),
            };

        if let Some(cleaned_metadata) = metadata_update.as_ref() {
            connection
                .execute(
                    "UPDATE ai_gateway_credentials
                     SET metadata_json = ?2, updated_at = CURRENT_TIMESTAMP
                     WHERE account_id = ?1",
                    params![account_id, cleaned_metadata],
                )
                .map_err(|source| {
                    MigrationCause::from_sqlite(
                        SharedSqliteError::MigrationFailed,
                        "oauth_metadata_cleanup_update",
                        &source,
                    )
                })?;
        }

        if record_type == "oauth_token_bundle" && metadata_update.is_some() {
            connection
                .execute(
                    "UPDATE ai_gateway_accounts
                     SET health_status = 'authorization_invalid',
                         health_reason_code = 'oauth_reauthorization_required',
                         updated_at = CURRENT_TIMESTAMP
                     WHERE id = ?1",
                    [account_id],
                )
                .map_err(|source| {
                    MigrationCause::from_sqlite(
                        SharedSqliteError::MigrationFailed,
                        "oauth_metadata_cleanup_health",
                        &source,
                    )
                })?;
        }
    }
    Ok(())
}

fn migration_data_contract_holds(
    connection: &Connection,
    version: u32,
) -> Result<bool, MigrationCause> {
    let settings: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM ai_gateway_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "defaults_settings",
                &source,
            )
        })?;
    let (canonical_default_groups, default_groups): (i64, i64) = connection
        .query_row(
            "SELECT
                 (SELECT COUNT(*) FROM ai_gateway_groups WHERE id = 'default' AND is_default = 1),
                 (SELECT COUNT(*) FROM ai_gateway_groups WHERE is_default = 1)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "defaults_default_group",
                &source,
            )
        })?;
    let models: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM ai_gateway_models WHERE id IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')",
            [],
            |row| row.get(0),
        )
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "defaults_models",
                &source,
            )
        })?;
    let prices: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM ai_gateway_model_prices WHERE id IN (
                'official-openai-api-pricing-2026-08-01-r1-gpt-5.6-sol',
                'official-openai-api-pricing-2026-08-01-r1-gpt-5.6-terra',
                'official-openai-api-pricing-2026-08-01-r1-gpt-5.6-luna'
            )",
            [],
            |row| row.get(0),
        )
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "defaults_prices",
                &source,
            )
        })?;
    if settings != 1
        || canonical_default_groups != 1
        || default_groups != 1
        || models != 3
        || prices != 3
        || version < 3
    {
        return Ok(settings == 1
            && canonical_default_groups == 1
            && default_groups == 1
            && models == 3
            && prices == 3);
    }
    let missing_snapshots: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM ai_gateway_request_logs
             WHERE (api_key_id IS NOT NULL AND api_key_id_snapshot IS NULL)
                OR (account_id IS NOT NULL AND account_id_snapshot IS NULL)",
            [],
            |row| row.get(0),
        )
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "defaults_snapshots",
                &source,
            )
        })?;
    Ok(missing_snapshots == 0)
}

fn read_legacy_version(
    connection: &Connection,
    has_gateway_schema: bool,
) -> Result<Option<u32>, MigrationCause> {
    if !table_exists(connection, LEGACY_HISTORY_TABLE)? {
        return Ok(None);
    }
    let mut statement = connection
        .prepare("SELECT subsystem, version FROM app_schema_migrations ORDER BY version")
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "legacy_history_prepare",
                &source,
            )
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "legacy_history_query",
                &source,
            )
        })?;
    let mut versions = Vec::new();
    let mut other_subsystems = false;
    for row in rows {
        let (subsystem, version) = row.map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "legacy_history_row",
                &source,
            )
        })?;
        if subsystem == AI_ROUTING_GATEWAY_SUBSYSTEM {
            versions.push(
                u32::try_from(version)
                    .map_err(|_| MigrationCause::state("legacy_history_version"))?,
            );
        } else {
            other_subsystems = true;
        }
    }
    if versions.is_empty() {
        return if has_gateway_schema && other_subsystems {
            Err(MigrationCause::state("legacy_history_other_subsystem"))
        } else {
            Ok(None)
        };
    }
    validate_continuous_versions(&versions)?;
    Ok(versions.last().copied())
}

fn read_refinery_version(connection: &Connection) -> Result<Option<u32>, MigrationCause> {
    if !table_exists(connection, REFINERY_HISTORY_TABLE)? {
        return Ok(None);
    }
    let expected = embedded::migrations::runner();
    let mut statement = connection
        .prepare(
            "SELECT version, name, applied_on, checksum
             FROM refinery_schema_history ORDER BY version",
        )
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "refinery_history_prepare",
                &source,
            )
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "refinery_history_query",
                &source,
            )
        })?;
    let mut versions = Vec::new();
    for row in rows {
        let (raw_version, name, applied_on, checksum) = row.map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "refinery_history_row",
                &source,
            )
        })?;
        let version = u32::try_from(raw_version)
            .map_err(|_| MigrationCause::state("refinery_history_version"))?;
        let migration = expected
            .get_migrations()
            .iter()
            .find(|migration| migration.version() == version)
            .ok_or_else(|| MigrationCause::state("refinery_history_unknown_version"))?;
        if migration.name() != name
            || chrono::DateTime::parse_from_rfc3339(&applied_on).is_err()
            || migration.checksum().to_string() != checksum
        {
            return Err(MigrationCause::state("refinery_history_entry_mismatch"));
        }
        versions.push(version);
    }
    if versions.is_empty() {
        return Ok(Some(0));
    }
    validate_continuous_versions(&versions)?;
    Ok(versions.last().copied())
}

fn validate_continuous_versions(versions: &[u32]) -> Result<(), MigrationCause> {
    if versions.len() > LATEST_VERSION as usize
        || versions
            .iter()
            .copied()
            .ne(1..=u32::try_from(versions.len()).unwrap_or(u32::MAX))
    {
        return Err(MigrationCause::state("migration_history_not_continuous"));
    }
    Ok(())
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool, MigrationCause> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|source| {
            MigrationCause::from_sqlite(
                SharedSqliteError::MigrationStateInvalid,
                "table_exists_query",
                &source,
            )
        })
}

#[cfg(test)]
pub(super) fn install_legacy_fixture(connection: &Connection, version: u32, with_history: bool) {
    let mut migrations = embedded::migrations::runner().get_migrations().clone();
    migrations.sort();
    for migration in &migrations {
        if migration.version() > version {
            break;
        }
        connection
            .execute_batch(migration.sql().expect("embedded SQL migration"))
            .expect("install legacy schema fixture");
    }
    if with_history {
        connection
            .execute_batch(
                "CREATE TABLE app_schema_migrations (
                    subsystem TEXT NOT NULL,
                    version INTEGER NOT NULL CHECK (version > 0),
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (subsystem, version)
                );",
            )
            .expect("create legacy history");
        for applied in 1..=version {
            connection
                .execute(
                    "INSERT INTO app_schema_migrations(subsystem, version) VALUES (?1, ?2)",
                    params![AI_ROUTING_GATEWAY_SUBSYSTEM, applied],
                )
                .expect("record legacy migration");
        }
    }
}

#[cfg(test)]
pub(super) fn apply_with_failure_after_baseline(
    connection: &Connection,
) -> Result<(), SharedSqliteError> {
    apply_inner(connection, true).map_err(|failure| failure.cause.error)
}
