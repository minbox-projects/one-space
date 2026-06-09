fn collect_files(base: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(current).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
        if is_ignored_name(name) {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_files(base, &path, files)?;
        } else if meta.is_file() {
            if is_duplicate_clone_file(&path) {
                continue;
            }
            let rel = path
                .strip_prefix(base)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            files.push(rel);
        }
    }
    Ok(())
}

fn hash_dir(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }
    let mut files = vec![];
    collect_files(path, path, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for rel in files {
        let rel_str = rel.to_string_lossy();
        hasher.update(rel_str.as_bytes());
        hasher.update([0]);
        let abs = path.join(&rel);
        let content = fs::read(&abs).map_err(|e| e.to_string())?;
        hasher.update((content.len() as u64).to_le_bytes());
        hasher.update([0]);
        hasher.update(&content);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn source_type_from_source_id(source_id: &str) -> String {
    if source_id.starts_with("local-") || source_id == "local" || source_id == "mirror-local" {
        "local_import".to_string()
    } else {
        "remote".to_string()
    }
}

fn upsert_repository_record(repositories: &mut Vec<RepositoryRecord>, record: RepositoryRecord) {
    if let Some(idx) = repositories
        .iter()
        .position(|r| r.repo_key == record.repo_key)
    {
        let mut next = record;
        let existing = &repositories[idx];
        if existing.created_at > 0
            && (next.created_at == 0 || next.created_at > existing.created_at)
        {
            next.created_at = existing.created_at;
        }
        next.ever_installed = next.ever_installed || existing.ever_installed;
        repositories[idx] = next;
    } else {
        let mut next = record;
        if next.created_at == 0 {
            next.created_at = now_ts();
        }
        repositories.push(next);
    }
}

fn mark_repo_ever_installed(state: &mut SkillsState, repo_key: &str) -> bool {
    if let Some(repo) = state
        .repositories
        .iter_mut()
        .find(|r| r.repo_key == repo_key)
    {
        if !repo.ever_installed {
            repo.ever_installed = true;
            repo.updated_at = Some(now_ts());
            return true;
        }
    }
    false
}

fn skill_matches_repository(skill: &SkillRecord, repo: &RepositoryRecord) -> bool {
    let same_repo_key = make_repo_key(&skill.source_id, &skill.source_rel_path) == repo.repo_key;
    let same_skill_id = skill.id == repo.skill_id;
    if same_repo_key || same_skill_id {
        return true;
    }

    let source_matches = if skill.source_id == repo.source_id {
        true
    } else if repo.source_type == "local_import" {
        (skill.source_id == "local" && repo.source_id.starts_with("local-"))
            || (repo.source_id == "local" && skill.source_id.starts_with("local-"))
    } else {
        false
    };
    if !source_matches {
        return false;
    }

    let skill_dir_name = skill.dir_name.trim();
    let repo_dir_name = repo.dir_name.trim();
    let skill_dir_name = if skill_dir_name.is_empty() {
        skill.id.as_str()
    } else {
        skill_dir_name
    };
    let repo_dir_name = if repo_dir_name.is_empty() {
        repo.skill_id.as_str()
    } else {
        repo_dir_name
    };
    if skill_dir_name == repo_dir_name {
        return true;
    }

    let skill_rel_tail = skill
        .source_rel_path
        .rsplit('/')
        .next()
        .unwrap_or(skill.source_rel_path.as_str());
    let repo_rel_tail = repo
        .source_rel_path
        .rsplit('/')
        .next()
        .unwrap_or(repo.source_rel_path.as_str());
    skill_rel_tail == repo_rel_tail
}

fn build_repo_install_state(
    installed_skills: &[SkillRecord],
    repo: &RepositoryRecord,
) -> RepoModelInstallState {
    let mut installed = RepoModelInstallState::default();
    for skill in installed_skills {
        if !skill_matches_repository(skill, repo) {
            continue;
        }
        match skill.model.as_str() {
            "claude" => installed.claude = true,
            "gemini" => installed.gemini = true,
            "codex" => installed.codex = true,
            "opencode" => installed.opencode = true,
            _ => {}
        }
    }
    installed
}

fn repository_pair_has_update(before: &Path, after: &Path) -> bool {
    match (hash_dir(before), hash_dir(after)) {
        (Ok(before_hash), Ok(after_hash)) => before_hash != after_hash,
        _ => false,
    }
}

fn repository_source_dir(repo: &RepositoryRecord, cfg: &StorageConfig) -> Result<PathBuf, String> {
    let source = get_source(cfg, &repo.source_id).ok_or("source not found".to_string())?;
    source_skill_abs_path(source, &repo.source_rel_path)
}

fn repository_has_remote_source_update(repo: &RepositoryRecord, cfg: &StorageConfig) -> bool {
    if repo.source_type != "remote" {
        return false;
    }

    let snapshot = match repo_storage_dir(&repo.repo_key) {
        Ok(path) => path,
        Err(_) => return false,
    };
    if !snapshot.exists() {
        return false;
    }

    let source_dir = match repository_source_dir(repo, cfg) {
        Ok(path) => path,
        Err(_) => return false,
    };
    if !source_dir.exists() {
        return false;
    }

    repository_pair_has_update(&snapshot, &source_dir)
}

fn repository_has_pending_index_update(repo: &RepositoryRecord) -> bool {
    let snapshot = match repo_storage_dir(&repo.repo_key) {
        Ok(path) => path,
        Err(_) => return false,
    };
    if !snapshot.exists() {
        return false;
    }

    let baseline = match repo_index_baseline_dir(&repo.repo_key) {
        Ok(path) => path,
        Err(_) => return false,
    };
    if !baseline.exists() {
        return true;
    }

    if repository_pair_has_update(&baseline, &snapshot) {
        return true;
    }

    false
}

fn repository_has_update(repo: &RepositoryRecord, cfg: &StorageConfig) -> bool {
    if repo.source_type == "remote" {
        return repository_has_remote_source_update(repo, cfg);
    }
    repository_has_pending_index_update(repo)
}

fn build_repository_views(
    shared_state: &SkillsState,
    installed_skills: &[SkillRecord],
    include_update: bool,
    cfg: Option<&StorageConfig>,
) -> Vec<RepositorySkillView> {
    let mut out = shared_state
        .repositories
        .iter()
        .filter_map(|repo| {
            let installed = build_repo_install_state(installed_skills, repo);
            let installed_any =
                installed.claude || installed.gemini || installed.codex || installed.opencode;
            if repo.source_type == "remote" && !repo.ever_installed && !installed_any {
                return None;
            }
            let has_update = if include_update {
                cfg.map(|value| repository_has_update(repo, value))
                    .unwrap_or(false)
            } else {
                false
            };
            Some(RepositorySkillView {
                repo_key: repo.repo_key.clone(),
                skill_id: repo.skill_id.clone(),
                dir_name: normalized_repo_dir_name(repo),
                source_id: repo.source_id.clone(),
                source_rel_path: repo.source_rel_path.clone(),
                source_type: repo.source_type.clone(),
                source_path: repo.source_path.clone(),
                name: repo.name.clone(),
                description: repo.description.clone(),
                models: repo.models.clone(),
                icon_seed: repo.icon_seed.clone(),
                hash: repo.hash.clone(),
                created_at: repo.created_at,
                updated_at: repo.updated_at,
                has_update,
                installed,
            })
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.updated_at.unwrap_or(0).cmp(&a.updated_at.unwrap_or(0)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

fn ensure_repositories_migrated(state: &mut SkillsState) -> Result<bool, String> {
    if !state.repositories.is_empty() {
        return Ok(false);
    }

    let mut changed = false;
    let mut seen = HashSet::new();
    for skill in &state.skills {
        let repo_key = make_repo_key(&skill.source_id, &skill.source_rel_path);
        if !seen.insert(repo_key.clone()) {
            continue;
        }

        let mut repo_hash = Some(skill.local_hash.clone());
        if let Ok(src) = record_local_dir(skill) {
            if src.exists() {
                let dst = repo_storage_dir(&repo_key)?;
                if replace_dir_atomic(&src, &dst).is_ok() {
                    let _ = snapshot_repository_index_baseline(&repo_key, &dst);
                    repo_hash = hash_dir(&dst).ok().or(repo_hash);
                }
            }
        }

        state.repositories.push(RepositoryRecord {
            repo_key: repo_key.clone(),
            skill_id: skill.id.clone(),
            dir_name: normalized_record_dir_name(skill),
            source_id: skill.source_id.clone(),
            source_rel_path: skill.source_rel_path.clone(),
            source_type: source_type_from_source_id(&skill.source_id),
            source_path: None,
            name: skill.name.clone(),
            description: skill.description.clone(),
            models: skill.models.clone(),
            icon_seed: skill.icon_seed.clone(),
            hash: repo_hash,
            created_at: now_ts(),
            updated_at: Some(now_ts()),
            ever_installed: true,
        });
        changed = true;
    }
    Ok(changed)
}

fn upsert_repository_from_dir(
    state: &mut SkillsState,
    source_dir: &Path,
    source_id: &str,
    source_rel_path: &str,
    skill_id: &str,
    dir_name: &str,
    source_type: &str,
    name: &str,
    description: &str,
    models: &[String],
    icon_seed: &str,
    source_path: Option<String>,
    hash_hint: Option<String>,
    mark_ever_installed: bool,
) -> Result<RepositoryRecord, String> {
    let repo_key = make_repo_key(source_id, source_rel_path);
    let repo_dst = repo_storage_dir(&repo_key)?;
    replace_dir_atomic(source_dir, &repo_dst)?;
    snapshot_repository_index_baseline(&repo_key, &repo_dst)?;
    let repo_hash = hash_dir(&repo_dst).ok().or(hash_hint);
    let existing_ever_installed = state
        .repositories
        .iter()
        .find(|r| r.repo_key == repo_key)
        .map(|r| r.ever_installed)
        .unwrap_or(false);
    let existing_created_at = state
        .repositories
        .iter()
        .find(|r| r.repo_key == repo_key)
        .map(|r| r.created_at)
        .unwrap_or(0);
    let created_at = if existing_created_at > 0 {
        existing_created_at
    } else {
        now_ts()
    };

    let record = RepositoryRecord {
        repo_key: repo_key.clone(),
        skill_id: skill_id.to_string(),
        dir_name: dir_name.to_string(),
        source_id: source_id.to_string(),
        source_rel_path: source_rel_path.to_string(),
        source_type: source_type.to_string(),
        source_path,
        name: name.to_string(),
        description: description.to_string(),
        models: models.to_vec(),
        icon_seed: icon_seed.to_string(),
        hash: repo_hash,
        created_at,
        updated_at: Some(now_ts()),
        ever_installed: mark_ever_installed || existing_ever_installed,
    };
    upsert_repository_record(&mut state.repositories, record.clone());
    Ok(record)
}
