use crate::config::{self};
use crate::subagents::{
    acquire_job_key, api_ok, combined_revision, compare_snapshot_dirs,
    ensure_repository_snapshots_materialized, ensure_within, find_current_installed_subagent,
    get_source, installed_models_for_repo, job_lock, load_local_subagents_state,
    load_subagents_state, load_sync_state, locate_existing_record_local_dir,
    normalize_install_scope, normalize_project_root_for_scope, normalized_record_dir_name,
    normalized_repo_dir_name, read_markdown_from_source_entry, reconcile_internal,
    record_local_dir, remove_codex_project_agent_entry, repo_index_baseline_dir, repo_storage_dir,
    resolve_effective_models, resolve_repo_reload_after_dir, resolve_subagent_target_dir,
    save_local_subagents_state, save_subagents_state, scope_project_match, source_entry_exists,
    source_subagent_abs_path, ApiOk, CatalogSubagent, CatalogSubagentDetail,
    CatalogSubagentKeyInput, ReloadPreview, RepoSubagentKeyInput, SubagentDetail, SubagentKeyInput,
    INSTALL_SCOPE_GLOBAL, INSTALL_SCOPE_PROJECT,
};
use std::fs;
use std::path::PathBuf;

#[tauri::command]
pub async fn subagents_uninstall(
    _app: tauri::AppHandle,
    input: SubagentKeyInput,
) -> Result<ApiOk<bool>, String> {
    let uninstall_scope = normalize_install_scope(input.scope.as_deref());
    let uninstall_project_root =
        normalize_project_root_for_scope(&uninstall_scope, input.project_root.as_deref())?;
    let dedupe_key = format!(
        "uninstall:{}:{}:{}:{}",
        input.model,
        input.subagent_id,
        uninstall_scope,
        uninstall_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_subagents_state()?;
            return api_ok(true, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let mut state = load_local_subagents_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    if let Ok(record) = find_current_installed_subagent(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.subagent_id,
        &uninstall_scope,
        uninstall_project_root.as_deref(),
    ) {
        let local = locate_existing_record_local_dir(&record)?;
        let (root, compat_roots) = resolve_subagent_target_dir(
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
        if input.model == "codex" && uninstall_scope == INSTALL_SCOPE_PROJECT {
            if let Some(project_root) = uninstall_project_root.as_deref() {
                let _ = remove_codex_project_agent_entry(project_root, &dir_name);
            }
        }
    }
    let revision = if uninstall_scope == INSTALL_SCOPE_GLOBAL {
        state.subagents.retain(|s| {
            !(s.model == input.model
                && s.id == input.subagent_id
                && scope_project_match(s, &uninstall_scope, uninstall_project_root.as_deref()))
        });
        save_local_subagents_state(state)?.revision
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
pub fn subagents_detail_get(input: SubagentKeyInput) -> Result<ApiOk<SubagentDetail>, String> {
    let detail_scope = normalize_install_scope(input.scope.as_deref());
    let detail_project_root =
        normalize_project_root_for_scope(&detail_scope, input.project_root.as_deref())?;
    let state = load_local_subagents_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let record = find_current_installed_subagent(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.subagent_id,
        &detail_scope,
        detail_project_root.as_deref(),
    )?;
    let local = record_local_dir(&record)?;
    let markdown = fs::read_to_string(local.join("AGENT.md")).unwrap_or_default();
    let detail = SubagentDetail {
        subagent: record,
        markdown,
        local_path: local.to_string_lossy().to_string(),
    };
    api_ok(detail, state.revision)
}

#[tauri::command]
pub fn subagents_catalog_detail_get(
    input: CatalogSubagentKeyInput,
) -> Result<ApiOk<CatalogSubagentDetail>, String> {
    let cfg = config::get_storage_config()?;
    let source = get_source(&cfg, &input.source_id).ok_or("source not found")?;
    let sync_state = load_sync_state()?;
    let mut catalog = sync_state
        .catalog
        .iter()
        .find(|c| {
            c.source_id == input.source_id
                && (c.rel_path == input.subagent_ref || c.id == input.subagent_ref)
        })
        .cloned()
        .ok_or("catalog subagent not found")?;
    let effective_models = resolve_effective_models(&catalog.models, &source.default_models);
    if effective_models.is_empty() {
        return Err("catalog subagent not found".to_string());
    }
    catalog.models = effective_models;
    let source_path = source_subagent_abs_path(source, &catalog.rel_path)?;
    let markdown = read_markdown_from_source_entry(&source_path).unwrap_or_default();
    let detail = CatalogSubagentDetail {
        subagent: catalog,
        markdown,
        source_path: source_path.to_string_lossy().to_string(),
    };
    let shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let revision = combined_revision(&shared_state, &local_state);
    api_ok(detail, revision)
}

#[tauri::command]
pub fn subagents_repo_detail_get(
    input: RepoSubagentKeyInput,
) -> Result<ApiOk<CatalogSubagentDetail>, String> {
    let mut state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let cfg = config::get_storage_config()?;
    if ensure_repository_snapshots_materialized(&mut state, &local_state, &cfg)? {
        state = save_subagents_state(state)?;
    }
    let repo = state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned()
        .ok_or("repo subagent not found")?;

    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    let mut markdown = String::new();
    let mut source_path = repo_snapshot.to_string_lossy().to_string();

    if repo_snapshot.join("AGENT.md").exists() {
        markdown = fs::read_to_string(repo_snapshot.join("AGENT.md")).unwrap_or_default();
    } else if let Some(src) = repo.source_path.clone() {
        let src_path = PathBuf::from(&src);
        if source_entry_exists(&src_path) {
            markdown = read_markdown_from_source_entry(&src_path).unwrap_or_default();
            source_path = src;
        }
    } else if repo.source_type == "remote" {
        if let Ok(cfg) = config::get_storage_config() {
            if let Some(source) = get_source(&cfg, &repo.source_id) {
                if let Ok(remote_path) = source_subagent_abs_path(source, &repo.source_rel_path) {
                    if source_entry_exists(&remote_path) {
                        markdown =
                            read_markdown_from_source_entry(&remote_path).unwrap_or_default();
                        source_path = remote_path.to_string_lossy().to_string();
                    }
                }
            }
        }
    }

    let detail = CatalogSubagentDetail {
        subagent: CatalogSubagent {
            source_id: repo.source_id.clone(),
            id: repo.subagent_id.clone(),
            rel_path: repo.source_rel_path.clone(),
            dir_name: normalized_repo_dir_name(&repo),
            name: repo.name.clone(),
            description: repo.description.clone(),
            models: repo.models.clone(),
            model: repo.model.clone(),
            tools: repo.tools.clone(),
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
pub fn subagents_repo_reload_preview(
    input: RepoSubagentKeyInput,
) -> Result<ApiOk<ReloadPreview>, String> {
    let mut shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let cfg = config::get_storage_config()?;
    if ensure_repository_snapshots_materialized(&mut shared_state, &local_state, &cfg)? {
        shared_state = save_subagents_state(shared_state)?;
    }
    let repo = shared_state
        .repositories
        .iter()
        .find(|r| r.repo_key == input.repo_key)
        .cloned()
        .ok_or("repo subagent not found")?;

    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    if !repo_snapshot.exists() {
        return Err("repository_snapshot_missing".to_string());
    }

    let baseline = repo_index_baseline_dir(&repo.repo_key)?;
    let before_exists = baseline.exists();
    let (after_dir, after_label) = resolve_repo_reload_after_dir(
        &repo,
        if before_exists {
            Some(baseline.as_path())
        } else {
            None
        },
        &repo_snapshot,
    )?;
    let (changed_files, text_diffs) = compare_snapshot_dirs(
        if before_exists {
            Some(baseline.as_path())
        } else {
            None
        },
        &after_dir,
    )?;
    let installed_models = installed_models_for_repo(&local_state, &repo);

    let preview = ReloadPreview {
        before_label: "Before Reload (Indexed Baseline)".to_string(),
        after_label,
        changed_files: changed_files.clone(),
        text_diffs,
        installed_models,
        has_changes: !changed_files.is_empty(),
    };
    api_ok(preview, combined_revision(&shared_state, &local_state))
}
