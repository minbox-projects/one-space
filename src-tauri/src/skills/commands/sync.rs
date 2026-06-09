use crate::config::{self};
use crate::skills::{
    acquire_job_key, api_ok, assign_catalog_first_seen, combined_revision, git_run,
    hydrate_local_records_from_catalog, job_lock, load_local_skills_state, load_skills_state,
    load_sync_state, now_ts, rebuild_local_installed_from_models,
    refresh_remote_repositories_from_catalog, refresh_repository_metadata_from_snapshots,
    save_local_skills_state, save_skills_state, save_sync_state, scan_source_catalog,
    sync_source_repo, touch_sync_timestamp, trigger_storage_sync, update_record_remote_flags,
    ApiOk, CatalogSkill, SkillsSyncState, SourceSyncState,
};

pub(in crate::skills) fn skills_sync_now_blocking(
    app: tauri::AppHandle,
) -> Result<ApiOk<SkillsSyncState>, String> {
    let _job = match acquire_job_key("sync:all")? {
        Some(v) => v,
        None => {
            let sync_state = load_sync_state()?;
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            let revision = combined_revision(&shared_state, &local_state);
            return api_ok(sync_state, revision);
        }
    };

    let (cfg, previous_sync_state) = {
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;
        let cfg = config::get_storage_config()?;
        let previous_sync_state = load_sync_state()?;
        let mut in_progress_sync_state = previous_sync_state.clone();
        in_progress_sync_state.status = "fetching_source".to_string();
        in_progress_sync_state.last_error = None;
        // Persist early so frontend can immediately detect in-progress sync.
        save_sync_state(&in_progress_sync_state)?;
        (cfg, previous_sync_state)
    };

    let mut next_catalog = vec![];
    let mut next_sources = vec![];
    let mut sync_last_error = None;
    for source in &cfg.skills_sources {
        let _source_job = match acquire_job_key(format!("sync:{}", source.id))? {
            Some(v) => Some(v),
            None => {
                let prev = previous_sync_state
                    .sources
                    .iter()
                    .find(|s| s.source_id == source.id)
                    .cloned()
                    .unwrap_or_default();
                next_sources.push(SourceSyncState {
                    source_id: source.id.clone(),
                    last_synced_at: prev.last_synced_at,
                    last_commit_sha: prev.last_commit_sha,
                    last_status: "skipped_busy".to_string(),
                    last_error: None,
                });
                None
            }
        };
        if _source_job.is_none() {
            continue;
        }

        if !source.enabled {
            let prev = previous_sync_state
                .sources
                .iter()
                .find(|s| s.source_id == source.id)
                .cloned()
                .unwrap_or_default();
            next_sources.push(SourceSyncState {
                source_id: source.id.clone(),
                last_synced_at: prev.last_synced_at,
                last_commit_sha: prev.last_commit_sha,
                last_status: "skipped".to_string(),
                last_error: None,
            });
            continue;
        }

        let mut retry = 0;
        let mut ok = false;
        let mut last_err = None;
        let mut commit = None;
        let mut indexed: Vec<CatalogSkill> = vec![];
        while retry < 5 {
            let sync_one = || -> Result<(String, Vec<CatalogSkill>), String> {
                let repo_dir = sync_source_repo(source)?;
                let current_commit = git_run(Some(&repo_dir), &["rev-parse", "HEAD"])?;
                let prev_commit = previous_sync_state
                    .sources
                    .iter()
                    .find(|s| s.source_id == source.id)
                    .and_then(|s| s.last_commit_sha.clone());

                if prev_commit.as_deref() == Some(current_commit.as_str()) {
                    let reused = previous_sync_state
                        .catalog
                        .iter()
                        .filter(|c| c.source_id == source.id)
                        .cloned()
                        .collect::<Vec<_>>();
                    Ok((current_commit, reused))
                } else {
                    let previous_source_catalog = previous_sync_state
                        .catalog
                        .iter()
                        .filter(|c| c.source_id == source.id)
                        .cloned()
                        .collect::<Vec<_>>();
                    let scanned = scan_source_catalog(&repo_dir, source)?;
                    let scanned = assign_catalog_first_seen(&previous_source_catalog, scanned);
                    Ok((current_commit, scanned))
                }
            };

            match sync_one() {
                Ok((c, list)) => {
                    commit = Some(c.clone());
                    indexed = list;
                    ok = true;
                    break;
                }
                Err(err) => {
                    last_err = Some(err);
                    retry += 1;
                    std::thread::sleep(std::time::Duration::from_secs(2u64.pow(retry)));
                }
            }
        }

        if ok {
            let status = if previous_sync_state
                .sources
                .iter()
                .find(|s| s.source_id == source.id)
                .and_then(|s| s.last_commit_sha.clone())
                .as_deref()
                == commit.as_deref()
            {
                "done_no_change"
            } else {
                "done"
            };
            next_catalog.extend(indexed);
            next_sources.push(SourceSyncState {
                source_id: source.id.clone(),
                last_synced_at: Some(now_ts()),
                last_commit_sha: commit,
                last_status: status.to_string(),
                last_error: None,
            });
        } else {
            next_sources.push(SourceSyncState {
                source_id: source.id.clone(),
                last_synced_at: Some(now_ts()),
                last_commit_sha: None,
                last_status: "error".to_string(),
                last_error: last_err.clone(),
            });
            sync_last_error = last_err;
        }
    }

    let (sync_state, revision, cfg_save) = {
        let _guard = job_lock().lock().map_err(|e| e.to_string())?;
        let mut sync_state = load_sync_state()?;
        sync_state.status = if sync_last_error.is_some() {
            "error".to_string()
        } else {
            "done".to_string()
        };
        sync_state.last_error = sync_last_error;
        sync_state.last_sync_at = Some(now_ts());
        sync_state.catalog = next_catalog;
        sync_state.sources = next_sources;
        save_sync_state(&sync_state)?;

        let mut shared_state = load_skills_state()?;
        let mut local_state = load_local_skills_state()?;
        rebuild_local_installed_from_models(&mut local_state)?;
        hydrate_local_records_from_catalog(&mut local_state, &sync_state);
        update_record_remote_flags(&mut local_state, &sync_state);
        refresh_remote_repositories_from_catalog(
            &mut shared_state,
            &local_state,
            &sync_state,
            &cfg,
        )?;
        let _ = refresh_repository_metadata_from_snapshots(&mut shared_state, &cfg);
        shared_state = save_skills_state(shared_state)?;
        local_state = save_local_skills_state(local_state)?;

        let mut cfg_save = config::get_storage_config()?;
        touch_sync_timestamp(&mut cfg_save);

        (
            sync_state,
            combined_revision(&shared_state, &local_state),
            cfg_save,
        )
    };

    tauri::async_runtime::block_on(config::save_storage_config(app.clone(), cfg_save))?;
    trigger_storage_sync(app, "skills_sync_now");

    api_ok(sync_state, revision)
}
