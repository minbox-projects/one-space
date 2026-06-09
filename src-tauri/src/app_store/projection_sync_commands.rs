use super::{
    api_error, api_ok, apply_projection, build_projection_diff, enqueue_sync_event, get_meta,
    load_migration_state, load_outbox_state, load_service_providers_state, load_sessions_state,
    lock_sessions_state_write, materialize_isolated_claude_profile_async, now_ts,
    process_sync_queue, process_sync_queue_impl, rollback_from_backup, run_migration_impl,
    save_migration_state, save_sessions_state, service_provider_to_provider_record,
    session_to_legacy, ApiErr, ApiMeta, ApiOk, MigrationState, OutboxState, SessionRecord,
    SessionsState, SCHEMA_VERSION,
};
use serde_json::{json, Value};
use tauri::Emitter;

#[tauri::command]
pub fn projection_dry_run(tool: String, provider_id: String) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let service_provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == tool)
        .ok_or_else(|| api_error("not_found", "provider not found"))?;
    let provider = service_provider_to_provider_record(service_provider);

    let diffs = build_projection_diff(&provider).map_err(|e| api_error("projection_failed", e))?;
    api_ok(
        json!({ "changes": diffs }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn projection_apply(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let service_provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == tool)
        .cloned()
        .ok_or_else(|| api_error("not_found", "provider not found"))?;

    if tool == "claude" {
        // Dual-write for Claude: profile dir + global ~/.claude
        materialize_isolated_claude_profile_async(&service_provider)
            .await
            .map_err(|e| api_error("projection_failed", e))?;
    }

    let provider = service_provider_to_provider_record(&service_provider);
    apply_projection(&provider).map_err(|e| api_error("projection_failed", e))?;

    enqueue_sync_event("projection", "projection_apply").map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({ "applied": true }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn sync_enqueue(app: tauri::AppHandle, reason: String) -> Result<ApiOk<Value>, ApiErr> {
    enqueue_sync_event("manual", &reason).map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "queued": true }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn sync_run_now(app: tauri::AppHandle) -> Result<ApiOk<Value>, ApiErr> {
    process_sync_queue_impl(app, true)
        .await
        .map_err(|e| api_error("sync_error", e))?;
    let outbox = load_outbox_state().map_err(|e| api_error("io_error", e))?;
    api_ok(
        serde_json::to_value(outbox).map_err(|e| api_error("serialize_error", e.to_string()))?,
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn sync_status() -> Result<ApiOk<OutboxState>, ApiErr> {
    let outbox = load_outbox_state().map_err(|e| api_error("io_error", e))?;
    api_ok(outbox, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub fn migration_status() -> Result<ApiOk<MigrationState>, ApiErr> {
    let state = load_migration_state().map_err(|e| api_error("io_error", e))?;
    api_ok(
        state,
        get_meta().unwrap_or(ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision: 0,
        }),
    )
}

#[tauri::command]
pub fn migration_run() -> Result<ApiOk<MigrationState>, ApiErr> {
    let state = run_migration_impl().map_err(|e| api_error("migration_failed", e))?;
    api_ok(state, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub fn migration_rollback(backup_id: String) -> Result<ApiOk<Value>, ApiErr> {
    rollback_from_backup(&backup_id).map_err(|e| api_error("rollback_failed", e))?;
    let mut state = load_migration_state().map_err(|e| api_error("io_error", e))?;
    state.migrated = false;
    state.last_error = None;
    save_migration_state(&state).map_err(|e| api_error("io_error", e))?;
    api_ok(
        json!({ "rolled_back": true, "backup_id": backup_id }),
        get_meta().unwrap_or(ApiMeta {
            schema_version: SCHEMA_VERSION,
            revision: 0,
        }),
    )
}

/// Core favorite logic, extracted for testability without Tauri runtime.
#[allow(dead_code)]
pub(in crate::app_store) fn set_session_favorite_impl(
    state: &mut SessionsState,
    session_id: &str,
    favorite: bool,
) -> Result<SessionRecord, ApiErr> {
    let record = state
        .sessions
        .iter_mut()
        .find(|s| s.id == session_id)
        .ok_or_else(|| api_error("not_found", "session not found"))?;

    if favorite {
        if record.favorited_at.is_none() {
            record.favorited_at = Some(now_ts());
        }
    } else {
        record.favorited_at = None;
    }

    let updated = record.clone();
    Ok(updated)
}

/// Set or unset the favorite status of a session.
/// When setting favorite, records the current timestamp as favorited_at.
/// Re-setting favorite to true keeps the original timestamp (idempotent).
#[tauri::command]
pub async fn sessions_set_favorite(
    app: tauri::AppHandle,
    session_id: String,
    favorite: bool,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let updated = {
        let _sessions_state_guard =
            lock_sessions_state_write().map_err(|e| api_error("io_error", e))?;
        let mut state = load_sessions_state().map_err(|e| api_error("io_error", e))?;
        let updated = set_session_favorite_impl(&mut state, &session_id, favorite)?;
        save_sessions_state(&state).map_err(|e| api_error("io_error", e))?;
        updated
    };

    let _ = app.emit("sessions-updated", ());

    api_ok(
        session_to_legacy(&updated),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}
