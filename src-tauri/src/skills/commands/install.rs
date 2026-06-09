#[tauri::command]
pub async fn skills_install(
    app: tauri::AppHandle,
    input: InstallInput,
) -> Result<ApiOk<SkillRecord>, String> {
    let install_scope = normalize_install_scope(input.scope.as_deref());
    let install_project_root =
        normalize_project_root_for_scope(&install_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "install:{}:{}:{}:{}",
        input.model,
        input.skill_ref,
        install_scope,
        install_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            let sync_state = load_sync_state()?;
            let cfg = config::get_storage_config()?;
            if let Some(found) = current_installed_skills(
                &state,
                &sync_state,
                &cfg,
                Some(&input.model),
                &install_scope,
                install_project_root.as_deref(),
            )?
            .into_iter()
            .find(|skill| skill.source_id == input.source_id)
            {
                return api_ok(found, state.revision);
            }
            return Err("duplicate job skipped".to_string());
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    if !MODELS.contains(&input.model.as_str()) {
        return Err("unsupported model".to_string());
    }

    let cfg = config::get_storage_config()?;
    let source = get_source(&cfg, &input.source_id).ok_or("source not found")?;
    let sync_state = load_sync_state()?;
    let catalog = sync_state
        .catalog
        .iter()
        .find(|c| {
            c.source_id == input.source_id
                && (c.rel_path == input.skill_ref || c.id == input.skill_ref)
        })
        .cloned()
        .ok_or("catalog skill not found")?;
    let allowed_models = resolve_effective_models(&catalog.models, &source.default_models);
    if !allowed_models.contains(&input.model) {
        return Err("skills/model_not_allowed".to_string());
    }

    let src = source_skill_abs_path(source, &catalog.rel_path)?;
    if !src.join("SKILL.md").exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }
    let catalog_dir_name = read_required_skill_dir_name(&src)?;

    let mut shared_state = load_skills_state()?;
    let mut local_state = load_local_skills_state()?;
    let expected_repo_key = make_repo_key(&catalog.source_id, &catalog.rel_path);
    let existing_repo = shared_state
        .repositories
        .iter()
        .find(|r| r.repo_key == expected_repo_key)
        .cloned();
    let mut shared_changed = false;
    let repo_record = if let Some(existing) = existing_repo {
        let repo_src = repo_storage_dir(&existing.repo_key)?;
        if repo_src.exists() {
            existing
        } else {
            shared_changed = true;
            upsert_repository_from_dir(
                &mut shared_state,
                &src,
                &catalog.source_id,
                &catalog.rel_path,
                &catalog.id,
                &catalog_dir_name,
                "remote",
                &catalog.name,
                &catalog.description,
                &allowed_models,
                &catalog.icon_seed,
                Some(src.to_string_lossy().to_string()),
                Some(catalog.remote_hash.clone()),
                true,
            )?
        }
    } else {
        shared_changed = true;
        upsert_repository_from_dir(
            &mut shared_state,
            &src,
            &catalog.source_id,
            &catalog.rel_path,
            &catalog.id,
            &catalog_dir_name,
            "remote",
            &catalog.name,
            &catalog.description,
            &allowed_models,
            &catalog.icon_seed,
            Some(src.to_string_lossy().to_string()),
            Some(catalog.remote_hash.clone()),
            true,
        )?
    };
    shared_changed =
        mark_repo_ever_installed(&mut shared_state, &repo_record.repo_key) || shared_changed;
    shared_changed = upsert_repo_dir_name(
        &mut shared_state,
        &repo_record.source_id,
        &repo_record.source_rel_path,
        &repo_record.skill_id,
        &catalog_dir_name,
    ) || shared_changed;

    ensure_model_dir_name_available(
        &local_state,
        &input.model,
        &install_scope,
        install_project_root.as_deref(),
        &catalog_dir_name,
        Some(repo_record.skill_id.as_str()),
    )?;
    let (model_root, compat_roots) = resolve_skill_target_dir(
        &input.model,
        &install_scope,
        install_project_root.as_deref(),
    )?;
    let dest = model_root.join(&catalog_dir_name);
    ensure_within(&model_root, &dest)?;
    remove_existing_record_dir_if_moved(
        &local_state,
        &input.model,
        &install_scope,
        install_project_root.as_deref(),
        &repo_record.skill_id,
        &dest,
    )?;
    let repo_src = repo_storage_dir(&repo_record.repo_key)?;
    replace_dir_atomic(&repo_src, &dest)?;

    let local_hash = hash_dir(&dest)?;
    let now = now_ts();
    let record = SkillRecord {
        id: repo_record.skill_id.clone(),
        dir_name: catalog_dir_name.clone(),
        model: input.model.clone(),
        models: allowed_models,
        name: catalog.name.clone(),
        description: catalog.description.clone(),
        source_id: repo_record.source_id.clone(),
        source_rel_path: repo_record.source_rel_path.clone(),
        installed_at: now,
        updated_at: None,
        last_synced_at: sync_state.last_sync_at,
        local_hash,
        remote_hash: repo_record.hash.clone(),
        has_update: false,
        icon_seed: repo_record.icon_seed.clone(),
        scope: install_scope.clone(),
        project_root: install_project_root.clone(),
        target_path: Some(dest.to_string_lossy().to_string()),
    };

    if install_scope == INSTALL_SCOPE_GLOBAL {
        local_state.skills.retain(|s| {
            !(s.model == input.model
                && s.id == repo_record.skill_id
                && scope_project_match(s, &install_scope, install_project_root.as_deref()))
        });
        local_state.skills.push(record.clone());
    }
    for compat_root in compat_roots {
        let compat_dest = compat_root.join(&catalog_dir_name);
        ensure_within(&compat_root, &compat_dest)?;
        replace_dir_atomic(&dest, &compat_dest)?;
    }
    if shared_changed {
        shared_state = save_skills_state(shared_state)?;
    }
    if install_scope == INSTALL_SCOPE_GLOBAL {
        local_state = save_local_skills_state(local_state)?;
    }

    let _ = reconcile_internal(
        Some(&input.model),
        Some(install_scope.as_str()),
        install_project_root.as_deref(),
    );
    if shared_changed {
        trigger_storage_sync(app, "skills_install");
    }
    api_ok(record, combined_revision(&shared_state, &local_state))
}
