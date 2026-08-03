use super::SharedSqliteError;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

pub(crate) const AI_ROUTING_GATEWAY_SUBSYSTEM: &str = "ai_routing_gateway";

#[derive(Clone, Copy)]
pub(super) struct Migration {
    pub(super) version: i64,
    pub(super) sql: &'static str,
}

pub(super) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../ai_routing_gateway/schema_v1.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("../ai_routing_gateway/schema_v2.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("../ai_routing_gateway/schema_v3.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("../ai_routing_gateway/schema_v4.sql"),
    },
];

pub(super) fn apply(
    connection: &Connection,
    migrations: &[Migration],
) -> Result<(), SharedSqliteError> {
    for migration in migrations {
        let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)
            .map_err(|_| SharedSqliteError::MigrationFailed)?;
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS app_schema_migrations (\n\
                    subsystem TEXT NOT NULL,\n\
                    version INTEGER NOT NULL CHECK (version > 0),\n\
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,\n\
                    PRIMARY KEY (subsystem, version)\n\
                );",
            )
            .map_err(|_| SharedSqliteError::MigrationFailed)?;
        let applied: Option<i64> = transaction
            .query_row(
                "SELECT version FROM app_schema_migrations WHERE subsystem = ?1 AND version = ?2",
                params![AI_ROUTING_GATEWAY_SUBSYSTEM, migration.version],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| SharedSqliteError::MigrationFailed)?;
        if applied.is_none() {
            transaction
                .execute_batch(migration.sql)
                .map_err(|_| SharedSqliteError::MigrationFailed)?;
            transaction
                .execute(
                    "INSERT INTO app_schema_migrations (subsystem, version) VALUES (?1, ?2)",
                    params![AI_ROUTING_GATEWAY_SUBSYSTEM, migration.version],
                )
                .map_err(|_| SharedSqliteError::MigrationFailed)?;
        }
        transaction
            .commit()
            .map_err(|_| SharedSqliteError::MigrationFailed)?;
    }
    Ok(())
}
