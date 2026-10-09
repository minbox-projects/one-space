use super::{
    acquire_session_create_lock, api_error, api_ok, apply_resolved_session_id_after_create,
    cli_lookup_session, launch_options_for_session_async, load_service_providers_state,
    load_sessions_state, lock_sessions_state_write, lookup_env_for_session_async,
    normalize_runtime_mode, now_ts, release_session_create_lock,
    resolve_permission_mode_for_tool, resolve_working_dir_for_session_create, run_migration_impl,
    save_sessions_state, session_install_scope_and_root, validate_and_resolve_permission_mode,
    validate_provider_uuid_option, validate_service_provider_reference, ApiErr, ApiMeta, ApiOk,
    SessionInput, SessionRecord, MANAGED_TOOLS,
};
use crate::{ai_sessions, workspaces};
use std::collections::HashSet;
use std::path::Path;
use std::process::ExitStatus;

struct CreateGuard(String);

impl Drop for CreateGuard {
    fn drop(&mut self) {
        release_session_create_lock(&self.0);
    }
}

/// Canonical creation shared by GUI and current-terminal launch adapters.
/// The adapter returns a discovered native ID and its own launch result; a
/// successfully started process's nonzero exit is a result, not a spawn error.
pub async fn create_session<F, T>(
    session: SessionInput,
    launch: F,
) -> Result<(ApiOk<SessionRecord>, T), ApiErr>
where
    F: FnOnce(
        &SessionRecord,
        Option<&str>,
        ai_sessions::TerminalPermissionMode,
        &ai_sessions::LaunchOptions,
    ) -> Result<(Option<String>, T), String>,
{
    let tool = session.tool.trim().to_lowercase();
    if !MANAGED_TOOLS.contains(&tool.as_str()) {
        return Err(api_error(
            "CLI_UNSUPPORTED",
            "Unsupported model type for native session",
        ));
    }
    {
        let _migration_guard = lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
        run_migration_impl().map_err(|e| api_error("migration_failed", e))?;
    }

    let mut input = session;
    input.tool = tool;
    let working_dir = ai_sessions::normalize_working_dir_for_terminal(
        &resolve_working_dir_for_session_create(&input),
    );
    if !Path::new(&working_dir).is_dir() {
        return Err(api_error("invalid_payload", "working directory not found"));
    }
    validate_provider_uuid_option(input.provider_id.as_deref())
        .map_err(|e| api_error("invalid_payload", e))?;
    let provider_id = input
        .provider_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(provider_id) = provider_id.as_deref() {
        validate_service_provider_reference(&input.tool, provider_id)
            .map_err(|e| api_error("invalid_payload", e))?;
    }
    let runtime_mode = normalize_runtime_mode(input.runtime_mode.as_deref());
    let runtime_profile_id = if runtime_mode == "strict" {
        input
            .runtime_profile_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    } else {
        None
    };
    let now = now_ts();
    let record = SessionRecord {
        id: input
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name.clone(),
        working_dir,
        tool: input.tool.clone(),
        tool_session_id: String::new(),
        model_name: None,
        name_source: if input.name.trim().is_empty() {
            "history"
        } else {
            "manual"
        }
        .to_string(),
        runtime_mode,
        runtime_profile_id,
        preset_id: input
            .preset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        created_at: now,
        last_used_at: now,
        status: "pending_bind".to_string(),
        favorited_at: None,
        provider_id,
    };
    let permission_mode = validate_and_resolve_permission_mode(
        &resolve_permission_mode_for_tool(&record.tool),
        input.permission_mode.as_deref(),
    )?;
    let mut options = launch_options_for_session_async(&record)
        .await
        .map_err(|e| api_error("launch_failed", e))?;
    options.initial_prompt = input
        .initial_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let key = format!(
        "{}|{}|{}|{}|{}",
        record.tool,
        record.working_dir,
        record.runtime_mode,
        record.runtime_profile_id.as_deref().unwrap_or_default(),
        record.preset_id.as_deref().unwrap_or_default()
    );
    let _create_guard = CreateGuard(
        acquire_session_create_lock(key)
            .map_err(|e| api_error("io_error", e))?
            .ok_or_else(|| {
                api_error(
                    "SESSION_CREATE_DUPLICATED",
                    "duplicate create request in progress",
                )
            })?,
    );

    // Prepare workspace capabilities before publishing a pending record.
    workspaces::apply_workspace_mcp_for_session(&record.working_dir, &record.tool)
        .map_err(|e| api_error("workspace_mcp_apply_failed", e))?;
    {
        let _state_guard = lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
        let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
        if state.sessions.iter().any(|item| item.id == record.id) {
            return Err(api_error("invalid_payload", "session id already exists"));
        }
        state.sessions.push(record.clone());
        save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
    }

    let (resolved_id, output) = match launch(
        &record,
        input.tool_session_id.as_deref(),
        permission_mode,
        &options,
    ) {
        Ok(result) => result,
        Err(error) => {
            let _state_guard = lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
            let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
            state.sessions.retain(|item| item.id != record.id);
            save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
            return Err(api_error("launch_failed", error));
        }
    };
    let _state_guard = lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let resolved_id = resolved_id.filter(|id| {
        !state.sessions.iter().any(|item| {
            item.id != record.id && item.tool == record.tool && item.tool_session_id == id.trim()
        })
    });
    let item = state
        .sessions
        .iter_mut()
        .find(|item| item.id == record.id)
        .ok_or_else(|| api_error("not_found", "session not found after create"))?;
    // History may already have bound this record while a CLI child was running.
    if item.tool_session_id.is_empty() {
        apply_resolved_session_id_after_create(item, resolved_id.as_deref(), now_ts());
    }
    let final_record = item.clone();
    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
    let response = api_ok(
        final_record,
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )?;
    Ok((response, output))
}

/// Launch a canonical session in the caller's terminal with unchanged extra argv.
pub async fn create_session_in_current_terminal(
    session: SessionInput,
    args: &[String],
) -> Result<(ApiOk<SessionRecord>, ExitStatus), ApiErr> {
    create_session(session, |record, requested_id, permission_mode, options| {
        options.launch_create_in_current_terminal(
            &record.working_dir,
            &record.tool,
            requested_id,
            permission_mode,
            args,
        )
    })
    .await
}

/// Args following the installed `onespace ai` command: tool, optional display
/// name, optional `--permission-mode <mode>` confirmation, then native argv. The
/// working directory is the invoking terminal's.
pub async fn create_cli_session(
    args: &[String],
) -> Result<(ApiOk<SessionRecord>, ExitStatus), ApiErr> {
    const AI_USAGE: &str =
        "Usage: onespace ai <tool> [display name] [extra args...] [--permission-mode default|full_access]";
    let tool = args
        .first()
        .map(|value| value.trim().to_lowercase())
        .ok_or_else(|| api_error("invalid_payload", AI_USAGE))?;
    if !MANAGED_TOOLS.contains(&tool.as_str()) {
        return Err(api_error(
            "CLI_UNSUPPORTED",
            "Unsupported model type for native session",
        ));
    }
    // Consume the first `--permission-mode <mode>` pair so the remaining
    // positional args stay tool, optional display name, then native argv. The
    // mode is validated downstream against the configured permission rules; it
    // is never treated as the display name or forwarded to the native tool.
    let mut positional = args.iter().skip(1).cloned().collect::<Vec<String>>();
    let mut permission_mode: Option<String> = None;
    if let Some(index) = positional.iter().position(|arg| arg == "--permission-mode") {
        if index + 1 >= positional.len() {
            return Err(api_error("invalid_payload", AI_USAGE));
        }
        permission_mode = Some(positional[index + 1].clone());
        positional.drain(index..=index + 1);
    }
    let working_dir = std::env::current_dir().map_err(|e| api_error("io_error", e.to_string()))?;
    let name = positional.first().cloned().unwrap_or_else(|| {
        format!(
            "{}_ai",
            working_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )
    });
    {
        let _migration_guard = lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
        run_migration_impl().map_err(|e| api_error("migration_failed", e))?;
    }
    let provider_id = if tool == "claude" {
        load_service_providers_state()
            .map_err(|e| api_error("io_error", e))?
            .active
            .get("claude")
            .cloned()
    } else {
        None
    };
    let session = SessionInput {
        id: None,
        name,
        working_dir: working_dir.to_string_lossy().to_string(),
        tool,
        tool_session_id: None,
        runtime_mode: None,
        runtime_profile_id: None,
        preset_id: None,
        status: None,
        provider_id,
        initial_prompt: None,
        permission_mode,
    };
    println!(
        "Starting OneSpace AI session: {} ({})",
        session.name, session.tool
    );
    create_session_in_current_terminal(session, positional.get(1..).unwrap_or_default()).await
}

/// Canonical resume preparation shared by GUI and current-terminal adapters.
/// Native-ID binding and last-used updates publish only after launch succeeds;
/// an already started child's nonzero exit remains a successful launch result.
pub async fn resume_session<F, T>(
    session_id: &str,
    permission_mode: Option<&str>,
    initial_prompt: Option<&str>,
    launch: F,
) -> Result<(ApiOk<SessionRecord>, T), ApiErr>
where
    F: FnOnce(
        &SessionRecord,
        ai_sessions::TerminalPermissionMode,
        &ai_sessions::LaunchOptions,
    ) -> Result<T, String>,
{
    let mut target = {
        let _state_guard = lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
        run_migration_impl().map_err(|e| api_error("migration_failed", e))?;
        load_sessions_state()
            .map_err(|e| api_error("io_error", e))?
            .sessions
            .into_iter()
            .find(|item| item.id == session_id)
            .ok_or_else(|| api_error("not_found", "session not found"))?
    };
    if !MANAGED_TOOLS.contains(&target.tool.as_str()) {
        return Err(api_error(
            "CLI_UNSUPPORTED",
            "Unsupported model type for native session",
        ));
    }
    if target.working_dir.trim().is_empty() {
        return Err(api_error(
            "invalid_payload",
            "session working directory is missing",
        ));
    }
    target.working_dir = ai_sessions::normalize_working_dir_for_terminal(&target.working_dir);
    if !Path::new(&target.working_dir).is_dir() {
        return Err(api_error("invalid_payload", "working directory not found"));
    }
    let permission_mode = validate_and_resolve_permission_mode(
        &resolve_permission_mode_for_tool(&target.tool),
        permission_mode,
    )?;

    if target.status == "unbound"
        || target.status == "pending_bind"
        || target.tool_session_id.trim().is_empty()
    {
        let occupied_ids = {
            let state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
            state
                .sessions
                .iter()
                .filter(|item| item.id != target.id && item.tool == target.tool)
                .map(|item| item.tool_session_id.trim().to_string())
                .filter(|id| !id.is_empty())
                .collect::<HashSet<_>>()
        };
        let lookup_env = lookup_env_for_session_async(&target)
            .await
            .map_err(|e| api_error("launch_failed", e))?;
        target.tool_session_id = ai_sessions::resolve_native_session_id_for_existing(
            &target.tool,
            &target.working_dir,
            lookup_env.as_ref(),
            Some((target.created_at as i64) * 1000),
            Some(&occupied_ids),
            target.status == "pending_bind",
        )
        .ok_or_else(|| {
            api_error(
                "SESSION_ID_MISSING",
                "session tool_session_id is empty; wait for native history, then retry",
            )
        })?;
    }
    {
        let state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
        if state.sessions.iter().any(|item| {
            item.id != target.id
                && item.tool == target.tool
                && item.tool_session_id.trim() == target.tool_session_id.trim()
        }) {
            return Err(api_error(
                "SESSION_ID_CONFLICT",
                "tool_session_id is already bound to another session",
            ));
        }
    }

    let (install_scope, install_project_root) = session_install_scope_and_root(&target);
    crate::skills::skills_reconcile_for_tool(
        &target.tool,
        Some(install_scope.as_str()),
        install_project_root.as_deref(),
    )
    .map_err(|e| api_error("skills_preflight_failed", e))?;
    crate::subagents::subagents_reconcile_for_tool(
        &target.tool,
        Some(install_scope.as_str()),
        install_project_root.as_deref(),
    )
    .map_err(|e| api_error("subagents_preflight_failed", e))?;
    workspaces::apply_workspace_mcp_for_session(&target.working_dir, &target.tool)
        .map_err(|e| api_error("workspace_mcp_apply_failed", e))?;
    let mut options = launch_options_for_session_async(&target)
        .await
        .map_err(|e| api_error("launch_failed", e))?;
    options.initial_prompt = initial_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let output = launch(&target, permission_mode, &options).map_err(|e| {
        if e.contains("Unsupported model type") {
            api_error("CLI_UNSUPPORTED", e)
        } else {
            api_error("RESUME_FAILED", e)
        }
    })?;

    // A CLI child can run while history or another writer updates canonical state.
    // Re-read and change only this record's binding/last-used fields under the gate.
    let _state_guard = lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
    let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    if state.sessions.iter().any(|item| {
        item.id != target.id
            && item.tool == target.tool
            && item.tool_session_id.trim() == target.tool_session_id.trim()
    }) {
        return Err(api_error(
            "SESSION_ID_CONFLICT",
            "tool_session_id is already bound to another session",
        ));
    }
    let item = state
        .sessions
        .iter_mut()
        .find(|item| item.id == target.id)
        .ok_or_else(|| api_error("not_found", "session not found after resume"))?;
    if item.tool != target.tool
        || (!item.tool_session_id.trim().is_empty()
            && item.tool_session_id.trim() != target.tool_session_id.trim())
    {
        return Err(api_error(
            "SESSION_ID_CONFLICT",
            "session binding changed during resume",
        ));
    }
    item.tool_session_id = target.tool_session_id.trim().to_string();
    item.status = "active".to_string();
    item.last_used_at = item.last_used_at.max(now_ts());
    let record = item.clone();
    let schema = save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
    let response = api_ok(
        record,
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )?;
    Ok((response, output))
}

/// Resume a canonical record in the caller's terminal, preserving child status.
pub async fn resume_session_in_current_terminal(
    session_id: &str,
    permission_mode: Option<&str>,
) -> Result<(ApiOk<SessionRecord>, ExitStatus), ApiErr> {
    resume_session(
        session_id,
        permission_mode,
        None,
        |record, permission_mode, options| {
            options.launch_resume_in_current_terminal(
                &record.working_dir,
                &record.tool,
                &record.tool_session_id,
                permission_mode,
            )
        },
    )
    .await
}

/// Args following `onespace resume`: canonical/native ID, plus an optional
/// explicit permission choice governed by the same rules as GUI resume.
pub async fn resume_cli_session(
    args: &[String],
) -> Result<(ApiOk<SessionRecord>, ExitStatus), ApiErr> {
    let (query, permission_mode) = match args {
        [query] if !query.trim().is_empty() => (query, None),
        [query, option, mode] if !query.trim().is_empty() && option == "--permission-mode" => {
            (query, Some(mode.as_str()))
        }
        _ => {
            return Err(api_error(
                "invalid_payload",
                "Usage: onespace resume <session_id> [--permission-mode default|full_access]",
            ));
        }
    };
    let session_id = {
        let _state_guard = lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
        run_migration_impl().map_err(|e| api_error("migration_failed", e))?;
        cli_lookup_session(query)
            .map_err(|e| api_error("io_error", e))?
            .ok_or_else(|| api_error("not_found", "session not found"))?
            .id
    };
    resume_session_in_current_terminal(&session_id, permission_mode).await
}
