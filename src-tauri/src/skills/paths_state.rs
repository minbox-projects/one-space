fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn skills_root() -> Result<PathBuf, String> {
    let p = crate::get_data_dir()?.join("data").join("skills");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_local_root() -> Result<PathBuf, String> {
    let p = skills_local_cache_base_root()?.join("local_state");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_models_root() -> Result<PathBuf, String> {
    let p = skills_local_root()?.join("models");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_meta_root() -> Result<PathBuf, String> {
    let p = skills_root()?.join("meta");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_local_cache_base_root() -> Result<PathBuf, String> {
    let p = if let Some(home) = dirs::home_dir() {
        home.join(".config").join("onespace").join("skills")
    } else {
        // Fallback to app-local config directory if home is unavailable.
        config::get_app_dir()?.join("skills")
    };
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_cache_root() -> Result<PathBuf, String> {
    // Remote git source caches are always local to reduce iCloud/git sync pressure.
    let p = skills_local_cache_base_root()?.join("remote_cache");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_state_path() -> Result<PathBuf, String> {
    Ok(skills_meta_root()?.join("state.json"))
}

fn skills_local_meta_root() -> Result<PathBuf, String> {
    let p = skills_local_root()?.join("meta");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn skills_local_state_path() -> Result<PathBuf, String> {
    Ok(skills_local_meta_root()?.join("installed_state.json"))
}

fn sync_state_path() -> Result<PathBuf, String> {
    Ok(skills_meta_root()?.join("sync_state.json"))
}

fn model_dir(model: &str) -> Result<PathBuf, String> {
    if !MODELS.contains(&model) {
        return Err(format!("unsupported model: {}", model));
    }
    let p = skills_models_root()?.join(model);
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

fn project_primary_dir(model: &str, project_root: &Path) -> Result<PathBuf, String> {
    let p = project_scan_root(model, project_root)?;
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn project_scan_root(model: &str, project_root: &Path) -> Result<PathBuf, String> {
    Ok(match model {
        "claude" => project_root.join(".claude").join("skills"),
        "codex" => project_root.join(".agents").join("skills"),
        "gemini" => project_root.join(".gemini").join("skills"),
        "opencode" => project_root.join(".opencode").join("skills"),
        _ => return Err(format!("unsupported model: {}", model)),
    })
}

fn project_compat_dirs(model: &str, project_root: &Path) -> Vec<PathBuf> {
    match model {
        "codex" => vec![project_root.join(".codex").join("skills")],
        _ => vec![],
    }
}

fn mirror_dir(model: &str) -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("home directory not found")?;
    let p = match model {
        "claude" => home.join(".claude").join("skills"),
        "codex" => home.join(".codex").join("skills"),
        "gemini" => home.join(".gemini").join("skills"),
        "opencode" => home.join(".config").join("opencode").join("skills"),
        _ => return Err(format!("unsupported model: {}", model)),
    };
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(fs::canonicalize(&p).unwrap_or(p))
}

fn resolve_skill_target_dir(
    model: &str,
    scope: &str,
    project_root: Option<&str>,
) -> Result<(PathBuf, Vec<PathBuf>), String> {
    if scope == INSTALL_SCOPE_PROJECT {
        let root = project_root.ok_or("skills/project_root_required")?;
        let project_root_path = PathBuf::from(root);
        let primary = project_primary_dir(model, &project_root_path)?;
        let compat = project_compat_dirs(model, &project_root_path);
        for path in &compat {
            fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        return Ok((primary, compat));
    }
    let primary = model_dir(model)?;
    let mirror = mirror_dir(model)?;
    Ok((primary, vec![mirror]))
}

fn make_repo_key(source_id: &str, source_rel_path: &str) -> String {
    format!("{}::{}", source_id, source_rel_path)
}

fn repo_storage_root() -> Result<PathBuf, String> {
    let p = skills_root()?.join("repository");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn repo_storage_dir(repo_key: &str) -> Result<PathBuf, String> {
    let digest = sha256_hex(repo_key);
    Ok(repo_storage_root()?.join(digest))
}

fn repo_index_baseline_root() -> Result<PathBuf, String> {
    let p = skills_root()?.join("index_baselines");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn repo_index_baseline_dir(repo_key: &str) -> Result<PathBuf, String> {
    let digest = sha256_hex(repo_key);
    Ok(repo_index_baseline_root()?.join(digest))
}

fn snapshot_repository_index_baseline(repo_key: &str, source_dir: &Path) -> Result<(), String> {
    let baseline = repo_index_baseline_dir(repo_key)?;
    replace_dir_atomic(source_dir, &baseline)
}

fn safe_slug(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn normalize_rel_path(rel: &Path) -> String {
    if rel == Path::new(".") {
        return ".".to_string();
    }
    rel.to_string_lossy().replace('\\', "/")
}

fn resolve_scan_root(root_path: &str) -> Result<PathBuf, String> {
    let raw = root_path.trim();
    if raw.is_empty() {
        return Err("skills/invalid_scan_root".to_string());
    }
    let root = PathBuf::from(raw);
    if !root.exists() {
        return Err("skills/invalid_scan_root".to_string());
    }
    if !root.is_dir() {
        return Err("skills/invalid_scan_root".to_string());
    }
    fs::canonicalize(&root).map_err(|_| "skills/invalid_scan_root".to_string())
}

fn local_source_id(root_can: &Path) -> String {
    let digest = sha256_hex(&root_can.to_string_lossy());
    format!("local-{}", &digest[..8])
}

fn local_skill_id(source_id: &str, rel_path: &str) -> String {
    let key = format!("{}:{}", source_id, rel_path);
    let digest = sha256_hex(&key);
    let slug = safe_slug(&key);
    let slug = if slug.is_empty() {
        source_id.to_string()
    } else {
        slug
    };
    format!("{}-{}", slug, &digest[..8])
}

fn has_path_traversal(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

fn is_ignored_name(name: &str) -> bool {
    IGNORE_NAMES.contains(&name)
}

fn parse_duplicate_file_name(name: &str) -> Option<String> {
    let dot_idx = name.rfind('.')?;
    let (stem, ext) = name.split_at(dot_idx);
    let space_idx = stem.rfind(' ')?;
    let suffix = &stem[space_idx + 1..];
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let base = &stem[..space_idx];
    if base.trim().is_empty() {
        return None;
    }
    Some(format!("{}{}", base, ext))
}

fn is_duplicate_clone_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|v| v.to_str()) else {
        return false;
    };
    let Some(counterpart_name) = parse_duplicate_file_name(file_name) else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let counterpart = parent.join(counterpart_name);
    if !counterpart.exists() {
        return false;
    }
    if !path.is_file() || !counterpart.is_file() {
        return false;
    }
    match (fs::read(path), fs::read(counterpart)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn ensure_within(root: &Path, target: &Path) -> Result<(), String> {
    if has_path_traversal(target) {
        return Err("skills/path_out_of_root".to_string());
    }
    let root_can = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let target_can = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    if !target_can.starts_with(&root_can) {
        return Err("skills/path_out_of_root".to_string());
    }
    Ok(())
}

fn read_json_or_default<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    crate::atomic_write_string(path, &raw)
}

fn merge_repository_record(
    mut keep: RepositoryRecord,
    candidate: RepositoryRecord,
) -> RepositoryRecord {
    if keep.source_path.is_none() && candidate.source_path.is_some() {
        keep.source_path = candidate.source_path.clone();
    }
    if keep.hash.as_deref().unwrap_or("").is_empty()
        && !candidate.hash.as_deref().unwrap_or("").is_empty()
    {
        keep.hash = candidate.hash.clone();
    }
    if keep.updated_at.unwrap_or(0) < candidate.updated_at.unwrap_or(0) {
        keep.updated_at = candidate.updated_at;
    }
    if keep.models.is_empty() && !candidate.models.is_empty() {
        keep.models = candidate.models.clone();
    }
    if keep.created_at == 0 {
        keep.created_at = candidate.created_at;
    } else if candidate.created_at > 0 {
        keep.created_at = keep.created_at.min(candidate.created_at);
    }
    keep.ever_installed = keep.ever_installed || candidate.ever_installed;
    keep
}

fn is_local_duplicate_repository(a: &RepositoryRecord, b: &RepositoryRecord) -> bool {
    if a.source_type != "local_import" || b.source_type != "local_import" {
        return false;
    }
    if a.skill_id == b.skill_id {
        return true;
    }
    let ah = a.hash.as_deref().unwrap_or("").trim();
    let bh = b.hash.as_deref().unwrap_or("").trim();
    if ah.is_empty() || bh.is_empty() || ah != bh {
        return false;
    }
    a.name.trim().eq_ignore_ascii_case(b.name.trim())
}

fn normalize_repositories(state: &mut SkillsState) -> bool {
    let mut changed = false;
    for repo in &mut state.repositories {
        if repo.source_type == "mirror" {
            repo.source_type = "local_import".to_string();
            changed = true;
        }
        if repo.dir_name.trim().is_empty() {
            repo.dir_name = repo.skill_id.clone();
            changed = true;
        }
        if repo.created_at == 0 {
            repo.created_at = repo.updated_at.unwrap_or_else(now_ts);
            changed = true;
        }
    }

    let mut deduped: Vec<RepositoryRecord> = vec![];
    for repo in state.repositories.drain(..) {
        if let Some(idx) = deduped
            .iter()
            .position(|existing| is_local_duplicate_repository(existing, &repo))
        {
            let merged = merge_repository_record(deduped[idx].clone(), repo);
            deduped[idx] = merged;
            changed = true;
        } else {
            deduped.push(repo);
        }
    }
    state.repositories = deduped;
    changed
}

fn load_skills_state() -> Result<SkillsState, String> {
    let mut state: SkillsState = read_json_or_default(&skills_state_path()?)?;
    let mut changed = false;
    if ensure_repositories_migrated(&mut state)? {
        changed = true;
    }
    if normalize_repositories(&mut state) {
        changed = true;
    }
    if !state.skills.is_empty() {
        // Installed records must not be persisted in shared storage.
        state.skills.clear();
        changed = true;
    }
    if changed {
        state = save_skills_state(state)?;
    }
    Ok(state)
}

fn save_skills_state(mut state: SkillsState) -> Result<SkillsState, String> {
    state.skills.clear();
    state.revision = state.revision.saturating_add(1);
    write_json(&skills_state_path()?, &state)?;
    Ok(state)
}

fn load_local_skills_state() -> Result<SkillsLocalState, String> {
    let mut state: SkillsLocalState = read_json_or_default(&skills_local_state_path()?)?;
    if normalize_local_skills_state(&mut state) {
        state = save_local_skills_state(state)?;
    }
    Ok(state)
}

fn save_local_skills_state(mut state: SkillsLocalState) -> Result<SkillsLocalState, String> {
    state.revision = state.revision.saturating_add(1);
    let path = skills_local_state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_json(&path, &state)?;
    Ok(state)
}

fn normalize_local_skills_state(state: &mut SkillsLocalState) -> bool {
    let before_len = state.skills.len();
    state
        .skills
        .retain(|skill| record_scope(skill) == INSTALL_SCOPE_GLOBAL);
    before_len != state.skills.len()
}

fn combined_revision(shared: &SkillsState, local: &SkillsLocalState) -> u64 {
    shared.revision.max(local.revision)
}

fn load_sync_state() -> Result<SkillsSyncState, String> {
    read_json_or_default(&sync_state_path()?)
}

fn save_sync_state(state: &SkillsSyncState) -> Result<(), String> {
    write_json(&sync_state_path()?, state)
}

fn api_ok<T: Serialize>(data: T, revision: u64) -> Result<ApiOk<T>, String> {
    Ok(ApiOk {
        ok: true,
        data,
        meta: ApiMeta {
            revision,
            ts: now_ts(),
        },
    })
}
