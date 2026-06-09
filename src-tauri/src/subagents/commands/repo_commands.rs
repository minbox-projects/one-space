use crate::config::{self};
use crate::subagents::{
    acquire_job_key, api_ok, build_repository_views, combined_revision,
    current_installed_subagents, ensure_model_dir_name_available, ensure_within, get_source,
    hash_dir, job_lock, load_local_subagents_state, load_subagents_state, load_sync_state,
    locate_existing_record_local_dir, make_repo_key, mark_repo_ever_installed,
    normalize_install_scope, normalize_project_root_for_scope, normalized_record_dir_name, now_ts,
    read_required_subagent_dir_name, read_required_subagent_dir_name_from_entry,
    reconcile_internal, remove_codex_project_agent_entry, remove_existing_record_dir_if_moved,
    replace_dir_atomic, repo_storage_dir, resolve_subagent_target_dir, save_local_subagents_state,
    save_subagents_state, scope_project_match, source_entry_exists, source_subagent_abs_path,
    subagents_sync_now, trigger_storage_sync, upsert_codex_project_agent_entry,
    upsert_repo_dir_name, upsert_repository_from_dir, ApiOk, RepoSetModelInput,
    RepoSubagentKeyInput, RepositorySubagentView, SubagentRecord, SubagentsSyncState,
    INSTALL_SCOPE_GLOBAL, INSTALL_SCOPE_PROJECT, MODELS,
};
use std::fs;

#[tauri::command]
pub fn subagents_sync_status_get() -> Result<ApiOk<SubagentsSyncState>, String> {
    let sync_state = load_sync_state()?;
    let shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let revision = combined_revision(&shared_state, &local_state);
    api_ok(sync_state, revision)
}

#[tauri::command]
pub fn subagents_repo_list(
    include_update: Option<bool>,
    scope: Option<String>,
    project_root: Option<String>,
) -> Result<ApiOk<Vec<RepositorySubagentView>>, String> {
    let repo_scope = normalize_install_scope(scope.as_deref());
    let repo_project_root = normalize_project_root_for_scope(&repo_scope, project_root.as_deref())?;
    let shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let cfg = config::get_storage_config()?;
    let sync_state = load_sync_state()?;
    let installed = current_installed_subagents(
        &local_state,
        &sync_state,
        &cfg,
        None,
        &repo_scope,
        repo_project_root.as_deref(),
    )?;
    let list = build_repository_views(&shared_state, &installed, include_update.unwrap_or(false));
    api_ok(list, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn subagents_repo_list_with_update(
    scope: Option<String>,
    project_root: Option<String>,
) -> Result<ApiOk<Vec<RepositorySubagentView>>, String> {
    let repo_scope = normalize_install_scope(scope.as_deref());
    let repo_project_root = normalize_project_root_for_scope(&repo_scope, project_root.as_deref())?;
    let shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let cfg = config::get_storage_config()?;
    let sync_state = load_sync_state()?;
    let installed = current_installed_subagents(
        &local_state,
        &sync_state,
        &cfg,
        None,
        &repo_scope,
        repo_project_root.as_deref(),
    )?;
    let list = build_repository_views(&shared_state, &installed, true);
    api_ok(list, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn subagents_repo_refresh(
    app: tauri::AppHandle,
) -> Result<ApiOk<Vec<RepositorySubagentView>>, String> {
    let _ = subagents_sync_now(app.clone()).await?;
    let shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let list = build_repository_views(&shared_state, &local_state.subagents, true);
    api_ok(list, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn subagents_repo_refresh_background(app: tauri::AppHandle) -> Result<ApiOk<bool>, String> {
    std::thread::spawn(move || {
        let _ = tauri::async_runtime::block_on(subagents_repo_refresh(app));
    });
    let shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    api_ok(true, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn subagents_repo_set_model(
    app: tauri::AppHandle,
    input: RepoSetModelInput,
) -> Result<ApiOk<RepositorySubagentView>, String> {
    if !MODELS.contains(&input.model.as_str()) {
        return Err("unsupported model".to_string());
    }

    let repo_scope = normalize_install_scope(input.scope.as_deref());
    let repo_project_root =
        normalize_project_root_for_scope(&repo_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "repo_set:{}:{}:{}:{}:{}",
        input.repo_key,
        input.model,
        input.enabled,
        repo_scope,
        repo_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_subagents_state()?;
            let local_state = load_local_subagents_state()?;
            let cfg = config::get_storage_config()?;
            let sync_state = load_sync_state()?;
            let installed = current_installed_subagents(
                &local_state,
                &sync_state,
                &cfg,
                None,
                &repo_scope,
                repo_project_root.as_deref(),
            )?;
            let view = build_repository_views(&shared_state, &installed, false)
                .into_iter()
                .find(|v| v.repo_key == input.repo_key)
                .ok_or("repo subagent not found")?;
            return api_ok(view, combined_revision(&shared_state, &local_state));
        }
    };

    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let mut shared_state = load_subagents_state()?;
    let mut local_state = load_local_subagents_state()?;
    let repo = shared_state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned()
        .ok_or("repo subagent not found")?;
    let mut shared_changed = false;
    let mut local_changed = false;

    let repo_src = repo_storage_dir(&repo.repo_key)?;
    if input.enabled && !repo_src.exists() {
        if repo.source_type == "remote" {
            let cfg = config::get_storage_config()?;
            let source = get_source(&cfg, &repo.source_id).ok_or("source not found")?;
            let source_path = source_subagent_abs_path(source, &repo.source_rel_path)?;
            if !source_entry_exists(&source_path) {
                return Err("subagents/invalid_subagent_dir".to_string());
            }
            let dir_name = read_required_subagent_dir_name_from_entry(&source_path)?;
            let _ = upsert_repository_from_dir(
                &mut shared_state,
                &source_path,
                &repo.source_id,
                &repo.source_rel_path,
                &repo.subagent_id,
                &dir_name,
                &repo.source_type,
                &repo.name,
                &repo.description,
                &repo.models,
                repo.model.as_deref(),
                &repo.tools,
                &repo.icon_seed,
                Some(source_path.to_string_lossy().to_string()),
                repo.hash.clone(),
                true,
            )?;
            shared_changed = true;
        } else {
            return Err("repository_snapshot_missing".to_string());
        }
    }

    if input.enabled {
        let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
        let repo_dir_name = read_required_subagent_dir_name(&repo_snapshot)?;
        shared_changed = upsert_repo_dir_name(
            &mut shared_state,
            &repo.source_id,
            &repo.source_rel_path,
            &repo.subagent_id,
            &repo_dir_name,
        ) || shared_changed;
        shared_changed =
            mark_repo_ever_installed(&mut shared_state, &repo.repo_key) || shared_changed;
        ensure_model_dir_name_available(
            &local_state,
            &input.model,
            &repo_scope,
            repo_project_root.as_deref(),
            &repo_dir_name,
            Some(repo.subagent_id.as_str()),
        )?;
        let (model_root, compat_roots) =
            resolve_subagent_target_dir(&input.model, &repo_scope, repo_project_root.as_deref())?;
        let dest = model_root.join(&repo_dir_name);
        ensure_within(&model_root, &dest)?;
        remove_existing_record_dir_if_moved(
            &local_state,
            &input.model,
            &repo_scope,
            repo_project_root.as_deref(),
            &repo.subagent_id,
            &dest,
        )?;
        let src = repo_storage_dir(&repo.repo_key)?;
        replace_dir_atomic(&src, &dest)?;
        for compat_root in compat_roots {
            let compat_dest = compat_root.join(&repo_dir_name);
            ensure_within(&compat_root, &compat_dest)?;
            replace_dir_atomic(&dest, &compat_dest)?;
        }
        let local_hash = hash_dir(&dest)?;
        if repo_scope == INSTALL_SCOPE_GLOBAL {
            local_state.subagents.retain(|s| {
                !(s.model == input.model
                    && s.id == repo.subagent_id
                    && scope_project_match(s, &repo_scope, repo_project_root.as_deref()))
            });
            local_state.subagents.push(SubagentRecord {
                id: repo.subagent_id.clone(),
                dir_name: repo_dir_name.clone(),
                model: input.model.clone(),
                models: repo.models.clone(),
                name: repo.name.clone(),
                description: repo.description.clone(),
                source_id: repo.source_id.clone(),
                source_rel_path: repo.source_rel_path.clone(),
                installed_at: now_ts(),
                updated_at: None,
                last_synced_at: shared_state.last_sync_at,
                local_hash,
                remote_hash: repo.hash.clone(),
                has_update: false,
                icon_seed: repo.icon_seed.clone(),
                scope: repo_scope.clone(),
                project_root: repo_project_root.clone(),
                target_path: Some(dest.to_string_lossy().to_string()),
            });
        }
        if input.model == "codex" && repo_scope == INSTALL_SCOPE_PROJECT {
            if let Some(project_root) = repo_project_root.as_deref() {
                let prompt = fs::read_to_string(dest.join("AGENT.md")).unwrap_or_default();
                upsert_codex_project_agent_entry(
                    project_root,
                    &repo_dir_name,
                    &repo.name,
                    repo.model.as_deref(),
                    &repo.tools,
                    &prompt,
                )?;
            }
        }
        local_changed = repo_scope == INSTALL_SCOPE_GLOBAL;
    } else {
        let (_, compat_roots) =
            resolve_subagent_target_dir(&input.model, &repo_scope, repo_project_root.as_deref())?;
        let records_to_remove = local_state
            .subagents
            .iter()
            .filter(|s| {
                s.model == input.model
                    && scope_project_match(s, &repo_scope, repo_project_root.as_deref())
                    && (s.id == repo.subagent_id
                        || make_repo_key(&s.source_id, &s.source_rel_path) == repo.repo_key)
            })
            .cloned()
            .collect::<Vec<_>>();
        for record in records_to_remove {
            let dest = locate_existing_record_local_dir(&record)?;
            if dest.exists() {
                fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
            }
            let dir_name = normalized_record_dir_name(&record);
            for compat_root in &compat_roots {
                let compat = compat_root.join(&dir_name);
                let _ = ensure_within(compat_root, &compat);
                if compat.exists() {
                    let _ = fs::remove_dir_all(&compat);
                }
            }
            if input.model == "codex" && repo_scope == INSTALL_SCOPE_PROJECT {
                if let Some(project_root) = repo_project_root.as_deref() {
                    let _ = remove_codex_project_agent_entry(project_root, &dir_name);
                }
            }
        }
        let before = local_state.subagents.len();
        local_state.subagents.retain(|s| {
            !(s.model == input.model
                && scope_project_match(s, &repo_scope, repo_project_root.as_deref())
                && (s.id == repo.subagent_id
                    || make_repo_key(&s.source_id, &s.source_rel_path) == repo.repo_key))
        });
        local_changed = local_changed || before != local_state.subagents.len();
    }

    if shared_changed {
        shared_state = save_subagents_state(shared_state)?;
    }
    if local_changed {
        local_state = save_local_subagents_state(local_state)?;
    }
    let _ = reconcile_internal(
        Some(&input.model),
        Some(repo_scope.as_str()),
        repo_project_root.as_deref(),
    );
    if shared_changed {
        trigger_storage_sync(app, "subagents_repo_set_model");
    }

    let cfg = config::get_storage_config()?;
    let sync_state = load_sync_state()?;
    let installed = current_installed_subagents(
        &local_state,
        &sync_state,
        &cfg,
        None,
        &repo_scope,
        repo_project_root.as_deref(),
    )?;
    let view = build_repository_views(&shared_state, &installed, false)
        .into_iter()
        .find(|v| v.repo_key == input.repo_key)
        .ok_or("repo subagent not found")?;
    api_ok(view, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn subagents_repo_delete(
    app: tauri::AppHandle,
    input: RepoSubagentKeyInput,
) -> Result<ApiOk<bool>, String> {
    let dedupe_key = format!("repo_delete:{}", input.repo_key);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_subagents_state()?;
            let local_state = load_local_subagents_state()?;
            return api_ok(true, combined_revision(&shared_state, &local_state));
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let repo = shared_state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned();

    let global_in_use = local_state.subagents.iter().any(|s| {
        make_repo_key(&s.source_id, &s.source_rel_path) == input.repo_key
            || repo
                .as_ref()
                .map(|r| s.id == r.subagent_id)
                .unwrap_or(false)
    });
    let project_in_use = crate::workspaces::workspace_roots()?
        .into_iter()
        .any(|project_root| {
            current_installed_subagents(
                &local_state,
                &sync_state,
                &cfg,
                None,
                INSTALL_SCOPE_PROJECT,
                Some(project_root.as_str()),
            )
            .map(|records| {
                records.iter().any(|subagent| {
                    make_repo_key(&subagent.source_id, &subagent.source_rel_path) == input.repo_key
                        || repo
                            .as_ref()
                            .map(|r| subagent.id == r.subagent_id)
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false)
        });
    if global_in_use || project_in_use {
        return Err("subagents/repo_in_use".to_string());
    }

    let before = shared_state.repositories.len();
    shared_state
        .repositories
        .retain(|r| r.repo_key != input.repo_key);
    let changed = before != shared_state.repositories.len();

    if changed {
        let repo_src = repo_storage_dir(&input.repo_key)?;
        if repo_src.exists() {
            fs::remove_dir_all(&repo_src).map_err(|e| e.to_string())?;
        }
        shared_state = save_subagents_state(shared_state)?;
        trigger_storage_sync(app, "subagents_repo_delete");
    }

    api_ok(true, combined_revision(&shared_state, &local_state))
}
