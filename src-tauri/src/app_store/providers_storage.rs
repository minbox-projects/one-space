fn provider_from_input(input: ProviderInput, old: Option<&ProviderRecord>) -> ProviderRecord {
    let mut tool_config = old.map(|o| o.tool_config.clone()).unwrap_or_default();
    let mut extra = old.map(|o| o.extra.clone()).unwrap_or_default();

    for (k, v) in input.fields {
        tool_config.insert(k, v);
    }

    if let Some(o) = old {
        for (k, v) in &o.extra {
            extra.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    let mut history = old.map(|o| o.history.clone()).unwrap_or_default();
    normalize_provider_history(&mut history);

    ProviderRecord {
        core: ProviderCore {
            id: input.id,
            name: input.name,
            tool: input.tool,
            api_key: input.api_key,
            code: input.code,
            base_url: input.base_url,
            model: input.model,
        },
        runtime_policy: ProviderRuntimePolicy {
            approval_policy: tool_config
                .get("approval_policy")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            sandbox_mode: tool_config
                .get("sandbox_mode")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        },
        favorite_at: input
            .favorite_at
            .or_else(|| old.and_then(|o| o.favorite_at)),
        tool_config,
        history,
        extra,
        is_enabled: input.is_enabled,
        provider_key: input.provider_key,
    }
}

fn read_providers_state_from_path(path: &Path) -> Result<ProvidersState, String> {
    if !path.exists() {
        return Ok(ProvidersState::default());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(ProvidersState::default());
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if let Ok(value) = CryptoService::decrypt_json(&blob) {
            if let Ok(state) = serde_json::from_value::<ProvidersState>(value) {
                return Ok(state);
            }
        }
    }

    serde_json::from_str::<ProvidersState>(&content).map_err(|e| e.to_string())
}

pub(crate) fn load_providers_state() -> Result<ProvidersState, String> {
    if StorageEngine::service_providers_path()?.exists() {
        let service_state = load_service_providers_state()?;
        return Ok(service_providers_to_provider_state(&service_state));
    }

    let state = read_providers_state_from_path(&StorageEngine::providers_path()?)?;
    if state
        .providers
        .iter()
        .any(|provider| provider_id_needs_uuid_migration(&provider.core.id))
    {
        let service_state = load_service_providers_state()?;
        return Ok(service_providers_to_provider_state(&service_state));
    }
    Ok(state)
}

fn load_legacy_providers_state_raw() -> Result<ProvidersState, String> {
    read_providers_state_from_path(&StorageEngine::providers_path()?)
}

fn save_providers_state_from_service_state(state: &ProvidersState) -> Result<SchemaMeta, String> {
    let mut service_state = migrate_providers_to_service_providers(state.clone());
    let (id_map, changed) = normalize_service_provider_ids(&mut service_state);
    let schema = save_service_providers_internal(&service_state)?;
    if changed {
        apply_provider_id_map_to_dependent_state(&id_map)?;
    }
    let legacy_state = service_providers_to_provider_state(&service_state);
    let _ = write_legacy_cli_providers_snapshot(&legacy_state);
    Ok(schema)
}

pub(crate) fn save_providers_state(state: &ProvidersState) -> Result<SchemaMeta, String> {
    save_providers_state_from_service_state(state)
}

fn write_legacy_cli_providers_snapshot(state: &ProvidersState) -> Result<(), String> {
    let data_dir = crate::get_data_dir()?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let target = data_dir.join("providers.json");

    let providers: Vec<Value> = state
        .providers
        .iter()
        .map(|p| {
            let mut obj = json!({
                "id": p.core.id,
                "name": p.core.name,
                "tool": p.core.tool,
            });
            if let Some(ref code) = p.core.code {
                obj["code"] = json!(code);
            }
            obj
        })
        .collect();

    let payload = json!({
        "active_claude": state.active.get("claude").cloned().unwrap_or_default(),
        "active_codex": state.active.get("codex").cloned().unwrap_or_default(),
        "active_gemini": state.active.get("gemini").cloned().unwrap_or_default(),
        "active_opencode": state.active.get("opencode").cloned().unwrap_or_default(),
        "providers": providers,
    });

    let content = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    crate::atomic_write_string(&target, &content)
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
        providers,
    }
}

fn restore_missing_service_provider_api_keys_from_legacy(
    state: &mut ServiceProvidersState,
) -> Result<bool, String> {
    if !StorageEngine::providers_path()?.exists() {
        return Ok(false);
    }

    let legacy = load_legacy_providers_state_raw()?;
    let mut changed = false;

    for service_provider in state.providers.iter_mut() {
        if api_key_has_value(&service_provider.api_key) {
            continue;
        }
        if let Some(legacy_provider) = legacy.providers.iter().find(|provider| {
            api_key_has_value(&provider.core.api_key)
                && provider_record_matches_service_provider(provider, service_provider)
        }) {
            service_provider.api_key = legacy_provider.core.api_key.clone();
            changed = true;
        }
    }

    Ok(changed)
}

/// Load service providers state, auto-migrating from old providers.json if needed.
pub(crate) fn load_service_providers_state() -> Result<ServiceProvidersState, String> {
    let (state, id_map) = load_service_providers_state_with_id_map()?;
    if !id_map.is_empty() {
        apply_provider_id_map_to_dependent_state(&id_map)?;
        let legacy_state = service_providers_to_provider_state(&state);
        let _ = write_legacy_cli_providers_snapshot(&legacy_state);
    }
    Ok(state)
}

fn load_service_providers_state_with_id_map(
) -> Result<(ServiceProvidersState, HashMap<String, String>), String> {
    let path = StorageEngine::service_providers_path()?;
    if path.exists() {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if content.trim().is_empty() {
            return Ok((ServiceProvidersState::default(), HashMap::new()));
        }
        if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
            if let Ok(value) = CryptoService::decrypt_json(&blob) {
                if let Ok(mut state) = serde_json::from_value::<ServiceProvidersState>(value) {
                    let (id_map, changed) = normalize_loaded_service_providers_state(&mut state)?;
                    if changed {
                        save_service_providers_internal(&state)?;
                    }
                    return Ok((state, id_map));
                }
            }
        }
        let mut state =
            serde_json::from_str::<ServiceProvidersState>(&content).map_err(|e| e.to_string())?;
        let (id_map, changed) = normalize_loaded_service_providers_state(&mut state)?;
        if changed {
            save_service_providers_internal(&state)?;
        }
        return Ok((state, id_map));
    }

    // Try to migrate from old providers.json
    let old_path = StorageEngine::providers_path()?;
    if old_path.exists() {
        let old = load_legacy_providers_state_raw()?;
        let mut new = migrate_providers_to_service_providers(old);
        let (id_map, _) = normalize_service_provider_ids(&mut new);
        save_service_providers_internal(&new)?;
        let legacy_state = service_providers_to_provider_state(&new);
        let _ = write_legacy_cli_providers_snapshot(&legacy_state);
        return Ok((new, id_map));
    }

    Ok((ServiceProvidersState::default(), HashMap::new()))
}

/// Save service providers state (internal, no side effects).
pub(crate) fn save_service_providers_internal(
    state: &ServiceProvidersState,
) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::service_providers_path()?, &blob)?;

    let legacy_state = service_providers_to_provider_state(state);
    let legacy_value = serde_json::to_value(&legacy_state).map_err(|e| e.to_string())?;
    let legacy_blob = CryptoService::encrypt_json(&legacy_value)?;
    StorageEngine::write_json(&StorageEngine::providers_path()?, &legacy_blob)?;
    let _ = write_legacy_cli_providers_snapshot(&legacy_state);

    StorageEngine::bump_revision()
}

fn load_sessions_state() -> Result<SessionsState, String> {
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

fn save_sessions_state(state: &SessionsState) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::sessions_path()?, &blob)?;
    StorageEngine::bump_revision()
}

fn cli_session_lookup_from_record(session: &SessionRecord) -> CliSessionLookup {
    CliSessionLookup {
        id: session.id.trim().to_string(),
        tool: session.tool.trim().to_string(),
        tool_session_id: session.tool_session_id.trim().to_string(),
        working_dir: session.working_dir.trim().to_string(),
    }
}

fn find_cli_session_in_state(state: &SessionsState, query: &str) -> Option<CliSessionLookup> {
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

fn history_tombstone_key(tool: &str, tool_session_id: &str) -> Option<String> {
    let normalized_tool = tool.trim().to_lowercase();
    let normalized_session_id = tool_session_id.trim();
    if normalized_tool.is_empty() || normalized_session_id.is_empty() {
        return None;
    }
    Some(format!("{}::{}", normalized_tool, normalized_session_id))
}

fn history_sync_requires_full_backfill(
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

fn stable_history_session_record_id(tool: &str, tool_session_id: &str) -> String {
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

fn history_entry_time_secs(entry: &ai_sessions::HistorySessionEntry) -> (u64, u64) {
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

fn normalize_session_working_dir(value: &str) -> String {
    ai_sessions::normalize_working_dir_for_terminal(value)
}

fn same_session_working_dir(left: &str, right: &str) -> bool {
    normalize_session_working_dir(left) == normalize_session_working_dir(right)
}

fn should_bind_history_entry_to_placeholder(
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

fn placeholder_preference_score(
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
struct SessionsHistorySyncOutcome {
    persisted: bool,
    list_changed: bool,
}

fn merge_history_entry_into_session(
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

fn apply_history_entries_to_sessions_state(
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

fn sessions_history_sync_tool(tool: String) -> Result<SessionsHistorySyncOutcome, String> {
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

fn load_launcher_state() -> Result<LauncherState, String> {
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

fn save_launcher_state(state: &LauncherState) -> Result<SchemaMeta, String> {
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    let blob = CryptoService::encrypt_json(&value)?;
    StorageEngine::write_json(&StorageEngine::launcher_path()?, &blob)?;
    StorageEngine::bump_revision()
}

fn load_outbox_state() -> Result<OutboxState, String> {
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

fn save_outbox_state(state: &OutboxState) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::outbox_path()?, state)
}

fn load_migration_state() -> Result<MigrationState, String> {
    StorageEngine::read_json(&StorageEngine::migration_state_path()?)
}

fn save_migration_state(state: &MigrationState) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::migration_state_path()?, state)
}

fn get_meta() -> Result<ApiMeta, String> {
    let schema = StorageEngine::load_schema()?;
    Ok(ApiMeta {
        schema_version: schema.schema_version,
        revision: schema.revision,
    })
}

fn parse_json_array_len(raw: &str) -> usize {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.as_array().map(|arr| arr.len()))
        .unwrap_or(0)
}

fn extract_fields(value: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            match k.as_str() {
                "id" | "name" | "tool" | "api_key" | "base_url" | "model" | "is_enabled"
                | "provider_key" | "code" => {}
                _ => {
                    out.insert(k.clone(), v.clone());
                }
            }
        }
    }
    out
}
