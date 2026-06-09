#[tauri::command]
pub fn ssh_tunnel_groups_list() -> Result<Vec<SshTunnelGroupView>, String> {
    let mut groups = load_state()?.groups;
    sort_groups(&mut groups);
    Ok(groups.iter().map(to_group_view).collect())
}

#[tauri::command]
pub fn ssh_tunnel_group_upsert(
    app: AppHandle,
    input: SshTunnelGroupUpsertInput,
) -> Result<SshTunnelGroupView, String> {
    let group = mutate_state(|state| {
        let editing_id = input.id.as_deref();
        if editing_id == Some(DEFAULT_TUNNEL_GROUP_ID) {
            return Err("The default environment group cannot be renamed".to_string());
        }
        let name = validate_group_name(&state.groups, &input.name, editing_id)?;
        let now = now_ts();
        if let Some(id) = editing_id {
            let group = state
                .groups
                .iter_mut()
                .find(|group| group.id == id)
                .ok_or_else(|| "Environment group not found".to_string())?;
            group.name = name;
            group.updated_at = now;
            return Ok(group.clone());
        }

        let group = SshTunnelGroupRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at: now,
            updated_at: now,
            is_default: false,
        };
        state.groups.push(group.clone());
        Ok(group)
    })?;
    emit_tunnels_updated(&app);
    Ok(to_group_view(&group))
}

#[tauri::command]
pub fn ssh_tunnel_group_delete(app: AppHandle, id: String) -> Result<(), String> {
    if id == DEFAULT_TUNNEL_GROUP_ID {
        return Err("The default environment group cannot be deleted".to_string());
    }
    mutate_state(|state| {
        let group_index = state
            .groups
            .iter()
            .position(|group| group.id == id)
            .ok_or_else(|| "Environment group not found".to_string())?;
        state.groups.remove(group_index);
        for tunnel in &mut state.tunnels {
            if tunnel.group_id == id {
                tunnel.group_id = DEFAULT_TUNNEL_GROUP_ID.to_string();
                tunnel.updated_at = now_ts();
            }
        }
        Ok(())
    })?;
    emit_tunnels_updated(&app);
    Ok(())
}

#[tauri::command]
pub fn ssh_tunnels_list() -> Result<Vec<SshTunnelView>, String> {
    let mut records = load_records()?;
    sort_tunnels(&mut records);
    Ok(records.iter().map(to_view).collect())
}

#[tauri::command]
pub async fn ssh_tunnel_upsert(
    app: AppHandle,
    input: SshTunnelUpsertInput,
) -> Result<SshTunnelView, String> {
    let state = load_state()?;
    let existing = input.id.as_ref().and_then(|id| {
        state
            .tunnels
            .iter()
            .find(|record| record.id == *id)
            .cloned()
    });
    validate_input(&input, existing.as_ref())?;

    if let Some(id) = input.id.as_ref() {
        let _ = disconnect_runtime(id);
    }

    let now = now_ts();
    let tunnel_id = input
        .id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let custom_config = match input.source_kind {
        SshTunnelSourceKind::Custom => {
            let custom = input
                .custom
                .clone()
                .ok_or_else(|| "Missing custom SSH configuration".to_string())?;
            Some(SshTunnelCustomConfig {
                host: custom.host.trim().to_string(),
                port: custom.port,
                user: custom.user.trim().to_string(),
                auth_kind: custom.auth_kind,
                key_path: custom
                    .key_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string()),
            })
        }
        SshTunnelSourceKind::SavedHost => None,
    };

    let record = SshTunnelRecord {
        id: tunnel_id.clone(),
        name: input.name.trim().to_string(),
        group_id: normalize_group_id(input.group_id.as_deref(), &state.groups),
        source_kind: input.source_kind.clone(),
        saved_host_name: input
            .saved_host_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string()),
        custom: custom_config,
        forward: SshTunnelForwardConfig {
            mode: input.forward.mode.clone(),
            local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
            local_port: input.forward.local_port,
            remote_bind_host: input
                .forward
                .remote_bind_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string()),
            remote_port: input.forward.remote_port,
            target_host: input
                .forward
                .target_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string()),
            target_port: input.forward.target_port,
            dynamic_probe_host: input
                .forward
                .dynamic_probe_host
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string()),
            dynamic_probe_port: input.forward.dynamic_probe_port,
        },
        auto_connect: input.auto_connect,
        auto_reconnect: input.auto_reconnect,
        created_at: existing
            .as_ref()
            .map(|record| record.created_at)
            .unwrap_or(now),
        updated_at: now,
        last_connected_at: existing
            .as_ref()
            .and_then(|record| record.last_connected_at),
        last_error: None,
    };

    mutate_records(|records| {
        if let Some(index) = records.iter().position(|item| item.id == record.id) {
            records[index] = record.clone();
        } else {
            records.push(record.clone());
        }
        Ok(())
    })?;

    let secret_key = secret_key_for_tunnel(&record.id);
    let should_remove_password = !matches!(
        input.custom.as_ref().map(|custom| &custom.auth_kind),
        Some(SshTunnelAuthKind::Password)
    );
    if should_remove_password {
        let _ = crate::secrets::delete_secret(app.clone(), secret_key.clone()).await;
    } else if let Some(custom) = input.custom.as_ref() {
        if let Some(password) = custom
            .password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            crate::secrets::save_secret(app.clone(), secret_key.clone(), password.to_string())
                .await?;
        } else if !custom.preserve_password.unwrap_or(false) {
            let _ = crate::secrets::delete_secret(app.clone(), secret_key.clone()).await;
        }
    }

    emit_tunnels_updated(&app);
    Ok(to_view(&record))
}

#[tauri::command]
pub async fn ssh_tunnel_delete(app: AppHandle, id: String) -> Result<(), String> {
    let _ = disconnect_runtime(&id);
    mutate_records(|records| {
        let before = records.len();
        records.retain(|record| record.id != id);
        if before == records.len() {
            return Err("Tunnel not found".to_string());
        }
        Ok(())
    })?;
    let _ = crate::secrets::delete_secret(app.clone(), secret_key_for_tunnel(&id)).await;
    emit_tunnels_updated(&app);
    Ok(())
}

#[tauri::command]
pub fn ssh_tunnel_connect(app: AppHandle, id: String) -> Result<SshTunnelRuntimeView, String> {
    connect_internal(app, id, false)
}

#[tauri::command]
pub fn ssh_tunnel_disconnect(app: AppHandle, id: String) -> Result<SshTunnelRuntimeView, String> {
    let mut record = load_records()?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| "Tunnel not found".to_string())?;
    disconnect_runtime(&record.id)?;
    let _ = clear_record_error(&record.id);
    record.last_error = None;
    emit_tunnels_updated(&app);
    Ok(default_runtime_view(&record))
}

#[tauri::command]
pub fn ssh_tunnel_group_connect(
    app: AppHandle,
    group_id: String,
) -> Result<SshTunnelBatchOperationResult, String> {
    let state = load_state()?;
    let group = state
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "Environment group not found".to_string())?;

    let target_group_id = if group_id == DEFAULT_TUNNEL_GROUP_ID {
        DEFAULT_TUNNEL_GROUP_ID.to_string()
    } else {
        group_id.clone()
    };

    let tunnels: Vec<SshTunnelRecord> = state
        .tunnels
        .iter()
        .filter(|t| {
            let normalized = normalize_group_id(Some(&t.group_id), &state.groups);
            normalized == target_group_id
        })
        .cloned()
        .collect();

    let total_count = tunnels.len();
    let group_name = if group.is_default {
        DEFAULT_TUNNEL_GROUP_NAME.to_string()
    } else {
        group.name.clone()
    };

    let mut success_count = 0;
    let mut skipped_count = 0;
    let mut failures: Vec<SshTunnelBatchFailureDetail> = Vec::new();

    let manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    let running_ids: HashSet<String> = manager.keys().cloned().collect();
    drop(manager);

    for tunnel in tunnels {
        if running_ids.contains(&tunnel.id) {
            skipped_count += 1;
            continue;
        }

        match connect_internal(app.clone(), tunnel.id.clone(), false) {
            Ok(_) => success_count += 1,
            Err(error) => {
                failures.push(SshTunnelBatchFailureDetail {
                    tunnel_id: tunnel.id.clone(),
                    tunnel_name: tunnel.name.clone(),
                    error,
                });
            }
        }
    }

    emit_tunnels_updated(&app);
    record_group_operation_failure(
        &app,
        &group_id,
        &group_name,
        "connect",
        total_count,
        &failures,
    );

    Ok(SshTunnelBatchOperationResult {
        operation: "connect".to_string(),
        group_id,
        group_name,
        success_count,
        failed_count: failures.len(),
        skipped_count,
        total_count,
        failures,
    })
}

#[tauri::command]
pub fn ssh_tunnel_group_disconnect(
    app: AppHandle,
    group_id: String,
) -> Result<SshTunnelBatchOperationResult, String> {
    let state = load_state()?;
    let group = state
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "Environment group not found".to_string())?;

    let target_group_id = if group_id == DEFAULT_TUNNEL_GROUP_ID {
        DEFAULT_TUNNEL_GROUP_ID.to_string()
    } else {
        group_id.clone()
    };

    let tunnels: Vec<SshTunnelRecord> = state
        .tunnels
        .iter()
        .filter(|t| {
            let normalized = normalize_group_id(Some(&t.group_id), &state.groups);
            normalized == target_group_id
        })
        .cloned()
        .collect();

    let total_count = tunnels.len();
    let group_name = if group.is_default {
        DEFAULT_TUNNEL_GROUP_NAME.to_string()
    } else {
        group.name.clone()
    };

    let mut success_count = 0;
    let mut skipped_count = 0;
    let mut failures: Vec<SshTunnelBatchFailureDetail> = Vec::new();

    let manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    let running_ids: HashSet<String> = manager.keys().cloned().collect();
    drop(manager);

    for tunnel in tunnels {
        if !running_ids.contains(&tunnel.id) {
            skipped_count += 1;
            continue;
        }

        match disconnect_runtime(&tunnel.id) {
            Ok(_) => success_count += 1,
            Err(error) => {
                failures.push(SshTunnelBatchFailureDetail {
                    tunnel_id: tunnel.id.clone(),
                    tunnel_name: tunnel.name.clone(),
                    error,
                });
            }
        }
    }

    emit_tunnels_updated(&app);
    record_group_operation_failure(
        &app,
        &group_id,
        &group_name,
        "disconnect",
        total_count,
        &failures,
    );

    Ok(SshTunnelBatchOperationResult {
        operation: "disconnect".to_string(),
        group_id,
        group_name,
        success_count,
        failed_count: failures.len(),
        skipped_count,
        total_count,
        failures,
    })
}

#[tauri::command]
pub fn ssh_tunnel_probe_draft(
    input: SshTunnelProbeDraftInput,
) -> Result<SshTunnelProbeResult, String> {
    validate_input(&input, None)?;
    let resolved = resolve_ssh_config_from_input(&input)?;
    let summary = tunnel_summary(&input.forward);
    match probe_forward(&input.forward, &resolved) {
        Ok(message) => Ok(SshTunnelProbeResult {
            ok: true,
            mode: input.forward.mode.clone(),
            summary,
            message,
            last_error: None,
        }),
        Err(error) => Ok(SshTunnelProbeResult {
            ok: false,
            mode: input.forward.mode.clone(),
            summary,
            message: error.clone(),
            last_error: Some(error),
        }),
    }
}

#[tauri::command]
pub fn ssh_tunnel_probe_saved(id: String) -> Result<SshTunnelProbeResult, String> {
    let record = load_records()?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| "Tunnel not found".to_string())?;
    let resolved = resolve_ssh_config_from_record(&record)?;
    let summary = tunnel_summary(&record.forward);
    match probe_forward(&record.forward, &resolved) {
        Ok(message) => Ok(SshTunnelProbeResult {
            ok: true,
            mode: record.forward.mode.clone(),
            summary,
            message,
            last_error: None,
        }),
        Err(error) => Ok(SshTunnelProbeResult {
            ok: false,
            mode: record.forward.mode.clone(),
            summary,
            message: error.clone(),
            last_error: Some(error),
        }),
    }
}

#[tauri::command]
pub fn ssh_tunnels_refresh_status() -> Result<Vec<SshTunnelRuntimeView>, String> {
    let mut records = load_records()?;
    sort_tunnels(&mut records);
    let mut manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    let finished_ids = manager
        .iter()
        .filter_map(|(id, running)| {
            if running
                .join
                .as_ref()
                .map(|handle| handle.is_finished())
                .unwrap_or(false)
            {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for id in finished_ids {
        if let Some(mut running) = manager.remove(&id) {
            if let Some(join) = running.join.take() {
                let _ = join.join();
            }
        }
    }
    let views = records
        .iter()
        .map(|record| runtime_view(record, manager.get(&record.id)))
        .collect::<Vec<_>>();
    Ok(views)
}

fn snapshot_state() -> Result<SshTunnelsSnapshot, String> {
    let mut state = load_state()?;
    sort_groups(&mut state.groups);
    sort_tunnels(&mut state.tunnels);
    let runtime = ssh_tunnels_refresh_status()?;
    Ok(SshTunnelsSnapshot {
        groups: state.groups.iter().map(to_group_view).collect(),
        tunnels: state.tunnels.iter().map(to_view).collect(),
        runtime,
    })
}

#[tauri::command]
pub fn ssh_tunnels_snapshot() -> Result<SshTunnelsSnapshot, String> {
    snapshot_state()
}

pub async fn ssh_tunnels_bootstrap(app: AppHandle) -> Result<(), String> {
    let records = load_records()?;
    for record in records.into_iter().filter(|record| record.auto_connect) {
        if let Err(error) = connect_internal(app.clone(), record.id.clone(), true) {
            let _ = update_record_error(&record.id, &error);
        }
    }
    Ok(())
}

pub fn ssh_tunnels_on_window_show(app: AppHandle) {
    let result = (|| -> Result<(), String> {
        let records = load_records()?;
        let reconnect_enabled_ids: HashSet<_> = records
            .iter()
            .filter(|r| r.auto_reconnect)
            .map(|r| r.id.clone())
            .collect();

        let failed_ids = {
            let manager = runtime_manager().lock().map_err(|e| e.to_string())?;
            manager
                .iter()
                .filter(|(id, running)| {
                    if !reconnect_enabled_ids.contains(*id) {
                        return false;
                    }
                    running
                        .state
                        .lock()
                        .map(|s| {
                            matches!(
                                s.status,
                                SshTunnelStatus::Error | SshTunnelStatus::Disconnected
                            )
                        })
                        .unwrap_or(false)
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        };

        if failed_ids.is_empty() {
            log::debug!("SSH tunnel window-show reconnect: no failed tunnels");
            return Ok(());
        }

        log::info!(
            "SSH tunnel window-show reconnect: {} failed tunnel(s)",
            failed_ids.len()
        );

        let total = failed_ids.len();
        let _ = app.emit(
            SSH_TUNNEL_WINDOW_RECONNECT_START_EVENT,
            serde_json::json!({ "total": total }),
        );

        let mut succeeded = 0usize;
        for id in failed_ids {
            match connect_internal(app.clone(), id.clone(), true) {
                Ok(_) => {
                    succeeded += 1;
                    log::info!("SSH tunnel window-show reconnected: {}", id);
                }
                Err(error) => {
                    let _ = update_record_error(&id, &error);
                    if let Ok(Some(record)) = load_record_by_id(&id) {
                        record_tunnel_failure(&app, &record, &error, "window-show-reconnect");
                    }
                    log::warn!(
                        "SSH tunnel window-show reconnect failed for {}: {}",
                        id,
                        error
                    );
                }
            }
        }

        let _ = app.emit(
            SSH_TUNNEL_WINDOW_RECONNECT_DONE_EVENT,
            SshTunnelWindowReconnectDoneEvent {
                total,
                succeeded,
                failed: total - succeeded,
            },
        );

        Ok(())
    })();

    if let Err(error) = result {
        log::warn!("SSH tunnel window-show reconnect error: {}", error);
    }
}

pub fn shutdown_runtime() -> Result<(), String> {
    let ids = runtime_manager()
        .lock()
        .map_err(|e| e.to_string())?
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for id in ids {
        let _ = disconnect_runtime(&id);
    }
    Ok(())
}
