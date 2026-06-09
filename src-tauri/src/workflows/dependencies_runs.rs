use super::{
    active_provider_id_for_tool, active_provider_name_for_tool, build_skill_indexes,
    install_scope_and_project_root, load_runs, normalize_launch_scope, normalize_tool, now_ts,
    provider_by_id_for_tool, repo_installed_for_tool, resolve_skill_target, target_installed,
    ProviderLite, ResolvedSkillTarget, WorkflowDependencyState, WorkflowPreset, WorkflowRun,
    LAUNCH_SCOPE_SHARED, LAUNCH_SCOPE_STRICT, PROMPT_STATUS_APPLIED, PROMPT_STATUS_MANUAL,
};
use crate::{app_store, mcp_servers, runtime_profiles, skills};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(in crate::workflows) fn detect_dependencies_for_working_dir(
    preset: &WorkflowPreset,
    working_dir_override: Option<&str>,
) -> Result<WorkflowDependencyState, String> {
    let tool = normalize_tool(&preset.tool);
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let effective_working_dir = working_dir_override.unwrap_or(&preset.working_dir);
    let (install_scope, install_project_root) =
        install_scope_and_project_root(&launch_scope, effective_working_dir);
    let active_provider_id = preset
        .provider_id
        .clone()
        .or_else(|| active_provider_id_for_tool(&tool));
    let active_provider_name = if let Some(provider_id) = active_provider_id.as_ref() {
        provider_by_id_for_tool(&tool, provider_id).map(|p| p.name)
    } else {
        active_provider_name_for_tool(&tool)
    };

    let mcp_state = mcp_servers::get_mcp_servers()?;
    let mut mcp_by_id = HashMap::new();
    for server in mcp_state.servers {
        mcp_by_id.insert(server.id.clone(), server);
    }

    let mut missing_mcp_server_ids = Vec::new();
    let mut missing_mcp_names = Vec::new();
    let mut inactive_mcp_server_ids = Vec::new();
    let mut inactive_mcp_names = Vec::new();
    for id in &preset.mcp_server_ids {
        match mcp_by_id.get(id) {
            None => {
                missing_mcp_server_ids.push(id.clone());
                missing_mcp_names.push(id.clone());
            }
            Some(server) => {
                if launch_scope == LAUNCH_SCOPE_SHARED {
                    if let Some(provider_id) = active_provider_id.as_ref() {
                        if !server.linked_provider_ids.iter().any(|p| p == provider_id) {
                            inactive_mcp_server_ids.push(id.clone());
                            inactive_mcp_names.push(server.name.clone());
                        }
                    }
                }
            }
        }
    }

    let indexes = build_skill_indexes(&tool, &install_scope, install_project_root.as_deref());
    let repo_installed_by_key = indexes
        .repo_by_key
        .iter()
        .map(|(repo_key, repo)| (repo_key.clone(), repo_installed_for_tool(repo, &tool)))
        .collect::<HashMap<_, _>>();

    let mut missing_skill_ids = Vec::new();
    let mut missing_skill_names = Vec::new();
    let mut installable_skill_ids = Vec::new();
    let mut unresolved_skill_ids = Vec::new();
    for skill_id in &preset.required_skill_ids {
        let resolved = resolve_skill_target(skill_id, &indexes);
        let installed = if let Some(target) = resolved.as_ref() {
            target_installed(
                target,
                &indexes.installed_ids,
                &indexes.installed_source_rel,
                &repo_installed_by_key,
            )
        } else {
            indexes.installed_ids.contains(skill_id)
        };
        if installed {
            continue;
        }
        missing_skill_ids.push(skill_id.clone());
        missing_skill_names.push(match resolved.as_ref() {
            Some(ResolvedSkillTarget::Catalog { skill_name, .. }) => skill_name.clone(),
            Some(ResolvedSkillTarget::Repo { skill_name, .. }) => skill_name.clone(),
            None => skill_id.clone(),
        });
        if resolved.is_some() {
            installable_skill_ids.push(skill_id.clone());
        } else {
            unresolved_skill_ids.push(skill_id.clone());
        }
    }

    Ok(WorkflowDependencyState {
        active_provider_id,
        active_provider_name,
        missing_mcp_server_ids,
        missing_mcp_names,
        inactive_mcp_server_ids,
        inactive_mcp_names,
        missing_skill_ids,
        missing_skill_names,
        installable_skill_ids,
        unresolved_skill_ids,
    })
}

pub(in crate::workflows) fn detect_dependencies(
    preset: &WorkflowPreset,
) -> Result<WorkflowDependencyState, String> {
    detect_dependencies_for_working_dir(preset, None)
}

pub(in crate::workflows) fn allowed_run_status(status: &str) -> bool {
    matches!(status, "running" | "success" | "failed" | "interrupted")
}

pub(in crate::workflows) fn make_run_for_launch(
    preset: &WorkflowPreset,
    working_dir: String,
    session_id: Option<String>,
    tool_session_id: Option<String>,
    runtime_mode: String,
    runtime_profile_id: Option<String>,
    prompt_apply_status: String,
    dependency_apply_mode: String,
    status: &str,
    error_message: Option<String>,
    replay_of_run_id: Option<String>,
) -> WorkflowRun {
    let started_at = now_ts();
    let ended_at = if status == "running" {
        None
    } else {
        Some(started_at)
    };
    WorkflowRun {
        id: uuid::Uuid::new_v4().to_string(),
        preset_id: preset.id.clone(),
        preset_name: preset.name.clone(),
        tool: normalize_tool(&preset.tool),
        working_dir,
        launch_prompt: preset.launch_prompt.clone(),
        launch_scope: normalize_launch_scope(Some(&preset.launch_scope)),
        session_id,
        tool_session_id,
        runtime_mode,
        runtime_profile_id,
        prompt_apply_status,
        dependency_apply_mode,
        status: status.to_string(),
        summary: None,
        error_message,
        started_at,
        ended_at,
        replay_of_run_id,
    }
}

pub(in crate::workflows) async fn create_session_for_preset(
    app: tauri::AppHandle,
    preset: &WorkflowPreset,
    _session_name: Option<String>,
    override_working_dir: Option<String>,
    runtime_mode: String,
    runtime_profile_id: Option<String>,
) -> Result<(Value, String, Option<String>), String> {
    let working_dir = override_working_dir
        .unwrap_or_else(|| preset.working_dir.clone())
        .trim()
        .to_string();
    let normalized_working_dir = if working_dir.is_empty() {
        "./".to_string()
    } else {
        working_dir
    };
    let tool = normalize_tool(&preset.tool);
    let resp = app_store::sessions_create(
        app,
        app_store::SessionInput {
            id: None,
            name: String::new(),
            working_dir: normalized_working_dir.clone(),
            tool: tool.clone(),
            tool_session_id: None,
            runtime_mode: Some(runtime_mode),
            runtime_profile_id,
            preset_id: Some(preset.id.clone()),
            status: Some("active".to_string()),
            provider_id: None,
            initial_prompt: None,
            permission_mode: None,
        },
    )
    .await
    .map_err(|e| e.message)?;

    let resolved_tool_session_id = resp
        .data
        .get("tool_session_id")
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    Ok((resp.data, normalized_working_dir, resolved_tool_session_id))
}

pub(in crate::workflows) fn prompt_apply_status_for_preset(preset: &WorkflowPreset) -> String {
    if preset
        .launch_prompt
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        PROMPT_STATUS_MANUAL.to_string()
    } else {
        PROMPT_STATUS_APPLIED.to_string()
    }
}

pub(in crate::workflows) fn strict_provider_for_preset(
    preset: &WorkflowPreset,
) -> Option<ProviderLite> {
    let tool = normalize_tool(&preset.tool);
    let provider_id = preset
        .provider_id
        .clone()
        .or_else(|| active_provider_id_for_tool(&tool))?;
    provider_by_id_for_tool(&tool, &provider_id)
}

pub(in crate::workflows) fn ensure_strict_provider_env_managed(
    preset: &WorkflowPreset,
) -> Result<(), String> {
    if normalize_launch_scope(Some(&preset.launch_scope)) != LAUNCH_SCOPE_STRICT {
        return Ok(());
    }
    let Some(provider) = strict_provider_for_preset(preset) else {
        return Ok(());
    };
    if !provider.env_managed {
        return Err(format!(
            "strict workflow launch requires env-managed provider: {}",
            provider.name
        ));
    }
    Ok(())
}

pub(in crate::workflows) fn selected_mcp_servers_for_preset(
    preset: &WorkflowPreset,
) -> Result<Vec<mcp_servers::MCPServer>, String> {
    let mcp_state = mcp_servers::get_mcp_servers()?;
    let mut by_id = HashMap::new();
    for server in mcp_state.servers {
        by_id.insert(server.id.clone(), server);
    }

    let mut out = Vec::new();
    let mut missing = Vec::new();
    for server_id in &preset.mcp_server_ids {
        if let Some(server) = by_id.get(server_id) {
            out.push(server.clone());
        } else {
            missing.push(server_id.clone());
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "workflow required MCP servers are missing: {}",
            missing.join(", ")
        ));
    }
    Ok(out)
}

pub(in crate::workflows) fn installed_skill_records_for_tool(
    tool: &str,
    scope: &str,
    project_root: Option<&str>,
) -> Vec<skills::SkillRecord> {
    skills::skills_list_installed(
        Some(tool.to_string()),
        Some(scope.to_string()),
        project_root.map(|v| v.to_string()),
    )
    .map(|resp| resp.data)
    .unwrap_or_default()
}

pub(in crate::workflows) fn resolve_skill_dir_names_for_preset(
    preset: &WorkflowPreset,
    working_dir: &str,
) -> Result<Vec<String>, String> {
    let tool = normalize_tool(&preset.tool);
    let launch_scope = normalize_launch_scope(Some(&preset.launch_scope));
    let (install_scope, install_project_root) =
        install_scope_and_project_root(&launch_scope, working_dir);
    let indexes = build_skill_indexes(&tool, &install_scope, install_project_root.as_deref());
    let installed =
        installed_skill_records_for_tool(&tool, &install_scope, install_project_root.as_deref());
    let mut by_id: HashMap<String, String> = HashMap::new();
    let mut by_source_rel: HashMap<(String, String), String> = HashMap::new();
    for item in installed {
        by_id.insert(item.id.clone(), item.dir_name.clone());
        by_source_rel.insert(
            (item.source_id.clone(), item.source_rel_path.clone()),
            item.dir_name,
        );
    }

    let mut out = Vec::new();
    let mut missing = Vec::new();
    for raw in &preset.required_skill_ids {
        let Some(target) = resolve_skill_target(raw, &indexes) else {
            missing.push(raw.clone());
            continue;
        };
        let dir_name = match &target {
            ResolvedSkillTarget::Catalog {
                source_id,
                source_rel_path,
                skill_id,
                ..
            } => by_id.get(skill_id).cloned().or_else(|| {
                by_source_rel
                    .get(&(source_id.clone(), source_rel_path.clone()))
                    .cloned()
            }),
            ResolvedSkillTarget::Repo {
                source_id,
                source_rel_path,
                skill_id,
                ..
            } => by_id.get(skill_id).cloned().or_else(|| {
                by_source_rel
                    .get(&(source_id.clone(), source_rel_path.clone()))
                    .cloned()
            }),
        };
        if let Some(name) = dir_name {
            if !name.trim().is_empty() && !out.contains(&name) {
                out.push(name);
            }
        } else {
            missing.push(raw.clone());
        }
    }

    if !missing.is_empty() {
        return Err(format!(
            "strict workflow required skills are not mirrored yet: {}",
            missing.join(", ")
        ));
    }
    Ok(out)
}

pub(in crate::workflows) fn protected_runtime_profile_ids_from_sessions() -> HashSet<String> {
    let mut protected = HashSet::new();
    if let Ok(resp) = app_store::sessions_list() {
        for item in resp.data {
            if let Some(profile_id) = item.get("runtime_profile_id").and_then(|v| v.as_str()) {
                let trimmed = profile_id.trim();
                if !trimmed.is_empty() {
                    protected.insert(trimmed.to_string());
                }
            }
        }
    }
    protected
}

pub(in crate::workflows) fn protected_runtime_profile_ids_from_runs(
    runs: &[WorkflowRun],
) -> HashSet<String> {
    let mut protected = HashSet::new();
    for run in runs {
        if run.status != "running" {
            continue;
        }
        if let Some(profile_id) = run.runtime_profile_id.as_ref() {
            let trimmed = profile_id.trim();
            if !trimmed.is_empty() {
                protected.insert(trimmed.to_string());
            }
        }
    }
    protected
}

pub(in crate::workflows) fn cleanup_runtime_profiles() -> Result<Vec<String>, String> {
    let runs = load_runs().unwrap_or_default();
    let mut protected = protected_runtime_profile_ids_from_sessions();
    for profile_id in protected_runtime_profile_ids_from_runs(&runs) {
        protected.insert(profile_id);
    }
    runtime_profiles::cleanup_stale_runtime_profiles(
        &protected,
        runtime_profiles::DEFAULT_PROFILE_TTL_SECS,
    )
}

pub fn workflows_cleanup_runtime_profiles_on_startup() -> Result<(), String> {
    let _ = cleanup_runtime_profiles()?;
    Ok(())
}
