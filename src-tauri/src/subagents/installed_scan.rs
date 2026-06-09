use super::{
    get_source, hash_dir, make_repo_key, metadata_timestamp, normalized_record_dir_name, now_ts,
    parse_subagent_md, project_scan_root, read_required_subagent_dir_name_from_entry,
    record_local_dir, scope_project_match, source_entry_exists, source_subagent_abs_path,
    subagent_has_markdown_update, upsert_repository_from_dir, upsert_repository_record,
    CatalogSubagent, RepositoryRecord, SubagentRecord, SubagentsLocalState, SubagentsState,
    SubagentsSyncState, INSTALL_SCOPE_PROJECT, MODELS,
};
use crate::config::StorageConfig;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub(in crate::subagents) fn touch_sync_timestamp(cfg: &mut StorageConfig) {
    cfg.subagents_last_synced_at = Some(now_ts() as i64);
}

pub(in crate::subagents) fn trigger_storage_sync(app: tauri::AppHandle, reason: &str) {
    let reason = reason.to_string();
    tauri::async_runtime::spawn(async move {
        let _ = crate::app_store::sync_enqueue(app, reason).await;
    });
}

pub(in crate::subagents) fn update_record_remote_flags(
    state: &mut SubagentsLocalState,
    sync_state: &SubagentsSyncState,
    cfg: &StorageConfig,
) {
    refresh_subagent_records_remote_flags(&mut state.subagents, sync_state, Some(cfg));
}

pub(in crate::subagents) fn refresh_local_hashes(
    state: &mut SubagentsLocalState,
    model_filter: Option<&str>,
    cfg: &StorageConfig,
) -> Result<bool, String> {
    let mut changed = false;
    for subagent in &mut state.subagents {
        if let Some(model) = model_filter {
            if subagent.model != model {
                continue;
            }
        }
        let local_dir = record_local_dir(subagent)?;
        let local_hash = hash_dir(&local_dir)?;
        if subagent.local_hash != local_hash {
            subagent.local_hash = local_hash;
            changed = true;
        }
        let has_update = subagent_has_markdown_update(subagent, cfg).unwrap_or(false);
        if subagent.has_update != has_update {
            subagent.has_update = has_update;
            changed = true;
        }
    }
    Ok(changed)
}

pub(in crate::subagents) fn hydrate_subagent_records_from_catalog(
    records: &mut [SubagentRecord],
    sync_state: &SubagentsSyncState,
) {
    let mut catalog_by_hash: HashMap<String, Vec<&CatalogSubagent>> = HashMap::new();
    let mut catalog_by_dir_name: HashMap<String, Vec<&CatalogSubagent>> = HashMap::new();
    for item in &sync_state.catalog {
        catalog_by_hash
            .entry(item.remote_hash.clone())
            .or_default()
            .push(item);
        if !item.dir_name.trim().is_empty() {
            catalog_by_dir_name
                .entry(item.dir_name.clone())
                .or_default()
                .push(item);
        }
    }

    for subagent in records {
        if subagent.source_id != "local" {
            continue;
        }

        let match_by_hash = catalog_by_hash
            .get(&subagent.local_hash)
            .and_then(|items| match items.as_slice() {
                [item] => Some(*item),
                _ => None,
            });
        let matched = match_by_hash.or_else(|| {
            let dir_name = normalized_record_dir_name(subagent);
            let candidates = catalog_by_dir_name.get(&dir_name)?;
            let matches = candidates
                .iter()
                .copied()
                .filter(|item| {
                    subagent.models.is_empty()
                        || item.models.is_empty()
                        || item
                            .models
                            .iter()
                            .any(|model| subagent.models.contains(model))
                })
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [item] => Some(*item),
                _ => None,
            }
        });
        let Some(item) = matched else {
            continue;
        };

        subagent.id = item.id.clone();
        subagent.source_id = item.source_id.clone();
        subagent.source_rel_path = item.rel_path.clone();
        subagent.remote_hash = Some(item.remote_hash.clone());
        subagent.has_update = false;
        subagent.last_synced_at = Some(now_ts());
        subagent.icon_seed = item.icon_seed.clone();
    }
}

pub(in crate::subagents) fn refresh_subagent_records_remote_flags(
    records: &mut [SubagentRecord],
    sync_state: &SubagentsSyncState,
    cfg: Option<&StorageConfig>,
) {
    let mut map = HashMap::new();
    for item in &sync_state.catalog {
        map.insert(
            (item.source_id.clone(), item.rel_path.clone()),
            item.remote_hash.clone(),
        );
    }
    for subagent in records {
        if let Some(remote_hash) =
            map.get(&(subagent.source_id.clone(), subagent.source_rel_path.clone()))
        {
            subagent.remote_hash = Some(remote_hash.clone());
            subagent.has_update = cfg
                .and_then(|config| subagent_has_markdown_update(subagent, config))
                .unwrap_or(false);
            subagent.last_synced_at = Some(now_ts());
        } else {
            subagent.remote_hash = None;
            subagent.has_update = false;
        }
    }
}

pub(in crate::subagents) fn scan_project_installed_subagents_for_model(
    model: &str,
    project_root: &str,
    sync_state: &SubagentsSyncState,
    cfg: &StorageConfig,
) -> Result<Vec<SubagentRecord>, String> {
    let root = project_scan_root(model, &PathBuf::from(project_root))?;
    if !root.exists() {
        return Ok(vec![]);
    }

    let mut entries = fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let markdown = path.join("AGENT.md");
            if !markdown.exists() {
                return None;
            }
            Some((
                entry.file_name().to_string_lossy().to_string(),
                path,
                markdown,
            ))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut records = Vec::with_capacity(entries.len());
    for (dir_name, path, markdown) in entries {
        let content = fs::read_to_string(&markdown).map_err(|e| e.to_string())?;
        let (name, description, models) = parse_subagent_md(&content, &[]);
        records.push(SubagentRecord {
            id: dir_name.clone(),
            dir_name: dir_name.clone(),
            model: model.to_string(),
            models,
            name,
            description,
            source_id: "local".to_string(),
            source_rel_path: dir_name.clone(),
            installed_at: metadata_timestamp(&path),
            updated_at: Some(metadata_timestamp(&markdown)),
            last_synced_at: None,
            local_hash: hash_dir(&path)?,
            remote_hash: None,
            has_update: false,
            icon_seed: dir_name.clone(),
            scope: INSTALL_SCOPE_PROJECT.to_string(),
            project_root: Some(project_root.to_string()),
            target_path: Some(path.to_string_lossy().to_string()),
        });
    }

    hydrate_subagent_records_from_catalog(&mut records, sync_state);
    refresh_subagent_records_remote_flags(&mut records, sync_state, Some(cfg));
    Ok(records)
}

pub(in crate::subagents) fn current_installed_subagents(
    local_state: &SubagentsLocalState,
    sync_state: &SubagentsSyncState,
    cfg: &StorageConfig,
    model: Option<&str>,
    scope: &str,
    project_root: Option<&str>,
) -> Result<Vec<SubagentRecord>, String> {
    if scope == INSTALL_SCOPE_PROJECT {
        let root = project_root.ok_or("subagents/project_root_required")?;
        let mut out = Vec::new();
        match model {
            Some(value) => out.extend(scan_project_installed_subagents_for_model(
                value, root, sync_state, cfg,
            )?),
            None => {
                for value in MODELS {
                    out.extend(scan_project_installed_subagents_for_model(
                        value, root, sync_state, cfg,
                    )?);
                }
            }
        }
        return Ok(out);
    }

    let mut list = local_state
        .subagents
        .iter()
        .filter(|subagent| model.map(|value| subagent.model == value).unwrap_or(true))
        .filter(|subagent| scope_project_match(subagent, scope, project_root))
        .cloned()
        .collect::<Vec<_>>();
    refresh_subagent_records_remote_flags(&mut list, sync_state, Some(cfg));
    Ok(list)
}

pub(in crate::subagents) fn find_current_installed_subagent(
    local_state: &SubagentsLocalState,
    sync_state: &SubagentsSyncState,
    cfg: &StorageConfig,
    model: &str,
    subagent_id: &str,
    scope: &str,
    project_root: Option<&str>,
) -> Result<SubagentRecord, String> {
    current_installed_subagents(
        local_state,
        sync_state,
        cfg,
        Some(model),
        scope,
        project_root,
    )?
    .into_iter()
    .find(|subagent| subagent.id == subagent_id)
    .ok_or("subagent not found".to_string())
}

pub(in crate::subagents) fn hydrate_local_records_from_catalog(
    state: &mut SubagentsLocalState,
    sync_state: &SubagentsSyncState,
) {
    hydrate_subagent_records_from_catalog(&mut state.subagents, sync_state);
}

pub(in crate::subagents) fn refresh_remote_repositories_from_catalog(
    state: &mut SubagentsState,
    local_state: &SubagentsLocalState,
    sync_state: &SubagentsSyncState,
    cfg: &StorageConfig,
) -> Result<(), String> {
    let existing_remote_usage = state
        .repositories
        .iter()
        .filter(|r| r.source_type == "remote")
        .map(|r| (r.repo_key.clone(), r.ever_installed))
        .collect::<HashMap<_, _>>();
    let mut tracked_remote_keys = HashSet::new();
    let installed_keys = local_state
        .subagents
        .iter()
        .map(|s| make_repo_key(&s.source_id, &s.source_rel_path))
        .collect::<HashSet<_>>();
    let installed_subagent_ids = local_state
        .subagents
        .iter()
        .map(|s| s.id.clone())
        .collect::<HashSet<_>>();

    for item in &sync_state.catalog {
        let repo_key = make_repo_key(&item.source_id, &item.rel_path);
        let ever_installed = existing_remote_usage
            .get(&repo_key)
            .copied()
            .unwrap_or(false);
        let should_track = ever_installed
            || installed_keys.contains(&repo_key)
            || installed_subagent_ids.contains(&item.id);
        if !should_track {
            continue;
        }
        tracked_remote_keys.insert(repo_key.clone());

        let source = get_source(cfg, &item.source_id);
        if let Some(src_cfg) = source {
            if let Ok(source_path) = source_subagent_abs_path(src_cfg, &item.rel_path) {
                if source_entry_exists(&source_path) {
                    let dir_name = read_required_subagent_dir_name_from_entry(&source_path)
                        .unwrap_or_else(|_| item.id.clone());
                    let _ = upsert_repository_from_dir(
                        state,
                        &source_path,
                        &item.source_id,
                        &item.rel_path,
                        &item.id,
                        &dir_name,
                        "remote",
                        &item.name,
                        &item.description,
                        &item.models,
                        item.model.as_deref(),
                        &item.tools,
                        &item.icon_seed,
                        Some(source_path.to_string_lossy().to_string()),
                        Some(item.remote_hash.clone()),
                        ever_installed,
                    );
                    continue;
                }
            }
        }

        let existing_created_at = state
            .repositories
            .iter()
            .find(|r| r.repo_key == repo_key)
            .map(|r| r.created_at)
            .unwrap_or(0);
        upsert_repository_record(
            &mut state.repositories,
            RepositoryRecord {
                repo_key: repo_key.clone(),
                subagent_id: item.id.clone(),
                dir_name: item.id.clone(),
                source_id: item.source_id.clone(),
                source_rel_path: item.rel_path.clone(),
                source_type: "remote".to_string(),
                source_path: None,
                name: item.name.clone(),
                description: item.description.clone(),
                models: item.models.clone(),
                model: item.model.clone(),
                tools: item.tools.clone(),
                icon_seed: item.icon_seed.clone(),
                hash: Some(item.remote_hash.clone()),
                created_at: if existing_created_at > 0 {
                    existing_created_at
                } else {
                    now_ts()
                },
                updated_at: Some(now_ts()),
                ever_installed,
            },
        );
    }

    state.repositories.retain(|repo| {
        if repo.source_type != "remote" {
            return true;
        }
        tracked_remote_keys.contains(&repo.repo_key)
            || installed_keys.contains(&repo.repo_key)
            || repo.ever_installed
    });

    Ok(())
}
