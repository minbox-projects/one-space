use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Days, NaiveDate, Utc};
use pbkdf2::pbkdf2_hmac;
use rand::{rngs::OsRng, RngCore};
use rusqlite::{
    params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension, Transaction,
};
use sha2::Sha256;

use super::{
    error::{GatewayError, GatewayErrorCategory},
    key_display_group::{self, DEFAULT_DISPLAY_GROUP_ID},
    security::{decrypt_credential, encrypt_credential, EncryptedCredential, RootKey},
};

const KEY_BYTES: usize = 32;
const SALT_BYTES: usize = 16;
const HASH_BYTES: usize = 32;
const HASH_ROUNDS: u32 = 120_000;
const LOOKUP_PREFIX_LENGTH: usize = 12;
const DISPLAY_PART_LENGTH: usize = 6;
const RECORD_TYPE: &str = "gateway_api_key";
const MAX_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GatewayKeyGrant {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) model_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedGatewayKey {
    pub(crate) grant: GatewayKeyGrant,
    pub(crate) key_prefix: String,
    pub(crate) plaintext: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayKeyStatus {
    Active,
    Disabled,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayKeyStatusFilter {
    All,
    Active,
    Disabled,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayKeySort {
    CreatedNewest,
    CreatedOldest,
    NameAscending,
    NameDescending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GatewayKeyUsage {
    pub(crate) total_tokens: u64,
    pub(crate) estimated_cost_usd: Option<String>,
    pub(crate) cost_calculable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GatewayKeyListItem {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) masked_key: String,
    pub(crate) display_group_id: String,
    pub(crate) display_group_name: String,
    pub(crate) status: GatewayKeyStatus,
    pub(crate) expires_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) model_ids: Vec<String>,
    pub(crate) today: GatewayKeyUsage,
    pub(crate) last_30_days: GatewayKeyUsage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GatewayKeyListPage {
    pub(crate) items: Vec<GatewayKeyListItem>,
    pub(crate) total: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct GatewayKeyListFilter<'a> {
    pub(crate) display_group_id: &'a str,
    pub(crate) text: Option<&'a str>,
    pub(crate) status: GatewayKeyStatusFilter,
    pub(crate) page: u32,
    pub(crate) page_size: u16,
    pub(crate) sort: GatewayKeySort,
}

pub(crate) fn create(
    connection: &mut Connection,
    root_key: &RootKey,
    name: &str,
    group_ids: &[String],
    model_ids: &[String],
    expires_at: Option<&str>,
) -> Result<CreatedGatewayKey, GatewayError> {
    create_in_display_group(
        connection,
        root_key,
        name,
        DEFAULT_DISPLAY_GROUP_ID,
        group_ids,
        model_ids,
        expires_at,
    )
}

pub(crate) fn create_in_display_group(
    connection: &mut Connection,
    root_key: &RootKey,
    name: &str,
    display_group_id: &str,
    group_ids: &[String],
    model_ids: &[String],
    expires_at: Option<&str>,
) -> Result<CreatedGatewayKey, GatewayError> {
    if name.trim().is_empty() || group_ids.is_empty() || model_ids.is_empty() {
        return Err(error(GatewayErrorCategory::InvalidInput, None));
    }
    validate_expiration(expires_at, Utc::now())?;
    let id = uuid::Uuid::new_v4().to_string();
    let (plaintext, prefix, suffix, salt, hash) = generate_material();
    let encrypted = encrypt_credential(root_key, RECORD_TYPE, &id, plaintext.as_bytes())?;
    let transaction = connection
        .transaction()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(&id)))?;
    transaction
        .execute(
            "INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_suffix, key_hash, hash_salt, expires_at, ciphertext, nonce, cipher_version, display_group_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, name.trim(), prefix, suffix, hash.as_slice(), salt.as_slice(), expires_at, encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version, display_group_id],
        )
        .map_err(|_| error(GatewayErrorCategory::InvalidInput, Some(&id)))?;
    replace_permissions(&transaction, &id, group_ids, model_ids)?;
    transaction
        .commit()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(&id)))?;
    Ok(CreatedGatewayKey {
        grant: GatewayKeyGrant {
            id,
            name: name.trim().to_owned(),
            group_ids: sorted_unique(group_ids),
            model_ids: sorted_unique(model_ids),
        },
        key_prefix: prefix,
        plaintext,
    })
}

pub(crate) fn update(
    connection: &mut Connection,
    key_id: &str,
    name: &str,
    display_group_id: &str,
    group_ids: &[String],
    model_ids: &[String],
    expires_at: Option<&str>,
) -> Result<GatewayKeyGrant, GatewayError> {
    if name.trim().is_empty() || group_ids.is_empty() || model_ids.is_empty() {
        return Err(error(GatewayErrorCategory::InvalidInput, Some(key_id)));
    }
    validate_expiration(expires_at, Utc::now())?;
    let transaction = connection
        .transaction()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    let changed = transaction
        .execute(
            "UPDATE ai_gateway_api_keys
             SET name = ?2, display_group_id = ?3, expires_at = ?4, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?1 AND revoked_at IS NULL",
            params![key_id, name.trim(), display_group_id, expires_at],
        )
        .map_err(|_| error(GatewayErrorCategory::InvalidInput, Some(key_id)))?;
    if changed == 0 {
        return Err(key_state_error(&transaction, key_id)?);
    }
    transaction
        .execute(
            "DELETE FROM ai_gateway_api_key_groups WHERE api_key_id = ?1",
            [key_id],
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    transaction
        .execute(
            "DELETE FROM ai_gateway_api_key_models WHERE api_key_id = ?1",
            [key_id],
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    replace_permissions(&transaction, key_id, group_ids, model_ids)?;
    let grant = load_grant(&transaction, key_id)?;
    transaction
        .commit()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    Ok(grant)
}

pub(crate) fn regenerate(
    connection: &mut Connection,
    root_key: &RootKey,
    key_id: &str,
) -> Result<CreatedGatewayKey, GatewayError> {
    ensure_actionable(connection, key_id, Utc::now())?;
    let (plaintext, prefix, suffix, salt, hash) = generate_material();
    let encrypted = encrypt_credential(root_key, RECORD_TYPE, key_id, plaintext.as_bytes())?;
    let transaction = connection
        .transaction()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    let changed = transaction
        .execute(
            "UPDATE ai_gateway_api_keys SET key_prefix = ?2, key_suffix = ?3, key_hash = ?4, hash_salt = ?5, ciphertext = ?6, nonce = ?7, cipher_version = ?8, enabled = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?1 AND revoked_at IS NULL",
            params![key_id, prefix, suffix, hash.as_slice(), salt.as_slice(), encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version],
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    if changed == 0 {
        return Err(error(GatewayErrorCategory::NotFound, Some(key_id)));
    }
    let grant = load_grant(&transaction, key_id)?;
    transaction
        .commit()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    Ok(CreatedGatewayKey {
        grant,
        key_prefix: prefix,
        plaintext,
    })
}

pub(crate) fn set_enabled(
    connection: &Connection,
    key_id: &str,
    enabled: bool,
) -> Result<(), GatewayError> {
    if enabled {
        ensure_actionable(connection, key_id, Utc::now())?;
    } else if !key_exists(connection, key_id)? {
        return Err(error(GatewayErrorCategory::NotFound, Some(key_id)));
    }
    update_status(connection, key_id, enabled)
}

pub(crate) fn revoke(connection: &Connection, key_id: &str) -> Result<(), GatewayError> {
    let changed = connection
        .execute(
            "UPDATE ai_gateway_api_keys SET revoked_at = CURRENT_TIMESTAMP, enabled = 0, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            [key_id],
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    if changed == 0 {
        Err(error(GatewayErrorCategory::NotFound, Some(key_id)))
    } else {
        Ok(())
    }
}

pub(crate) fn copy_plaintext(
    connection: &Connection,
    root_key: &RootKey,
    key_id: &str,
) -> Result<String, GatewayError> {
    ensure_actionable(connection, key_id, Utc::now())?;
    let encrypted = connection
        .query_row(
            "SELECT ciphertext, nonce, cipher_version FROM ai_gateway_api_keys WHERE id = ?1",
            [key_id],
            |row| {
                Ok((
                    row.get::<_, Option<Vec<u8>>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?
        .ok_or_else(|| error(GatewayErrorCategory::NotFound, Some(key_id)))?;
    let (Some(ciphertext), Some(nonce), Some(cipher_version)) = encrypted else {
        return Err(error(GatewayErrorCategory::CredentialMissing, Some(key_id)));
    };
    let nonce = nonce
        .try_into()
        .map_err(|_| error(GatewayErrorCategory::CredentialInvalid, Some(key_id)))?;
    let plaintext = decrypt_credential(
        root_key,
        RECORD_TYPE,
        key_id,
        &EncryptedCredential {
            ciphertext,
            nonce,
            cipher_version,
        },
    )?;
    String::from_utf8(plaintext)
        .map_err(|_| error(GatewayErrorCategory::CredentialInvalid, Some(key_id)))
}

pub(crate) fn replace_groups(
    connection: &mut Connection,
    key_id: &str,
    group_ids: &[String],
) -> Result<Vec<String>, GatewayError> {
    if group_ids.is_empty() {
        return Err(error(GatewayErrorCategory::InvalidInput, Some(key_id)));
    }
    let groups = sorted_unique(group_ids);
    let transaction = connection
        .transaction()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    if transaction
        .query_row(
            "SELECT 1 FROM ai_gateway_api_keys WHERE id = ?1",
            [key_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?
        .is_none()
    {
        return Err(error(GatewayErrorCategory::NotFound, Some(key_id)));
    }
    transaction
        .execute(
            "DELETE FROM ai_gateway_api_key_groups WHERE api_key_id = ?1",
            [key_id],
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    for group_id in &groups {
        transaction
            .execute(
                "INSERT INTO ai_gateway_api_key_groups (api_key_id, group_id) VALUES (?1, ?2)",
                params![key_id, group_id],
            )
            .map_err(|_| error(GatewayErrorCategory::InvalidInput, Some(key_id)))?;
    }
    transaction
        .commit()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    Ok(groups)
}

pub(crate) fn list(
    connection: &Connection,
    filter: &GatewayKeyListFilter<'_>,
    now: DateTime<Utc>,
    local_today: NaiveDate,
) -> Result<GatewayKeyListPage, GatewayError> {
    if filter.page == 0 || filter.page_size == 0 || filter.page_size > MAX_PAGE_SIZE {
        return Err(error(GatewayErrorCategory::InvalidInput, None));
    }
    if !key_display_group::exists(connection, filter.display_group_id)? {
        return Err(error(
            GatewayErrorCategory::InvalidInput,
            Some(filter.display_group_id),
        ));
    }

    let now = now.to_rfc3339();
    let mut predicate = String::from(
        " FROM ai_gateway_api_keys key
          JOIN ai_gateway_key_display_groups display_group ON display_group.id = key.display_group_id
          WHERE key.display_group_id = ? AND key.revoked_at IS NULL",
    );
    let mut values = vec![SqlValue::Text(filter.display_group_id.to_owned())];
    if let Some(text) = filter.text.map(str::trim).filter(|text| !text.is_empty()) {
        predicate.push_str(
            " AND (lower(key.name) LIKE ? ESCAPE '\\'
                    OR lower(key.key_prefix) LIKE ? ESCAPE '\\'
                    OR lower(COALESCE(key.key_suffix, '')) LIKE ? ESCAPE '\\'
                    OR lower(substr(key.key_prefix, 1, 6) || '******' || COALESCE(key.key_suffix, '')) LIKE ? ESCAPE '\\')",
        );
        let pattern = format!("%{}%", escape_like(&text.to_lowercase()));
        values.extend((0..4).map(|_| SqlValue::Text(pattern.clone())));
    }
    match filter.status {
        GatewayKeyStatusFilter::All => {}
        GatewayKeyStatusFilter::Active => {
            predicate.push_str(
                " AND (key.expires_at IS NULL OR datetime(key.expires_at) > datetime(?)) AND key.enabled = 1",
            );
            values.push(SqlValue::Text(now.clone()));
        }
        GatewayKeyStatusFilter::Disabled => {
            predicate.push_str(
                " AND (key.expires_at IS NULL OR datetime(key.expires_at) > datetime(?)) AND key.enabled = 0",
            );
            values.push(SqlValue::Text(now.clone()));
        }
        GatewayKeyStatusFilter::Expired => {
            predicate.push_str(
                " AND key.expires_at IS NOT NULL AND datetime(key.expires_at) <= datetime(?)",
            );
            values.push(SqlValue::Text(now.clone()));
        }
    }

    let total = connection
        .query_row(
            &(String::from("SELECT COUNT(*)") + &predicate),
            params_from_iter(values.clone()),
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, None))?;

    let mut sql = String::from(
        "SELECT key.id, key.name, key.key_prefix, key.key_suffix, key.display_group_id,
                display_group.name, key.enabled, key.expires_at, key.created_at",
    ) + &predicate;
    sql.push_str(match filter.sort {
        GatewayKeySort::CreatedNewest => " ORDER BY key.created_at DESC, key.id DESC",
        GatewayKeySort::CreatedOldest => " ORDER BY key.created_at, key.id",
        GatewayKeySort::NameAscending => " ORDER BY key.name COLLATE NOCASE, key.id",
        GatewayKeySort::NameDescending => " ORDER BY key.name COLLATE NOCASE DESC, key.id DESC",
    });
    sql.push_str(" LIMIT ? OFFSET ?");
    values.push(SqlValue::Integer(i64::from(filter.page_size)));
    let offset = u64::from(filter.page - 1) * u64::from(filter.page_size);
    let offset =
        i64::try_from(offset).map_err(|_| error(GatewayErrorCategory::InvalidInput, None))?;
    values.push(SqlValue::Integer(offset));

    let mut statement = connection
        .prepare(&sql)
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, None))?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, bool>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, None))?;
    let rows = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, None))?;
    drop(statement);

    let window_start = local_today
        .checked_sub_days(Days::new(29))
        .ok_or_else(|| error(GatewayErrorCategory::InvalidInput, None))?;
    let mut items = Vec::with_capacity(rows.len());
    for (
        id,
        name,
        prefix,
        suffix,
        display_group_id,
        display_group_name,
        enabled,
        expires_at,
        created_at,
    ) in rows
    {
        items.push(GatewayKeyListItem {
            masked_key: masked_value(&prefix, suffix.as_deref()),
            status: status_at(enabled, expires_at.as_deref(), &now)?,
            group_ids: query_strings(
                connection,
                "SELECT group_id FROM ai_gateway_api_key_groups WHERE api_key_id = ?1 ORDER BY group_id",
                &id,
            )?,
            model_ids: query_strings(
                connection,
                "SELECT model_id FROM ai_gateway_api_key_models WHERE api_key_id = ?1 ORDER BY model_id",
                &id,
            )?,
            today: usage(connection, &id, local_today, local_today)?,
            last_30_days: usage(connection, &id, window_start, local_today)?,
            id,
            name,
            display_group_id,
            display_group_name,
            expires_at,
            created_at,
        });
    }
    Ok(GatewayKeyListPage {
        items,
        total: u64::try_from(total).unwrap_or(0),
    })
}

pub(crate) fn masked_value(prefix: &str, suffix: Option<&str>) -> String {
    let first = prefix.chars().take(DISPLAY_PART_LENGTH).collect::<String>();
    match suffix {
        Some(suffix) if suffix.chars().count() == DISPLAY_PART_LENGTH => {
            format!("{first}******{suffix}")
        }
        _ => format!("{first}******"),
    }
}

pub(crate) fn authenticate(
    connection: &Connection,
    plaintext: &str,
) -> Result<GatewayKeyGrant, GatewayError> {
    if !plaintext.starts_with("osk_") || plaintext.len() < LOOKUP_PREFIX_LENGTH {
        return Err(error(GatewayErrorCategory::CredentialInvalid, None));
    }
    let prefix = &plaintext[..LOOKUP_PREFIX_LENGTH];
    let row: Option<(String, Vec<u8>, Vec<u8>)> = connection
        .query_row(
            "SELECT id, key_hash, hash_salt FROM ai_gateway_api_keys WHERE key_prefix = ?1 AND enabled = 1 AND revoked_at IS NULL AND (expires_at IS NULL OR datetime(expires_at) > CURRENT_TIMESTAMP)",
            [prefix],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, None))?;
    let Some((id, expected, salt)) = row else {
        return Err(error(GatewayErrorCategory::CredentialInvalid, None));
    };
    let mut actual = [0u8; HASH_BYTES];
    pbkdf2_hmac::<Sha256>(plaintext.as_bytes(), &salt, HASH_ROUNDS, &mut actual);
    if !constant_time_equal(&actual, &expected) {
        return Err(error(GatewayErrorCategory::CredentialInvalid, None));
    }
    connection
        .execute(
            "UPDATE ai_gateway_api_keys SET last_used_at = CURRENT_TIMESTAMP WHERE id = ?1",
            [&id],
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(&id)))?;
    load_grant(connection, &id)
}

fn generate_material() -> (String, String, String, [u8; SALT_BYTES], [u8; HASH_BYTES]) {
    let mut random = [0u8; KEY_BYTES];
    let mut salt = [0u8; SALT_BYTES];
    OsRng.fill_bytes(&mut random);
    OsRng.fill_bytes(&mut salt);
    let plaintext = format!("osk_{}", URL_SAFE_NO_PAD.encode(random));
    let prefix = plaintext[..LOOKUP_PREFIX_LENGTH].to_owned();
    let suffix = plaintext[plaintext.len() - DISPLAY_PART_LENGTH..].to_owned();
    let mut hash = [0u8; HASH_BYTES];
    pbkdf2_hmac::<Sha256>(plaintext.as_bytes(), &salt, HASH_ROUNDS, &mut hash);
    (plaintext, prefix, suffix, salt, hash)
}

fn replace_permissions(
    transaction: &Transaction<'_>,
    key_id: &str,
    group_ids: &[String],
    model_ids: &[String],
) -> Result<(), GatewayError> {
    for group_id in sorted_unique(group_ids) {
        transaction
            .execute(
                "INSERT INTO ai_gateway_api_key_groups (api_key_id, group_id) VALUES (?1, ?2)",
                params![key_id, group_id],
            )
            .map_err(|_| error(GatewayErrorCategory::InvalidInput, Some(key_id)))?;
    }
    for model_id in sorted_unique(model_ids) {
        transaction
            .execute(
                "INSERT INTO ai_gateway_api_key_models (api_key_id, model_id) VALUES (?1, ?2)",
                params![key_id, model_id],
            )
            .map_err(|_| error(GatewayErrorCategory::InvalidInput, Some(key_id)))?;
    }
    Ok(())
}

fn load_grant(connection: &Connection, key_id: &str) -> Result<GatewayKeyGrant, GatewayError> {
    let name = connection
        .query_row(
            "SELECT name FROM ai_gateway_api_keys WHERE id = ?1",
            [key_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?
        .ok_or_else(|| error(GatewayErrorCategory::NotFound, Some(key_id)))?;
    Ok(GatewayKeyGrant {
        id: key_id.to_owned(),
        name,
        group_ids: query_strings(
            connection,
            "SELECT group_id FROM ai_gateway_api_key_groups WHERE api_key_id = ?1 ORDER BY group_id",
            key_id,
        )?,
        model_ids: query_strings(
            connection,
            "SELECT model_id FROM ai_gateway_api_key_models WHERE api_key_id = ?1 ORDER BY model_id",
            key_id,
        )?,
    })
}

fn query_strings(
    connection: &Connection,
    sql: &str,
    key_id: &str,
) -> Result<Vec<String>, GatewayError> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    let values = statement
        .query_map([key_id], |row| row.get(0))
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    Ok(values)
}

fn update_status(connection: &Connection, key_id: &str, value: bool) -> Result<(), GatewayError> {
    let changed = connection
        .execute(
            "UPDATE ai_gateway_api_keys SET enabled = ?2, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?1 AND revoked_at IS NULL",
            params![key_id, value],
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    if changed == 0 {
        Err(key_state_error(connection, key_id)?)
    } else {
        Ok(())
    }
}

fn validate_expiration(value: Option<&str>, now: DateTime<Utc>) -> Result<(), GatewayError> {
    let Some(value) = value else {
        return Ok(());
    };
    let expiration = DateTime::parse_from_rfc3339(value)
        .map_err(|_| error(GatewayErrorCategory::InvalidInput, None))?
        .with_timezone(&Utc);
    if expiration <= now {
        Err(error(GatewayErrorCategory::InvalidInput, None))
    } else {
        Ok(())
    }
}

fn ensure_actionable(
    connection: &Connection,
    key_id: &str,
    now: DateTime<Utc>,
) -> Result<(), GatewayError> {
    let state = connection
        .query_row(
            "SELECT revoked_at, expires_at FROM ai_gateway_api_keys WHERE id = ?1",
            [key_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?
        .ok_or_else(|| error(GatewayErrorCategory::NotFound, Some(key_id)))?;
    if state.0.is_some() || expiration_reached(state.1.as_deref(), now)? {
        Err(error(GatewayErrorCategory::Conflict, Some(key_id)))
    } else {
        Ok(())
    }
}

fn expiration_reached(value: Option<&str>, now: DateTime<Utc>) -> Result<bool, GatewayError> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|expiration| expiration.with_timezone(&Utc) <= now)
                .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, None))
        })
        .transpose()
        .map(|value| value.unwrap_or(false))
}

fn status_at(
    enabled: bool,
    expires_at: Option<&str>,
    now: &str,
) -> Result<GatewayKeyStatus, GatewayError> {
    let now = DateTime::parse_from_rfc3339(now)
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, None))?
        .with_timezone(&Utc);
    if expiration_reached(expires_at, now)? {
        Ok(GatewayKeyStatus::Expired)
    } else if enabled {
        Ok(GatewayKeyStatus::Active)
    } else {
        Ok(GatewayKeyStatus::Disabled)
    }
}

fn key_exists(connection: &Connection, key_id: &str) -> Result<bool, GatewayError> {
    connection
        .query_row(
            "SELECT 1 FROM ai_gateway_api_keys WHERE id = ?1",
            [key_id],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))
}

fn key_state_error(connection: &Connection, key_id: &str) -> Result<GatewayError, GatewayError> {
    if key_exists(connection, key_id)? {
        Ok(error(GatewayErrorCategory::Conflict, Some(key_id)))
    } else {
        Ok(error(GatewayErrorCategory::NotFound, Some(key_id)))
    }
}

fn usage(
    connection: &Connection,
    key_id: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<GatewayKeyUsage, GatewayError> {
    let mut statement = connection
        .prepare(
            "SELECT total_tokens, estimated_cost_usd, cost_calculable
             FROM ai_gateway_request_logs
             WHERE api_key_id_snapshot = ?1 AND local_date >= ?2 AND local_date <= ?3",
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    let rows = statement
        .query_map(
            params![key_id, start_date.to_string(), end_date.to_string()],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            },
        )
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    let mut total_tokens = 0u64;
    let mut total_cost = 0u128;
    let mut cost_calculable = true;
    for row in rows {
        let (tokens, cost, calculable) =
            row.map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
        if let Some(tokens) = tokens {
            total_tokens =
                total_tokens
                    .checked_add(u64::try_from(tokens).map_err(|_| {
                        error(GatewayErrorCategory::StorageUnavailable, Some(key_id))
                    })?)
                    .ok_or_else(|| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
        }
        match (calculable, cost) {
            (true, Some(cost)) if cost_calculable => {
                total_cost = total_cost
                    .checked_add(parse_cost(&cost).ok_or_else(|| {
                        error(GatewayErrorCategory::StorageUnavailable, Some(key_id))
                    })?)
                    .ok_or_else(|| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
            }
            (true, Some(_)) => {}
            _ => cost_calculable = false,
        }
    }
    Ok(GatewayKeyUsage {
        total_tokens,
        estimated_cost_usd: cost_calculable.then(|| format_cost(total_cost)),
        cost_calculable,
    })
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn parse_cost(value: &str) -> Option<u128> {
    let mut parts = value.split('.');
    let whole = parts.next()?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some()
        || whole.is_empty()
        || fraction.len() > 9
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.parse::<u128>().ok()?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u128>().ok()? * 10u128.pow((9 - fraction.len()) as u32)
    };
    whole.checked_mul(1_000_000_000)?.checked_add(fraction)
}

fn format_cost(value: u128) -> String {
    let whole = value / 1_000_000_000;
    let fraction = value % 1_000_000_000;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:09}")
            .trim_end_matches('0')
            .to_owned()
    }
}

fn sorted_unique(values: &[String]) -> Vec<String> {
    let mut values = values.to_vec();
    values.sort();
    values.dedup();
    values
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

fn error(category: GatewayErrorCategory, entity_id: Option<&str>) -> GatewayError {
    GatewayError::new(category, entity_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_sqlite;

    fn root_key(byte: u8) -> RootKey {
        RootKey::try_from(vec![byte; 32]).unwrap()
    }

    #[test]
    fn plaintext_is_returned_once_and_status_changes_apply_immediately() {
        let path = std::env::temp_dir().join(format!(
            "onespace-gateway-key-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let mut connection = shared_sqlite::open_at(&path).unwrap();
        let key = root_key(31);
        let created = create(
            &mut connection,
            &key,
            "CLI",
            &["default".into()],
            &["gpt-5.6-sol".into()],
            None,
        )
        .unwrap();
        assert!(created.plaintext.starts_with("osk_"));
        let serialized: String = connection
            .query_row(
                "SELECT hex(key_hash) || hex(hash_salt) FROM ai_gateway_api_keys WHERE id = ?1",
                [&created.grant.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!serialized.contains(&created.plaintext));
        let persisted: (Vec<u8>, String, String) = connection
            .query_row(
                "SELECT ciphertext, key_prefix, key_suffix FROM ai_gateway_api_keys WHERE id = ?1",
                [&created.grant.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert!(!persisted
            .0
            .windows(created.plaintext.len())
            .any(|window| window == created.plaintext.as_bytes()));
        assert_eq!(
            masked_value(&persisted.1, Some(&persisted.2))
                .chars()
                .count(),
            18
        );
        assert_eq!(
            copy_plaintext(&connection, &key, &created.grant.id).unwrap(),
            created.plaintext
        );
        assert!(copy_plaintext(&connection, &root_key(32), &created.grant.id).is_err());
        assert_eq!(
            authenticate(&connection, &created.plaintext)
                .unwrap()
                .model_ids,
            vec!["gpt-5.6-sol"]
        );
        set_enabled(&connection, &created.grant.id, false).unwrap();
        assert!(authenticate(&connection, &created.plaintext).is_err());
        set_enabled(&connection, &created.grant.id, true).unwrap();
        let regenerated = regenerate(&mut connection, &key, &created.grant.id).unwrap();
        assert_ne!(created.plaintext, regenerated.plaintext);
        assert!(authenticate(&connection, &created.plaintext).is_err());
        assert!(authenticate(&connection, &regenerated.plaintext).is_ok());
        replace_groups(&mut connection, &created.grant.id, &["default".into()]).unwrap();
        assert_eq!(
            load_grant(&connection, &created.grant.id)
                .unwrap()
                .group_ids,
            vec!["default"]
        );
        connection
            .execute(
                "UPDATE ai_gateway_api_keys SET expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
                [&created.grant.id],
            )
            .unwrap();
        assert!(authenticate(&connection, &regenerated.plaintext).is_err());
        connection
            .execute(
                "UPDATE ai_gateway_api_keys SET expires_at = NULL WHERE id = ?1",
                [&created.grant.id],
            )
            .unwrap();
        connection
            .execute(
                "DELETE FROM ai_gateway_api_key_models WHERE api_key_id = ?1",
                [&created.grant.id],
            )
            .unwrap();
        assert!(authenticate(&connection, &regenerated.plaintext)
            .unwrap()
            .model_ids
            .is_empty());
        revoke(&connection, &created.grant.id).unwrap();
        assert!(authenticate(&connection, &regenerated.plaintext).is_err());
        assert_eq!(
            copy_plaintext(&connection, &key, &created.grant.id)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::Conflict
        );
        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn display_group_edit_list_lifecycle_and_usage_contracts_hold() {
        let path = std::env::temp_dir().join(format!(
            "onespace-gateway-key-workflow-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let mut connection = shared_sqlite::open_at(&path).unwrap();
        let root_key = root_key(44);
        let custom = key_display_group::create(&connection, "Team A").unwrap();
        assert_eq!(key_display_group::list(&connection).unwrap().len(), 2);
        assert_eq!(
            key_display_group::rename(&connection, DEFAULT_DISPLAY_GROUP_ID, "Renamed")
                .unwrap_err()
                .category(),
            GatewayErrorCategory::InvalidInput
        );

        let created = create_in_display_group(
            &mut connection,
            &root_key,
            "CLI Production",
            &custom.id,
            &["default".into()],
            &["gpt-5.6-sol".into()],
            Some("2030-01-01T00:00:00Z"),
        )
        .unwrap();
        let material_before: (Vec<u8>, String, String, Vec<u8>, Vec<u8>) = connection
            .query_row(
                "SELECT ciphertext, key_prefix, key_suffix, key_hash, hash_salt
                 FROM ai_gateway_api_keys WHERE id = ?1",
                [&created.grant.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        update(
            &mut connection,
            &created.grant.id,
            "CLI Edited",
            &custom.id,
            &["default".into()],
            &["gpt-5.6-terra".into()],
            None,
        )
        .unwrap();
        let material_after: (Vec<u8>, String, String, Vec<u8>, Vec<u8>) = connection
            .query_row(
                "SELECT ciphertext, key_prefix, key_suffix, key_hash, hash_salt
                 FROM ai_gateway_api_keys WHERE id = ?1",
                [&created.grant.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(material_after, material_before);
        assert_eq!(
            copy_plaintext(&connection, &root_key, &created.grant.id).unwrap(),
            created.plaintext
        );

        connection
            .execute_batch(&format!(
                "INSERT INTO ai_gateway_request_logs
                    (id, request_id, started_at, local_date, timezone_name, endpoint,
                     public_model_id, api_key_id, api_key_id_snapshot, status, total_tokens,
                     estimated_cost_usd, cost_calculable)
                 VALUES
                    ('usage-today', 'usage-request-today', '2026-08-09T01:00:00Z', '2026-08-09',
                     'UTC', 'responses', 'gpt-5.6-sol', '{0}', '{0}', 'succeeded', 10, '1.25', 1),
                    ('usage-prior', 'usage-request-prior', '2026-07-20T01:00:00Z', '2026-07-20',
                     'UTC', 'responses', 'gpt-5.6-sol', '{0}', '{0}', 'succeeded', 20, NULL, 0);",
                created.grant.id
            ))
            .unwrap();
        let now = DateTime::parse_from_rfc3339("2026-08-09T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let page = list(
            &connection,
            &GatewayKeyListFilter {
                display_group_id: &custom.id,
                text: Some(&material_before.2),
                status: GatewayKeyStatusFilter::Active,
                page: 1,
                page_size: 20,
                sort: GatewayKeySort::NameAscending,
            },
            now,
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
        )
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].name, "CLI Edited");
        assert_eq!(page.items[0].status, GatewayKeyStatus::Active);
        assert_eq!(page.items[0].today.total_tokens, 10);
        assert_eq!(
            page.items[0].today.estimated_cost_usd.as_deref(),
            Some("1.25")
        );
        assert!(page.items[0].today.cost_calculable);
        assert_eq!(page.items[0].last_30_days.total_tokens, 30);
        assert_eq!(page.items[0].last_30_days.estimated_cost_usd, None);
        assert!(!page.items[0].last_30_days.cost_calculable);

        set_enabled(&connection, &created.grant.id, false).unwrap();
        let disabled = list(
            &connection,
            &GatewayKeyListFilter {
                display_group_id: &custom.id,
                text: Some("Edited"),
                status: GatewayKeyStatusFilter::Disabled,
                page: 1,
                page_size: 20,
                sort: GatewayKeySort::NameDescending,
            },
            now,
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
        )
        .unwrap();
        assert_eq!(disabled.items.len(), 1);
        assert_eq!(disabled.items[0].status, GatewayKeyStatus::Disabled);
        set_enabled(&connection, &created.grant.id, true).unwrap();

        connection
            .execute(
                "UPDATE ai_gateway_api_keys SET expires_at = '2026-08-09T12:00:00Z', enabled = 1 WHERE id = ?1",
                [&created.grant.id],
            )
            .unwrap();
        let expired = list(
            &connection,
            &GatewayKeyListFilter {
                display_group_id: &custom.id,
                text: Some("CLI"),
                status: GatewayKeyStatusFilter::Expired,
                page: 1,
                page_size: 20,
                sort: GatewayKeySort::CreatedNewest,
            },
            now,
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
        )
        .unwrap();
        assert_eq!(expired.items[0].status, GatewayKeyStatus::Expired);
        connection
            .execute(
                "UPDATE ai_gateway_api_keys SET expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
                [&created.grant.id],
            )
            .unwrap();
        assert_eq!(
            set_enabled(&connection, &created.grant.id, true)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::Conflict
        );

        connection
            .execute(
                "UPDATE ai_gateway_api_keys SET expires_at = NULL WHERE id = ?1",
                [&created.grant.id],
            )
            .unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_custom_group_delete BEFORE DELETE ON ai_gateway_key_display_groups
                 WHEN OLD.id <> 'gateway-key-default'
                 BEGIN SELECT RAISE(ABORT, 'test_group_delete_failure'); END;",
            )
            .unwrap();
        assert!(key_display_group::delete(&mut connection, &custom.id).is_err());
        let unchanged_group: String = connection
            .query_row(
                "SELECT display_group_id FROM ai_gateway_api_keys WHERE id = ?1",
                [&created.grant.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unchanged_group, custom.id);
        connection
            .execute("DROP TRIGGER fail_custom_group_delete", [])
            .unwrap();
        key_display_group::delete(&mut connection, &custom.id).unwrap();
        let migrated_group: String = connection
            .query_row(
                "SELECT display_group_id FROM ai_gateway_api_keys WHERE id = ?1",
                [&created.grant.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migrated_group, DEFAULT_DISPLAY_GROUP_ID);

        revoke(&connection, &created.grant.id).unwrap();
        assert!(authenticate(&connection, &created.plaintext).is_err());
        let hidden = list(
            &connection,
            &GatewayKeyListFilter {
                display_group_id: DEFAULT_DISPLAY_GROUP_ID,
                text: None,
                status: GatewayKeyStatusFilter::All,
                page: 1,
                page_size: 20,
                sort: GatewayKeySort::CreatedOldest,
            },
            now,
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
        )
        .unwrap();
        assert!(hidden.items.is_empty());
        assert_eq!(
            regenerate(&mut connection, &root_key, &created.grant.id)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::Conflict
        );

        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }
}
