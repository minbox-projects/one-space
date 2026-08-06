use super::SharedSqliteError;
use refinery::{embed_migrations, Migration, Target};
use refinery_core::traits::sync::{Migrate, Query, Transaction};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) const AI_ROUTING_GATEWAY_SUBSYSTEM: &str = "ai_routing_gateway";
const LEGACY_HISTORY_TABLE: &str = "app_schema_migrations";
const REFINERY_HISTORY_TABLE: &str = "refinery_schema_history";
const LATEST_VERSION: u32 = 4;

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
    apply_inner(connection, false)
}

fn apply_inner(
    connection: &Connection,
    fail_after_baseline: bool,
) -> Result<(), SharedSqliteError> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|_| SharedSqliteError::MigrationFailed)?;
    let result = migrate_in_transaction(connection, fail_after_baseline);
    match result {
        Ok(()) => match connection.execute_batch("COMMIT") {
            Ok(()) => Ok(()),
            Err(_) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(SharedSqliteError::MigrationFailed)
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
) -> Result<(), SharedSqliteError> {
    let schema_version = identify_schema_version(connection)?;
    let legacy_version = read_legacy_version(connection, schema_version > 0)?;
    let refinery_version = read_refinery_version(connection)?;

    let baseline = match refinery_version {
        Some(version) if version > 0 => {
            if schema_version != version || legacy_version.is_some_and(|legacy| legacy > version) {
                return Err(SharedSqliteError::MigrationStateInvalid);
            }
            version
        }
        _ => match legacy_version {
            Some(version) if schema_version == version => version,
            Some(_) => return Err(SharedSqliteError::MigrationStateInvalid),
            None => schema_version,
        },
    };

    let mut adapter = AtomicRefineryConnection { connection };
    if refinery_version.unwrap_or(0) == 0 && baseline > 0 {
        embedded::migrations::runner()
            .set_target(Target::FakeVersion(baseline))
            .set_grouped(true)
            .run(&mut adapter)
            .map_err(|_| SharedSqliteError::MigrationFailed)?;
    }
    if fail_after_baseline {
        return Err(SharedSqliteError::MigrationFailed);
    }
    embedded::migrations::runner()
        .set_grouped(true)
        .run(&mut adapter)
        .map_err(|_| SharedSqliteError::MigrationFailed)?;
    Ok(())
}

fn identify_schema_version(connection: &Connection) -> Result<u32, SharedSqliteError> {
    let actual = gateway_schema(connection)?;
    if actual.is_empty() {
        return Ok(0);
    }
    let mut matches = Vec::new();
    for version in 1..=LATEST_VERSION {
        if actual == expected_schema(version)? && data_contract_holds(connection, version)? {
            matches.push(version);
        }
    }
    match matches.as_slice() {
        [version] => Ok(*version),
        _ => Err(SharedSqliteError::MigrationStateInvalid),
    }
}

type SchemaObject = (String, String, String, String);

fn gateway_schema(connection: &Connection) -> Result<Vec<SchemaObject>, SharedSqliteError> {
    let mut statement = connection
        .prepare(
            "SELECT type, name, tbl_name, sql
             FROM sqlite_schema
             WHERE type IN ('table', 'index', 'trigger')
               AND name GLOB 'ai_gateway_*'
               AND sql IS NOT NULL
             ORDER BY type, name",
        )
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
    let objects = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
    Ok(objects)
}

fn expected_schema(version: u32) -> Result<Vec<SchemaObject>, SharedSqliteError> {
    let connection =
        Connection::open_in_memory().map_err(|_| SharedSqliteError::MigrationFailed)?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|_| SharedSqliteError::MigrationFailed)?;
    let mut migrations = embedded::migrations::runner().get_migrations().clone();
    migrations.sort();
    for migration in &migrations {
        if migration.version() > version {
            break;
        }
        connection
            .execute_batch(migration.sql().ok_or(SharedSqliteError::MigrationFailed)?)
            .map_err(|_| SharedSqliteError::MigrationFailed)?;
    }
    gateway_schema(&connection)
}

fn data_contract_holds(connection: &Connection, version: u32) -> Result<bool, SharedSqliteError> {
    let core_defaults: i64 = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM ai_gateway_settings WHERE id = 1) +
                (SELECT COUNT(*) FROM ai_gateway_groups WHERE is_default = 1) +
                (SELECT COUNT(*) FROM ai_gateway_models WHERE id IN ('gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna')) +
                (SELECT COUNT(*) FROM ai_gateway_model_prices WHERE id LIKE 'official-openai-api-pricing-2026-08-01-r1-%')",
            [],
            |row| row.get(0),
        )
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
    if core_defaults != 8 || version < 3 {
        return Ok(core_defaults == 8);
    }
    let unsafe_metadata: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM ai_gateway_credentials
             WHERE metadata_json IS NOT NULL
               AND (NOT json_valid(metadata_json)
                    OR json_type(metadata_json) <> 'object'
                    OR EXISTS (
                        SELECT 1 FROM json_each(metadata_json)
                        WHERE key <> 'token_endpoint' OR type <> 'text'
                    ))",
            [],
            |row| row.get(0),
        )
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
    let missing_snapshots: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM ai_gateway_request_logs
             WHERE (api_key_id IS NOT NULL AND api_key_id_snapshot IS NULL)
                OR (account_id IS NOT NULL AND account_id_snapshot IS NULL)",
            [],
            |row| row.get(0),
        )
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
    Ok(unsafe_metadata == 0 && missing_snapshots == 0)
}

fn read_legacy_version(
    connection: &Connection,
    has_gateway_schema: bool,
) -> Result<Option<u32>, SharedSqliteError> {
    if !table_exists(connection, LEGACY_HISTORY_TABLE)? {
        return Ok(None);
    }
    let mut statement = connection
        .prepare("SELECT subsystem, version FROM app_schema_migrations ORDER BY version")
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
    let mut versions = Vec::new();
    let mut other_subsystems = false;
    for row in rows {
        let (subsystem, version) = row.map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
        if subsystem == AI_ROUTING_GATEWAY_SUBSYSTEM {
            versions.push(
                u32::try_from(version).map_err(|_| SharedSqliteError::MigrationStateInvalid)?,
            );
        } else {
            other_subsystems = true;
        }
    }
    if versions.is_empty() {
        return if has_gateway_schema && other_subsystems {
            Err(SharedSqliteError::MigrationStateInvalid)
        } else {
            Ok(None)
        };
    }
    validate_continuous_versions(&versions)?;
    Ok(versions.last().copied())
}

fn read_refinery_version(connection: &Connection) -> Result<Option<u32>, SharedSqliteError> {
    if !table_exists(connection, REFINERY_HISTORY_TABLE)? {
        return Ok(None);
    }
    let expected = embedded::migrations::runner();
    let mut statement = connection
        .prepare(
            "SELECT version, name, applied_on, checksum
             FROM refinery_schema_history ORDER BY version",
        )
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
    let mut versions = Vec::new();
    for row in rows {
        let (raw_version, name, applied_on, checksum) =
            row.map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
        let version =
            u32::try_from(raw_version).map_err(|_| SharedSqliteError::MigrationStateInvalid)?;
        let migration = expected
            .get_migrations()
            .iter()
            .find(|migration| migration.version() == version)
            .ok_or(SharedSqliteError::MigrationStateInvalid)?;
        if migration.name() != name
            || chrono::DateTime::parse_from_rfc3339(&applied_on).is_err()
            || migration.checksum().to_string() != checksum
        {
            return Err(SharedSqliteError::MigrationStateInvalid);
        }
        versions.push(version);
    }
    if versions.is_empty() {
        return Ok(Some(0));
    }
    validate_continuous_versions(&versions)?;
    Ok(versions.last().copied())
}

fn validate_continuous_versions(versions: &[u32]) -> Result<(), SharedSqliteError> {
    if versions.len() > LATEST_VERSION as usize
        || versions
            .iter()
            .copied()
            .ne(1..=u32::try_from(versions.len()).unwrap_or(u32::MAX))
    {
        return Err(SharedSqliteError::MigrationStateInvalid);
    }
    Ok(())
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool, SharedSqliteError> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|_| SharedSqliteError::MigrationStateInvalid)
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
    apply_inner(connection, true)
}
