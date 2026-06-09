use super::{
    ensure_within, get_source, has_dir_name_conflict, hash_dir, locate_existing_record_local_dir,
    model_dir, normalized_record_dir_name, now_ts, parse_required_subagent_dir_name,
    parse_subagent_frontmatter_meta, parse_subagent_md, record_scope, record_target_root,
    replace_dir_atomic, replace_source_entry_atomic, repo_storage_dir,
    snapshot_repository_index_baseline, source_entry_exists, source_subagent_abs_path,
    subagent_matches_repository, upsert_repo_dir_name, RepositoryRecord, SubagentRecord,
    SubagentsLocalState, SubagentsState, INSTALL_SCOPE_GLOBAL, MODELS,
};
use crate::config::StorageConfig;
use std::fs;
use std::path::{Path, PathBuf};

pub(in crate::subagents) fn resolve_repo_reload_after_dir(
    repo: &RepositoryRecord,
    baseline: Option<&Path>,
    repo_snapshot: &Path,
) -> Result<(PathBuf, String), String> {
    let _ = repo;
    let _ = baseline;
    Ok((
        repo_snapshot.to_path_buf(),
        "After Reload (Current Snapshot)".to_string(),
    ))
}

pub(in crate::subagents) fn installed_models_for_repo(
    local_state: &SubagentsLocalState,
    repo: &RepositoryRecord,
) -> Vec<String> {
    let mut out = vec![];
    for model in MODELS {
        let installed = local_state
            .subagents
            .iter()
            .any(|s| s.model == model && subagent_matches_repository(s, repo));
        if installed {
            out.push(model.to_string());
        }
    }
    out
}

pub(in crate::subagents) fn refresh_repository_record_from_snapshot(
    repo: &mut RepositoryRecord,
) -> Result<(), String> {
    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    let markdown = fs::read_to_string(repo_snapshot.join("AGENT.md")).unwrap_or_default();
    let (name, description, models) = parse_subagent_md(&markdown, &[]);
    let (model, tools) = parse_subagent_frontmatter_meta(&markdown);
    let dir_name = parse_required_subagent_dir_name(&markdown)?;
    repo.name = name;
    repo.description = description;
    repo.models = models;
    repo.model = model;
    repo.tools = tools;
    repo.dir_name = dir_name;
    repo.hash = Some(hash_dir(&repo_snapshot)?);
    repo.updated_at = Some(now_ts());
    Ok(())
}

pub(in crate::subagents) fn materialize_repository_snapshot_if_missing(
    repo: &RepositoryRecord,
    local_state: &SubagentsLocalState,
    cfg: &StorageConfig,
) -> Result<bool, String> {
    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    if repo_snapshot.exists() {
        return Ok(false);
    }

    if let Some(src) = repo.source_path.as_ref() {
        let src_path = PathBuf::from(src);
        if source_entry_exists(&src_path) {
            replace_source_entry_atomic(&src_path, &repo_snapshot)?;
            snapshot_repository_index_baseline(&repo.repo_key, &repo_snapshot)?;
            return Ok(true);
        }
    }

    if let Some(local_record) = local_state
        .subagents
        .iter()
        .find(|s| subagent_matches_repository(s, repo))
    {
        let local_dir = record_local_dir(local_record)?;
        if local_dir.join("AGENT.md").exists() {
            replace_dir_atomic(&local_dir, &repo_snapshot)?;
            snapshot_repository_index_baseline(&repo.repo_key, &repo_snapshot)?;
            return Ok(true);
        }
    }

    if repo.source_type == "remote" {
        if let Some(source) = get_source(cfg, &repo.source_id) {
            if let Ok(source_path) = source_subagent_abs_path(source, &repo.source_rel_path) {
                if source_entry_exists(&source_path) {
                    replace_source_entry_atomic(&source_path, &repo_snapshot)?;
                    snapshot_repository_index_baseline(&repo.repo_key, &repo_snapshot)?;
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

pub(in crate::subagents) fn ensure_repository_snapshots_materialized(
    state: &mut SubagentsState,
    local_state: &SubagentsLocalState,
    cfg: &StorageConfig,
) -> Result<bool, String> {
    let mut changed = false;
    for repo in &mut state.repositories {
        if materialize_repository_snapshot_if_missing(repo, local_state, cfg)? {
            refresh_repository_record_from_snapshot(repo)?;
            changed = true;
        }
    }
    Ok(changed)
}

pub(in crate::subagents) fn record_local_dir(record: &SubagentRecord) -> Result<PathBuf, String> {
    Ok(record_target_root(record)?.join(normalized_record_dir_name(record)))
}

pub(in crate::subagents) fn migrate_installed_dir_names(
    shared_state: &mut SubagentsState,
    local_state: &mut SubagentsLocalState,
) -> Result<(bool, bool), String> {
    let mut shared_changed = false;
    let mut local_changed = false;

    for model in MODELS {
        let model_root = model_dir(model)?;
        let indices = local_state
            .subagents
            .iter()
            .enumerate()
            .filter_map(|(idx, s)| {
                if s.model == model && record_scope(s) == INSTALL_SCOPE_GLOBAL {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for idx in indices {
            let record = local_state.subagents[idx].clone();
            let current_dir = locate_existing_record_local_dir(&record)?;
            if !current_dir.exists() {
                continue;
            }
            let md = current_dir.join("AGENT.md");
            if !md.exists() {
                continue;
            }
            let md_raw = fs::read_to_string(&md).unwrap_or_default();
            let desired_dir_name = match parse_required_subagent_dir_name(&md_raw) {
                Ok(name) => name,
                Err(_) => continue,
            };

            if has_dir_name_conflict(
                local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                &desired_dir_name,
                Some(record.id.as_str()),
            ) {
                continue;
            }

            let target_dir = model_root.join(&desired_dir_name);
            ensure_within(&model_root, &target_dir)?;

            if current_dir != target_dir {
                if target_dir.exists() {
                    continue;
                }
                match fs::rename(&current_dir, &target_dir) {
                    Ok(_) => {}
                    Err(_) => {
                        replace_dir_atomic(&current_dir, &target_dir)?;
                        if current_dir.exists() {
                            fs::remove_dir_all(&current_dir).map_err(|e| e.to_string())?;
                        }
                    }
                }
                local_changed = true;
            }

            let new_hash = hash_dir(&target_dir)?;
            if local_state.subagents[idx].dir_name != desired_dir_name {
                local_state.subagents[idx].dir_name = desired_dir_name.clone();
                local_changed = true;
            }
            if local_state.subagents[idx].local_hash != new_hash {
                local_state.subagents[idx].local_hash = new_hash;
                local_changed = true;
            }

            if upsert_repo_dir_name(
                shared_state,
                &local_state.subagents[idx].source_id,
                &local_state.subagents[idx].source_rel_path,
                &local_state.subagents[idx].id,
                &desired_dir_name,
            ) {
                shared_changed = true;
            }
        }
    }

    Ok((shared_changed, local_changed))
}
