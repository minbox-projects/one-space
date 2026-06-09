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

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|v| v.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

fn source_entry_markdown_path(entry: &Path) -> Option<PathBuf> {
    if entry.is_dir() {
        let md = entry.join("AGENT.md");
        if md.exists() {
            return Some(md);
        }
        return None;
    }
    if entry.is_file() && is_markdown_file(entry) {
        return Some(entry.to_path_buf());
    }
    None
}

fn source_entry_exists(entry: &Path) -> bool {
    source_entry_markdown_path(entry).is_some()
}

fn read_markdown_from_source_entry(entry: &Path) -> Result<String, String> {
    let markdown_path =
        source_entry_markdown_path(entry).ok_or("subagents/invalid_subagent_dir".to_string())?;
    fs::read_to_string(markdown_path).map_err(|e| e.to_string())
}

fn read_required_subagent_dir_name_from_entry(entry: &Path) -> Result<String, String> {
    let raw = read_markdown_from_source_entry(entry)?;
    parse_required_subagent_dir_name(&raw)
}

fn hash_source_entry(entry: &Path) -> Result<String, String> {
    if entry.is_dir() {
        return hash_dir(entry);
    }
    let markdown_path =
        source_entry_markdown_path(entry).ok_or("subagents/invalid_subagent_dir".to_string())?;
    let content = fs::read(&markdown_path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(b"subagent-file-entry");
    hasher.update([0]);
    hasher.update((content.len() as u64).to_le_bytes());
    hasher.update([0]);
    hasher.update(&content);
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

fn mark_repo_ever_installed(state: &mut SubagentsState, repo_key: &str) -> bool {
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

fn subagent_matches_repository(subagent: &SubagentRecord, repo: &RepositoryRecord) -> bool {
    let same_repo_key =
        make_repo_key(&subagent.source_id, &subagent.source_rel_path) == repo.repo_key;
    let same_subagent_id = subagent.id == repo.subagent_id;
    if same_repo_key || same_subagent_id {
        return true;
    }

    let source_matches = if subagent.source_id == repo.source_id {
        true
    } else if repo.source_type == "local_import" {
        (subagent.source_id == "local" && repo.source_id.starts_with("local-"))
            || (repo.source_id == "local" && subagent.source_id.starts_with("local-"))
    } else {
        false
    };
    if !source_matches {
        return false;
    }

    let subagent_dir_name = subagent.dir_name.trim();
    let repo_dir_name = repo.dir_name.trim();
    let subagent_dir_name = if subagent_dir_name.is_empty() {
        subagent.id.as_str()
    } else {
        subagent_dir_name
    };
    let repo_dir_name = if repo_dir_name.is_empty() {
        repo.subagent_id.as_str()
    } else {
        repo_dir_name
    };
    if subagent_dir_name == repo_dir_name {
        return true;
    }

    let subagent_rel_tail = subagent
        .source_rel_path
        .rsplit('/')
        .next()
        .unwrap_or(subagent.source_rel_path.as_str());
    let repo_rel_tail = repo
        .source_rel_path
        .rsplit('/')
        .next()
        .unwrap_or(repo.source_rel_path.as_str());
    subagent_rel_tail == repo_rel_tail
}

fn build_repo_install_state(
    installed_subagents: &[SubagentRecord],
    repo: &RepositoryRecord,
) -> RepoModelInstallState {
    let mut installed = RepoModelInstallState::default();
    for subagent in installed_subagents {
        if !subagent_matches_repository(subagent, repo) {
            continue;
        }
        match subagent.model.as_str() {
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

fn build_repository_views(
    shared_state: &SubagentsState,
    installed_subagents: &[SubagentRecord],
    include_update: bool,
) -> Vec<RepositorySubagentView> {
    let mut out = shared_state
        .repositories
        .iter()
        .filter_map(|repo| {
            let installed = build_repo_install_state(installed_subagents, repo);
            let installed_any =
                installed.claude || installed.gemini || installed.codex || installed.opencode;
            if repo.source_type == "remote" && !repo.ever_installed && !installed_any {
                return None;
            }
            let has_update = if include_update {
                repository_has_pending_index_update(repo)
            } else {
                false
            };
            Some(RepositorySubagentView {
                repo_key: repo.repo_key.clone(),
                subagent_id: repo.subagent_id.clone(),
                dir_name: normalized_repo_dir_name(repo),
                source_id: repo.source_id.clone(),
                source_rel_path: repo.source_rel_path.clone(),
                source_type: repo.source_type.clone(),
                source_path: repo.source_path.clone(),
                name: repo.name.clone(),
                description: repo.description.clone(),
                models: repo.models.clone(),
                model: repo.model.clone(),
                tools: repo.tools.clone(),
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

fn ensure_repositories_migrated(state: &mut SubagentsState) -> Result<bool, String> {
    if !state.repositories.is_empty() {
        return Ok(false);
    }

    let mut changed = false;
    let mut seen = HashSet::new();
    for subagent in &state.subagents {
        let repo_key = make_repo_key(&subagent.source_id, &subagent.source_rel_path);
        if !seen.insert(repo_key.clone()) {
            continue;
        }

        let mut repo_hash = Some(subagent.local_hash.clone());
        if let Ok(src) = record_local_dir(subagent) {
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
            subagent_id: subagent.id.clone(),
            dir_name: normalized_record_dir_name(subagent),
            source_id: subagent.source_id.clone(),
            source_rel_path: subagent.source_rel_path.clone(),
            source_type: source_type_from_source_id(&subagent.source_id),
            source_path: None,
            name: subagent.name.clone(),
            description: subagent.description.clone(),
            models: subagent.models.clone(),
            model: None,
            tools: vec![],
            icon_seed: subagent.icon_seed.clone(),
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
    state: &mut SubagentsState,
    source_dir: &Path,
    source_id: &str,
    source_rel_path: &str,
    subagent_id: &str,
    dir_name: &str,
    source_type: &str,
    name: &str,
    description: &str,
    models: &[String],
    model: Option<&str>,
    tools: &[String],
    icon_seed: &str,
    source_path: Option<String>,
    hash_hint: Option<String>,
    mark_ever_installed: bool,
) -> Result<RepositoryRecord, String> {
    let repo_key = make_repo_key(source_id, source_rel_path);
    let repo_dst = repo_storage_dir(&repo_key)?;
    replace_source_entry_atomic(source_dir, &repo_dst)?;
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
        subagent_id: subagent_id.to_string(),
        dir_name: dir_name.to_string(),
        source_id: source_id.to_string(),
        source_rel_path: source_rel_path.to_string(),
        source_type: source_type.to_string(),
        source_path,
        name: name.to_string(),
        description: description.to_string(),
        models: models.to_vec(),
        model: model
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        tools: tools
            .iter()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect(),
        icon_seed: icon_seed.to_string(),
        hash: repo_hash,
        created_at,
        updated_at: Some(now_ts()),
        ever_installed: mark_ever_installed || existing_ever_installed,
    };
    upsert_repository_record(&mut state.repositories, record.clone());
    Ok(record)
}
