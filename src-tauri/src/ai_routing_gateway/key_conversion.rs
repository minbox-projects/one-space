use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashSet;

use crate::app_store::{
    load_service_providers_state, lock_service_provider_operation, save_service_providers_internal,
    ServiceProviderRecord, ServiceProvidersState,
};

use super::{
    error::{GatewayError, GatewayErrorCategory},
    gateway_key,
    security::RootKey,
    storage,
};

const TOOLS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConversionToolState {
    pub(crate) tool: String,
    pub(crate) converted: bool,
    pub(crate) service_provider_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConvertedProviderSummary {
    pub(crate) tool: String,
    pub(crate) service_provider_id: String,
    pub(crate) name: String,
    pub(crate) activated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConversionResult {
    pub(crate) key_id: String,
    pub(crate) providers: Vec<ConvertedProviderSummary>,
    pub(crate) tools: Vec<ConversionToolState>,
}

pub(crate) fn available_tools(
    connection: &Connection,
    key_id: &str,
    now: DateTime<Utc>,
) -> Result<Vec<ConversionToolState>, GatewayError> {
    ensure_convertible_key(connection, key_id, now)?;
    tool_states(connection, key_id)
}

pub(crate) fn convert(
    connection: &mut Connection,
    root_key: &RootKey,
    key_id: &str,
    tools: &[String],
    activate: bool,
) -> Result<ConversionResult, GatewayError> {
    let _operation = lock_service_provider_operation().map_err(|_| storage_error(Some(key_id)))?;
    let original = load_service_providers_state().map_err(|_| storage_error(Some(key_id)))?;
    let rollback = original.clone();
    convert_coordinated(
        connection,
        root_key,
        key_id,
        tools,
        activate,
        &original,
        |state| {
            save_service_providers_internal(state)
                .map(|_| ())
                .map_err(|_| storage_error(Some(key_id)))
        },
        || {
            save_service_providers_internal(&rollback)
                .map(|_| ())
                .map_err(|_| storage_error(Some(key_id)))
        },
    )
}

fn convert_coordinated<P, R>(
    connection: &mut Connection,
    root_key: &RootKey,
    key_id: &str,
    tools: &[String],
    activate: bool,
    original: &ServiceProvidersState,
    mut persist: P,
    mut restore: R,
) -> Result<ConversionResult, GatewayError>
where
    P: FnMut(&ServiceProvidersState) -> Result<(), GatewayError>,
    R: FnMut() -> Result<(), GatewayError>,
{
    let requested = validate_tools(tools, key_id)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| storage_error(Some(key_id)))?;
    ensure_convertible_key(&transaction, key_id, Utc::now())?;
    reject_existing_relations(&transaction, key_id, &requested)?;

    let key_name = transaction
        .query_row(
            "SELECT name FROM ai_gateway_api_keys WHERE id = ?1",
            [key_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| storage_error(Some(key_id)))?;
    let port = transaction
        .query_row(
            "SELECT port FROM ai_gateway_settings WHERE id = 1",
            [],
            |row| row.get::<_, u16>(0),
        )
        .map_err(|_| storage_error(Some(key_id)))?;
    let plaintext = gateway_key::copy_plaintext(&transaction, root_key, key_id)?;
    let mut next = original.clone();
    let mut summaries = Vec::with_capacity(requested.len());

    for tool in requested {
        let provider = build_provider(&next, &key_name, tool, &plaintext, port);
        let provider_id = provider.id.clone();
        let name = provider.name.clone();
        next.providers.push(provider);
        if activate {
            if tool == "opencode" {
                if !next.active_opencode.contains(&provider_id) {
                    next.active_opencode.push(provider_id.clone());
                }
            } else {
                next.active.insert(tool.to_string(), provider_id.clone());
            }
        }
        transaction
            .execute(
                "INSERT INTO ai_gateway_key_provider_conversions
                    (gateway_key_id, tool, service_provider_id)
                 VALUES (?1, ?2, ?3)",
                params![key_id, tool, provider_id],
            )
            .map_err(|error| relation_write_error(error, key_id))?;
        summaries.push(ConvertedProviderSummary {
            tool: tool.to_string(),
            service_provider_id: provider_id,
            name,
            activated: activate,
        });
    }

    let tools = tool_states(&transaction, key_id)?;
    persist(&next)?;
    if transaction.commit().is_err() {
        restore()?;
        return Err(storage_error(Some(key_id)));
    }

    Ok(ConversionResult {
        key_id: key_id.to_string(),
        providers: summaries,
        tools,
    })
}

pub(crate) fn detach_service_provider(provider_id: &str) -> Result<(), GatewayError> {
    let mut connection = storage::open()?;
    detach_service_provider_in(&mut connection, provider_id)
}

fn detach_service_provider_in(
    connection: &mut Connection,
    provider_id: &str,
) -> Result<(), GatewayError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| storage_error(Some(provider_id)))?;
    transaction
        .execute(
            "DELETE FROM ai_gateway_key_provider_conversions WHERE service_provider_id = ?1",
            [provider_id],
        )
        .map_err(|_| storage_error(Some(provider_id)))?;
    transaction
        .commit()
        .map_err(|_| storage_error(Some(provider_id)))
}

fn ensure_convertible_key(
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
        .map_err(|_| storage_error(Some(key_id)))?
        .ok_or_else(|| GatewayError::new(GatewayErrorCategory::NotFound, Some(key_id)))?;
    let expired = state
        .1
        .as_deref()
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|expiration| expiration.with_timezone(&Utc) <= now)
                .map_err(|_| storage_error(Some(key_id)))
        })
        .transpose()?
        .unwrap_or(false);
    if state.0.is_some() || expired {
        Err(GatewayError::new(
            GatewayErrorCategory::Conflict,
            Some(key_id),
        ))
    } else {
        Ok(())
    }
}

fn validate_tools<'a>(tools: &'a [String], key_id: &str) -> Result<Vec<&'a str>, GatewayError> {
    if tools.is_empty() {
        return Err(invalid(Some(key_id)));
    }
    let mut seen = HashSet::new();
    let mut requested = Vec::with_capacity(tools.len());
    for tool in tools {
        let tool = tool.as_str();
        if !TOOLS.contains(&tool) || !seen.insert(tool) {
            return Err(invalid(Some(key_id)));
        }
        requested.push(tool);
    }
    Ok(requested)
}

fn reject_existing_relations(
    connection: &Connection,
    key_id: &str,
    tools: &[&str],
) -> Result<(), GatewayError> {
    for tool in tools {
        let exists = connection
            .query_row(
                "SELECT 1 FROM ai_gateway_key_provider_conversions
                 WHERE gateway_key_id = ?1 AND tool = ?2",
                params![key_id, tool],
                |_| Ok(true),
            )
            .optional()
            .map_err(|_| storage_error(Some(key_id)))?
            .unwrap_or(false);
        if exists {
            return Err(GatewayError::new(
                GatewayErrorCategory::Conflict,
                Some(key_id),
            ));
        }
    }
    Ok(())
}

fn tool_states(
    connection: &Connection,
    key_id: &str,
) -> Result<Vec<ConversionToolState>, GatewayError> {
    TOOLS
        .into_iter()
        .map(|tool| {
            let provider_id = connection
                .query_row(
                    "SELECT service_provider_id FROM ai_gateway_key_provider_conversions
                     WHERE gateway_key_id = ?1 AND tool = ?2",
                    params![key_id, tool],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|_| storage_error(Some(key_id)))?;
            Ok(ConversionToolState {
                tool: tool.to_string(),
                converted: provider_id.is_some(),
                service_provider_id: provider_id,
            })
        })
        .collect()
}

fn build_provider(
    state: &ServiceProvidersState,
    key_name: &str,
    tool: &str,
    plaintext: &str,
    port: u16,
) -> ServiceProviderRecord {
    let base_name = format!("{} ({} Gateway)", key_name.trim(), tool_label(tool));
    let name = unique_name(state, tool, &base_name);
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut provider = ServiceProviderRecord {
        id: unique_provider_id(state),
        name,
        tool: tool.to_string(),
        api_key: plaintext.to_string(),
        base_url: Some(base_url.clone()),
        is_enabled: Some(true),
        env_managed: (tool != "opencode").then_some(true),
        ..ServiceProviderRecord::default()
    };
    match tool {
        "claude" => {
            provider.code = Some(unique_identifier(state, tool, key_name, true));
            provider.claude_api_format = "open_ai_responses".to_string();
            provider.claude_connection_mode = "protocol_router".to_string();
            provider.protocol_router_wire_api = "open_ai_responses".to_string();
        }
        "codex" => {
            provider
                .tool_config
                .insert("wire_api".into(), Value::String("responses".into()));
        }
        "gemini" => {
            provider.tool_config.insert(
                "gemini_auth_type".into(),
                Value::String("gemini-api-key".into()),
            );
        }
        "opencode" => {
            provider.provider_key = Some(unique_identifier(state, tool, key_name, false));
            let mut options = Map::new();
            options.insert("apiKey".into(), Value::String(plaintext.to_string()));
            options.insert("baseURL".into(), Value::String(base_url));
            provider.tool_config.insert(
                "npm".into(),
                Value::String("@ai-sdk/openai-compatible".into()),
            );
            provider
                .tool_config
                .insert("options".into(), Value::Object(options));
        }
        _ => unreachable!("validated tool"),
    }
    provider
}

fn unique_provider_id(state: &ServiceProvidersState) -> String {
    loop {
        let candidate = uuid::Uuid::new_v4().to_string();
        if !state
            .providers
            .iter()
            .any(|provider| provider.id == candidate)
        {
            return candidate;
        }
    }
}

fn unique_name(state: &ServiceProvidersState, tool: &str, base: &str) -> String {
    if !state
        .providers
        .iter()
        .any(|provider| provider.tool == tool && provider.name.eq_ignore_ascii_case(base))
    {
        return base.to_string();
    }
    for suffix in 2u32.. {
        let candidate = format!("{base} {suffix}");
        if !state
            .providers
            .iter()
            .any(|provider| provider.tool == tool && provider.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
    }
    unreachable!()
}

fn unique_identifier(
    state: &ServiceProvidersState,
    tool: &str,
    key_name: &str,
    hyphenated: bool,
) -> String {
    let mut stem = key_name
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if hyphenated && (character == ' ' || character == '-' || character == '_') {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();
    while stem.contains("--") {
        stem = stem.replace("--", "-");
    }
    stem = stem.trim_matches('-').to_string();
    if stem.is_empty() {
        stem = "gateway".to_string();
    }
    let base = if hyphenated {
        format!("gateway-{stem}")
    } else {
        format!("gateway{stem}")
    };
    let used = |candidate: &str| {
        state.providers.iter().any(|provider| {
            provider.tool == tool
                && if tool == "claude" {
                    provider.code.as_deref() == Some(candidate)
                } else {
                    provider.provider_key.as_deref() == Some(candidate)
                }
        })
    };
    if !used(&base) {
        return base;
    }
    for suffix in 2u32.. {
        let candidate = if hyphenated {
            format!("{base}-{suffix}")
        } else {
            format!("{base}{suffix}")
        };
        if !used(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}

fn tool_label(tool: &str) -> &'static str {
    match tool {
        "claude" => "Claude",
        "codex" => "Codex",
        "gemini" => "Gemini",
        "opencode" => "OpenCode",
        _ => unreachable!("validated tool"),
    }
}

fn relation_write_error(error: rusqlite::Error, key_id: &str) -> GatewayError {
    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
        GatewayError::new(GatewayErrorCategory::Conflict, Some(key_id))
    } else {
        storage_error(Some(key_id))
    }
}

fn invalid(entity_id: Option<&str>) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::InvalidInput, entity_id)
}

fn storage_error(entity_id: Option<&str>) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::StorageUnavailable, entity_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ai_routing_gateway::key_display_group::DEFAULT_DISPLAY_GROUP_ID, shared_sqlite};
    use std::cell::RefCell;

    fn root_key() -> RootKey {
        RootKey::try_from(vec![91; 32]).unwrap()
    }

    fn fixture() -> (std::path::PathBuf, Connection, RootKey, String, String) {
        let path = std::env::temp_dir().join(format!(
            "onespace-key-conversion-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let mut connection = shared_sqlite::open_at(&path).unwrap();
        let root_key = root_key();
        let created = gateway_key::create_in_display_group(
            &mut connection,
            &root_key,
            "Gateway Key",
            DEFAULT_DISPLAY_GROUP_ID,
            &["default".into()],
            &["gpt-5.6-sol".into()],
            None,
        )
        .unwrap();
        (
            path,
            connection,
            root_key,
            created.grant.id,
            created.plaintext,
        )
    }

    fn cleanup(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn four_tool_conversion_is_atomic_and_never_returns_plaintext() {
        let (path, mut connection, root_key, key_id, plaintext) = fixture();
        let original = ServiceProvidersState::default();
        let persisted = RefCell::new(None);
        let tools = TOOLS.map(str::to_string);
        let result = convert_coordinated(
            &mut connection,
            &root_key,
            &key_id,
            &tools,
            false,
            &original,
            |state| {
                *persisted.borrow_mut() = Some(state.clone());
                Ok(())
            },
            || Ok(()),
        )
        .unwrap();
        let state = persisted.into_inner().unwrap();
        assert_eq!(state.providers.len(), 4);
        assert!(state.active.is_empty());
        assert!(state.active_opencode.is_empty());
        assert!(state
            .providers
            .iter()
            .all(|provider| provider.api_key == plaintext));
        let claude = state
            .providers
            .iter()
            .find(|provider| provider.tool == "claude")
            .unwrap();
        assert_eq!(claude.claude_api_format, "open_ai_responses");
        assert_eq!(claude.claude_connection_mode, "protocol_router");
        let opencode = state
            .providers
            .iter()
            .find(|provider| provider.tool == "opencode")
            .unwrap();
        assert!(opencode.provider_key.is_some());
        assert!(opencode
            .tool_config
            .get("options")
            .and_then(Value::as_object)
            .and_then(|options| options.get("apiKey"))
            .and_then(Value::as_str)
            .map(|value| value == plaintext)
            .unwrap_or(false));
        assert_eq!(result.providers.len(), 4);
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains(&plaintext));
        assert!(available_tools(&connection, &key_id, Utc::now())
            .unwrap()
            .iter()
            .all(|tool| tool.converted));
        cleanup(&path);
    }

    #[test]
    fn activation_replaces_single_active_and_appends_opencode() {
        let (path, mut connection, root_key, key_id, _) = fixture();
        let mut original = ServiceProvidersState::default();
        original.active.insert("claude".into(), "old-claude".into());
        original.active.insert("codex".into(), "old-codex".into());
        original.active.insert("gemini".into(), "old-gemini".into());
        original.active_opencode.push("old-opencode".into());
        let persisted = RefCell::new(None);
        let tools = TOOLS.map(str::to_string);
        convert_coordinated(
            &mut connection,
            &root_key,
            &key_id,
            &tools,
            true,
            &original,
            |state| {
                *persisted.borrow_mut() = Some(state.clone());
                Ok(())
            },
            || Ok(()),
        )
        .unwrap();
        let state = persisted.into_inner().unwrap();
        for tool in ["claude", "codex", "gemini"] {
            let active = state.active.get(tool).unwrap();
            assert_ne!(active, &format!("old-{tool}"));
            assert!(state
                .providers
                .iter()
                .any(|provider| provider.id == *active && provider.tool == tool));
        }
        assert_eq!(state.active_opencode.first().unwrap(), "old-opencode");
        assert_eq!(state.active_opencode.len(), 2);
        cleanup(&path);
    }

    #[test]
    fn persist_failure_rolls_back_every_relation() {
        let (path, mut connection, root_key, key_id, _) = fixture();
        let tools = TOOLS.map(str::to_string);
        let error = convert_coordinated(
            &mut connection,
            &root_key,
            &key_id,
            &tools,
            true,
            &ServiceProvidersState::default(),
            |_| Err(storage_error(None)),
            || Ok(()),
        )
        .unwrap_err();
        assert_eq!(error.category(), GatewayErrorCategory::StorageUnavailable);
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_key_provider_conversions WHERE gateway_key_id = ?1",
                [&key_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        cleanup(&path);
    }

    #[test]
    fn relation_failure_and_decryption_failure_leave_both_stores_unchanged() {
        let (path, mut connection, root_key, key_id, _) = fixture();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_gemini_conversion
                 BEFORE INSERT ON ai_gateway_key_provider_conversions
                 WHEN NEW.tool = 'gemini'
                 BEGIN SELECT RAISE(ABORT, 'injected relation failure'); END;",
            )
            .unwrap();
        let persist_calls = RefCell::new(0);
        let error = convert_coordinated(
            &mut connection,
            &root_key,
            &key_id,
            &["claude".into(), "gemini".into()],
            true,
            &ServiceProvidersState::default(),
            |_| {
                *persist_calls.borrow_mut() += 1;
                Ok(())
            },
            || Ok(()),
        )
        .unwrap_err();
        assert_eq!(error.category(), GatewayErrorCategory::Conflict);
        assert_eq!(*persist_calls.borrow(), 0);
        let relation_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_key_provider_conversions WHERE gateway_key_id = ?1",
                [&key_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(relation_count, 0);

        connection
            .execute_batch("DROP TRIGGER fail_gemini_conversion;")
            .unwrap();
        let wrong_root_key = RootKey::try_from(vec![92; 32]).unwrap();
        let error = convert_coordinated(
            &mut connection,
            &wrong_root_key,
            &key_id,
            &["codex".into()],
            false,
            &ServiceProvidersState::default(),
            |_| panic!("decryption failure must not persist providers"),
            || Ok(()),
        )
        .unwrap_err();
        assert_eq!(
            error.category(),
            GatewayErrorCategory::CredentialAuthenticationFailed
        );
        let relation_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_key_provider_conversions WHERE gateway_key_id = ?1",
                [&key_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(relation_count, 0);
        cleanup(&path);
    }

    #[test]
    fn duplicate_invalid_and_revoked_requests_do_not_write() {
        let (path, mut connection, root_key, key_id, _) = fixture();
        let persisted = RefCell::new(None);
        convert_coordinated(
            &mut connection,
            &root_key,
            &key_id,
            &["claude".into()],
            false,
            &ServiceProvidersState::default(),
            |state| {
                *persisted.borrow_mut() = Some(state.clone());
                Ok(())
            },
            || Ok(()),
        )
        .unwrap();
        let state = persisted.into_inner().unwrap();
        let duplicate = convert_coordinated(
            &mut connection,
            &root_key,
            &key_id,
            &["claude".into()],
            false,
            &state,
            |_| Ok(()),
            || Ok(()),
        )
        .unwrap_err();
        assert_eq!(duplicate.category(), GatewayErrorCategory::Conflict);
        assert_eq!(
            validate_tools(&["unknown".into()], &key_id)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::InvalidInput
        );
        gateway_key::revoke(&connection, &key_id).unwrap();
        assert_eq!(
            available_tools(&connection, &key_id, Utc::now())
                .unwrap_err()
                .category(),
            GatewayErrorCategory::Conflict
        );
        cleanup(&path);
    }

    #[test]
    fn detaching_provider_allows_same_tool_to_be_converted_again() {
        let (path, mut connection, root_key, key_id, _) = fixture();
        let persisted = RefCell::new(None);
        let first = convert_coordinated(
            &mut connection,
            &root_key,
            &key_id,
            &["codex".into()],
            false,
            &ServiceProvidersState::default(),
            |state| {
                *persisted.borrow_mut() = Some(state.clone());
                Ok(())
            },
            || Ok(()),
        )
        .unwrap();
        detach_service_provider_in(&mut connection, &first.providers[0].service_provider_id)
            .unwrap();
        assert!(!available_tools(&connection, &key_id, Utc::now()).unwrap()[1].converted);
        let state = persisted.into_inner().unwrap();
        assert!(convert_coordinated(
            &mut connection,
            &root_key,
            &key_id,
            &["codex".into()],
            false,
            &state,
            |_| Ok(()),
            || Ok(()),
        )
        .is_ok());
        cleanup(&path);
    }
}
