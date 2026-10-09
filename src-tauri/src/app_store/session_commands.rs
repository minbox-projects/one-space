use super::{
    api_error, api_ok, filter_sessions_by_history_window, get_meta, history_tombstone_key,
    load_service_providers_state, load_sessions_state, lock_sessions_state_write,
    materialize_isolated_claude_profile, materialize_isolated_claude_profile_async,
    normalize_runtime_mode, now_ts, run_migration_impl, save_sessions_state,
    session_to_legacy, validate_provider_uuid_param, ApiErr, ApiMeta, ApiOk, SessionInput,
    SessionRecord,
};
use crate::{ai_sessions, workspaces};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

#[tauri::command]
pub fn sessions_list() -> Result<ApiOk<Vec<Value>>, ApiErr> {
    let _sessions_state_guard =
        lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let mut normalized = false;

    let mut owner_by_tool_session = HashMap::<(String, String), String>::new();
    for session in state.sessions.iter_mut() {
        let tool_session_id = session.tool_session_id.trim();
        if tool_session_id.is_empty() {
            continue;
        }
        let key = (session.tool.clone(), tool_session_id.to_string());
        if let Some(owner_id) = owner_by_tool_session.get(&key) {
            if owner_id != &session.id {
                session.tool_session_id.clear();
                session.status = "unbound".to_string();
                normalized = true;
                continue;
            }
        } else {
            owner_by_tool_session.insert(key, session.id.clone());
        }
    }
    if normalized {
        let _ = save_sessions_state(&state);
    }

    let filtered = filter_sessions_by_history_window(state.sessions.iter());
    api_ok(
        filtered.iter().map(session_to_legacy).collect(),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

/// 通过 provider_id 解析 Claude profile 的配置目录路径。
/// 加载 providers state，查找对应 provider，使用其 code 或 id 作为目录名。
pub(in crate::app_store) fn resolve_claude_config_dir_for_provider_id(
    provider_id: &str,
) -> Result<PathBuf, String> {
    let state = load_service_providers_state()?;
    let provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == "claude")
        .cloned()
        .ok_or_else(|| format!("Claude service provider not found: {provider_id}"))?;
    materialize_isolated_claude_profile(&provider)
}

pub(in crate::app_store) async fn resolve_claude_config_dir_for_provider_id_async(
    provider_id: &str,
) -> Result<PathBuf, String> {
    let state = load_service_providers_state()?;
    let provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == "claude")
        .cloned()
        .ok_or_else(|| format!("Claude service provider not found: {provider_id}"))?;
    materialize_isolated_claude_profile_async(&provider).await
}

pub(in crate::app_store) async fn launch_options_for_session_async(
    record: &SessionRecord,
) -> Result<ai_sessions::LaunchOptions, String> {
    let mode = normalize_runtime_mode(Some(&record.runtime_mode));
    let mut env: HashMap<String, String> = HashMap::new();

    if mode == "strict" {
        let profile_id = record
            .runtime_profile_id
            .clone()
            .ok_or_else(|| "strict runtime profile id is required".to_string())?;
        let strict_env = crate::runtime_profiles::runtime_env_for_profile(&profile_id)?;
        env.extend(strict_env);
    }

    if record.tool == "claude" {
        if let Some(provider_id) = &record.provider_id {
            validate_provider_uuid_param(provider_id)?;
            let config_dir = resolve_claude_config_dir_for_provider_id_async(provider_id).await?;
            env.insert(
                "CLAUDE_CONFIG_DIR".to_string(),
                config_dir.to_string_lossy().to_string(),
            );
        }
    }

    Ok(ai_sessions::LaunchOptions {
        env: if env.is_empty() { None } else { Some(env) },
        initial_prompt: None,
    })
}

pub(in crate::app_store) async fn lookup_env_for_session_async(
    record: &SessionRecord,
) -> Result<Option<HashMap<String, String>>, String> {
    let mode = normalize_runtime_mode(Some(&record.runtime_mode));
    let mut env: HashMap<String, String> = HashMap::new();

    if mode == "strict" {
        let profile_id = match record.runtime_profile_id.as_ref() {
            Some(profile_id) => profile_id,
            None => return Ok(None),
        };
        let strict_env = crate::runtime_profiles::runtime_env_for_profile(profile_id)?;
        env.extend(strict_env);
    }

    if record.tool == "claude" {
        if let Some(provider_id) = &record.provider_id {
            validate_provider_uuid_param(provider_id)?;
            let config_dir = resolve_claude_config_dir_for_provider_id_async(provider_id).await?;
            env.insert(
                "CLAUDE_CONFIG_DIR".to_string(),
                config_dir.to_string_lossy().to_string(),
            );
        }
    }

    if env.is_empty() {
        Ok(None)
    } else {
        Ok(Some(env))
    }
}

pub(in crate::app_store) fn apply_resolved_session_id_after_create(
    session: &mut SessionRecord,
    resolved_tool_session_id: Option<&str>,
    now: u64,
) {
    session.last_used_at = now;
    if let Some(tool_session_id) = resolved_tool_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        session.tool_session_id = tool_session_id.to_string();
        session.status = "active".to_string();
    } else {
        session.tool_session_id.clear();
        session.status = "pending_bind".to_string();
    }
}

#[tauri::command]
pub async fn sessions_create(
    app: tauri::AppHandle,
    session: SessionInput,
) -> Result<ApiOk<Value>, ApiErr> {
    let (response, ()) = super::session_service::create_session(
        session,
        |record, requested_id, permission_mode, options| {
            ai_sessions::launch_native_session_for_create_with_options(
                &record.working_dir,
                &record.tool,
                requested_id,
                permission_mode,
                options,
            )
            .map(|id| (id, ()))
        },
    )
    .await?;
    workspaces::schedule_sync_from_sessions(app);
    api_ok(session_to_legacy(&response.data), response.meta)
}

#[tauri::command]
pub async fn sessions_update(
    app: tauri::AppHandle,
    session: SessionInput,
) -> Result<ApiOk<Value>, ApiErr> {
    let _sessions_state_guard =
        lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let id = session
        .id
        .clone()
        .ok_or_else(|| api_error("invalid_payload", "session.id required"))?;

    // Reload from disk right before saving to avoid overwriting concurrent changes
    // (e.g., history sync adding new sessions, concurrent favorite changes).
    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;

    // Capture the values we want to apply.
    let update_name = session.name.clone();
    let update_name_source = "manual".to_string();
    let update_working_dir = ai_sessions::normalize_working_dir_for_terminal(&session.working_dir);
    let update_tool = session.tool.clone();
    let update_runtime_mode = session.runtime_mode.is_some();
    let update_runtime_mode_val = normalize_runtime_mode(session.runtime_mode.as_deref());
    let update_runtime_profile_id = session.runtime_profile_id.clone().and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let update_preset_id = session.preset_id.clone().and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let update_last_used = now_ts();

    let mut found = false;
    for s in state.sessions.iter_mut() {
        if s.id == id {
            if let Some(tool_session_id) = &session.tool_session_id {
                let requested = tool_session_id.trim();
                if requested != s.tool_session_id {
                    return Err(api_error(
                        "IMMUTABLE_FIELD",
                        "tool_session_id is system-managed and cannot be updated",
                    ));
                }
            }
            if let Some(status) = &session.status {
                let requested_status = status.trim();
                if !requested_status.is_empty() && requested_status != s.status {
                    return Err(api_error(
                        "IMMUTABLE_FIELD",
                        "status is system-managed and cannot be updated",
                    ));
                }
            }
            if let Some(provider_id) = &session.provider_id {
                if !provider_id.trim().is_empty()
                    && s.provider_id.as_deref() != Some(provider_id.trim())
                {
                    validate_provider_uuid_param(provider_id.trim())
                        .map_err(|e| api_error("invalid_payload", e))?;
                    return Err(api_error(
                        "IMMUTABLE_FIELD",
                        "provider_id is system-managed and cannot be updated",
                    ));
                }
            }
            if s.name != update_name {
                s.name = update_name.clone();
                s.name_source = update_name_source.clone();
            }
            s.working_dir = update_working_dir.clone();
            s.tool = update_tool.clone();
            if update_runtime_mode {
                s.runtime_mode = update_runtime_mode_val.clone();
                if s.runtime_mode != "strict" {
                    s.runtime_profile_id = None;
                }
            }
            if session.runtime_profile_id.is_some() {
                s.runtime_profile_id = update_runtime_profile_id.clone();
            }
            if session.preset_id.is_some() {
                s.preset_id = update_preset_id.clone();
            }
            s.last_used_at = update_last_used;
            found = true;
            break;
        }
    }

    if !found {
        return Err(api_error("not_found", "session not found"));
    }

    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
    workspaces::schedule_sync_from_sessions(app);

    let updated = state
        .sessions
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| api_error("not_found", "session not found"))?;

    api_ok(
        session_to_legacy(&updated),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn sessions_delete(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    let _sessions_state_guard =
        lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let tombstone_key = state
        .sessions
        .iter()
        .find(|s| s.id == session_id)
        .and_then(|session| history_tombstone_key(&session.tool, &session.tool_session_id));
    if let Some(key) = &tombstone_key {
        state.tombstones.insert(key.clone());
    }
    state.sessions.retain(|s| s.id != session_id);
    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
    workspaces::schedule_sync_from_sessions(app);

    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

/// Read the configured permission mode for a given tool. Defaults to Default.
pub(crate) fn resolve_permission_mode_for_tool(tool: &str) -> ai_sessions::TerminalPermissionMode {
    let key = tool.trim().to_lowercase();
    if let Ok(cfg) = crate::config::get_config() {
        if let Some(modes) = &cfg.ai_model_permission_modes {
            if let Some(value) = modes.get(&key) {
                return ai_sessions::TerminalPermissionMode::from_str(value);
            }
        }
    }
    ai_sessions::TerminalPermissionMode::Default
}

pub(in crate::app_store) fn resolve_working_dir_for_session_create(
    session: &SessionInput,
) -> String {
    let provided = session.working_dir.trim();
    if !provided.is_empty() {
        return provided.to_string();
    }

    let is_claude_provider_launch = session.tool.trim().eq_ignore_ascii_case("claude")
        && session
            .provider_id
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);

    if !is_claude_provider_launch {
        return String::new();
    }

    crate::config::get_config()
        .ok()
        .and_then(|cfg| cfg.claude_provider_launch_dir)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
}

/// Validate caller's permission mode against config and resolve the effective mode.
/// - config=default, caller=full_access → INVALID_PERMISSION_MODE
/// - config=full_access, caller=missing  → PERMISSION_CONFIRMATION_REQUIRED
/// - config=full_access, caller=default  → Default
/// - config=full_access, caller=full_access → FullAccess
/// - config=default, caller=default or missing → Default
pub(in crate::app_store) fn validate_and_resolve_permission_mode(
    config_mode: &ai_sessions::TerminalPermissionMode,
    caller_mode: Option<&str>,
) -> Result<ai_sessions::TerminalPermissionMode, ApiErr> {
    // Strictly parse caller mode — only 'default' and 'full_access' are valid
    let parsed_caller = caller_mode
        .map(|v| match ai_sessions::TerminalPermissionMode::from_str(v) {
            ai_sessions::TerminalPermissionMode::Default => {
                // from_str maps unknown values to Default, but we want strict validation
                if v == "default" {
                    Ok(ai_sessions::TerminalPermissionMode::Default)
                } else {
                    Err(api_error(
                        "INVALID_PERMISSION_MODE",
                        "permission_mode must be 'default' or 'full_access'",
                    ))
                }
            }
            ai_sessions::TerminalPermissionMode::FullAccess => {
                if v == "full_access" {
                    Ok(ai_sessions::TerminalPermissionMode::FullAccess)
                } else {
                    Err(api_error(
                        "INVALID_PERMISSION_MODE",
                        "permission_mode must be 'default' or 'full_access'",
                    ))
                }
            }
        })
        .transpose()?;

    match (config_mode, parsed_caller) {
        // config default: caller cannot elevate to full_access
        (
            ai_sessions::TerminalPermissionMode::Default,
            Some(ai_sessions::TerminalPermissionMode::FullAccess),
        ) => Err(api_error(
            "INVALID_PERMISSION_MODE",
            "cannot elevate to full_access when tool is configured as default",
        )),
        // config full_access: caller must confirm
        (ai_sessions::TerminalPermissionMode::FullAccess, None) => Err(api_error(
            "PERMISSION_CONFIRMATION_REQUIRED",
            "tool is configured as full_access; caller must confirm permission mode",
        )),
        // config full_access with explicit caller confirmation
        (_, Some(mode)) => Ok(mode),
        // config default with explicit default or missing → Default
        _ => Ok(ai_sessions::TerminalPermissionMode::Default),
    }
}

#[tauri::command]
pub async fn sessions_launch(
    app: tauri::AppHandle,
    session_id: String,
    permission_mode: Option<String>,
) -> Result<ApiOk<Value>, ApiErr> {
    sessions_launch_impl(app, session_id, permission_mode, None).await
}

pub(crate) async fn sessions_launch_impl(
    app: tauri::AppHandle,
    session_id: String,
    permission_mode: Option<String>,
    initial_prompt: Option<String>,
) -> Result<ApiOk<Value>, ApiErr> {
    let (response, ()) = super::session_service::resume_session(
        &session_id,
        permission_mode.as_deref(),
        initial_prompt.as_deref(),
        |record, permission_mode, options| {
            ai_sessions::launch_native_session_with_options(
                &record.working_dir,
                &record.tool,
                &record.tool_session_id,
                permission_mode,
                options,
            )
        },
    )
    .await?;
    workspaces::schedule_sync_from_sessions(app);
    api_ok(session_to_legacy(&response.data), response.meta)
}
