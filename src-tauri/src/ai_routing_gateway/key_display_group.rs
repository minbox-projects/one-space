use rusqlite::{params, Connection, OptionalExtension};

use super::error::{GatewayError, GatewayErrorCategory};

pub(crate) const DEFAULT_DISPLAY_GROUP_ID: &str = "gateway-key-default";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyDisplayGroup {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) is_default: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

pub(crate) fn list(connection: &Connection) -> Result<Vec<KeyDisplayGroup>, GatewayError> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, is_default, created_at, updated_at
             FROM ai_gateway_key_display_groups
             ORDER BY is_default DESC, created_at, id",
        )
        .map_err(|_| storage(None))?;
    let groups = statement
        .query_map([], map_group)
        .map_err(|_| storage(None))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| storage(None))?;
    Ok(groups)
}

pub(crate) fn create(connection: &Connection, name: &str) -> Result<KeyDisplayGroup, GatewayError> {
    let name = validated_name(name)?;
    let id = uuid::Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO ai_gateway_key_display_groups (id, name) VALUES (?1, ?2)",
            params![id, name],
        )
        .map_err(|_| invalid(Some(&id)))?;
    load(connection, &id)
}

pub(crate) fn rename(
    connection: &Connection,
    group_id: &str,
    name: &str,
) -> Result<KeyDisplayGroup, GatewayError> {
    let name = validated_name(name)?;
    let changed = connection
        .execute(
            "UPDATE ai_gateway_key_display_groups
             SET name = ?2, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?1 AND is_default = 0",
            params![group_id, name],
        )
        .map_err(|_| invalid(Some(group_id)))?;
    if changed == 0 {
        return Err(group_write_error(connection, group_id)?);
    }
    load(connection, group_id)
}

pub(crate) fn delete(connection: &mut Connection, group_id: &str) -> Result<(), GatewayError> {
    let transaction = connection
        .transaction()
        .map_err(|_| storage(Some(group_id)))?;
    let is_default = transaction
        .query_row(
            "SELECT is_default FROM ai_gateway_key_display_groups WHERE id = ?1",
            [group_id],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map_err(|_| storage(Some(group_id)))?
        .ok_or_else(|| not_found(group_id))?;
    if is_default {
        return Err(invalid(Some(group_id)));
    }
    transaction
        .execute(
            "UPDATE ai_gateway_api_keys SET display_group_id = ?1, updated_at = CURRENT_TIMESTAMP
             WHERE display_group_id = ?2",
            params![DEFAULT_DISPLAY_GROUP_ID, group_id],
        )
        .map_err(|_| storage(Some(group_id)))?;
    let changed = transaction
        .execute(
            "DELETE FROM ai_gateway_key_display_groups WHERE id = ?1 AND is_default = 0",
            [group_id],
        )
        .map_err(|_| storage(Some(group_id)))?;
    if changed != 1 {
        return Err(storage(Some(group_id)));
    }
    transaction.commit().map_err(|_| storage(Some(group_id)))
}

pub(crate) fn exists(connection: &Connection, group_id: &str) -> Result<bool, GatewayError> {
    connection
        .query_row(
            "SELECT 1 FROM ai_gateway_key_display_groups WHERE id = ?1",
            [group_id],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|_| storage(Some(group_id)))
}

fn load(connection: &Connection, group_id: &str) -> Result<KeyDisplayGroup, GatewayError> {
    connection
        .query_row(
            "SELECT id, name, is_default, created_at, updated_at
             FROM ai_gateway_key_display_groups WHERE id = ?1",
            [group_id],
            map_group,
        )
        .optional()
        .map_err(|_| storage(Some(group_id)))?
        .ok_or_else(|| not_found(group_id))
}

fn map_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<KeyDisplayGroup> {
    Ok(KeyDisplayGroup {
        id: row.get(0)?,
        name: row.get(1)?,
        is_default: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn validated_name(name: &str) -> Result<&str, GatewayError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        Err(invalid(None))
    } else {
        Ok(name)
    }
}

fn group_write_error(
    connection: &Connection,
    group_id: &str,
) -> Result<GatewayError, GatewayError> {
    if exists(connection, group_id)? {
        Ok(invalid(Some(group_id)))
    } else {
        Ok(not_found(group_id))
    }
}

fn invalid(entity_id: Option<&str>) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::InvalidInput, entity_id)
}

fn not_found(entity_id: &str) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::NotFound, Some(entity_id))
}

fn storage(entity_id: Option<&str>) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::StorageUnavailable, entity_id)
}
