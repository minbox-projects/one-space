use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use pbkdf2::pbkdf2_hmac;
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sha2::Sha256;

use super::{
    error::{GatewayError, GatewayErrorCategory},
    security::{decrypt_credential, encrypt_credential, EncryptedCredential, RootKey},
};

const KEY_BYTES: usize = 32;
const SALT_BYTES: usize = 16;
const HASH_BYTES: usize = 32;
const HASH_ROUNDS: u32 = 120_000;
const LOOKUP_PREFIX_LENGTH: usize = 12;
const DISPLAY_PART_LENGTH: usize = 6;
const RECORD_TYPE: &str = "gateway_api_key";

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

pub(crate) fn create(
    connection: &mut Connection,
    root_key: &RootKey,
    name: &str,
    group_ids: &[String],
    model_ids: &[String],
    expires_at: Option<&str>,
) -> Result<CreatedGatewayKey, GatewayError> {
    if name.trim().is_empty() || group_ids.is_empty() || model_ids.is_empty() {
        return Err(error(GatewayErrorCategory::InvalidInput, None));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let (plaintext, prefix, suffix, salt, hash) = generate_material();
    let encrypted = encrypt_credential(root_key, RECORD_TYPE, &id, plaintext.as_bytes())?;
    let transaction = connection
        .transaction()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(&id)))?;
    transaction
        .execute(
            "INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_suffix, key_hash, hash_salt, expires_at, ciphertext, nonce, cipher_version) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, name.trim(), prefix, suffix, hash.as_slice(), salt.as_slice(), expires_at, encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version],
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

pub(crate) fn regenerate(
    connection: &mut Connection,
    root_key: &RootKey,
    key_id: &str,
) -> Result<CreatedGatewayKey, GatewayError> {
    let (plaintext, prefix, suffix, salt, hash) = generate_material();
    let encrypted = encrypt_credential(root_key, RECORD_TYPE, key_id, plaintext.as_bytes())?;
    let transaction = connection
        .transaction()
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    let changed = transaction
        .execute(
            "UPDATE ai_gateway_api_keys SET key_prefix = ?2, key_suffix = ?3, key_hash = ?4, hash_salt = ?5, ciphertext = ?6, nonce = ?7, cipher_version = ?8, enabled = 1, revoked_at = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
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
    update_status(
        connection,
        key_id,
        "UPDATE ai_gateway_api_keys SET enabled = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        enabled,
    )
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

fn update_status(
    connection: &Connection,
    key_id: &str,
    sql: &str,
    value: bool,
) -> Result<(), GatewayError> {
    let changed = connection
        .execute(sql, params![key_id, value])
        .map_err(|_| error(GatewayErrorCategory::StorageUnavailable, Some(key_id)))?;
    if changed == 0 {
        Err(error(GatewayErrorCategory::NotFound, Some(key_id)))
    } else {
        Ok(())
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
            copy_plaintext(&connection, &key, &created.grant.id).unwrap(),
            regenerated.plaintext
        );
        drop(connection);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }
}
