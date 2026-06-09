async fn apply_dependencies_for_preset(
    app: tauri::AppHandle,
    preset: &WorkflowPreset,
    launch_scope: &str,
    working_dir: &str,
) -> Result<WorkflowDependencyApplyResult, String> {
    let tool = normalize_tool(&preset.tool);
    let (install_scope, install_project_root) =
        install_scope_and_project_root(launch_scope, working_dir);
    let provider_id = preset
        .provider_id
        .clone()
        .or_else(|| active_provider_id_for_tool(&tool));
    let mut linked_mcp_count = 0usize;
    let mut enabled_mcp_switch_count = 0usize;
    let mut installed_skill_count = 0usize;
    let mut failed_skill_installs: Vec<String> = Vec::new();

    let mcp_state = mcp_servers::get_mcp_servers()?;
    let mut mcp_by_id = HashMap::new();
    for server in mcp_state.servers {
        mcp_by_id.insert(server.id.clone(), server);
    }

    if launch_scope == LAUNCH_SCOPE_SHARED {
        if let Some(provider) = provider_id {
            for server_id in &preset.mcp_server_ids {
                if let Some(server) = mcp_by_id.get(server_id) {
                    if !server.linked_provider_ids.iter().any(|id| id == &provider) {
                        let mut next_links = server.linked_provider_ids.clone();
                        next_links.push(provider.clone());
                        next_links = dedup_non_empty(&next_links);
                        mcp_servers::link_mcp_to_providers(
                            app.clone(),
                            server_id.clone(),
                            next_links,
                        )?;
                        linked_mcp_count += 1;
                    }
                    if mcp_servers::set_mcp_model_switch(server_id.clone(), tool.clone(), true)
                        .is_ok()
                    {
                        enabled_mcp_switch_count += 1;
                    }
                }
            }
        }
    }

    let indexes = build_skill_indexes(&tool, &install_scope, install_project_root.as_deref());
    let mut installed_ids = indexes.installed_ids.clone();
    let mut installed_source_rel = indexes.installed_source_rel.clone();
    let mut repo_installed_by_key = indexes
        .repo_by_key
        .iter()
        .map(|(repo_key, repo)| (repo_key.clone(), repo_installed_for_tool(repo, &tool)))
        .collect::<HashMap<_, _>>();

    for skill_id in &preset.required_skill_ids {
        let Some(target) = resolve_skill_target(skill_id, &indexes) else {
            failed_skill_installs.push(format!("{} (unresolved)", skill_id));
            continue;
        };
        if target_installed(
            &target,
            &installed_ids,
            &installed_source_rel,
            &repo_installed_by_key,
        ) {
            continue;
        }

        let install_result: Result<(), String> = match target {
            ResolvedSkillTarget::Catalog {
                source_id,
                skill_ref,
                ..
            } => match skills::skills_install(
                app.clone(),
                skills::InstallInput {
                    source_id,
                    skill_ref,
                    model: tool.clone(),
                    scope: Some(install_scope.clone()),
                    project_root: install_project_root.clone(),
                },
            )
            .await
            {
                Ok(result) => {
                    installed_ids.insert(result.data.id.clone());
                    installed_source_rel.insert((
                        result.data.source_id.clone(),
                        result.data.source_rel_path.clone(),
                    ));
                    repo_installed_by_key.insert(
                        make_repo_key(&result.data.source_id, &result.data.source_rel_path),
                        true,
                    );
                    Ok(())
                }
                Err(err) => Err(err),
            },
            ResolvedSkillTarget::Repo {
                repo_key,
                source_id,
                source_rel_path,
                ..
            } => match skills::skills_repo_set_model(
                app.clone(),
                skills::RepoSetModelInput {
                    repo_key: repo_key.clone(),
                    model: tool.clone(),
                    enabled: true,
                    scope: Some(install_scope.clone()),
                    project_root: install_project_root.clone(),
                },
            )
            .await
            {
                Ok(result) => {
                    repo_installed_by_key
                        .insert(repo_key, repo_installed_for_tool(&result.data, &tool));
                    installed_ids.insert(result.data.skill_id.clone());
                    installed_source_rel.insert((source_id, source_rel_path));
                    Ok(())
                }
                Err(err) => Err(err),
            },
        };

        match install_result {
            Ok(()) => {
                installed_skill_count += 1;
            }
            Err(err) => {
                failed_skill_installs.push(format!("{} ({})", skill_id, err));
            }
        }
    }

    let deps_after = detect_dependencies_for_working_dir(preset, Some(working_dir))?;
    Ok(WorkflowDependencyApplyResult {
        preset_id: preset.id.clone(),
        linked_mcp_count,
        enabled_mcp_switch_count,
        installed_skill_count,
        failed_skill_installs,
        dependencies_after: deps_after,
    })
}

fn build_missing_skill_error(deps: &WorkflowDependencyState) -> Option<String> {
    if deps.missing_skill_ids.is_empty() {
        return None;
    }
    let display = if deps.missing_skill_names.is_empty() {
        deps.missing_skill_ids.join(", ")
    } else {
        deps.missing_skill_names.join(", ")
    };
    Some(format!(
        "workflow required skills are not ready: {}",
        display
    ))
}
