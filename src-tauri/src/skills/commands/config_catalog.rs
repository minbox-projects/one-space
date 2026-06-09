#[tauri::command]
pub fn skills_config_get() -> Result<ApiOk<SkillsConfigPayload>, String> {
    let cfg = config::get_storage_config()?;
    let payload = SkillsConfigPayload {
        skills_sync_enabled: cfg.skills_sync_enabled.unwrap_or(true),
        skills_auto_update_enabled: cfg.skills_auto_update_enabled.unwrap_or(false),
        skills_sync_interval_minutes: cfg.skills_sync_interval_minutes.unwrap_or(60).max(5),
        skills_new_badge_hours: cfg.skills_new_badge_hours.unwrap_or(72).clamp(1, 720),
        skills_sources: cfg.skills_sources,
    };
    let state = load_skills_state()?;
    api_ok(payload, state.revision)
}

#[tauri::command]
pub async fn skills_config_save(
    app: tauri::AppHandle,
    config_payload: SkillsConfigPayload,
) -> Result<ApiOk<SkillsConfigPayload>, String> {
    {
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;
        let mut cfg = config::get_storage_config()?;
        cfg.skills_sync_enabled = Some(config_payload.skills_sync_enabled);
        cfg.skills_auto_update_enabled = Some(config_payload.skills_auto_update_enabled);
        cfg.skills_sync_interval_minutes = Some(config_payload.skills_sync_interval_minutes.max(5));
        cfg.skills_new_badge_hours = Some(config_payload.skills_new_badge_hours.clamp(1, 720));
        cfg.skills_sources = config_payload.skills_sources.clone();
        drop(_guard);
        config::save_storage_config(app.clone(), cfg).await?;
    }
    let state = load_skills_state()?;
    api_ok(config_payload, state.revision)
}

#[tauri::command]
pub fn skills_sources_export_to_path(
    output_path: String,
    skills_sources: Vec<SkillSourceConfig>,
) -> Result<String, String> {
    let payload = SkillsSourcesExportPayload {
        version: 1,
        exported_at: chrono::Utc::now().to_rfc3339(),
        skills_sources,
    };
    let content = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    let path = PathBuf::from(&output_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(output_path)
}

#[tauri::command]
pub fn skills_list_installed(
    model: Option<String>,
    scope: Option<String>,
    project_root: Option<String>,
) -> Result<ApiOk<Vec<SkillRecord>>, String> {
    let list_scope = normalize_install_scope(scope.as_deref());
    let list_project_root = normalize_project_root_for_scope(&list_scope, project_root.as_deref())?;
    let lock_guard = match job_lock().try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::WouldBlock) => None,
        Err(TryLockError::Poisoned(err)) => return Err(err.to_string()),
    };
    let mut shared_state = load_skills_state()?;
    let mut local_state = load_local_skills_state()?;
    let cfg = config::get_storage_config()?;
    let sync_state = load_sync_state()?;

    if lock_guard.is_some() {
        let (shared_changed, migrated_local_changed) =
            migrate_installed_dir_names(&mut shared_state, &mut local_state)?;
        let refreshed_local_changed = if model.is_some() {
            refresh_local_hashes(&mut local_state, model.as_deref())?
        } else {
            false
        };
        let local_changed = migrated_local_changed || refreshed_local_changed;
        if shared_changed {
            shared_state = save_skills_state(shared_state)?;
        }
        if local_changed {
            local_state = save_local_skills_state(local_state)?;
        }
    }

    let list = current_installed_skills(
        &local_state,
        &sync_state,
        &cfg,
        model.as_deref(),
        &list_scope,
        list_project_root.as_deref(),
    )?;
    api_ok(list, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn skills_list_catalog(model: Option<String>) -> Result<ApiOk<Vec<CatalogSkill>>, String> {
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let source_allow_map: HashMap<String, Vec<String>> = cfg
        .skills_sources
        .iter()
        .map(|s| (s.id.clone(), normalize_models(&s.default_models)))
        .collect();
    let requested_model = model.as_ref().and_then(|m| normalized_model(m));
    if model.is_some() && requested_model.is_none() {
        let revision = load_skills_state()?.revision;
        return api_ok(Vec::<CatalogSkill>::new(), revision);
    }
    let list = sync_state
        .catalog
        .iter()
        .filter_map(|s| {
            let source_allowed = source_allow_map.get(&s.source_id)?;
            let effective_models = resolve_effective_models(&s.models, source_allowed);
            if effective_models.is_empty() {
                return None;
            }
            if let Some(target) = requested_model.as_ref() {
                if !effective_models.contains(target) {
                    return None;
                }
            }
            let mut entry = s.clone();
            entry.models = effective_models;
            Some(entry)
        })
        .collect::<Vec<_>>();
    let revision = load_skills_state()?.revision;
    api_ok(list, revision)
}

#[tauri::command]
pub async fn skills_sync_now(app: tauri::AppHandle) -> Result<ApiOk<SkillsSyncState>, String> {
    tauri::async_runtime::spawn_blocking(move || skills_sync_now_blocking(app))
        .await
        .map_err(|e| e.to_string())?
}
