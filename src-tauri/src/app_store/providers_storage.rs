use super::{
    apply_provider_id_map_to_dependent_state, infer_claude_api_format,
    infer_claude_connection_mode, infer_protocol_router_wire_api, lock_sessions_state_write,
    migrate_launcher_to_local_if_needed, migrate_sessions_to_local_if_needed,
    normalize_loaded_service_providers_state, normalize_service_provider_record,
    normalize_sessions_state, now_ts, parse_first_json_value, required_history_parser_version,
    resolved_claude_model_mappings, sort_sessions_for_display, strip_legacy_claude_model_keys,
    ApiMeta, ClaudeModelMapping, CliSessionLookup, CryptoService, EncryptedBlob, LauncherState,
    MigrationState, OutboxState, ProvidersState, SchemaMeta, ServiceProviderRecord,
    ServiceProvidersState, SessionRecord, SessionsHistoryToolState, SessionsState, StorageEngine,
    HISTORY_BIND_WINDOW_SECS, HISTORY_SYNC_TOOLS, SESSIONS_HISTORY_SYNC_RUNNING,
};
use crate::{ai_sessions, workspaces};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self};
use std::path::Path;
use std::sync::atomic::Ordering;
use tauri::Emitter;

struct LoadedServiceProvidersState {
    state: ServiceProvidersState,
    migrated_from_legacy_schema: bool,
}

/// Migrate an old ProvidersState into a ServiceProvidersState.
pub(crate) fn migrate_providers_to_service_providers(old: ProvidersState) -> ServiceProvidersState {
    let providers: Vec<ServiceProviderRecord> = old
        .providers
        .into_iter()
        .map(|p| {
            let is_claude = p.core.tool == "claude";
            let icon = p
                .tool_config
                .get("icon")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string());
            let legacy_api_format = p
                .tool_config
                .get("claude_api_format")
                .and_then(|v| v.as_str());
            let legacy_connection_mode = p
                .tool_config
                .get("claude_connection_mode")
                .and_then(|v| v.as_str());
            let legacy_wire_api = p
                .tool_config
                .get("protocol_router_wire_api")
                .and_then(|v| v.as_str())
                .or_else(|| p.tool_config.get("wire_api").and_then(|v| v.as_str()));
            let claude_model_mappings = if is_claude {
                let mappings = resolved_claude_model_mappings(&p.tool_config);
                if mappings.iter().any(|mapping| {
                    !mapping.upstream_model.trim().is_empty()
                        || mapping
                            .supported_capabilities
                            .as_ref()
                            .map(|values| !values.is_empty())
                            .unwrap_or(false)
                }) {
                    mappings
                } else {
                    vec![
                        ClaudeModelMapping {
                            family: "haiku".to_string(),
                            display_name: "Haiku".to_string(),
                            upstream_model: "claude-haiku-4-3-20250514".to_string(),
                            supports_1m: Some(false),
                            supported_capabilities: None,
                        },
                        ClaudeModelMapping {
                            family: "sonnet".to_string(),
                            display_name: "Sonnet".to_string(),
                            upstream_model: "claude-sonnet-4-20250514".to_string(),
                            supports_1m: Some(false),
                            supported_capabilities: None,
                        },
                        ClaudeModelMapping {
                            family: "opus".to_string(),
                            display_name: "Opus".to_string(),
                            upstream_model: "claude-opus-4-20250514".to_string(),
                            supports_1m: Some(false),
                            supported_capabilities: None,
                        },
                    ]
                }
            } else {
                vec![]
            };

            // Determine auth env key: if the old record used api_key (non-empty), keep ANTHROPIC_API_KEY
            let claude_auth_env_key = if is_claude && !p.core.api_key.is_empty() {
                "ANTHROPIC_API_KEY".to_string()
            } else {
                "ANTHROPIC_API_KEY".to_string()
            };

            let inferred_api_format =
                infer_claude_api_format(legacy_api_format, legacy_connection_mode, legacy_wire_api);
            let inferred_connection_mode =
                infer_claude_connection_mode(legacy_connection_mode, &inferred_api_format);
            let mut record = ServiceProviderRecord {
                id: p.core.id,
                name: p.core.name,
                tool: p.core.tool,
                icon,
                api_key: p.core.api_key,
                base_url: p.core.base_url,
                model: p.core.model,
                claude_api_format: inferred_api_format.clone(),
                claude_connection_mode: inferred_connection_mode.clone(),
                protocol_router_upstream_provider_id: None,
                protocol_router_wire_api: infer_protocol_router_wire_api(
                    legacy_wire_api,
                    &inferred_api_format,
                    Some(&inferred_connection_mode),
                ),
                claude_auth_env_key,
                claude_model_mappings,
                claude_enable_tool_search: None,
                claude_auto_memory_enabled: None,
                claude_always_thinking_enabled: None,
                claude_away_summary_enabled: None,
                claude_include_git_instructions: None,
                claude_enable_attribution: None,
                code: p.core.code,
                is_enabled: p.is_enabled,
                provider_key: p.provider_key,
                env_managed: None,
                favorite_at: p.favorite_at,
                tool_config: p.tool_config,
                history: p.history,
                extra: p.extra,
                fetched_models: None,
            };
            if !record.claude_model_mappings.is_empty() {
                record.tool_config.insert(
                    "claude_model_mappings".to_string(),
                    serde_json::to_value(&record.claude_model_mappings)
                        .unwrap_or_else(|_| Value::Array(vec![])),
                );
            }
            strip_legacy_claude_model_keys(&mut record.tool_config);
            normalize_service_provider_record(&mut record);
            record
        })
        .collect();

    ServiceProvidersState {
        active: old.active,
        active_opencode: Vec::new(),
        providers,
    }
}

pub(in crate::app_store) fn restore_missing_service_provider_api_keys_from_legacy(
    state: &mut ServiceProvidersState,
) -> Result<bool, String> {
    let _ = state;
    Ok(false)
}

fn service_providers_state_from_value(value: Value) -> Result<LoadedServiceProvidersState, String> {
    if let Ok(state) = serde_json::from_value::<ServiceProvidersState>(value.clone()) {
        return Ok(LoadedServiceProvidersState {
            state,
            migrated_from_legacy_schema: false,
        });
    }

    let legacy = serde_json::from_value::<ProvidersState>(value).map_err(|e| e.to_string())?;
    Ok(LoadedServiceProvidersState {
        state: migrate_providers_to_service_providers(legacy),
        migrated_from_legacy_schema: true,
    })
}

fn read_service_providers_state_from_path(
    path: &Path,
) -> Result<Option<LoadedServiceProvidersState>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(Some(LoadedServiceProvidersState {
            state: ServiceProvidersState::default(),
            migrated_from_legacy_schema: false,
        }));
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            return service_providers_state_from_value(value).map(Some);
        }
    }

    let value = serde_json::from_str::<Value>(&content).map_err(|e| e.to_string())?;
    service_providers_state_from_value(value).map(Some)
}

/// Load service providers state from the canonical data/providers/state.json path.
pub(crate) fn load_service_providers_state() -> Result<ServiceProvidersState, String> {
    let (state, id_map) = load_service_providers_state_with_id_map()?;
    if !id_map.is_empty() {
        apply_provider_id_map_to_dependent_state(&id_map)?;
    }
    Ok(state)
}

pub(in crate::app_store) fn load_service_providers_state_with_id_map(
) -> Result<(ServiceProvidersState, HashMap<String, String>), String> {
    let path = StorageEngine::providers_path()?;
    if let Some(loaded) = read_service_providers_state_from_path(&path)? {
        let mut state = loaded.state;
        let (id_map, changed) = normalize_loaded_service_providers_state(&mut state)?;
        if changed || loaded.migrated_from_legacy_schema {
            save_service_providers_internal(&state)?;
        }
        return Ok((state, id_map));
    }

    let migration_state = load_migration_state().unwrap_or_default();
    if migration_state.migrated {
        return Err(format!(
            "service_providers state missing after migration: {}",
            path.display()
        ));
    }

    Ok((ServiceProvidersState::default(), HashMap::new()))
}

/// Save service providers state (internal, no side effects).
pub(crate) fn save_service_providers_internal(
    state: &ServiceProvidersState,
) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::providers_path()?, &blob)?;

    StorageEngine::bump_revision()
}

pub(in crate::app_store) fn load_sessions_state() -> Result<SessionsState, String> {
    let path = StorageEngine::sessions_path()?;
    let _ = migrate_sessions_to_local_if_needed(&path);
    if !path.exists() {
        return Ok(SessionsState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(SessionsState::default());
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            if let Ok(mut state) = serde_json::from_value::<SessionsState>(value) {
                let _ = normalize_sessions_state(&mut state);
                return Ok(state);
            }
        }
    }

    let mut state = serde_json::from_str::<SessionsState>(&content).map_err(|e| e.to_string())?;
    let _ = normalize_sessions_state(&mut state);
    Ok(state)
}

pub(in crate::app_store) fn save_sessions_state(
    state: &SessionsState,
) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::sessions_path()?, &blob)?;
    StorageEngine::bump_revision()
}

pub(in crate::app_store) fn cli_session_lookup_from_record(
    session: &SessionRecord,
) -> CliSessionLookup {
    CliSessionLookup {
        id: session.id.trim().to_string(),
        tool: session.tool.trim().to_string(),
        tool_session_id: session.tool_session_id.trim().to_string(),
        working_dir: session.working_dir.trim().to_string(),
    }
}

pub(in crate::app_store) fn find_cli_session_in_state(
    state: &SessionsState,
    query: &str,
) -> Option<CliSessionLookup> {
    let lookup = query.trim();
    if lookup.is_empty() {
        return None;
    }

    state
        .sessions
        .iter()
        .find(|session| session.tool_session_id.trim() == lookup)
        .or_else(|| {
            state
                .sessions
                .iter()
                .find(|session| session.id.trim() == lookup)
        })
        .map(cli_session_lookup_from_record)
}

pub(crate) fn cli_lookup_session(query: &str) -> Result<Option<CliSessionLookup>, String> {
    let state = load_sessions_state()?;
    Ok(find_cli_session_in_state(&state, query))
}

pub(in crate::app_store) fn history_tombstone_key(
    tool: &str,
    tool_session_id: &str,
) -> Option<String> {
    let normalized_tool = tool.trim().to_lowercase();
    let normalized_session_id = tool_session_id.trim();
    if normalized_tool.is_empty() || normalized_session_id.is_empty() {
        return None;
    }
    Some(format!("{}::{}", normalized_tool, normalized_session_id))
}

pub(in crate::app_store) fn history_sync_requires_full_backfill(
    tool: &str,
    tool_state: Option<&SessionsHistoryToolState>,
) -> bool {
    let required_parser_version = required_history_parser_version(tool);
    tool_state
        .map(|tool_state| {
            !tool_state.full_backfill_done || tool_state.parser_version < required_parser_version
        })
        .unwrap_or(true)
}

pub(in crate::app_store) fn stable_history_session_record_id(
    tool: &str,
    tool_session_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tool.trim().to_lowercase().as_bytes());
    hasher.update(b":");
    hasher.update(tool_session_id.trim().as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!(
        "history-{}-{}",
        tool.trim().to_lowercase(),
        &digest[..16.min(digest.len())]
    )
}

pub(in crate::app_store) fn history_entry_time_secs(
    entry: &ai_sessions::HistorySessionEntry,
) -> (u64, u64) {
    let created_at = if entry.created_at_ms > 0 {
        (entry.created_at_ms as u64) / 1000
    } else if entry.updated_at_ms > 0 {
        (entry.updated_at_ms as u64) / 1000
    } else {
        now_ts()
    };
    let updated_at = if entry.updated_at_ms > 0 {
        (entry.updated_at_ms as u64) / 1000
    } else {
        created_at
    };
    (created_at, updated_at)
}

pub(in crate::app_store) fn normalize_session_working_dir(value: &str) -> String {
    ai_sessions::normalize_working_dir_for_terminal(value)
}

pub(in crate::app_store) fn same_session_working_dir(left: &str, right: &str) -> bool {
    normalize_session_working_dir(left) == normalize_session_working_dir(right)
}

pub(in crate::app_store) fn should_bind_history_entry_to_placeholder(
    session: &SessionRecord,
    entry: &ai_sessions::HistorySessionEntry,
) -> bool {
    if session.tool != entry.tool {
        return false;
    }
    if !session.tool_session_id.trim().is_empty() {
        return false;
    }
    if session.status != "pending_bind" && session.status != "unbound" {
        return false;
    }
    if !same_session_working_dir(&session.working_dir, &entry.working_dir) {
        return false;
    }
    let (created_at, updated_at) = history_entry_time_secs(entry);
    let target_ts = if created_at > 0 {
        created_at
    } else {
        updated_at
    };
    if target_ts == 0 {
        return false;
    }
    session.created_at.abs_diff(target_ts) <= HISTORY_BIND_WINDOW_SECS
}

pub(in crate::app_store) fn placeholder_preference_score(
    session: &SessionRecord,
    entry: &ai_sessions::HistorySessionEntry,
) -> (u8, u64, u64) {
    let (created_at, updated_at) = history_entry_time_secs(entry);
    let target_ts = if created_at > 0 {
        created_at
    } else {
        updated_at
    };
    (
        if session.status == "pending_bind" {
            0
        } else {
            1
        },
        session.created_at.abs_diff(target_ts),
        u64::MAX - session.created_at,
    )
}

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::app_store) struct SessionsHistorySyncOutcome {
    pub(in crate::app_store) persisted: bool,
    pub(in crate::app_store) list_changed: bool,
}

pub(in crate::app_store) fn merge_history_entry_into_session(
    session: &mut SessionRecord,
    entry: &ai_sessions::HistorySessionEntry,
) -> bool {
    let mut changed = false;
    let (created_at, updated_at) = history_entry_time_secs(entry);
    let history_name = entry.title.trim();
    let history_model = entry
        .model_name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if session.tool_session_id.trim() != entry.tool_session_id.trim() {
        session.tool_session_id = entry.tool_session_id.trim().to_string();
        changed = true;
    }
    if session.status != "active" {
        session.status = "active".to_string();
        changed = true;
    }
    if session.name_source != "manual" && session.name.trim().is_empty() && !history_name.is_empty()
    {
        session.name = history_name.to_string();
        changed = true;
    }
    if session.model_name != history_model {
        session.model_name = history_model;
        changed = true;
    }
    let normalized_working_dir = normalize_session_working_dir(&entry.working_dir);
    if !normalized_working_dir.is_empty() && session.working_dir != normalized_working_dir {
        session.working_dir = normalized_working_dir;
        changed = true;
    }
    if created_at > 0 && session.created_at != created_at {
        session.created_at = created_at;
        changed = true;
    }
    let next_last_used_at = session.last_used_at.max(updated_at.max(created_at));
    if session.last_used_at != next_last_used_at {
        session.last_used_at = next_last_used_at;
        changed = true;
    }
    changed
}

pub(in crate::app_store) fn apply_history_entries_to_sessions_state(
    state: &mut SessionsState,
    tool: &str,
    entries: Vec<ai_sessions::HistorySessionEntry>,
    synced_at: u64,
) -> SessionsHistorySyncOutcome {
    let mut outcome = SessionsHistorySyncOutcome::default();
    let normalized_tool = tool.trim().to_lowercase();
    let mut session_index_by_tool_session = HashMap::<String, usize>::new();

    for (idx, session) in state.sessions.iter().enumerate() {
        if session.tool != normalized_tool {
            continue;
        }
        let tool_session_id = session.tool_session_id.trim();
        if tool_session_id.is_empty() {
            continue;
        }
        session_index_by_tool_session.insert(tool_session_id.to_string(), idx);
    }

    let mut claimed_placeholders = HashSet::<String>::new();
    let mut max_seen_updated_at_ms = state
        .history_sync
        .tools
        .get(&normalized_tool)
        .map(|tool_state| tool_state.last_seen_updated_at_ms)
        .unwrap_or(0);

    for entry in entries {
        if entry.tool != normalized_tool {
            continue;
        }
        max_seen_updated_at_ms =
            max_seen_updated_at_ms.max(entry.updated_at_ms.max(entry.created_at_ms));
        let Some(tombstone_key) = history_tombstone_key(&entry.tool, &entry.tool_session_id) else {
            continue;
        };
        if state.tombstones.contains(&tombstone_key) {
            continue;
        }

        if let Some(&idx) = session_index_by_tool_session.get(entry.tool_session_id.trim()) {
            if merge_history_entry_into_session(&mut state.sessions[idx], &entry) {
                outcome.persisted = true;
                outcome.list_changed = true;
            }
            continue;
        }

        let placeholder_idx = state
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                !claimed_placeholders.contains(&session.id)
                    && should_bind_history_entry_to_placeholder(session, &entry)
            })
            .min_by_key(|(_, session)| placeholder_preference_score(session, &entry))
            .map(|(idx, _)| idx);

        if let Some(idx) = placeholder_idx {
            claimed_placeholders.insert(state.sessions[idx].id.clone());
            if merge_history_entry_into_session(&mut state.sessions[idx], &entry) {
                outcome.persisted = true;
                outcome.list_changed = true;
            }
            session_index_by_tool_session.insert(entry.tool_session_id.clone(), idx);
            continue;
        }

        let (created_at, updated_at) = history_entry_time_secs(&entry);
        let record = SessionRecord {
            id: stable_history_session_record_id(&entry.tool, &entry.tool_session_id),
            name: entry.title.clone(),
            working_dir: normalize_session_working_dir(&entry.working_dir),
            tool: entry.tool.clone(),
            tool_session_id: entry.tool_session_id.clone(),
            model_name: entry.model_name.clone(),
            name_source: "history".to_string(),
            runtime_mode: "shared".to_string(),
            runtime_profile_id: None,
            preset_id: None,
            created_at,
            last_used_at: updated_at.max(created_at),
            status: "active".to_string(),
            favorited_at: None,
            provider_id: None,
        };
        session_index_by_tool_session.insert(record.tool_session_id.clone(), state.sessions.len());
        state.sessions.push(record);
        outcome.persisted = true;
        outcome.list_changed = true;
    }

    let tool_state = state
        .history_sync
        .tools
        .entry(normalized_tool)
        .or_insert_with(SessionsHistoryToolState::default);
    if !tool_state.full_backfill_done {
        tool_state.full_backfill_done = true;
        outcome.persisted = true;
    }
    let required_parser_version = required_history_parser_version(tool);
    if tool_state.parser_version != required_parser_version {
        tool_state.parser_version = required_parser_version;
        outcome.persisted = true;
    }
    if tool_state.last_seen_updated_at_ms != max_seen_updated_at_ms {
        tool_state.last_seen_updated_at_ms = max_seen_updated_at_ms;
        outcome.persisted = true;
    }
    if tool_state.last_completed_at != Some(synced_at) {
        tool_state.last_completed_at = Some(synced_at);
        outcome.persisted = true;
    }

    if outcome.list_changed {
        sort_sessions_for_display(&mut state.sessions);
    }

    outcome
}

pub(in crate::app_store) fn sessions_history_sync_tool(
    tool: String,
) -> Result<SessionsHistorySyncOutcome, String> {
    let normalized_tool = tool.trim().to_lowercase();
    if !HISTORY_SYNC_TOOLS.contains(&normalized_tool.as_str()) {
        return Ok(SessionsHistorySyncOutcome::default());
    }

    let state_for_cursor = load_sessions_state()?;
    let requires_full_backfill = history_sync_requires_full_backfill(
        &normalized_tool,
        state_for_cursor.history_sync.tools.get(&normalized_tool),
    );
    let min_updated_at_ms = state_for_cursor
        .history_sync
        .tools
        .get(&normalized_tool)
        .and_then(|tool_state| {
            if !requires_full_backfill && tool_state.full_backfill_done {
                Some(tool_state.last_seen_updated_at_ms.saturating_sub(15_000))
            } else {
                None
            }
        });

    let entries =
        ai_sessions::collect_history_sessions_for_tool(&normalized_tool, min_updated_at_ms)?;
    let outcome = {
        let _sessions_state_guard = lock_sessions_state_write()?;
        let mut latest_state = load_sessions_state()?;
        let outcome = apply_history_entries_to_sessions_state(
            &mut latest_state,
            &normalized_tool,
            entries,
            now_ts(),
        );

        if outcome.persisted {
            save_sessions_state(&latest_state)?;
        }
        outcome
    };
    if outcome.persisted {
        let _ = workspaces::sync_from_sessions();
    }

    Ok(outcome)
}

pub(crate) async fn run_sessions_history_sync_pass(app: tauri::AppHandle) -> Result<bool, String> {
    if SESSIONS_HISTORY_SYNC_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(false);
    }

    let mut any_list_change = false;
    let result = async {
        for tool in HISTORY_SYNC_TOOLS {
            let tool_name = tool.to_string();
            match tauri::async_runtime::spawn_blocking(move || {
                sessions_history_sync_tool(tool_name)
            })
            .await
            {
                Ok(Ok(outcome)) => {
                    any_list_change |= outcome.list_changed;
                }
                Ok(Err(err)) => {
                    log::warn!("sessions history sync skipped due to tool error: {}", err);
                }
                Err(err) => {
                    log::warn!("sessions history sync worker join failed: {}", err);
                }
            }
        }
        if any_list_change {
            let _ = app.emit("sessions-updated", ());
            let _ = app.emit("refresh-counts", ());
        }
        Ok(any_list_change)
    }
    .await;

    SESSIONS_HISTORY_SYNC_RUNNING.store(false, Ordering::SeqCst);
    result
}

pub(in crate::app_store) fn load_launcher_state() -> Result<LauncherState, String> {
    let path = StorageEngine::launcher_path()?;
    let _ = migrate_launcher_to_local_if_needed(&path);
    if !path.exists() {
        return Ok(LauncherState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(LauncherState::default());
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            if let Ok(state) = serde_json::from_value::<LauncherState>(value) {
                return Ok(state);
            }
        }
    }

    serde_json::from_str::<LauncherState>(&content).map_err(|e| e.to_string())
}

pub(in crate::app_store) fn save_launcher_state(
    state: &LauncherState,
) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::launcher_path()?, &blob)?;
    StorageEngine::bump_revision()
}

pub(in crate::app_store) fn load_outbox_state() -> Result<OutboxState, String> {
    let path = StorageEngine::outbox_path()?;
    if !path.exists() {
        return Ok(OutboxState::default());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(OutboxState::default());
    }

    match serde_json::from_str::<OutboxState>(&content) {
        Ok(state) => Ok(state),
        Err(strict_err) => {
            if let Some(recovered) = parse_first_json_value::<OutboxState>(&content) {
                // Self-heal corrupted trailing bytes and continue.
                let _ = StorageEngine::write_json(&path, &recovered);
                Ok(recovered)
            } else {
                Err(strict_err.to_string())
            }
        }
    }
}

pub(in crate::app_store) fn save_outbox_state(state: &OutboxState) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::outbox_path()?, state)
}

pub(in crate::app_store) fn load_migration_state() -> Result<MigrationState, String> {
    StorageEngine::read_json(&StorageEngine::migration_state_path()?)
}

pub(in crate::app_store) fn save_migration_state(state: &MigrationState) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::migration_state_path()?, state)
}

pub(in crate::app_store) fn get_meta() -> Result<ApiMeta, String> {
    let schema = StorageEngine::load_schema()?;
    Ok(ApiMeta {
        schema_version: schema.schema_version,
        revision: schema.revision,
    })
}

pub(in crate::app_store) fn parse_json_array_len(raw: &str) -> usize {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.as_array().map(|arr| arr.len()))
        .unwrap_or(0)
}
