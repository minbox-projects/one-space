use crate::config::{self};
use crate::skills::{
    acquire_job_key, api_ok, apply_repository_update_from_dir, combined_revision,
    ensure_repository_snapshots_materialized, job_lock, load_local_skills_state, load_skills_state,
    now_ts, reconcile_internal, repo_storage_dir, repository_has_remote_source_update,
    repository_source_dir, save_local_skills_state, save_skills_state, trigger_storage_sync, ApiOk,
    RepoAutoUpdateResult,
};

#[tauri::command]
pub async fn skills_repo_auto_update_pending(
    app: tauri::AppHandle,
) -> Result<ApiOk<RepoAutoUpdateResult>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dedupe_key = "repo_auto_update_pending";
        let _job = match acquire_job_key(dedupe_key)? {
            Some(v) => v,
            None => {
                let shared_state = load_skills_state()?;
                let local_state = load_local_skills_state()?;
                let result = RepoAutoUpdateResult {
                    applied_at: now_ts(),
                    ..RepoAutoUpdateResult::default()
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

        let remote_repos = shared_state
            .repositories
            .iter()
            .filter(|repo| repo.source_type == "remote")
            .cloned()
            .collect::<Vec<_>>();

        let mut updated_repo_keys = vec![];
        let mut updated_skill_names = vec![];
        let mut synced_targets = vec![];

        for repo in remote_repos {
            if !repository_has_remote_source_update(&repo, &cfg) {
                continue;
            }
            let source_dir = match repository_source_dir(&repo, &cfg) {
                Ok(path) => path,
                Err(_) => continue,
            };
            let compare_before_dir = repo_storage_dir(&repo.repo_key)?;
            let result = apply_repository_update_from_dir(
                &mut shared_state,
                &mut local_state,
                &repo.repo_key,
                Some(compare_before_dir.as_path()),
                &source_dir,
                true,
            )?;
            if result.updated_files_count == 0 {
                continue;
            }
            updated_repo_keys.push(repo.repo_key.clone());
            let latest_name = shared_state
                .repositories
                .iter()
                .find(|item| item.repo_key == repo.repo_key)
                .map(|item| item.name.clone())
                .unwrap_or_else(|| repo.name.clone());
            updated_skill_names.push(latest_name);
            synced_targets.extend(result.synced_targets);
        }

        if !updated_repo_keys.is_empty() {
            shared_state = save_skills_state(shared_state)?;
            local_state = save_local_skills_state(local_state)?;
            for target in &synced_targets {
                let _ = reconcile_internal(
                    Some(&target.model),
                    Some(target.scope.as_str()),
                    target.project_root.as_deref(),
                );
            }
            trigger_storage_sync(app, "skills_repo_auto_update_pending");
        }

        let result = RepoAutoUpdateResult {
            updated_repo_count: updated_repo_keys.len() as u64,
            synced_target_count: synced_targets.len() as u64,
            updated_repo_keys,
            updated_skill_names,
            synced_targets,
            applied_at: now_ts(),
        };
        api_ok(result, combined_revision(&shared_state, &local_state))
    })
    .await
    .map_err(|e| e.to_string())?
}
