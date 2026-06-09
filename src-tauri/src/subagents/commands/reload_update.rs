use crate::config::{self};
use crate::subagents::{
    acquire_job_key, api_ok, calculate_changes, combined_revision, compare_snapshot_dirs,
    ensure_model_dir_name_available, ensure_repository_snapshots_materialized, ensure_within,
    find_current_installed_subagent, get_source, hash_dir, hash_source_entry,
    installed_models_for_repo, job_lock, load_local_subagents_state, load_subagents_state,
    load_sync_state, locate_existing_record_local_dir, model_dir, normalize_install_scope,
    normalize_project_root_for_scope, normalized_repo_dir_name, now_ts,
    read_markdown_from_source_entry, read_required_subagent_dir_name_from_entry,
    reconcile_internal, record_local_dir, record_project_root, record_scope,
    refresh_repository_record_from_snapshot, remove_existing_record_dir_if_moved,
    replace_dir_atomic, replace_source_entry_atomic, repo_index_baseline_dir, repo_storage_dir,
    resolve_repo_reload_after_dir, resolve_subagent_target_dir, save_local_subagents_state,
    save_subagents_state, scope_project_match, snapshot_repository_index_baseline,
    source_entry_exists, source_subagent_abs_path, subagent_has_markdown_update,
    subagent_matches_repository, trigger_storage_sync, ApiOk, ReloadApplyResult,
    RepoReloadApplyInput, SubagentKeyInput, SubagentRecord, UpdateDiff, INSTALL_SCOPE_GLOBAL,
    INSTALL_SCOPE_PROJECT, MODELS,
};
use std::collections::{HashMap, HashSet};
use std::fs;

#[tauri::command]
pub async fn subagents_repo_reload_apply(
    app: tauri::AppHandle,
    input: RepoReloadApplyInput,
) -> Result<ApiOk<ReloadApplyResult>, String> {
    let dedupe_key = format!("repo_reload_apply:{}", input.repo_key);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_subagents_state()?;
            let local_state = load_local_subagents_state()?;
            let result = ReloadApplyResult {
                index_refreshed: false,
                synced_models: vec![],
                updated_files_count: 0,
                applied_at: now_ts(),
            };
            return api_ok(result, combined_revision(&shared_state, &local_state));
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut shared_state = load_subagents_state()?;
    let mut local_state = load_local_subagents_state()?;
    let cfg = config::get_storage_config()?;
    if ensure_repository_snapshots_materialized(&mut shared_state, &local_state, &cfg)? {
        shared_state = save_subagents_state(shared_state)?;
    }
    let repo_idx = shared_state
        .repositories
        .iter()
        .position(|r| r.repo_key == input.repo_key)
        .ok_or("repo subagent not found")?;
    let repo_snapshot = repo_storage_dir(&input.repo_key)?;
    if !repo_snapshot.exists() {
        return Err("repository_snapshot_missing".to_string());
    }

    let baseline = repo_index_baseline_dir(&input.repo_key)?;
    let (apply_source_dir, _after_label) = resolve_repo_reload_after_dir(
        &shared_state.repositories[repo_idx],
        if baseline.exists() {
            Some(baseline.as_path())
        } else {
            None
        },
        &repo_snapshot,
    )?;
    let (changed_files, _) = compare_snapshot_dirs(
        if baseline.exists() {
            Some(baseline.as_path())
        } else {
            None
        },
        &apply_source_dir,
    )?;
    let updated_files_count = changed_files.len() as u64;
    if apply_source_dir != repo_snapshot {
        replace_dir_atomic(&apply_source_dir, &repo_snapshot)?;
    }

    {
        let repo = shared_state
            .repositories
            .get_mut(repo_idx)
            .ok_or("repo subagent not found")?;
        refresh_repository_record_from_snapshot(repo)?;
    }

    let repo = shared_state.repositories[repo_idx].clone();
    let installed_models = installed_models_for_repo(&local_state, &repo);
    let should_sync_to_models = input.sync_to_models || !installed_models.is_empty();
    let now = now_ts();
    let repo_dir_name = normalized_repo_dir_name(&repo);
    let mut model_match_subagent_ids = HashMap::<String, String>::new();
    for model in MODELS {
        if let Some(subagent) = local_state
            .subagents
            .iter()
            .find(|s| s.model == model && subagent_matches_repository(s, &repo))
        {
            model_match_subagent_ids.insert(model.to_string(), subagent.id.clone());
        }
    }
    let previous_local_dirs = local_state
        .subagents
        .iter()
        .filter_map(|s| {
            if !subagent_matches_repository(s, &repo) {
                return None;
            }
            locate_existing_record_local_dir(s)
                .ok()
                .map(|dir| (s.model.clone(), dir))
        })
        .collect::<HashMap<_, _>>();

    for s in &mut local_state.subagents {
        if !subagent_matches_repository(s, &repo) {
            continue;
        }
        s.dir_name = repo_dir_name.clone();
        s.name = repo.name.clone();
        s.description = repo.description.clone();
        s.models = repo.models.clone();
        s.remote_hash = repo.hash.clone();
        s.has_update = subagent_has_markdown_update(s, &cfg).unwrap_or(false);
        s.updated_at = Some(now);
    }

    let mut synced_models = vec![];
    if should_sync_to_models {
        let installed_set = installed_models
            .iter()
            .cloned()
            .collect::<HashSet<String>>();
        for model in MODELS {
            if !installed_set.contains(model) {
                continue;
            }
            let ignore_subagent_id = model_match_subagent_ids
                .get(model)
                .map(|s| s.as_str())
                .unwrap_or(repo.subagent_id.as_str());
            ensure_model_dir_name_available(
                &local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                &repo_dir_name,
                Some(ignore_subagent_id),
            )?;
            let model_root = model_dir(model)?;
            let dest = model_root.join(&repo_dir_name);
            ensure_within(&model_root, &dest)?;
            remove_existing_record_dir_if_moved(
                &local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                ignore_subagent_id,
                &dest,
            )?;
            replace_dir_atomic(&repo_snapshot, &dest)?;
            if let Some(previous_dir) = previous_local_dirs.get(model) {
                if previous_dir != &dest && previous_dir.exists() {
                    let _ = fs::remove_dir_all(previous_dir);
                }
            }
            let local_hash = hash_dir(&dest)?;
            for s in &mut local_state.subagents {
                if s.model == model && subagent_matches_repository(s, &repo) {
                    s.dir_name = repo_dir_name.clone();
                    s.local_hash = local_hash.clone();
                    s.remote_hash = repo.hash.clone();
                    s.has_update = false;
                    s.last_synced_at = Some(now);
                    s.updated_at = Some(now);
                }
            }
            synced_models.push(model.to_string());
        }
    }

    // Only move baseline forward after model sync path has completed successfully.
    // This prevents "has update" from being cleared while installed models are still stale.
    snapshot_repository_index_baseline(&input.repo_key, &repo_snapshot)?;

    shared_state = save_subagents_state(shared_state)?;
    local_state = save_local_subagents_state(local_state)?;

    for model in &synced_models {
        let _ = reconcile_internal(Some(model), Some(INSTALL_SCOPE_GLOBAL), None);
    }
    trigger_storage_sync(app, "subagents_repo_reload_apply");

    let result = ReloadApplyResult {
        index_refreshed: true,
        synced_models,
        updated_files_count,
        applied_at: now,
    };
    api_ok(result, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn subagents_update_check(input: SubagentKeyInput) -> Result<ApiOk<bool>, String> {
    let update_scope = normalize_install_scope(input.scope.as_deref());
    let update_project_root =
        normalize_project_root_for_scope(&update_scope, input.project_root.as_deref())?;
    let mut state = load_local_subagents_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    if update_scope == INSTALL_SCOPE_PROJECT {
        let record = find_current_installed_subagent(
            &state,
            &sync_state,
            &cfg,
            &input.model,
            &input.subagent_id,
            &update_scope,
            update_project_root.as_deref(),
        )?;
        return api_ok(record.has_update, state.revision);
    }
    let mut changed = false;
    for s in &mut state.subagents {
        if s.model == input.model
            && s.id == input.subagent_id
            && scope_project_match(s, &update_scope, update_project_root.as_deref())
        {
            if let Some(c) = sync_state
                .catalog
                .iter()
                .find(|c| c.source_id == s.source_id && c.rel_path == s.source_rel_path)
            {
                s.remote_hash = Some(c.remote_hash.clone());
                s.has_update = subagent_has_markdown_update(s, &cfg).unwrap_or(false);
                changed = true;
            }
        }
    }
    let has_update = state
        .subagents
        .iter()
        .find(|s| {
            s.model == input.model
                && s.id == input.subagent_id
                && scope_project_match(s, &update_scope, update_project_root.as_deref())
        })
        .map(|s| s.has_update)
        .unwrap_or(false);
    let state = if changed {
        save_local_subagents_state(state)?
    } else {
        state
    };
    api_ok(has_update, state.revision)
}

#[tauri::command]
pub fn subagents_update_diff_preview(input: SubagentKeyInput) -> Result<ApiOk<UpdateDiff>, String> {
    let diff_scope = normalize_install_scope(input.scope.as_deref());
    let diff_project_root =
        normalize_project_root_for_scope(&diff_scope, input.project_root.as_deref())?;
    let state = load_local_subagents_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let record = find_current_installed_subagent(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.subagent_id,
        &diff_scope,
        diff_project_root.as_deref(),
    )?;

    let source = get_source(&cfg, &record.source_id).ok_or("source not found")?;
    let local_md =
        fs::read_to_string(record_local_dir(&record)?.join("AGENT.md")).unwrap_or_default();
    let remote_entry = source_subagent_abs_path(source, &record.source_rel_path)?;
    let remote_md = read_markdown_from_source_entry(&remote_entry).unwrap_or_default();

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
pub async fn subagents_update_apply(
    _app: tauri::AppHandle,
    input: SubagentKeyInput,
) -> Result<ApiOk<SubagentRecord>, String> {
    let update_scope = normalize_install_scope(input.scope.as_deref());
    let update_project_root =
        normalize_project_root_for_scope(&update_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "update:{}:{}:{}:{}",
        input.model,
        input.subagent_id,
        update_scope,
        update_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_subagents_state()?;
            let sync_state = load_sync_state()?;
            let cfg = config::get_storage_config()?;
            let record = find_current_installed_subagent(
                &state,
                &sync_state,
                &cfg,
                &input.model,
                &input.subagent_id,
                &update_scope,
                update_project_root.as_deref(),
            )?;
            return api_ok(record, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let cfg = config::get_storage_config()?;
    let mut state = load_local_subagents_state()?;
    let sync_state = load_sync_state()?;
    let mut record = find_current_installed_subagent(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.subagent_id,
        &update_scope,
        update_project_root.as_deref(),
    )?;
    let source = get_source(&cfg, &record.source_id).ok_or("source not found")?;
    let remote = source_subagent_abs_path(source, &record.source_rel_path)?;
    if !source_entry_exists(&remote) {
        return Err("subagents/invalid_subagent_dir".to_string());
    }
    let remote_dir_name = read_required_subagent_dir_name_from_entry(&remote)?;
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
    let (model_root, compat_roots) = resolve_subagent_target_dir(
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

    replace_source_entry_atomic(&remote, &local)?;
    for compat_root in compat_roots {
        let compat_dest = compat_root.join(&remote_dir_name);
        ensure_within(&compat_root, &compat_dest)?;
        replace_dir_atomic(&local, &compat_dest)?;
    }
    record.dir_name = remote_dir_name;
    record.local_hash = hash_dir(&local)?;
    record.remote_hash = Some(hash_source_entry(&remote)?);
    record.updated_at = Some(now_ts());
    record.has_update = false;
    let revision = if update_scope == INSTALL_SCOPE_GLOBAL {
        if let Some(existing) = state.subagents.iter_mut().find(|subagent| {
            subagent.model == input.model
                && subagent.id == input.subagent_id
                && scope_project_match(subagent, &update_scope, update_project_root.as_deref())
        }) {
            *existing = record.clone();
        }
        save_local_subagents_state(state)?.revision
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
