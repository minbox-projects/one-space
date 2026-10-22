use crate::config;
use crate::subagents::{
    api_ok, combined_revision, current_installed_subagents, get_source, git_run, job_lock,
    load_local_subagents_state, load_subagents_state, load_sync_state, migrate_installed_dir_names,
    normalize_install_scope, normalize_models, normalize_project_root_for_scope, normalized_model,
    refresh_local_hashes, resolve_effective_models, save_local_subagents_state,
    save_subagents_state, scan_source_catalog_with_diagnostics, subagents_cache_root,
    subagents_sync_now_blocking, sync_source_repo, ApiOk, CatalogSubagent, SubagentRecord,
    SubagentSourceDiagnoseInput, SubagentSourceDiagnoseResult, SubagentsSyncState,
};
use std::collections::HashMap;
use std::sync::TryLockError;

#[tauri::command]
pub fn subagents_list_installed(
    model: Option<String>,
    scope: Option<String>,
    project_root: Option<String>,
) -> Result<ApiOk<Vec<SubagentRecord>>, String> {
    let list_scope = normalize_install_scope(scope.as_deref());
    let list_project_root = normalize_project_root_for_scope(&list_scope, project_root.as_deref())?;
    let lock_guard = match job_lock().try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::WouldBlock) => None,
        Err(TryLockError::Poisoned(err)) => return Err(err.to_string()),
    };
    let mut shared_state = load_subagents_state()?;
    let mut local_state = load_local_subagents_state()?;
    let cfg = config::get_storage_config()?;
    let sync_state = load_sync_state()?;

    if lock_guard.is_some() {
        let (shared_changed, migrated_local_changed) =
            migrate_installed_dir_names(&mut shared_state, &mut local_state)?;
        let refreshed_local_changed = if model.is_some() {
            refresh_local_hashes(&mut local_state, model.as_deref(), &cfg)?
        } else {
            false
        };
        let local_changed = migrated_local_changed || refreshed_local_changed;
        if shared_changed {
            shared_state = save_subagents_state(shared_state)?;
        }
        if local_changed {
            local_state = save_local_subagents_state(local_state)?;
        }
    }

    let list = current_installed_subagents(
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
pub fn subagents_list_catalog(
    model: Option<String>,
) -> Result<ApiOk<Vec<CatalogSubagent>>, String> {
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let source_allow_map: HashMap<String, Vec<String>> = cfg
        .subagents_sources
        .iter()
        .map(|s| (s.id.clone(), normalize_models(&s.default_models)))
        .collect();
    let requested_model = model.as_ref().and_then(|m| normalized_model(m));
    if model.is_some() && requested_model.is_none() {
        let revision = load_subagents_state()?.revision;
        return api_ok(Vec::<CatalogSubagent>::new(), revision);
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
    let revision = load_subagents_state()?.revision;
    api_ok(list, revision)
}

#[tauri::command]
pub fn subagents_source_diagnose(
    input: SubagentSourceDiagnoseInput,
) -> Result<ApiOk<SubagentSourceDiagnoseResult>, String> {
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let cfg = config::get_storage_config()?;
    let source = get_source(&cfg, &input.source_id).ok_or("source not found")?;

    let cache_repo_dir = subagents_cache_root()?.join(&source.id);
    let repo_dir = if input.sync_first || !cache_repo_dir.exists() {
        sync_source_repo(source)?
    } else {
        cache_repo_dir
    };

    let (_catalog, mut diagnostics) = scan_source_catalog_with_diagnostics(&repo_dir, source)?;
    diagnostics.last_commit_sha = git_run(Some(&repo_dir), &["rev-parse", "HEAD"]).ok();

    let shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    api_ok(diagnostics, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn subagents_sync_now(
    app: tauri::AppHandle,
) -> Result<ApiOk<SubagentsSyncState>, String> {
    tauri::async_runtime::spawn_blocking(move || subagents_sync_now_blocking(app))
        .await
        .map_err(|e| e.to_string())?
}
