use super::{
    get_source, hash_dir, make_repo_key, metadata_timestamp, normalized_record_dir_name, now_ts,
    parse_skill_md, project_scan_root, read_required_skill_dir_name, record_local_dir,
    repo_storage_dir, scope_project_match, skill_has_markdown_update, source_skill_abs_path,
    upsert_repository_from_dir, upsert_repository_record, CatalogSkill, RepositoryRecord,
    SkillRecord, SkillsLocalState, SkillsState, SkillsSyncState, INSTALL_SCOPE_PROJECT, MODELS,
};
use crate::config::StorageConfig;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub(in crate::skills) fn touch_sync_timestamp(cfg: &mut StorageConfig) {
    cfg.skills_last_synced_at = Some(now_ts() as i64);
}

pub(in crate::skills) fn trigger_storage_sync(app: tauri::AppHandle, reason: &str) {
    let reason = reason.to_string();
    tauri::async_runtime::spawn(async move {
        let _ = crate::app_store::sync_enqueue(app, reason).await;
    });
}

pub(in crate::skills) fn update_record_remote_flags(
    state: &mut SkillsLocalState,
    sync_state: &SkillsSyncState,
) {
    refresh_skill_records_remote_flags(&mut state.skills, sync_state, None);
}

pub(in crate::skills) fn refresh_local_hashes(
    state: &mut SkillsLocalState,
    model_filter: Option<&str>,
) -> Result<bool, String> {
    let mut changed = false;
    for skill in &mut state.skills {
        if let Some(model) = model_filter {
            if skill.model != model {
                continue;
            }
        }
        let local_dir = record_local_dir(skill)?;
        let local_hash = hash_dir(&local_dir)?;
        if skill.local_hash != local_hash {
            skill.local_hash = local_hash;
            changed = true;
        }
        if skill.has_update {
            skill.has_update = false;
            changed = true;
        }
    }
    Ok(changed)
}

pub(in crate::skills) fn hydrate_skill_records_from_catalog(
    records: &mut [SkillRecord],
    sync_state: &SkillsSyncState,
) {
    let mut catalog_by_hash: HashMap<String, Vec<&CatalogSkill>> = HashMap::new();
    let mut catalog_by_dir_name: HashMap<String, Vec<&CatalogSkill>> = HashMap::new();
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

    for skill in records {
        if skill.source_id != "local" {
            continue;
        }

        let match_by_hash =
            catalog_by_hash
                .get(&skill.local_hash)
                .and_then(|items| match items.as_slice() {
                    [item] => Some(*item),
                    _ => None,
                });
        let matched = match_by_hash.or_else(|| {
            let dir_name = normalized_record_dir_name(skill);
            let candidates = catalog_by_dir_name.get(&dir_name)?;
            let matches = candidates
                .iter()
                .copied()
                .filter(|item| {
                    skill.models.is_empty()
                        || item.models.is_empty()
                        || item.models.iter().any(|model| skill.models.contains(model))
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

        skill.id = item.id.clone();
        skill.source_id = item.source_id.clone();
        skill.source_rel_path = item.rel_path.clone();
        skill.remote_hash = Some(item.remote_hash.clone());
        skill.has_update = false;
        skill.last_synced_at = Some(now_ts());
        skill.icon_seed = item.icon_seed.clone();
    }
}

pub(in crate::skills) fn refresh_skill_records_remote_flags(
    records: &mut [SkillRecord],
    sync_state: &SkillsSyncState,
    cfg: Option<&StorageConfig>,
) {
    let mut map = HashMap::new();
    for c in &sync_state.catalog {
        map.insert(
            (c.source_id.clone(), c.rel_path.clone()),
            c.remote_hash.clone(),
        );
    }
    for skill in records {
        if let Some(remote_hash) =
            map.get(&(skill.source_id.clone(), skill.source_rel_path.clone()))
        {
            skill.remote_hash = Some(remote_hash.clone());
            skill.has_update = cfg
                .and_then(|config| skill_has_markdown_update(skill, config))
                .unwrap_or(false);
            skill.last_synced_at = Some(now_ts());
        } else {
            skill.remote_hash = None;
            skill.has_update = false;
        }
    }
}

pub(in crate::skills) fn scan_project_installed_skills_for_model(
    model: &str,
    project_root: &str,
    sync_state: &SkillsSyncState,
    cfg: &StorageConfig,
) -> Result<Vec<SkillRecord>, String> {
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
            let markdown = path.join("SKILL.md");
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
        let (name, description, models) = parse_skill_md(&content, &[]);
        records.push(SkillRecord {
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

    hydrate_skill_records_from_catalog(&mut records, sync_state);
    refresh_skill_records_remote_flags(&mut records, sync_state, Some(cfg));
    Ok(records)
}

pub(in crate::skills) fn current_installed_skills(
    local_state: &SkillsLocalState,
    sync_state: &SkillsSyncState,
    cfg: &StorageConfig,
    model: Option<&str>,
    scope: &str,
    project_root: Option<&str>,
) -> Result<Vec<SkillRecord>, String> {
    if scope == INSTALL_SCOPE_PROJECT {
        let root = project_root.ok_or("skills/project_root_required")?;
        let mut out = Vec::new();
        match model {
            Some(value) => out.extend(scan_project_installed_skills_for_model(
                value, root, sync_state, cfg,
            )?),
            None => {
                for value in MODELS {
                    out.extend(scan_project_installed_skills_for_model(
                        value, root, sync_state, cfg,
                    )?);
                }
            }
        }
        return Ok(out);
    }

    let mut list = local_state
        .skills
        .iter()
        .filter(|skill| model.map(|value| skill.model == value).unwrap_or(true))
        .filter(|skill| scope_project_match(skill, scope, project_root))
        .cloned()
        .collect::<Vec<_>>();
    refresh_skill_records_remote_flags(&mut list, sync_state, Some(cfg));
    Ok(list)
}

pub(in crate::skills) fn find_current_installed_skill(
    local_state: &SkillsLocalState,
    sync_state: &SkillsSyncState,
    cfg: &StorageConfig,
    model: &str,
    skill_id: &str,
    scope: &str,
    project_root: Option<&str>,
) -> Result<SkillRecord, String> {
    current_installed_skills(
        local_state,
        sync_state,
        cfg,
        Some(model),
        scope,
        project_root,
    )?
    .into_iter()
    .find(|skill| skill.id == skill_id)
    .ok_or("skill not found".to_string())
}

pub(in crate::skills) fn hydrate_local_records_from_catalog(
    state: &mut SkillsLocalState,
    sync_state: &SkillsSyncState,
) {
    hydrate_skill_records_from_catalog(&mut state.skills, sync_state);
}

pub(in crate::skills) fn refresh_remote_repositories_from_catalog(
    state: &mut SkillsState,
    local_state: &SkillsLocalState,
    sync_state: &SkillsSyncState,
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
        .skills
        .iter()
        .map(|s| make_repo_key(&s.source_id, &s.source_rel_path))
        .collect::<HashSet<_>>();
    let installed_skill_ids = local_state
        .skills
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
            || installed_skill_ids.contains(&item.id);
        if !should_track {
            continue;
        }
        tracked_remote_keys.insert(repo_key.clone());

        let source = get_source(cfg, &item.source_id);
        if let Some(src_cfg) = source {
            if let Ok(source_path) = source_skill_abs_path(src_cfg, &item.rel_path) {
                if source_path.join("SKILL.md").exists() {
                    let dir_name = read_required_skill_dir_name(&source_path)
                        .unwrap_or_else(|_| item.id.clone());
                    let repo_snapshot = repo_storage_dir(&repo_key)?;
                    if !repo_snapshot.exists() {
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
                            &item.icon_seed,
                            Some(source_path.to_string_lossy().to_string()),
                            Some(item.remote_hash.clone()),
                            ever_installed,
                        );
                        continue;
                    }

                    let existing = state
                        .repositories
                        .iter()
                        .find(|r| r.repo_key == repo_key)
                        .cloned();
                    let snapshot_hash = hash_dir(&repo_snapshot).ok();
                    let created_at = existing
                        .as_ref()
                        .map(|r| r.created_at)
                        .filter(|value| *value > 0)
                        .unwrap_or_else(now_ts);
                    let updated_at = existing
                        .as_ref()
                        .and_then(|r| r.updated_at)
                        .or(Some(created_at));
                    upsert_repository_record(
                        &mut state.repositories,
                        RepositoryRecord {
                            repo_key: repo_key.clone(),
                            skill_id: item.id.clone(),
                            dir_name,
                            source_id: item.source_id.clone(),
                            source_rel_path: item.rel_path.clone(),
                            source_type: "remote".to_string(),
                            source_path: Some(source_path.to_string_lossy().to_string()),
                            name: item.name.clone(),
                            description: item.description.clone(),
                            models: item.models.clone(),
                            icon_seed: item.icon_seed.clone(),
                            hash: snapshot_hash
                                .or_else(|| existing.as_ref().and_then(|r| r.hash.clone())),
                            created_at,
                            updated_at,
                            ever_installed,
                        },
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
                skill_id: item.id.clone(),
                dir_name: item.id.clone(),
                source_id: item.source_id.clone(),
                source_rel_path: item.rel_path.clone(),
                source_type: "remote".to_string(),
                source_path: None,
                name: item.name.clone(),
                description: item.description.clone(),
                models: item.models.clone(),
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
