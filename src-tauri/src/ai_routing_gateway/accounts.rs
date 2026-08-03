use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use super::{
    error::{GatewayError, GatewayErrorCategory},
    security::{decrypt_credential, encrypt_credential, EncryptedCredential, RootKey},
    types::{AccountDto, AccountType, GroupDto, ModelMappingDto, UpstreamProtocol},
};

const API_KEY_RECORD_TYPE: &str = "third_party_api_key";
const OAUTH_RECORD_TYPE: &str = "oauth_token_bundle";
const OAUTH_REAUTH_REQUIRED_REASON: &str = "oauth_reauthorization_required";
const DELETE_CONFIRMATION_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug)]
pub(crate) struct CreateApiKeyAccount<'a> {
    pub(crate) name: &'a str,
    pub(crate) base_url: &'a str,
    pub(crate) api_key: &'a str,
    pub(crate) auth_method: &'a str,
    pub(crate) upstream_protocol: UpstreamProtocol,
    pub(crate) note: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OAuthTokenBundle {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires_at: Option<String>,
    pub(crate) token_type: String,
    pub(crate) scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OAuthCredentialPayload {
    token_bundle: OAuthTokenBundle,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OAuthRefreshMaterial {
    pub(crate) token_bundle: OAuthTokenBundle,
    pub(crate) token_endpoint: Option<String>,
    pub(crate) client_id: Option<String>,
    pub(crate) client_secret: Option<String>,
}

#[derive(Debug)]
pub(crate) struct UpsertOAuthAccount<'a> {
    pub(crate) stable_external_id: &'a str,
    pub(crate) name: &'a str,
    pub(crate) token_bundle: &'a OAuthTokenBundle,
    pub(crate) metadata_json: &'a str,
}

#[derive(Debug)]
pub(crate) struct UpdateAccount<'a> {
    pub(crate) name: &'a str,
    pub(crate) group_id: &'a str,
    pub(crate) sort_order: i64,
    pub(crate) note: &'a str,
    pub(crate) enabled: bool,
    pub(crate) quota_threshold_override_percent: Option<u8>,
}

#[derive(Debug)]
pub(crate) struct UpdateApiKeyConnection<'a> {
    pub(crate) base_url: &'a str,
    pub(crate) api_key: Option<&'a str>,
    pub(crate) auth_method: &'a str,
    pub(crate) upstream_protocol: UpstreamProtocol,
}

#[derive(Debug, Default)]
pub(crate) struct DeleteConfirmationStore {
    tokens: Mutex<HashMap<String, DeleteConfirmation>>,
}

#[derive(Debug)]
struct DeleteConfirmation {
    account_id: String,
    expires_at: Instant,
}

impl DeleteConfirmationStore {
    pub(crate) fn issue(&self, account_id: &str) -> Result<String, GatewayError> {
        validate_non_empty(account_id, Some(account_id))?;
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let token =
            base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes);
        let confirmation = DeleteConfirmation {
            account_id: account_id.to_owned(),
            expires_at: Instant::now() + DELETE_CONFIRMATION_TTL,
        };
        self.tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(token.clone(), confirmation);
        Ok(token)
    }

    fn consume(&self, account_id: &str, token: &str) -> bool {
        let mut tokens = self
            .tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        tokens.retain(|_, confirmation| confirmation.expires_at > Instant::now());
        matches!(tokens.remove(token), Some(confirmation) if confirmation.account_id == account_id)
    }
}

pub(crate) fn create_group(
    connection: &Connection,
    name: &str,
    sort_order: i64,
) -> Result<GroupDto, GatewayError> {
    validate_non_empty(name, None)?;
    let id = uuid::Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO ai_gateway_groups (id, name, sort_order, is_default) VALUES (?1, ?2, ?3, 0)",
            params![id, name.trim(), sort_order],
        )
        .map_err(|_| domain_error(GatewayErrorCategory::Conflict, Some(&id)))?;
    Ok(GroupDto {
        id,
        name: name.trim().to_owned(),
        sort_order,
        is_default: false,
    })
}

pub(crate) fn delete_group(
    connection: &mut Connection,
    group_id: &str,
) -> Result<(), GatewayError> {
    let transaction = connection
        .transaction()
        .map_err(|_| storage_error(Some(group_id)))?;
    let is_default: Option<bool> = transaction
        .query_row(
            "SELECT is_default FROM ai_gateway_groups WHERE id = ?1",
            [group_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| storage_error(Some(group_id)))?;
    match is_default {
        None => return Err(domain_error(GatewayErrorCategory::NotFound, Some(group_id))),
        Some(true) => return Err(domain_error(GatewayErrorCategory::Conflict, Some(group_id))),
        Some(false) => {}
    }
    let default_group = default_group_id(&transaction)?;
    transaction
        .execute(
            "UPDATE ai_gateway_accounts SET group_id = ?1, updated_at = CURRENT_TIMESTAMP WHERE group_id = ?2",
            params![default_group, group_id],
        )
        .map_err(|_| storage_error(Some(group_id)))?;
    transaction
        .execute("DELETE FROM ai_gateway_groups WHERE id = ?1", [group_id])
        .map_err(|_| storage_error(Some(group_id)))?;
    transaction
        .commit()
        .map_err(|_| storage_error(Some(group_id)))
}

pub(crate) fn create_api_key_account(
    connection: &mut Connection,
    root_key: &RootKey,
    input: CreateApiKeyAccount<'_>,
) -> Result<AccountDto, GatewayError> {
    validate_non_empty(input.name, None)?;
    validate_non_empty(input.api_key, None)?;
    validate_base_url(input.base_url)?;
    if !matches!(input.auth_method, "bearer" | "api_key_header") {
        return Err(domain_error(GatewayErrorCategory::InvalidInput, None));
    }
    let account_id = uuid::Uuid::new_v4().to_string();
    // 明文只在这里存在；进入事务前即转换为带账号 AAD 的密文。
    let encrypted = encrypt_credential(
        root_key,
        API_KEY_RECORD_TYPE,
        &account_id,
        input.api_key.as_bytes(),
    )?;
    let transaction = connection
        .transaction()
        .map_err(|_| storage_error(Some(&account_id)))?;
    let group_id = default_group_id(&transaction)?;
    transaction
        .execute(
            "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, note, base_url, auth_method, upstream_protocol) VALUES (?1, 'api_key', ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                account_id,
                input.name.trim(),
                group_id,
                input.note,
                input.base_url,
                input.auth_method,
                input.upstream_protocol.as_str()
            ],
        )
        .map_err(|_| storage_error(Some(&account_id)))?;
    insert_credential(
        &transaction,
        &account_id,
        API_KEY_RECORD_TYPE,
        &encrypted,
        None,
    )?;
    transaction
        .execute(
            "INSERT INTO ai_gateway_account_model_mappings (account_id, public_model_id, upstream_model_id, enabled) SELECT ?1, id, id, 1 FROM ai_gateway_models WHERE source = 'official' ON CONFLICT(account_id, public_model_id) DO NOTHING",
            [&account_id],
        )
        .map_err(|_| storage_error(Some(&account_id)))?;
    transaction
        .commit()
        .map_err(|_| storage_error(Some(&account_id)))?;
    get_account(connection, &account_id)
}

pub(crate) fn upsert_oauth_account(
    connection: &mut Connection,
    root_key: &RootKey,
    input: UpsertOAuthAccount<'_>,
) -> Result<AccountDto, GatewayError> {
    validate_non_empty(input.stable_external_id, None)?;
    validate_non_empty(input.name, None)?;
    validate_token_bundle(input.token_bundle)?;
    let (metadata_json, client_id, client_secret) = prepare_oauth_metadata(input.metadata_json)?;

    let existing_id: Option<String> = connection
        .query_row(
            "SELECT id FROM ai_gateway_accounts WHERE account_type = 'oauth' AND stable_external_id = ?1",
            [input.stable_external_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| storage_error(None))?;
    let account_id = existing_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let plaintext = serde_json::to_vec(&OAuthCredentialPayload {
        token_bundle: input.token_bundle.clone(),
        client_id,
        client_secret,
    })
    .map_err(|_| domain_error(GatewayErrorCategory::InvalidInput, Some(&account_id)))?;
    let encrypted = encrypt_credential(root_key, OAUTH_RECORD_TYPE, &account_id, &plaintext)?;

    let transaction = connection
        .transaction()
        .map_err(|_| storage_error(Some(&account_id)))?;
    if transaction
        .query_row(
            "SELECT 1 FROM ai_gateway_accounts WHERE id = ?1",
            [&account_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|_| storage_error(Some(&account_id)))?
        .is_some()
    {
        transaction
            .execute(
                "UPDATE ai_gateway_accounts SET name = ?2, health_status = 'unknown', health_reason_code = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
                params![account_id, input.name.trim()],
            )
            .map_err(|_| storage_error(Some(&account_id)))?;
    } else {
        let group_id = default_group_id(&transaction)?;
        transaction
            .execute(
                "INSERT INTO ai_gateway_accounts (id, stable_external_id, account_type, name, group_id) VALUES (?1, ?2, 'oauth', ?3, ?4)",
                params![account_id, input.stable_external_id, input.name.trim(), group_id],
            )
            .map_err(|_| storage_error(Some(&account_id)))?;
    }
    transaction
        .execute(
            "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version, metadata_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(account_id) DO UPDATE SET record_type = excluded.record_type, ciphertext = excluded.ciphertext, nonce = excluded.nonce, cipher_version = excluded.cipher_version, metadata_json = excluded.metadata_json, updated_at = CURRENT_TIMESTAMP",
            params![account_id, OAUTH_RECORD_TYPE, encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version, metadata_json],
        )
        .map_err(|_| storage_error(Some(&account_id)))?;
    transaction
        .commit()
        .map_err(|_| storage_error(Some(&account_id)))?;
    get_account(connection, &account_id)
}

pub(crate) fn update_account(
    connection: &Connection,
    account_id: &str,
    input: UpdateAccount<'_>,
) -> Result<AccountDto, GatewayError> {
    validate_non_empty(input.name, Some(account_id))?;
    validate_non_empty(input.group_id, Some(account_id))?;
    if input
        .quota_threshold_override_percent
        .is_some_and(|threshold| threshold > 100)
    {
        return Err(domain_error(
            GatewayErrorCategory::InvalidInput,
            Some(account_id),
        ));
    }
    let changed = connection
        .execute(
            "UPDATE ai_gateway_accounts SET name = ?2, group_id = ?3, sort_order = ?4, note = ?5, enabled = ?6, quota_threshold_override_percent = ?7, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![account_id, input.name.trim(), input.group_id, input.sort_order, input.note, input.enabled, input.quota_threshold_override_percent],
        )
        .map_err(|_| storage_error(Some(account_id)))?;
    if changed == 0 {
        return Err(domain_error(
            GatewayErrorCategory::NotFound,
            Some(account_id),
        ));
    }
    get_account(connection, account_id)
}

pub(crate) fn update_api_key_connection(
    transaction: &Transaction<'_>,
    root_key: &RootKey,
    account_id: &str,
    input: UpdateApiKeyConnection<'_>,
) -> Result<(), GatewayError> {
    validate_base_url(input.base_url)?;
    if !matches!(input.auth_method, "bearer" | "api_key_header") {
        return Err(domain_error(
            GatewayErrorCategory::InvalidInput,
            Some(account_id),
        ));
    }
    let account_type: Option<String> = transaction
        .query_row(
            "SELECT account_type FROM ai_gateway_accounts WHERE id = ?1",
            [account_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| storage_error(Some(account_id)))?;
    match account_type.as_deref() {
        None => {
            return Err(domain_error(
                GatewayErrorCategory::NotFound,
                Some(account_id),
            ))
        }
        Some("api_key") => {}
        Some(_) => {
            return Err(domain_error(
                GatewayErrorCategory::InvalidInput,
                Some(account_id),
            ))
        }
    }
    transaction
        .execute(
            "UPDATE ai_gateway_accounts SET base_url = ?2, auth_method = ?3, upstream_protocol = ?4, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![account_id, input.base_url, input.auth_method, input.upstream_protocol.as_str()],
        )
        .map_err(|_| storage_error(Some(account_id)))?;
    if let Some(api_key) = input.api_key.filter(|value| !value.is_empty()) {
        validate_non_empty(api_key, Some(account_id))?;
        let encrypted = encrypt_credential(
            root_key,
            API_KEY_RECORD_TYPE,
            account_id,
            api_key.as_bytes(),
        )?;
        transaction
            .execute(
                "UPDATE ai_gateway_credentials SET record_type = ?2, ciphertext = ?3, nonce = ?4, cipher_version = ?5, updated_at = CURRENT_TIMESTAMP WHERE account_id = ?1",
                params![account_id, API_KEY_RECORD_TYPE, encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version],
            )
            .map_err(|_| storage_error(Some(account_id)))?;
    }
    Ok(())
}

pub(crate) fn move_account(
    connection: &mut Connection,
    account_id: &str,
    direction: i8,
) -> Result<AccountDto, GatewayError> {
    if !matches!(direction, -1 | 1) {
        return Err(domain_error(
            GatewayErrorCategory::InvalidInput,
            Some(account_id),
        ));
    }
    let transaction = connection
        .transaction()
        .map_err(|_| storage_error(Some(account_id)))?;
    let group_id: String = transaction
        .query_row(
            "SELECT group_id FROM ai_gateway_accounts WHERE id = ?1",
            [account_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| storage_error(Some(account_id)))?
        .ok_or_else(|| domain_error(GatewayErrorCategory::NotFound, Some(account_id)))?;
    let mut ids = {
        let mut statement = transaction
            .prepare(
                "SELECT id FROM ai_gateway_accounts WHERE group_id = ?1 ORDER BY sort_order, id",
            )
            .map_err(|_| storage_error(Some(account_id)))?;
        let result = statement
            .query_map([&group_id], |row| row.get::<_, String>(0))
            .map_err(|_| storage_error(Some(account_id)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| storage_error(Some(account_id)))?;
        result
    };
    let position = ids
        .iter()
        .position(|id| id == account_id)
        .ok_or_else(|| domain_error(GatewayErrorCategory::NotFound, Some(account_id)))?;
    let target = if direction < 0 {
        position.checked_sub(1)
    } else {
        (position + 1 < ids.len()).then_some(position + 1)
    };
    if let Some(target) = target {
        ids.swap(position, target);
    }

    // 先写入临时负序号，再写入连续序号，避免交换过程产生中间冲突。
    for (index, id) in ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE ai_gateway_accounts SET sort_order = ?1 WHERE id = ?2",
                params![-(index as i64) - 1, id],
            )
            .map_err(|_| storage_error(Some(account_id)))?;
    }
    for (index, id) in ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE ai_gateway_accounts SET sort_order = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![index as i64, id],
            )
            .map_err(|_| storage_error(Some(account_id)))?;
    }
    transaction
        .commit()
        .map_err(|_| storage_error(Some(account_id)))?;
    get_account(connection, account_id)
}

pub(crate) fn update_account_health(
    connection: &Connection,
    account_id: &str,
    health_status: &str,
    reason_code: Option<&str>,
) -> Result<(), GatewayError> {
    if !matches!(
        health_status,
        "unknown" | "healthy" | "degraded" | "unavailable" | "authorization_invalid"
    ) {
        return Err(domain_error(
            GatewayErrorCategory::InvalidInput,
            Some(account_id),
        ));
    }
    let changed = connection
        .execute(
            "UPDATE ai_gateway_accounts SET health_status = ?2, health_reason_code = ?3, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![account_id, health_status, reason_code],
        )
        .map_err(|_| storage_error(Some(account_id)))?;
    if changed == 0 {
        return Err(domain_error(
            GatewayErrorCategory::NotFound,
            Some(account_id),
        ));
    }
    Ok(())
}

pub(crate) fn decrypt_api_key(
    connection: &Connection,
    root_key: &RootKey,
    account_id: &str,
) -> Result<Vec<u8>, GatewayError> {
    let encrypted = read_credential(connection, account_id, API_KEY_RECORD_TYPE)?;
    decrypt_credential(root_key, API_KEY_RECORD_TYPE, account_id, &encrypted)
}

pub(crate) fn decrypt_oauth_tokens(
    connection: &Connection,
    root_key: &RootKey,
    account_id: &str,
) -> Result<OAuthTokenBundle, GatewayError> {
    Ok(decrypt_oauth_payload(connection, root_key, account_id)?.token_bundle)
}

pub(crate) fn load_oauth_refresh_material(
    connection: &Connection,
    root_key: &RootKey,
    account_id: &str,
) -> Result<OAuthRefreshMaterial, GatewayError> {
    let requires_reauthorization: bool = connection
        .query_row(
            "SELECT COALESCE(health_reason_code = ?2, 0) FROM ai_gateway_accounts WHERE id = ?1",
            params![account_id, OAUTH_REAUTH_REQUIRED_REASON],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| storage_error(Some(account_id)))?
        .unwrap_or(false);
    if requires_reauthorization {
        return Err(domain_error(
            GatewayErrorCategory::OAuthReauthorizationRequired,
            Some(account_id),
        ));
    }
    let payload = decrypt_oauth_payload(connection, root_key, account_id)?;
    let metadata: Option<String> = connection
        .query_row(
            "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = ?1",
            [account_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| storage_error(Some(account_id)))?;
    let metadata = load_public_oauth_metadata(metadata.as_deref(), account_id)?;
    let object = metadata.as_ref().and_then(serde_json::Value::as_object);
    Ok(OAuthRefreshMaterial {
        token_bundle: payload.token_bundle,
        token_endpoint: object
            .and_then(|value| value.get("token_endpoint"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        client_id: payload.client_id,
        client_secret: payload.client_secret,
    })
}

pub(crate) fn replace_oauth_tokens(
    connection: &mut Connection,
    root_key: &RootKey,
    account_id: &str,
    token_bundle: &OAuthTokenBundle,
) -> Result<(), GatewayError> {
    validate_token_bundle(token_bundle)?;
    let existing = decrypt_oauth_payload(connection, root_key, account_id)?;
    let plaintext = serde_json::to_vec(&OAuthCredentialPayload {
        token_bundle: token_bundle.clone(),
        client_id: existing.client_id,
        client_secret: existing.client_secret,
    })
    .map_err(|_| domain_error(GatewayErrorCategory::CredentialInvalid, Some(account_id)))?;
    let encrypted = encrypt_credential(root_key, OAUTH_RECORD_TYPE, account_id, &plaintext)?;
    let transaction = connection
        .transaction()
        .map_err(|_| storage_error(Some(account_id)))?;
    let changed = transaction
        .execute(
            "UPDATE ai_gateway_credentials SET record_type = ?2, ciphertext = ?3, nonce = ?4, cipher_version = ?5, updated_at = CURRENT_TIMESTAMP WHERE account_id = ?1",
            params![account_id, OAUTH_RECORD_TYPE, encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version],
        )
        .map_err(|_| storage_error(Some(account_id)))?;
    if changed == 0 {
        return Err(domain_error(
            GatewayErrorCategory::CredentialMissing,
            Some(account_id),
        ));
    }
    transaction
        .execute(
            "UPDATE ai_gateway_accounts SET health_status = 'unknown', health_reason_code = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            [account_id],
        )
        .map_err(|_| storage_error(Some(account_id)))?;
    transaction
        .commit()
        .map_err(|_| storage_error(Some(account_id)))
}

pub(crate) fn permanent_delete_account(
    connection: &mut Connection,
    confirmations: &DeleteConfirmationStore,
    account_id: &str,
    confirmation_token: &str,
) -> Result<(), GatewayError> {
    if !confirmations.consume(account_id, confirmation_token) {
        return Err(domain_error(
            GatewayErrorCategory::ConfirmationRequired,
            Some(account_id),
        ));
    }
    let transaction = connection
        .transaction()
        .map_err(|_| storage_error(Some(account_id)))?;
    let deleted = transaction
        .execute(
            "DELETE FROM ai_gateway_accounts WHERE id = ?1",
            [account_id],
        )
        .map_err(|_| storage_error(Some(account_id)))?;
    if deleted == 0 {
        return Err(domain_error(
            GatewayErrorCategory::NotFound,
            Some(account_id),
        ));
    }
    transaction
        .commit()
        .map_err(|_| storage_error(Some(account_id)))
}

pub(crate) fn replace_account_tags(
    connection: &mut Connection,
    account_id: &str,
    tag_names: &[String],
) -> Result<(), GatewayError> {
    let transaction = connection
        .transaction()
        .map_err(|_| storage_error(Some(account_id)))?;
    replace_account_tags_in_transaction(&transaction, account_id, tag_names)?;
    transaction
        .commit()
        .map_err(|_| storage_error(Some(account_id)))
}

pub(crate) fn replace_account_tags_in_transaction(
    transaction: &Transaction<'_>,
    account_id: &str,
    tag_names: &[String],
) -> Result<(), GatewayError> {
    ensure_account_exists(&transaction, account_id)?;
    transaction
        .execute(
            "DELETE FROM ai_gateway_account_tags WHERE account_id = ?1",
            [account_id],
        )
        .map_err(|_| storage_error(Some(account_id)))?;
    for name in tag_names {
        validate_non_empty(name, Some(account_id))?;
        let normalized = name.trim();
        let tag_id = transaction
            .query_row(
                "SELECT id FROM ai_gateway_tags WHERE name = ?1",
                [normalized],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| storage_error(Some(account_id)))?
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        transaction
            .execute(
                "INSERT OR IGNORE INTO ai_gateway_tags (id, name) VALUES (?1, ?2)",
                params![tag_id, normalized],
            )
            .map_err(|_| storage_error(Some(account_id)))?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO ai_gateway_account_tags (account_id, tag_id) VALUES (?1, ?2)",
                params![account_id, tag_id],
            )
            .map_err(|_| storage_error(Some(account_id)))?;
    }
    Ok(())
}

pub(crate) fn upsert_public_model(
    connection: &Connection,
    model_id: &str,
    display_name: &str,
    capabilities_json: &str,
) -> Result<(), GatewayError> {
    validate_non_empty(model_id, None)?;
    validate_non_empty(display_name, Some(model_id))?;
    serde_json::from_str::<serde_json::Value>(capabilities_json)
        .map_err(|_| domain_error(GatewayErrorCategory::InvalidInput, Some(model_id)))?;
    connection
        .execute(
            "INSERT INTO ai_gateway_models (id, display_name, source, capabilities_json) VALUES (?1, ?2, 'official', ?3) ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, capabilities_json = excluded.capabilities_json, updated_at = CURRENT_TIMESTAMP",
            params![model_id, display_name.trim(), capabilities_json],
        )
        .map_err(|_| storage_error(Some(model_id)))?;
    Ok(())
}

pub(crate) fn set_model_mapping(
    connection: &Connection,
    mapping: &ModelMappingDto,
) -> Result<(), GatewayError> {
    validate_non_empty(&mapping.upstream_model_id, Some(&mapping.account_id))?;
    connection
        .execute(
            "INSERT INTO ai_gateway_account_model_mappings (account_id, public_model_id, upstream_model_id, enabled) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(account_id, public_model_id) DO UPDATE SET upstream_model_id = excluded.upstream_model_id, enabled = excluded.enabled, updated_at = CURRENT_TIMESTAMP",
            params![mapping.account_id, mapping.public_model_id, mapping.upstream_model_id, mapping.enabled],
        )
        .map_err(|_| storage_error(Some(&mapping.account_id)))?;
    Ok(())
}

pub(crate) fn resolve_upstream_model(
    connection: &Connection,
    account_id: &str,
    public_model_id: &str,
) -> Result<Option<String>, GatewayError> {
    connection
        .query_row(
            "SELECT mapping.upstream_model_id FROM ai_gateway_account_model_mappings mapping JOIN ai_gateway_models model ON model.id = mapping.public_model_id WHERE mapping.account_id = ?1 AND mapping.public_model_id = ?2 AND mapping.enabled = 1 AND model.enabled = 1",
            params![account_id, public_model_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| storage_error(Some(account_id)))
}

pub(crate) fn get_account(
    connection: &Connection,
    account_id: &str,
) -> Result<AccountDto, GatewayError> {
    let mut account = connection
        .query_row(
            "SELECT id, stable_external_id, account_type, name, group_id, sort_order, note, enabled, health_status, quota_threshold_override_percent, base_url, auth_method, upstream_protocol FROM ai_gateway_accounts WHERE id = ?1",
            [account_id],
            account_from_row,
        )
        .optional()
        .map_err(|_| storage_error(Some(account_id)))?
        .ok_or_else(|| domain_error(GatewayErrorCategory::NotFound, Some(account_id)))?;
    let mut statement = connection
        .prepare("SELECT tag.name FROM ai_gateway_tags tag JOIN ai_gateway_account_tags link ON link.tag_id = tag.id WHERE link.account_id = ?1 ORDER BY tag.name")
        .map_err(|_| storage_error(Some(account_id)))?;
    account.tags = statement
        .query_map([account_id], |row| row.get(0))
        .map_err(|_| storage_error(Some(account_id)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| storage_error(Some(account_id)))?;
    let mut statement = connection
        .prepare("SELECT ?1, model.id, COALESCE(mapping.upstream_model_id, model.id), COALESCE(mapping.enabled, 1) FROM ai_gateway_models model LEFT JOIN ai_gateway_account_model_mappings mapping ON mapping.account_id = ?1 AND mapping.public_model_id = model.id WHERE model.source = 'official' ORDER BY model.id")
        .map_err(|_| storage_error(Some(account_id)))?;
    account.model_mappings = statement
        .query_map([account_id], |row| {
            Ok(ModelMappingDto {
                account_id: row.get(0)?,
                public_model_id: row.get(1)?,
                upstream_model_id: row.get(2)?,
                enabled: row.get(3)?,
            })
        })
        .map_err(|_| storage_error(Some(account_id)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| storage_error(Some(account_id)))?;
    Ok(account)
}

fn account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AccountDto> {
    let account_type: String = row.get(2)?;
    let upstream_protocol: Option<String> = row.get(12)?;
    Ok(AccountDto {
        id: row.get(0)?,
        stable_external_id: row.get(1)?,
        account_type: if account_type == "oauth" {
            AccountType::OAuth
        } else {
            AccountType::ApiKey
        },
        name: row.get(3)?,
        group_id: row.get(4)?,
        sort_order: row.get(5)?,
        note: row.get(6)?,
        enabled: row.get(7)?,
        health_status: row.get(8)?,
        quota_threshold_override_percent: row.get(9)?,
        base_url: row.get(10)?,
        auth_method: row.get(11)?,
        upstream_protocol: upstream_protocol.map(|value| {
            if value == "responses" {
                UpstreamProtocol::Responses
            } else {
                UpstreamProtocol::ChatCompletions
            }
        }),
        tags: Vec::new(),
        model_mappings: Vec::new(),
    })
}

fn insert_credential(
    transaction: &Transaction<'_>,
    account_id: &str,
    record_type: &str,
    credential: &EncryptedCredential,
    metadata_json: Option<&str>,
) -> Result<(), GatewayError> {
    transaction
        .execute(
            "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version, metadata_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![account_id, record_type, credential.ciphertext, credential.nonce.as_slice(), credential.cipher_version, metadata_json],
        )
        .map_err(|_| storage_error(Some(account_id)))?;
    Ok(())
}

fn read_credential(
    connection: &Connection,
    account_id: &str,
    expected_record_type: &str,
) -> Result<EncryptedCredential, GatewayError> {
    let (record_type, ciphertext, nonce, cipher_version): (String, Vec<u8>, Vec<u8>, i64) =
        connection
            .query_row(
                "SELECT record_type, ciphertext, nonce, cipher_version FROM ai_gateway_credentials WHERE account_id = ?1",
                [account_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|_| storage_error(Some(account_id)))?
            .ok_or_else(|| domain_error(GatewayErrorCategory::CredentialMissing, Some(account_id)))?;
    if record_type != expected_record_type {
        return Err(domain_error(
            GatewayErrorCategory::CredentialInvalid,
            Some(account_id),
        ));
    }
    let nonce = nonce
        .try_into()
        .map_err(|_| domain_error(GatewayErrorCategory::CredentialInvalid, Some(account_id)))?;
    Ok(EncryptedCredential {
        ciphertext,
        nonce,
        cipher_version,
    })
}

fn decrypt_oauth_payload(
    connection: &Connection,
    root_key: &RootKey,
    account_id: &str,
) -> Result<OAuthCredentialPayload, GatewayError> {
    let encrypted = read_credential(connection, account_id, OAUTH_RECORD_TYPE)?;
    let plaintext = decrypt_credential(root_key, OAUTH_RECORD_TYPE, account_id, &encrypted)?;
    if let Ok(payload) = serde_json::from_slice::<OAuthCredentialPayload>(&plaintext) {
        validate_token_bundle(&payload.token_bundle)?;
        return Ok(payload);
    }
    let token_bundle: OAuthTokenBundle = serde_json::from_slice(&plaintext)
        .map_err(|_| domain_error(GatewayErrorCategory::CredentialInvalid, Some(account_id)))?;
    validate_token_bundle(&token_bundle)?;
    Ok(OAuthCredentialPayload {
        token_bundle,
        client_id: None,
        client_secret: None,
    })
}

fn prepare_oauth_metadata(
    metadata_json: &str,
) -> Result<(String, Option<String>, Option<String>), GatewayError> {
    let mut metadata = serde_json::from_str::<serde_json::Value>(metadata_json)
        .map_err(|_| domain_error(GatewayErrorCategory::InvalidInput, None))?;
    let object = metadata
        .as_object_mut()
        .ok_or_else(|| domain_error(GatewayErrorCategory::InvalidInput, None))?;
    let client_id = take_private_metadata(object, "client_id")?;
    let client_secret = take_private_metadata(object, "client_secret")?;
    reject_sensitive_metadata(&metadata)?;
    let metadata_json = serde_json::to_string(&metadata)
        .map_err(|_| domain_error(GatewayErrorCategory::InvalidInput, None))?;
    Ok((metadata_json, client_id, client_secret))
}

fn take_private_metadata(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, GatewayError> {
    let Some(value) = object.remove(key) else {
        return Ok(None);
    };
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) if !value.is_empty() => Ok(Some(value)),
        _ => Err(domain_error(GatewayErrorCategory::InvalidInput, None)),
    }
}

fn load_public_oauth_metadata(
    metadata_json: Option<&str>,
    account_id: &str,
) -> Result<Option<serde_json::Value>, GatewayError> {
    let Some(metadata_json) = metadata_json else {
        return Ok(None);
    };
    let metadata = serde_json::from_str::<serde_json::Value>(metadata_json)
        .map_err(|_| domain_error(GatewayErrorCategory::CredentialInvalid, Some(account_id)))?;
    reject_sensitive_metadata(&metadata)
        .map_err(|_| domain_error(GatewayErrorCategory::CredentialInvalid, Some(account_id)))?;
    Ok(Some(metadata))
}

fn reject_sensitive_metadata(value: &serde_json::Value) -> Result<(), GatewayError> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "access_token"
                        | "refresh_token"
                        | "client_id"
                        | "client_secret"
                        | "api_key"
                        | "authorization"
                        | "credential"
                        | "password"
                        | "private_key"
                        | "secret"
                        | "token"
                ) {
                    return Err(domain_error(GatewayErrorCategory::InvalidInput, None));
                }
                reject_sensitive_metadata(value)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reject_sensitive_metadata(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn default_group_id(connection: &Connection) -> Result<String, GatewayError> {
    connection
        .query_row(
            "SELECT id FROM ai_gateway_groups WHERE is_default = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| storage_error(None))
}

fn ensure_account_exists(connection: &Connection, account_id: &str) -> Result<(), GatewayError> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM ai_gateway_accounts WHERE id = ?1",
            [account_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|_| storage_error(Some(account_id)))?;
    exists.ok_or_else(|| domain_error(GatewayErrorCategory::NotFound, Some(account_id)))
}

fn validate_base_url(value: &str) -> Result<(), GatewayError> {
    let parsed = url::Url::parse(value)
        .map_err(|_| domain_error(GatewayErrorCategory::InvalidInput, None))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(domain_error(GatewayErrorCategory::InvalidInput, None));
    }
    Ok(())
}

fn validate_token_bundle(bundle: &OAuthTokenBundle) -> Result<(), GatewayError> {
    validate_non_empty(&bundle.access_token, None)?;
    validate_non_empty(&bundle.refresh_token, None)?;
    validate_non_empty(&bundle.token_type, None)?;
    validate_non_empty(&bundle.scope, None)
}

fn validate_non_empty(value: &str, entity_id: Option<&str>) -> Result<(), GatewayError> {
    if value.trim().is_empty() {
        Err(domain_error(GatewayErrorCategory::InvalidInput, entity_id))
    } else {
        Ok(())
    }
}

fn domain_error(category: GatewayErrorCategory, entity_id: Option<&str>) -> GatewayError {
    GatewayError::new(category, entity_id)
}

fn storage_error(entity_id: Option<&str>) -> GatewayError {
    domain_error(GatewayErrorCategory::StorageUnavailable, entity_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_sqlite;
    use rusqlite::Connection;
    use std::path::{Path, PathBuf};

    fn database(name: &str) -> (PathBuf, Connection) {
        let path = std::env::temp_dir().join(format!(
            "onespace-accounts-{name}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = shared_sqlite::open_at(&path).unwrap();
        (path, connection)
    }

    fn cleanup(path: &Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    fn key() -> RootKey {
        RootKey::try_from(vec![41; 32]).unwrap()
    }

    fn api_input<'a>(secret: &'a str) -> CreateApiKeyAccount<'a> {
        CreateApiKeyAccount {
            name: "Third Party",
            base_url: "http://127.0.0.1:19191/v1",
            api_key: secret,
            auth_method: "bearer",
            upstream_protocol: UpstreamProtocol::Responses,
            note: "local fixture",
        }
    }

    #[test]
    fn group_delete_moves_accounts_atomically_and_default_is_immutable() {
        let (path, mut connection) = database("groups");
        let group = create_group(&connection, "Secondary", 3).unwrap();
        connection.execute(
            "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-1', 'api_key', 'Account', ?1)",
            [&group.id],
        ).unwrap();
        delete_group(&mut connection, &group.id).unwrap();
        let target: String = connection
            .query_row(
                "SELECT group_id FROM ai_gateway_accounts WHERE id = 'account-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(target, "default");
        assert_eq!(
            delete_group(&mut connection, "default")
                .unwrap_err()
                .category(),
            GatewayErrorCategory::Conflict
        );
        drop(connection);
        cleanup(&path);
    }

    #[test]
    fn api_key_is_encrypted_before_storage_and_read_dto_never_contains_plaintext() {
        let (path, mut connection) = database("api-key");
        let secret = "sensitive-third-party-key";
        let account = create_api_key_account(&mut connection, &key(), api_input(secret)).unwrap();
        assert_eq!(account.group_id, "default");
        assert_eq!(account.upstream_protocol, Some(UpstreamProtocol::Responses));
        let (ciphertext, metadata): (Vec<u8>, Option<String>) = connection.query_row(
            "SELECT ciphertext, metadata_json FROM ai_gateway_credentials WHERE account_id = ?1",
            [&account.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert!(!ciphertext
            .windows(secret.len())
            .any(|window| window == secret.as_bytes()));
        assert!(metadata.is_none());
        let serialized =
            serde_json::to_string(&get_account(&connection, &account.id).unwrap()).unwrap();
        assert!(!serialized.contains(secret));
        assert_eq!(
            create_api_key_account(
                &mut connection,
                &key(),
                CreateApiKeyAccount {
                    upstream_protocol: UpstreamProtocol::ChatCompletions,
                    auth_method: "unsupported",
                    ..api_input("another")
                }
            )
            .unwrap_err()
            .category(),
            GatewayErrorCategory::InvalidInput
        );
        drop(connection);
        cleanup(&path);
    }

    #[test]
    fn account_supports_multiple_tags_and_unmapped_models_never_resolve() {
        let (path, mut connection) = database("tags-models");
        let account = create_api_key_account(&mut connection, &key(), api_input("secret")).unwrap();
        replace_account_tags(
            &mut connection,
            &account.id,
            &["work".into(), "priority".into()],
        )
        .unwrap();
        let account = get_account(&connection, &account.id).unwrap();
        assert_eq!(account.tags, vec!["priority", "work"]);
        upsert_public_model(
            &connection,
            "public-model",
            "Public Model",
            r#"{"tools":true}"#,
        )
        .unwrap();
        assert_eq!(
            resolve_upstream_model(&connection, &account.id, "public-model").unwrap(),
            None
        );
        let mapping = ModelMappingDto {
            account_id: account.id.clone(),
            public_model_id: "public-model".into(),
            upstream_model_id: "vendor-model".into(),
            enabled: true,
        };
        set_model_mapping(&connection, &mapping).unwrap();
        assert_eq!(
            resolve_upstream_model(&connection, &account.id, "public-model")
                .unwrap()
                .as_deref(),
            Some("vendor-model")
        );
        set_model_mapping(
            &connection,
            &ModelMappingDto {
                enabled: false,
                ..mapping.clone()
            },
        )
        .unwrap();
        assert_eq!(
            resolve_upstream_model(&connection, &account.id, "public-model").unwrap(),
            None
        );
        set_model_mapping(
            &connection,
            &ModelMappingDto {
                enabled: true,
                ..mapping
            },
        )
        .unwrap();
        connection
            .execute(
                "UPDATE ai_gateway_models SET enabled = 0 WHERE id = 'public-model'",
                [],
            )
            .unwrap();
        assert_eq!(
            resolve_upstream_model(&connection, &account.id, "public-model").unwrap(),
            None
        );
        drop(connection);
        cleanup(&path);
    }

    #[test]
    fn account_updates_group_sort_note_enable_threshold_and_health_without_exposing_secret() {
        let (path, mut connection) = database("account-update");
        let group = create_group(&connection, "Work", 5).unwrap();
        let account =
            create_api_key_account(&mut connection, &key(), api_input("update-secret")).unwrap();
        let updated = update_account(
            &connection,
            &account.id,
            UpdateAccount {
                name: "Renamed",
                group_id: &group.id,
                sort_order: 7,
                note: "updated note",
                enabled: false,
                quota_threshold_override_percent: Some(100),
            },
        )
        .unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.group_id, group.id);
        assert_eq!(updated.sort_order, 7);
        assert_eq!(updated.note, "updated note");
        assert!(!updated.enabled);
        assert_eq!(updated.quota_threshold_override_percent, Some(100));
        update_account_health(&connection, &account.id, "degraded", Some("quota_stale")).unwrap();
        assert_eq!(
            get_account(&connection, &account.id).unwrap().health_status,
            "degraded"
        );
        assert_eq!(
            decrypt_api_key(&connection, &key(), &account.id).unwrap(),
            b"update-secret"
        );
        let transaction = connection.transaction().unwrap();
        update_api_key_connection(
            &transaction,
            &key(),
            &account.id,
            UpdateApiKeyConnection {
                base_url: "https://new.example.com/v1",
                api_key: Some("replacement-secret"),
                auth_method: "api_key_header",
                upstream_protocol: UpstreamProtocol::ChatCompletions,
            },
        )
        .unwrap();
        transaction.commit().unwrap();
        let updated = get_account(&connection, &account.id).unwrap();
        assert_eq!(
            updated.base_url.as_deref(),
            Some("https://new.example.com/v1")
        );
        assert_eq!(updated.auth_method.as_deref(), Some("api_key_header"));
        assert_eq!(
            updated.upstream_protocol,
            Some(UpstreamProtocol::ChatCompletions)
        );
        assert_eq!(updated.model_mappings.len(), 3);
        assert!(updated
            .model_mappings
            .iter()
            .all(|mapping| mapping.public_model_id == mapping.upstream_model_id));
        assert_eq!(
            decrypt_api_key(&connection, &key(), &account.id).unwrap(),
            b"replacement-secret"
        );
        drop(connection);
        cleanup(&path);
    }

    #[test]
    fn account_move_reorders_a_group_atomically_and_rolls_back_on_failure() {
        let (path, mut connection) = database("account-move");
        for (id, order) in [("account-a", 0), ("account-b", 1), ("account-c", 2)] {
            connection
                .execute(
                    "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id, sort_order) VALUES (?1, 'api_key', ?1, 'default', ?2)",
                    params![id, order],
                )
                .unwrap();
        }

        move_account(&mut connection, "account-b", -1).unwrap();
        let ordered = connection
            .prepare(
                "SELECT id FROM ai_gateway_accounts WHERE group_id = 'default' ORDER BY sort_order",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(ordered, vec!["account-b", "account-a", "account-c"]);
        let sort_orders = connection
            .prepare("SELECT sort_order FROM ai_gateway_accounts WHERE group_id = 'default' ORDER BY sort_order")
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(sort_orders, vec![0, 1, 2]);

        connection
            .execute_batch(
                "CREATE TRIGGER reject_temporary_reorder BEFORE UPDATE OF sort_order ON ai_gateway_accounts WHEN NEW.sort_order < 0 BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
            )
            .unwrap();
        assert!(move_account(&mut connection, "account-c", -1).is_err());
        let unchanged = connection
            .prepare("SELECT id, sort_order FROM ai_gateway_accounts WHERE group_id = 'default' ORDER BY sort_order")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            unchanged,
            vec![
                ("account-b".into(), 0),
                ("account-a".into(), 1),
                ("account-c".into(), 2)
            ]
        );
        drop(connection);
        cleanup(&path);
    }

    #[test]
    fn oauth_reauthorization_keeps_stable_account_and_atomically_replaces_tokens() {
        let (path, mut connection) = database("oauth-upsert");
        let first = OAuthTokenBundle {
            access_token: "access-one".into(),
            refresh_token: "refresh-one".into(),
            expires_at: Some("2026-08-01T01:00:00Z".into()),
            token_type: "Bearer".into(),
            scope: "fixed-test-scope".into(),
        };
        let account = upsert_oauth_account(
            &mut connection,
            &key(),
            UpsertOAuthAccount {
                stable_external_id: "official-user-1",
                name: "Original",
                token_bundle: &first,
                metadata_json: r#"{"issuer":"fixture"}"#,
            },
        )
        .unwrap();
        let second = OAuthTokenBundle {
            access_token: "access-two".into(),
            refresh_token: "refresh-two".into(),
            expires_at: Some("2026-08-02T01:00:00Z".into()),
            token_type: "Bearer".into(),
            scope: "fixed-test-scope".into(),
        };
        let updated = upsert_oauth_account(
            &mut connection,
            &key(),
            UpsertOAuthAccount {
                stable_external_id: "official-user-1",
                name: "Updated",
                token_bundle: &second,
                metadata_json: r#"{"issuer":"fixture","version":2}"#,
            },
        )
        .unwrap();
        assert_eq!(updated.id, account.id);
        assert_eq!(updated.name, "Updated");
        assert_eq!(
            decrypt_oauth_tokens(&connection, &key(), &account.id).unwrap(),
            second
        );
        let account_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM ai_gateway_accounts", [], |row| {
                row.get(0)
            })
            .unwrap();
        let credential_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM ai_gateway_credentials", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((account_count, credential_count), (1, 1));
        drop(connection);
        cleanup(&path);
    }

    #[test]
    fn oauth_private_refresh_material_is_encrypted_and_metadata_scan_is_clean() {
        let (path, mut connection) = database("oauth-private-material");
        let account = upsert_oauth_account(
            &mut connection,
            &key(),
            UpsertOAuthAccount {
                stable_external_id: "private-material-user",
                name: "Private Material",
                token_bundle: &OAuthTokenBundle {
                    access_token: "access-private".into(),
                    refresh_token: "refresh-private".into(),
                    expires_at: None,
                    token_type: "Bearer".into(),
                    scope: "fixture".into(),
                },
                metadata_json: r#"{"token_endpoint":"https://issuer.example/token","client_id":"public-client","client_secret":"private-client-secret"}"#,
            },
        )
        .unwrap();
        let metadata: String = connection
            .query_row(
                "SELECT metadata_json FROM ai_gateway_credentials WHERE account_id = ?1",
                [&account.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!metadata.contains("client_id"));
        assert!(!metadata.contains("client_secret"));
        let material = load_oauth_refresh_material(&connection, &key(), &account.id).unwrap();
        assert_eq!(material.client_id.as_deref(), Some("public-client"));
        assert_eq!(
            material.client_secret.as_deref(),
            Some("private-client-secret")
        );
        assert_eq!(
            material.token_endpoint.as_deref(),
            Some("https://issuer.example/token")
        );
        let sensitive_metadata: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_credentials WHERE lower(COALESCE(metadata_json, '')) LIKE '%client_secret%' OR lower(COALESCE(metadata_json, '')) LIKE '%access_token%' OR lower(COALESCE(metadata_json, '')) LIKE '%refresh_token%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sensitive_metadata, 0);
        let rejected = upsert_oauth_account(
            &mut connection,
            &key(),
            UpsertOAuthAccount {
                stable_external_id: "private-material-user",
                name: "Private Material",
                token_bundle: &material.token_bundle,
                metadata_json: r#"{"nested":{"access_token":"must-not-be-public"}}"#,
            },
        )
        .unwrap_err();
        assert_eq!(rejected.category(), GatewayErrorCategory::InvalidInput);
        drop(connection);
        cleanup(&path);
    }

    #[test]
    fn permanent_delete_requires_one_time_confirmation_and_preserves_history_snapshots() {
        let (path, mut connection) = database("permanent-delete");
        let account =
            create_api_key_account(&mut connection, &key(), api_input("delete-secret")).unwrap();
        upsert_public_model(&connection, "model-1", "Model", "{}").unwrap();
        set_model_mapping(
            &connection,
            &ModelMappingDto {
                account_id: account.id.clone(),
                public_model_id: "model-1".into(),
                upstream_model_id: "upstream".into(),
                enabled: true,
            },
        )
        .unwrap();
        connection.execute("INSERT INTO ai_gateway_quota_windows (id, account_id, name, scope_type) VALUES ('quota-1', ?1, 'Window', 'global')", [&account.id]).unwrap();
        connection.execute(
            "INSERT INTO ai_gateway_request_logs (id, request_id, started_at, local_date, timezone_name, endpoint, public_model_id, account_id, account_name_snapshot, status) VALUES ('log-1', 'request-1', CURRENT_TIMESTAMP, '2026-08-01', 'UTC', '/v1/responses', 'model-1', ?1, 'Third Party', 'succeeded')",
            [&account.id],
        ).unwrap();
        connection.execute(
            "INSERT INTO ai_gateway_daily_aggregates (local_date, timezone_name, account_id_snapshot, account_name_snapshot, public_model_id, request_count) VALUES ('2026-08-01', 'UTC', ?1, 'Third Party', 'model-1', 1)",
            [&account.id],
        ).unwrap();
        let confirmations = DeleteConfirmationStore::default();
        assert_eq!(
            permanent_delete_account(&mut connection, &confirmations, &account.id, "wrong")
                .unwrap_err()
                .category(),
            GatewayErrorCategory::ConfirmationRequired
        );
        let token = confirmations.issue(&account.id).unwrap();
        permanent_delete_account(&mut connection, &confirmations, &account.id, &token).unwrap();
        for table in [
            "ai_gateway_credentials",
            "ai_gateway_quota_windows",
            "ai_gateway_account_model_mappings",
        ] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} must cascade");
        }
        let history: (i64, Option<String>, String) = connection.query_row(
            "SELECT COUNT(*), account_id, account_name_snapshot FROM ai_gateway_request_logs WHERE id = 'log-1'", [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!(history, (1, None, "Third Party".into()));
        let aggregate_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_daily_aggregates",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(aggregate_count, 1);
        assert_eq!(
            permanent_delete_account(&mut connection, &confirmations, &account.id, &token)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::ConfirmationRequired
        );
        drop(connection);
        cleanup(&path);
    }
}
