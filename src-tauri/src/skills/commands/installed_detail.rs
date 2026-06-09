#[tauri::command]
pub async fn skills_uninstall(
    _app: tauri::AppHandle,
    input: SkillKeyInput,
) -> Result<ApiOk<bool>, String> {
    let uninstall_scope = normalize_install_scope(input.scope.as_deref());
    let uninstall_project_root =
        normalize_project_root_for_scope(&uninstall_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "uninstall:{}:{}:{}:{}",
        input.model,
        input.skill_id,
        uninstall_scope,
        uninstall_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            return api_ok(true, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let mut state = load_local_skills_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    if let Ok(record) = find_current_installed_skill(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.skill_id,
        &uninstall_scope,
        uninstall_project_root.as_deref(),
    ) {
        let local = locate_existing_record_local_dir(&record)?;
        let (root, compat_roots) = resolve_skill_target_dir(
            &input.model,
            &uninstall_scope,
            uninstall_project_root.as_deref(),
        )?;
        ensure_within(&root, &local)?;
        if local.exists() {
            fs::remove_dir_all(&local).map_err(|e| e.to_string())?;
        }
        let dir_name = normalized_record_dir_name(&record);
        for compat_root in compat_roots {
            let compat_path = compat_root.join(&dir_name);
            let _ = ensure_within(&compat_root, &compat_path);
            if compat_path.exists() {
                let _ = fs::remove_dir_all(&compat_path);
            }
        }
    }
    let revision = if uninstall_scope == INSTALL_SCOPE_GLOBAL {
        state.skills.retain(|s| {
            !(s.model == input.model
                && s.id == input.skill_id
                && scope_project_match(s, &uninstall_scope, uninstall_project_root.as_deref()))
        });
        save_local_skills_state(state)?.revision
    } else {
        state.revision
    };

    let _ = reconcile_internal(
        Some(&input.model),
        Some(uninstall_scope.as_str()),
        uninstall_project_root.as_deref(),
    );
    api_ok(true, revision)
}

#[tauri::command]
pub fn skills_detail_get(input: SkillKeyInput) -> Result<ApiOk<SkillDetail>, String> {
    let detail_scope = normalize_install_scope(input.scope.as_deref());
    let detail_project_root =
        normalize_project_root_for_scope(&detail_scope, input.project_root.as_deref())?;
    let state = load_local_skills_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let record = find_current_installed_skill(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.skill_id,
        &detail_scope,
        detail_project_root.as_deref(),
    )?;
    let local = record_local_dir(&record)?;
    let markdown = fs::read_to_string(local.join("SKILL.md")).unwrap_or_default();
    let detail = SkillDetail {
        skill: record,
        markdown,
        local_path: local.to_string_lossy().to_string(),
    };
    api_ok(detail, state.revision)
}

#[tauri::command]
pub fn skills_catalog_detail_get(
    input: CatalogSkillKeyInput,
) -> Result<ApiOk<CatalogSkillDetail>, String> {
    let cfg = config::get_storage_config()?;
    let source = get_source(&cfg, &input.source_id).ok_or("source not found")?;
    let sync_state = load_sync_state()?;
    let mut catalog = sync_state
        .catalog
        .iter()
        .find(|c| {
            c.source_id == input.source_id
                && (c.rel_path == input.skill_ref || c.id == input.skill_ref)
        })
        .cloned()
        .ok_or("catalog skill not found")?;
    let effective_models = resolve_effective_models(&catalog.models, &source.default_models);
    if effective_models.is_empty() {
        return Err("catalog skill not found".to_string());
    }
    catalog.models = effective_models;
    let source_path = source_skill_abs_path(source, &catalog.rel_path)?;
    let markdown = fs::read_to_string(source_path.join("SKILL.md")).unwrap_or_default();
    let detail = CatalogSkillDetail {
        skill: catalog,
        markdown,
        source_path: source_path.to_string_lossy().to_string(),
    };
    let shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let revision = combined_revision(&shared_state, &local_state);
    api_ok(detail, revision)
}

#[tauri::command]
pub fn skills_repo_detail_get(
    input: RepoSkillKeyInput,
) -> Result<ApiOk<CatalogSkillDetail>, String> {
    let mut state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let cfg = config::get_storage_config()?;
    if ensure_repository_snapshots_materialized(&mut state, &local_state, &cfg)? {
        state = save_skills_state(state)?;
    }
    let repo = state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned()
        .ok_or("repo skill not found")?;

    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    let mut markdown = String::new();
    let mut source_path = repo_snapshot.to_string_lossy().to_string();

    if repo_snapshot.join("SKILL.md").exists() {
        markdown = fs::read_to_string(repo_snapshot.join("SKILL.md")).unwrap_or_default();
    } else if let Some(src) = repo.source_path.clone() {
        let src_path = PathBuf::from(&src);
        if src_path.join("SKILL.md").exists() {
            markdown = fs::read_to_string(src_path.join("SKILL.md")).unwrap_or_default();
            source_path = src;
        }
    } else if repo.source_type == "remote" {
        if let Ok(cfg) = config::get_storage_config() {
            if let Some(source) = get_source(&cfg, &repo.source_id) {
                if let Ok(remote_path) = source_skill_abs_path(source, &repo.source_rel_path) {
                    if remote_path.join("SKILL.md").exists() {
                        markdown =
                            fs::read_to_string(remote_path.join("SKILL.md")).unwrap_or_default();
                        source_path = remote_path.to_string_lossy().to_string();
                    }
                }
            }
        }
    }

    let detail = CatalogSkillDetail {
        skill: CatalogSkill {
            source_id: repo.source_id.clone(),
            id: repo.skill_id.clone(),
            rel_path: repo.source_rel_path.clone(),
            dir_name: normalized_repo_dir_name(&repo),
            name: repo.name.clone(),
            description: repo.description.clone(),
            models: repo.models.clone(),
            remote_hash: repo.hash.clone().unwrap_or_default(),
            icon_seed: repo.icon_seed.clone(),
            first_seen_at: None,
        },
        markdown,
        source_path,
    };
    api_ok(detail, combined_revision(&state, &local_state))
}

#[tauri::command]
pub fn skills_repo_reload_preview(
    input: RepoSkillKeyInput,
) -> Result<ApiOk<ReloadPreview>, String> {
    let mut shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let cfg = config::get_storage_config()?;
    if ensure_repository_snapshots_materialized(&mut shared_state, &local_state, &cfg)? {
        shared_state = save_skills_state(shared_state)?;
    }
    let repo = shared_state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned()
        .ok_or("repo skill not found")?;

    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    if !repo_snapshot.exists() {
        return Err("repository_snapshot_missing".to_string());
    }

    let (before_dir, before_label, after_dir, after_label) =
        resolve_repo_reload_compare(&repo, &cfg)?;
    let (changed_files, text_diffs) = compare_snapshot_dirs(before_dir.as_deref(), &after_dir)?;
    let installed_targets = installed_targets_for_repo(&local_state, &repo);
    let installed_models = unique_models_from_targets(&installed_targets);

    let preview = ReloadPreview {
        before_label,
        after_label,
        changed_files: changed_files.clone(),
        text_diffs,
        installed_models,
        installed_targets,
        has_changes: !changed_files.is_empty(),
    };
    api_ok(preview, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn skills_repo_reload_apply(
    app: tauri::AppHandle,
    input: RepoReloadApplyInput,
) -> Result<ApiOk<ReloadApplyResult>, String> {
    let dedupe_key = format!("repo_reload_apply:{}", input.repo_key);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            let result = ReloadApplyResult {
                index_refreshed: false,
                synced_models: vec![],
                synced_targets: vec![],
                updated_files_count: 0,
                applied_at: now_ts(),
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
    let repo_idx = shared_state
        .repositories
        .iter()
        .position(|r| r.repo_key == input.repo_key)
        .ok_or("repo skill not found")?;
    let repo_snapshot = repo_storage_dir(&input.repo_key)?;
    if !repo_snapshot.exists() {
        return Err("repository_snapshot_missing".to_string());
    }

    let repo = shared_state.repositories[repo_idx].clone();
    let installed_targets = installed_targets_for_repo(&local_state, &repo);
    let should_sync_to_models = input.sync_to_models || !installed_targets.is_empty();
    let (before_dir, _before_label, apply_source_dir, _after_label) =
        resolve_repo_reload_compare(&repo, &cfg)?;
    let result = apply_repository_update_from_dir(
        &mut shared_state,
        &mut local_state,
        &input.repo_key,
        before_dir.as_deref(),
        &apply_source_dir,
        should_sync_to_models,
    )?;

    shared_state = save_skills_state(shared_state)?;
    local_state = save_local_skills_state(local_state)?;

    for target in &result.synced_targets {
        let _ = reconcile_internal(
            Some(&target.model),
            Some(target.scope.as_str()),
            target.project_root.as_deref(),
        );
    }
    trigger_storage_sync(app, "skills_repo_reload_apply");

    api_ok(result, combined_revision(&shared_state, &local_state))
}
