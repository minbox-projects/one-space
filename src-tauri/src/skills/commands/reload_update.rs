#[tauri::command]
pub async fn skills_repo_auto_update_pending(
    app: tauri::AppHandle,
) -> Result<ApiOk<RepoAutoUpdateResult>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dedupe_key = "repo_auto_update_pending";
        let _job = match acquire_job_key(dedupe_key)? {
            Some(v) => v,
            None => {
                let shared_state = load_skills_state()?;
                let local_state = load_local_skills_state()?;
                let result = RepoAutoUpdateResult {
                    applied_at: now_ts(),
                    ..RepoAutoUpdateResult::default()
                };
                return api_ok(result, combined_revision(&shared_state, &local_state));
            }
        };
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;

        let mut shared_state = load_skills_state()?;
        let mut local_state = load_local_skills_state()?;
        let cfg = config::get_storage_config()?;
        if ensure_repository_snapshots_materialized(&mut shared_state, &local_state, &cfg)? {
            shared_state = save_skills_state(shared_state)?;
        }

        let remote_repos = shared_state
            .repositories
            .iter()
            .filter(|repo| repo.source_type == "remote")
            .cloned()
            .collect::<Vec<_>>();

        let mut updated_repo_keys = vec![];
        let mut updated_skill_names = vec![];
        let mut synced_targets = vec![];

        for repo in remote_repos {
            if !repository_has_remote_source_update(&repo, &cfg) {
                continue;
            }
            let source_dir = match repository_source_dir(&repo, &cfg) {
                Ok(path) => path,
                Err(_) => continue,
            };
            let compare_before_dir = repo_storage_dir(&repo.repo_key)?;
            let result = apply_repository_update_from_dir(
                &mut shared_state,
                &mut local_state,
                &repo.repo_key,
                Some(compare_before_dir.as_path()),
                &source_dir,
                true,
            )?;
            if result.updated_files_count == 0 {
                continue;
            }
            updated_repo_keys.push(repo.repo_key.clone());
            let latest_name = shared_state
                .repositories
                .iter()
                .find(|item| item.repo_key == repo.repo_key)
                .map(|item| item.name.clone())
                .unwrap_or_else(|| repo.name.clone());
            updated_skill_names.push(latest_name);
            synced_targets.extend(result.synced_targets);
        }

        if !updated_repo_keys.is_empty() {
            shared_state = save_skills_state(shared_state)?;
            local_state = save_local_skills_state(local_state)?;
            for target in &synced_targets {
                let _ = reconcile_internal(
                    Some(&target.model),
                    Some(target.scope.as_str()),
                    target.project_root.as_deref(),
                );
            }
            trigger_storage_sync(app, "skills_repo_auto_update_pending");
        }

        let result = RepoAutoUpdateResult {
            updated_repo_count: updated_repo_keys.len() as u64,
            synced_target_count: synced_targets.len() as u64,
            updated_repo_keys,
            updated_skill_names,
            synced_targets,
            applied_at: now_ts(),
        };
        api_ok(result, combined_revision(&shared_state, &local_state))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn skills_update_check(input: SkillKeyInput) -> Result<ApiOk<bool>, String> {
    let update_scope = normalize_install_scope(input.scope.as_deref());
    let update_project_root =
        normalize_project_root_for_scope(&update_scope, input.project_root.as_deref())?;
    let mut state = load_local_skills_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    if update_scope == INSTALL_SCOPE_PROJECT {
        let record = find_current_installed_skill(
            &state,
            &sync_state,
            &cfg,
            &input.model,
            &input.skill_id,
            &update_scope,
            update_project_root.as_deref(),
        )?;
        return api_ok(record.has_update, state.revision);
    }
    let mut changed = false;
    for s in &mut state.skills {
        if s.model == input.model
            && s.id == input.skill_id
            && scope_project_match(s, &update_scope, update_project_root.as_deref())
        {
            if let Some(c) = sync_state
                .catalog
                .iter()
                .find(|c| c.source_id == s.source_id && c.rel_path == s.source_rel_path)
            {
                s.remote_hash = Some(c.remote_hash.clone());
                s.has_update = skill_has_markdown_update(s, &cfg).unwrap_or(false);
                changed = true;
            }
        }
    }
    let has_update = state
        .skills
        .iter()
        .find(|s| {
            s.model == input.model
                && s.id == input.skill_id
                && scope_project_match(s, &update_scope, update_project_root.as_deref())
        })
        .map(|s| s.has_update)
        .unwrap_or(false);
    let state = if changed {
        save_local_skills_state(state)?
    } else {
        state
    };
    api_ok(has_update, state.revision)
}

#[tauri::command]
pub fn skills_update_diff_preview(input: SkillKeyInput) -> Result<ApiOk<UpdateDiff>, String> {
    let diff_scope = normalize_install_scope(input.scope.as_deref());
    let diff_project_root =
        normalize_project_root_for_scope(&diff_scope, input.project_root.as_deref())?;
    let state = load_local_skills_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let record = find_current_installed_skill(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.skill_id,
        &diff_scope,
        diff_project_root.as_deref(),
    )?;

    let source = get_source(&cfg, &record.source_id).ok_or("source not found")?;
    let local_md =
        fs::read_to_string(record_local_dir(&record)?.join("SKILL.md")).unwrap_or_default();
    let remote_md = fs::read_to_string(
        source_skill_abs_path(source, &record.source_rel_path)?.join("SKILL.md"),
    )
    .unwrap_or_default();

    let (local_changed, remote_changed, local_blocks, remote_blocks) =
        calculate_changes(&local_md, &remote_md);
    let diff = UpdateDiff {
        local_markdown: local_md,
        remote_markdown: remote_md,
        local_changed_lines: local_changed,
        remote_changed_lines: remote_changed,
        local_changed_blocks: local_blocks,
        remote_changed_blocks: remote_blocks,
    };
    api_ok(diff, state.revision)
}

#[tauri::command]
pub async fn skills_update_apply(
    _app: tauri::AppHandle,
    input: SkillKeyInput,
) -> Result<ApiOk<SkillRecord>, String> {
    let update_scope = normalize_install_scope(input.scope.as_deref());
    let update_project_root =
        normalize_project_root_for_scope(&update_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "update:{}:{}:{}:{}",
        input.model,
        input.skill_id,
        update_scope,
        update_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            let sync_state = load_sync_state()?;
            let cfg = config::get_storage_config()?;
            let record = find_current_installed_skill(
                &state,
                &sync_state,
                &cfg,
                &input.model,
                &input.skill_id,
                &update_scope,
                update_project_root.as_deref(),
            )?;
            return api_ok(record, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let cfg = config::get_storage_config()?;
    let mut state = load_local_skills_state()?;
    let sync_state = load_sync_state()?;
    let mut record = find_current_installed_skill(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.skill_id,
        &update_scope,
        update_project_root.as_deref(),
    )?;
    let source = get_source(&cfg, &record.source_id).ok_or("source not found")?;
    let remote = source_skill_abs_path(source, &record.source_rel_path)?;
    let remote_dir_name = read_required_skill_dir_name(&remote)?;
    let record_scope_value = record_scope(&record);
    let record_project_root = record_project_root(&record);
    ensure_model_dir_name_available(
        &state,
        &input.model,
        &record_scope_value,
        record_project_root.as_deref(),
        &remote_dir_name,
        Some(record.id.as_str()),
    )?;
    let (model_root, compat_roots) = resolve_skill_target_dir(
        &input.model,
        &record_scope_value,
        record_project_root.as_deref(),
    )?;
    let local = model_root.join(&remote_dir_name);
    ensure_within(&model_root, &local)?;
    remove_existing_record_dir_if_moved(
        &state,
        &input.model,
        &record_scope_value,
        record_project_root.as_deref(),
        &record.id,
        &local,
    )?;

    replace_dir_atomic(&remote, &local)?;
    for compat_root in compat_roots {
        let compat_dest = compat_root.join(&remote_dir_name);
        ensure_within(&compat_root, &compat_dest)?;
        replace_dir_atomic(&local, &compat_dest)?;
    }
    record.dir_name = remote_dir_name;
    record.local_hash = hash_dir(&local)?;
    record.remote_hash = Some(hash_dir(&remote)?);
    record.updated_at = Some(now_ts());
    record.has_update = false;
    let revision = if update_scope == INSTALL_SCOPE_GLOBAL {
        if let Some(existing) = state.skills.iter_mut().find(|skill| {
            skill.model == input.model
                && skill.id == input.skill_id
                && scope_project_match(skill, &update_scope, update_project_root.as_deref())
        }) {
            *existing = record.clone();
        }
        save_local_skills_state(state)?.revision
    } else {
        state.revision
    };

    let _ = reconcile_internal(
        Some(&input.model),
        Some(record_scope_value.as_str()),
        record_project_root.as_deref(),
    );
    api_ok(record, revision)
}
