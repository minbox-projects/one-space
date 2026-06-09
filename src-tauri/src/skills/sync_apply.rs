#[derive(Debug, Clone)]
struct RepositorySyncTargetPlan {
    record: SkillRecord,
    scope: String,
    project_root: Option<String>,
    dest: PathBuf,
    compat_dests: Vec<PathBuf>,
    old_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct DirectoryBackupEntry {
    target: PathBuf,
    backup: Option<PathBuf>,
}

#[derive(Debug)]
struct DirectoryBackupManager {
    root: PathBuf,
    captured: HashSet<PathBuf>,
    entries: Vec<DirectoryBackupEntry>,
}

impl DirectoryBackupManager {
    fn new(label: &str) -> Result<Self, String> {
        let root = skills_root()?.join("transactions").join(format!(
            "{}-{}",
            label,
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(Self {
            root,
            captured: HashSet::new(),
            entries: vec![],
        })
    }

    fn capture(&mut self, target: &Path) -> Result<(), String> {
        let target_buf = target.to_path_buf();
        if !self.captured.insert(target_buf.clone()) {
            return Ok(());
        }
        let backup = if target.exists() {
            let backup_path = self.root.join(format!(
                "{}-{}",
                self.entries.len(),
                uuid::Uuid::new_v4().simple()
            ));
            copy_dir_secure_internal(target, target, &self.root, &backup_path)?;
            Some(backup_path)
        } else {
            None
        };
        self.entries.push(DirectoryBackupEntry {
            target: target_buf,
            backup,
        });
        Ok(())
    }

    fn rollback(&self) -> Result<(), String> {
        let mut first_error = None;
        for entry in self.entries.iter().rev() {
            if entry.target.exists() {
                if let Err(err) = fs::remove_dir_all(&entry.target) {
                    if first_error.is_none() {
                        first_error = Some(err.to_string());
                    }
                    continue;
                }
            }
            if let Some(backup) = entry.backup.as_ref() {
                if let Err(err) = replace_dir_atomic(backup, &entry.target) {
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
            }
        }
        if let Some(err) = first_error {
            return Err(err);
        }
        Ok(())
    }
}

impl Drop for DirectoryBackupManager {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unique_models_from_targets(targets: &[InstalledSkillTarget]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    for target in targets {
        if seen.insert(target.model.clone()) {
            out.push(target.model.clone());
        }
    }
    out
}

fn installed_targets_for_repo(
    local_state: &SkillsLocalState,
    repo: &RepositoryRecord,
) -> Vec<InstalledSkillTarget> {
    let mut targets = local_state
        .skills
        .iter()
        .filter(|s| skill_matches_repository(s, repo))
        .map(build_installed_target)
        .collect::<Vec<_>>();
    targets.sort_by(|a, b| {
        a.model
            .cmp(&b.model)
            .then_with(|| a.scope.cmp(&b.scope))
            .then_with(|| a.project_root.cmp(&b.project_root))
            .then_with(|| a.dir_name.cmp(&b.dir_name))
    });
    targets
}

fn build_repository_sync_target_plans(
    local_state: &SkillsLocalState,
    repo: &RepositoryRecord,
) -> Result<Vec<RepositorySyncTargetPlan>, String> {
    let matching_records = local_state
        .skills
        .iter()
        .filter(|s| skill_matches_repository(s, repo))
        .cloned()
        .collect::<Vec<_>>();
    let repo_dir_name = normalized_repo_dir_name(repo);
    let mut plans = vec![];

    for record in matching_records {
        let scope = record_scope(&record);
        let project_root = record_project_root(&record);
        ensure_model_dir_name_available(
            local_state,
            &record.model,
            &scope,
            project_root.as_deref(),
            &repo_dir_name,
            Some(record.id.as_str()),
        )?;
        let (target_root, compat_roots) =
            resolve_skill_target_dir(&record.model, &scope, project_root.as_deref())?;
        let dest = target_root.join(&repo_dir_name);
        ensure_within(&target_root, &dest)?;
        let mut compat_dests = vec![];
        for compat_root in compat_roots {
            let compat_dest = compat_root.join(&repo_dir_name);
            ensure_within(&compat_root, &compat_dest)?;
            compat_dests.push(compat_dest);
        }
        let old_dir = locate_existing_record_local_dir(&record)?;
        let old_dir = if old_dir != dest && old_dir.exists() {
            Some(old_dir)
        } else {
            None
        };
        plans.push(RepositorySyncTargetPlan {
            record,
            scope,
            project_root,
            dest,
            compat_dests,
            old_dir,
        });
    }

    Ok(plans)
}

fn sync_repository_snapshot_to_installed_targets(
    local_state: &mut SkillsLocalState,
    repo: &RepositoryRecord,
    repo_snapshot: &Path,
    plans: &[RepositorySyncTargetPlan],
    now: u64,
) -> Result<Vec<InstalledSkillTarget>, String> {
    let repo_dir_name = normalized_repo_dir_name(repo);
    let mut synced_targets = vec![];

    for plan in plans {
        let record = &plan.record;
        remove_existing_record_dir_if_moved(
            local_state,
            &record.model,
            &plan.scope,
            plan.project_root.as_deref(),
            &record.id,
            &plan.dest,
        )?;
        replace_dir_atomic(repo_snapshot, &plan.dest)?;
        for compat_dest in &plan.compat_dests {
            replace_dir_atomic(&plan.dest, compat_dest)?;
        }
        let local_hash = hash_dir(&plan.dest)?;
        if let Some(next) = local_state.skills.iter_mut().find(|s| {
            s.model == record.model
                && s.id == record.id
                && scope_project_match(s, &plan.scope, plan.project_root.as_deref())
        }) {
            next.dir_name = repo_dir_name.clone();
            next.name = repo.name.clone();
            next.description = repo.description.clone();
            next.models = repo.models.clone();
            next.local_hash = local_hash.clone();
            next.remote_hash = repo.hash.clone();
            next.has_update = false;
            next.last_synced_at = Some(now);
            next.updated_at = Some(now);
            next.target_path = Some(plan.dest.to_string_lossy().to_string());
        }
        synced_targets.push(InstalledSkillTarget {
            model: record.model.clone(),
            scope: plan.scope.clone(),
            project_root: plan.project_root.clone(),
            dir_name: repo_dir_name.clone(),
        });
    }

    Ok(synced_targets)
}

fn resolve_repo_reload_compare(
    repo: &RepositoryRecord,
    cfg: &StorageConfig,
) -> Result<(Option<PathBuf>, String, PathBuf, String), String> {
    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    if repo.source_type == "remote" {
        let source_dir = repository_source_dir(repo, cfg)?;
        return Ok((
            Some(repo_snapshot),
            "Repository Snapshot (Current)".to_string(),
            source_dir,
            "Source Snapshot (Latest)".to_string(),
        ));
    }

    let baseline = repo_index_baseline_dir(&repo.repo_key)?;
    let before_dir = if baseline.exists() {
        Some(baseline)
    } else {
        None
    };
    Ok((
        before_dir,
        "Before Reload (Indexed Baseline)".to_string(),
        repo_snapshot,
        "After Reload (Current Snapshot)".to_string(),
    ))
}

fn apply_repository_update_from_dir(
    shared_state: &mut SkillsState,
    local_state: &mut SkillsLocalState,
    repo_key: &str,
    compare_before_dir: Option<&Path>,
    apply_source_dir: &Path,
    sync_to_targets: bool,
) -> Result<ReloadApplyResult, String> {
    let repo_idx = shared_state
        .repositories
        .iter()
        .position(|r| r.repo_key == repo_key)
        .ok_or("repo skill not found")?;
    let repo_snapshot = repo_storage_dir(repo_key)?;
    if !repo_snapshot.exists() {
        return Err("repository_snapshot_missing".to_string());
    }

    let (changed_files, _) = compare_snapshot_dirs(compare_before_dir, apply_source_dir)?;
    let updated_files_count = changed_files.len() as u64;
    let mut backup_manager = DirectoryBackupManager::new("skills-repo-apply")?;
    let result = (|| -> Result<ReloadApplyResult, String> {
        if apply_source_dir != repo_snapshot {
            backup_manager.capture(&repo_snapshot)?;
            replace_dir_atomic(apply_source_dir, &repo_snapshot)?;
        }

        {
            let repo = shared_state
                .repositories
                .get_mut(repo_idx)
                .ok_or("repo skill not found")?;
            refresh_repository_record_from_snapshot(repo)?;
        }

        let repo = shared_state.repositories[repo_idx].clone();
        let now = now_ts();
        let synced_targets = if sync_to_targets {
            let plans = build_repository_sync_target_plans(local_state, &repo)?;
            for plan in &plans {
                if let Some(old_dir) = plan.old_dir.as_ref() {
                    backup_manager.capture(old_dir)?;
                }
                backup_manager.capture(&plan.dest)?;
                for compat_dest in &plan.compat_dests {
                    backup_manager.capture(compat_dest)?;
                }
            }
            sync_repository_snapshot_to_installed_targets(
                local_state,
                &repo,
                &repo_snapshot,
                &plans,
                now,
            )?
        } else {
            vec![]
        };

        let baseline = repo_index_baseline_dir(repo_key)?;
        backup_manager.capture(&baseline)?;
        snapshot_repository_index_baseline(repo_key, &repo_snapshot)?;

        Ok(ReloadApplyResult {
            index_refreshed: true,
            synced_models: unique_models_from_targets(&synced_targets),
            synced_targets,
            updated_files_count,
            applied_at: now,
        })
    })();

    match result {
        Ok(value) => Ok(value),
        Err(err) => match backup_manager.rollback() {
            Ok(()) => Err(err),
            Err(rollback_err) => Err(format!("{err}; rollback_failed: {rollback_err}")),
        },
    }
}

fn refresh_repository_record_from_snapshot(repo: &mut RepositoryRecord) -> Result<(), String> {
    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    let markdown = fs::read_to_string(repo_snapshot.join("SKILL.md")).unwrap_or_default();
    let (name, description, models) = parse_skill_md(&markdown, &[]);
    let dir_name = parse_required_skill_dir_name(&markdown)?;
    repo.name = name;
    repo.description = description;
    repo.models = models;
    repo.dir_name = dir_name;
    repo.hash = Some(hash_dir(&repo_snapshot)?);
    repo.updated_at = Some(now_ts());
    Ok(())
}

fn materialize_repository_snapshot_if_missing(
    repo: &RepositoryRecord,
    local_state: &SkillsLocalState,
    cfg: &StorageConfig,
) -> Result<bool, String> {
    let repo_snapshot = repo_storage_dir(&repo.repo_key)?;
    if repo_snapshot.exists() {
        return Ok(false);
    }

    if let Some(src) = repo.source_path.as_ref() {
        let src_path = PathBuf::from(src);
        if src_path.join("SKILL.md").exists() {
            replace_dir_atomic(&src_path, &repo_snapshot)?;
            snapshot_repository_index_baseline(&repo.repo_key, &repo_snapshot)?;
            return Ok(true);
        }
    }

    if let Some(local_record) = local_state
        .skills
        .iter()
        .find(|s| skill_matches_repository(s, repo))
    {
        let local_dir = record_local_dir(local_record)?;
        if local_dir.join("SKILL.md").exists() {
            replace_dir_atomic(&local_dir, &repo_snapshot)?;
            snapshot_repository_index_baseline(&repo.repo_key, &repo_snapshot)?;
            return Ok(true);
        }
    }

    if repo.source_type == "remote" {
        if let Some(source) = get_source(cfg, &repo.source_id) {
            if let Ok(source_path) = source_skill_abs_path(source, &repo.source_rel_path) {
                if source_path.join("SKILL.md").exists() {
                    replace_dir_atomic(&source_path, &repo_snapshot)?;
                    snapshot_repository_index_baseline(&repo.repo_key, &repo_snapshot)?;
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

fn ensure_repository_snapshots_materialized(
    state: &mut SkillsState,
    local_state: &SkillsLocalState,
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

fn record_local_dir(record: &SkillRecord) -> Result<PathBuf, String> {
    Ok(record_target_root(record)?.join(normalized_record_dir_name(record)))
}

fn migrate_installed_dir_names(
    shared_state: &mut SkillsState,
    local_state: &mut SkillsLocalState,
) -> Result<(bool, bool), String> {
    let mut shared_changed = false;
    let mut local_changed = false;

    for model in MODELS {
        let model_root = model_dir(model)?;
        let indices = local_state
            .skills
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
            let record = local_state.skills[idx].clone();
            let current_dir = locate_existing_record_local_dir(&record)?;
            if !current_dir.exists() {
                continue;
            }
            let md = current_dir.join("SKILL.md");
            if !md.exists() {
                continue;
            }
            let md_raw = fs::read_to_string(&md).unwrap_or_default();
            let desired_dir_name = match parse_required_skill_dir_name(&md_raw) {
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
            if local_state.skills[idx].dir_name != desired_dir_name {
                local_state.skills[idx].dir_name = desired_dir_name.clone();
                local_changed = true;
            }
            if local_state.skills[idx].local_hash != new_hash {
                local_state.skills[idx].local_hash = new_hash;
                local_changed = true;
            }

            if upsert_repo_dir_name(
                shared_state,
                &local_state.skills[idx].source_id,
                &local_state.skills[idx].source_rel_path,
                &local_state.skills[idx].id,
                &desired_dir_name,
            ) {
                shared_changed = true;
            }
        }
    }

    Ok((shared_changed, local_changed))
}
