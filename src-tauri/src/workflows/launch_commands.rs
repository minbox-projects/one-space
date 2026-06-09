use super::{
    api_ok, apply_dependencies_for_preset, build_missing_skill_error, canonicalize_working_dir,
    cleanup_runtime_profiles, create_session_for_preset, ensure_strict_provider_env_managed,
    load_presets, load_runs, make_run_for_launch, normalize_launch_scope, normalize_tool, now_ts,
    prompt_apply_status_for_preset, resolve_skill_dir_names_for_preset, save_runs,
    selected_mcp_servers_for_preset, ApiOk, WorkflowLaunchInput, WorkflowReplayInput,
    DEP_MODE_SHARED_GLOBAL, DEP_MODE_STRICT_LOCAL, LAUNCH_SCOPE_STRICT,
};
use crate::runtime_profiles;
use serde_json::{json, Value};

#[tauri::command]
pub async fn workflows_launch_preset(
    app: tauri::AppHandle,
    input: WorkflowLaunchInput,
) -> Result<ApiOk<Value>, String> {
    let presets = load_presets()?;
    let preset = presets
        .iter()
        .find(|p| p.id == input.preset_id)
        .cloned()
        .ok_or_else(|| "workflow preset not found".to_string())?;
    let mut runs = load_runs()?;
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let prompt_apply_status = prompt_apply_status_for_preset(&preset);
    let _ = cleanup_runtime_profiles();
    let default_working_dir = input
        .override_working_dir
        .clone()
        .unwrap_or_else(|| preset.working_dir.clone());

    if let Err(err) = ensure_strict_provider_env_managed(&preset) {
        let run = make_run_for_launch(
            &preset,
            default_working_dir.clone(),
            None,
            None,
            launch_scope.clone(),
            None,
            prompt_apply_status.clone(),
            if launch_scope == LAUNCH_SCOPE_STRICT {
                DEP_MODE_STRICT_LOCAL.to_string()
            } else {
                DEP_MODE_SHARED_GLOBAL.to_string()
            },
            "failed",
            Some(err.clone()),
            None,
        );
        runs.push(run);
        save_runs(&runs)?;
        return Err(err);
    }

    let dependency_result =
        apply_dependencies_for_preset(app.clone(), &preset, &launch_scope, &default_working_dir)
            .await;
    let dependency_apply_mode = if launch_scope == LAUNCH_SCOPE_STRICT {
        DEP_MODE_STRICT_LOCAL.to_string()
    } else {
        DEP_MODE_SHARED_GLOBAL.to_string()
    };
    match dependency_result {
        Ok(result) => {
            if let Some(dep_err) = build_missing_skill_error(&result.dependencies_after) {
                let run = make_run_for_launch(
                    &preset,
                    default_working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    None,
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(dep_err.clone()),
                    None,
                );
                runs.push(run);
                save_runs(&runs)?;
                return Err(dep_err);
            }
        }
        Err(err) => {
            let run = make_run_for_launch(
                &preset,
                default_working_dir.clone(),
                None,
                None,
                launch_scope.clone(),
                None,
                prompt_apply_status.clone(),
                dependency_apply_mode.clone(),
                "failed",
                Some(err.clone()),
                None,
            );
            runs.push(run);
            save_runs(&runs)?;
            return Err(err);
        }
    }

    let mut runtime_profile_id: Option<String> = None;
    if launch_scope == LAUNCH_SCOPE_STRICT {
        let selected_mcp = match selected_mcp_servers_for_preset(&preset) {
            Ok(v) => v,
            Err(err) => {
                let run = make_run_for_launch(
                    &preset,
                    default_working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    None,
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(err.clone()),
                    None,
                );
                runs.push(run);
                save_runs(&runs)?;
                return Err(err);
            }
        };
        let skill_dir_names =
            match resolve_skill_dir_names_for_preset(&preset, &default_working_dir) {
                Ok(v) => v,
                Err(err) => {
                    let run = make_run_for_launch(
                        &preset,
                        default_working_dir.clone(),
                        None,
                        None,
                        launch_scope.clone(),
                        None,
                        prompt_apply_status.clone(),
                        dependency_apply_mode.clone(),
                        "failed",
                        Some(err.clone()),
                        None,
                    );
                    runs.push(run);
                    save_runs(&runs)?;
                    return Err(err);
                }
            };
        let profile_id = format!("rp-{}", uuid::Uuid::new_v4());
        match runtime_profiles::materialize_strict_profile(runtime_profiles::StrictProfileInput {
            profile_id: profile_id.clone(),
            tool: normalize_tool(&preset.tool),
            mcp_servers: selected_mcp,
            skill_dir_names,
            install_scope: Some("project".to_string()),
            project_root: canonicalize_working_dir(&default_working_dir),
            reuse_existing: false,
        }) {
            Ok(result) => {
                runtime_profile_id = Some(result.profile_id);
            }
            Err(err) => {
                let run = make_run_for_launch(
                    &preset,
                    default_working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    Some(profile_id),
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(err.clone()),
                    None,
                );
                runs.push(run);
                save_runs(&runs)?;
                return Err(err);
            }
        }
    }

    let launch_result = create_session_for_preset(
        app,
        &preset,
        input.session_name.clone(),
        input.override_working_dir.clone(),
        launch_scope.clone(),
        runtime_profile_id.clone(),
    )
    .await;

    match launch_result {
        Ok((session_value, used_working_dir, tool_session_id)) => {
            let session_id = session_value
                .get("id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let run = make_run_for_launch(
                &preset,
                used_working_dir,
                session_id.clone(),
                tool_session_id,
                launch_scope.clone(),
                runtime_profile_id,
                prompt_apply_status,
                dependency_apply_mode,
                "running",
                None,
                None,
            );
            runs.push(run.clone());
            save_runs(&runs)?;
            api_ok(json!({
                "preset": preset,
                "session": session_value,
                "run": run
            }))
        }
        Err(err) => {
            let run = make_run_for_launch(
                &preset,
                input
                    .override_working_dir
                    .unwrap_or_else(|| preset.working_dir.clone()),
                None,
                None,
                launch_scope,
                runtime_profile_id,
                prompt_apply_status,
                dependency_apply_mode,
                "failed",
                Some(err.clone()),
                None,
            );
            runs.push(run);
            save_runs(&runs)?;
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn workflows_replay_run(
    app: tauri::AppHandle,
    input: WorkflowReplayInput,
) -> Result<ApiOk<Value>, String> {
    let presets = load_presets()?;
    let mut runs = load_runs()?;
    let idx = runs
        .iter()
        .position(|r| r.id == input.run_id)
        .ok_or_else(|| "workflow run not found".to_string())?;
    let base_run = runs[idx].clone();
    let preset = presets
        .iter()
        .find(|p| p.id == base_run.preset_id)
        .cloned()
        .ok_or_else(|| "workflow preset not found for replay".to_string())?;
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let prompt_apply_status = prompt_apply_status_for_preset(&preset);
    let _ = cleanup_runtime_profiles();

    if base_run.status == "running" {
        runs[idx].status = "interrupted".to_string();
        runs[idx].ended_at = Some(now_ts());
        if runs[idx].error_message.is_none() {
            runs[idx].error_message =
                Some("Replay requested while previous run still running".to_string());
        }
    }

    if let Err(err) = ensure_strict_provider_env_managed(&preset) {
        let failed_run = make_run_for_launch(
            &preset,
            base_run.working_dir.clone(),
            None,
            None,
            launch_scope.clone(),
            None,
            prompt_apply_status.clone(),
            if launch_scope == LAUNCH_SCOPE_STRICT {
                DEP_MODE_STRICT_LOCAL.to_string()
            } else {
                DEP_MODE_SHARED_GLOBAL.to_string()
            },
            "failed",
            Some(err.clone()),
            Some(base_run.id.clone()),
        );
        runs.push(failed_run);
        save_runs(&runs)?;
        return Err(err);
    }

    let dependency_result =
        apply_dependencies_for_preset(app.clone(), &preset, &launch_scope, &base_run.working_dir)
            .await;
    let dependency_apply_mode = if launch_scope == LAUNCH_SCOPE_STRICT {
        DEP_MODE_STRICT_LOCAL.to_string()
    } else {
        DEP_MODE_SHARED_GLOBAL.to_string()
    };
    match dependency_result {
        Ok(result) => {
            if let Some(dep_err) = build_missing_skill_error(&result.dependencies_after) {
                let failed_run = make_run_for_launch(
                    &preset,
                    base_run.working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    None,
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(dep_err.clone()),
                    Some(base_run.id.clone()),
                );
                runs.push(failed_run);
                save_runs(&runs)?;
                return Err(dep_err);
            }
        }
        Err(err) => {
            let failed_run = make_run_for_launch(
                &preset,
                base_run.working_dir.clone(),
                None,
                None,
                launch_scope.clone(),
                None,
                prompt_apply_status.clone(),
                dependency_apply_mode.clone(),
                "failed",
                Some(err.clone()),
                Some(base_run.id.clone()),
            );
            runs.push(failed_run);
            save_runs(&runs)?;
            return Err(err);
        }
    }

    let mut runtime_profile_id = if launch_scope == LAUNCH_SCOPE_STRICT {
        base_run.runtime_profile_id.clone()
    } else {
        None
    };
    if launch_scope == LAUNCH_SCOPE_STRICT {
        let selected_mcp = match selected_mcp_servers_for_preset(&preset) {
            Ok(v) => v,
            Err(err) => {
                let failed_run = make_run_for_launch(
                    &preset,
                    base_run.working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    runtime_profile_id.clone(),
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(err.clone()),
                    Some(base_run.id.clone()),
                );
                runs.push(failed_run);
                save_runs(&runs)?;
                return Err(err);
            }
        };
        let skill_dir_names =
            match resolve_skill_dir_names_for_preset(&preset, &base_run.working_dir) {
                Ok(v) => v,
                Err(err) => {
                    let failed_run = make_run_for_launch(
                        &preset,
                        base_run.working_dir.clone(),
                        None,
                        None,
                        launch_scope.clone(),
                        runtime_profile_id.clone(),
                        prompt_apply_status.clone(),
                        dependency_apply_mode.clone(),
                        "failed",
                        Some(err.clone()),
                        Some(base_run.id.clone()),
                    );
                    runs.push(failed_run);
                    save_runs(&runs)?;
                    return Err(err);
                }
            };
        let desired_profile_id = if let Some(existing) = runtime_profile_id.clone() {
            if runtime_profiles::runtime_profile_exists(&existing).unwrap_or(false) {
                existing
            } else {
                format!("rp-{}", uuid::Uuid::new_v4())
            }
        } else {
            format!("rp-{}", uuid::Uuid::new_v4())
        };
        match runtime_profiles::materialize_strict_profile(runtime_profiles::StrictProfileInput {
            profile_id: desired_profile_id.clone(),
            tool: normalize_tool(&preset.tool),
            mcp_servers: selected_mcp,
            skill_dir_names,
            install_scope: Some("project".to_string()),
            project_root: canonicalize_working_dir(&base_run.working_dir),
            reuse_existing: true,
        }) {
            Ok(result) => {
                runtime_profile_id = Some(result.profile_id);
            }
            Err(err) => {
                let failed_run = make_run_for_launch(
                    &preset,
                    base_run.working_dir.clone(),
                    None,
                    None,
                    launch_scope.clone(),
                    Some(desired_profile_id),
                    prompt_apply_status.clone(),
                    dependency_apply_mode.clone(),
                    "failed",
                    Some(err.clone()),
                    Some(base_run.id.clone()),
                );
                runs.push(failed_run);
                save_runs(&runs)?;
                return Err(err);
            }
        }
    }

    let launch_result = create_session_for_preset(
        app,
        &preset,
        input.session_name.clone(),
        Some(base_run.working_dir.clone()),
        launch_scope.clone(),
        runtime_profile_id.clone(),
    )
    .await;

    match launch_result {
        Ok((session_value, used_working_dir, tool_session_id)) => {
            let session_id = session_value
                .get("id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let run = make_run_for_launch(
                &preset,
                used_working_dir,
                session_id,
                tool_session_id,
                launch_scope.clone(),
                runtime_profile_id,
                prompt_apply_status,
                dependency_apply_mode,
                "running",
                None,
                Some(base_run.id.clone()),
            );
            runs.push(run.clone());
            save_runs(&runs)?;
            api_ok(json!({
                "replay_of": base_run,
                "session": session_value,
                "run": run
            }))
        }
        Err(err) => {
            let failed_run = make_run_for_launch(
                &preset,
                base_run.working_dir,
                None,
                None,
                launch_scope,
                runtime_profile_id,
                prompt_apply_status,
                dependency_apply_mode,
                "failed",
                Some(err.clone()),
                Some(base_run.id.clone()),
            );
            runs.push(failed_run);
            save_runs(&runs)?;
            Err(err)
        }
    }
}
