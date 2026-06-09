fn normalized_model(value: &str) -> Option<String> {
    let v = value.trim().to_lowercase();
    if MODELS.contains(&v.as_str()) {
        Some(v)
    } else {
        None
    }
}

fn normalize_models(models: &[String]) -> Vec<String> {
    let mut out = vec![];
    for raw in models {
        if let Some(m) = normalized_model(raw) {
            if !out.contains(&m) {
                out.push(m);
            }
        }
    }
    out
}

fn all_models_vec() -> Vec<String> {
    MODELS.iter().map(|v| v.to_string()).collect()
}

fn resolve_effective_models(
    declared_models: &[String],
    source_allowed_models: &[String],
) -> Vec<String> {
    let mut declared = normalize_models(declared_models);
    if declared.is_empty() {
        declared = all_models_vec();
    }
    let allowed = normalize_models(source_allowed_models);
    if allowed.is_empty() {
        return declared;
    }
    declared
        .into_iter()
        .filter(|model| allowed.contains(model))
        .collect::<Vec<_>>()
}

fn parse_models(text: &str, source_default: &[String]) -> Vec<String> {
    let mut out = vec![];
    for line in text.lines() {
        let lower = line.trim().to_lowercase();
        if lower.starts_with("models:") {
            let body = line.split_once(':').map(|(_, v)| v).unwrap_or("").trim();
            let body = body.trim_matches('[').trim_matches(']');
            for item in body.split(',') {
                let m = item
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_lowercase();
                if MODELS.contains(&m.as_str()) && !out.contains(&m) {
                    out.push(m);
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    for v in normalize_models(source_default) {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    if out.is_empty() {
        all_models_vec()
    } else {
        out
    }
}

fn normalize_skill_markdown_for_parse(md: &str) -> String {
    let no_bom = md.strip_prefix('\u{feff}').unwrap_or(md);
    no_bom.replace("\r\n", "\n").replace('\r', "\n")
}

fn split_frontmatter_block(md: &str) -> (Option<&str>, &str) {
    let trimmed = md.trim_start_matches(|c: char| c.is_whitespace());
    if !trimmed.starts_with("---\n") {
        return (None, md);
    }

    let body = &trimmed[4..];
    let mut cursor = 0usize;
    for segment in body.split_inclusive('\n') {
        let line = segment.trim_end_matches('\n').trim_end_matches('\r');
        if line.trim() == "---" {
            let frontmatter = if cursor > 0 {
                &body[..cursor.saturating_sub(1)]
            } else {
                ""
            };
            let content = &body[cursor + segment.len()..];
            return (Some(frontmatter), content);
        }
        cursor += segment.len();
    }

    if body.trim() == "---" {
        return (Some(""), "");
    }

    (None, md)
}

fn parse_title_and_description(content: &str) -> (Option<String>, Option<String>) {
    let mut title = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let candidate = trimmed.trim_start_matches('#').trim().to_string();
            if !candidate.is_empty() {
                title = Some(candidate);
                break;
            }
        }
    }

    let mut desc = String::new();
    let mut in_para = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if in_para {
                break;
            }
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        in_para = true;
        if !desc.is_empty() {
            desc.push(' ');
        }
        desc.push_str(trimmed);
    }

    let description = if desc.is_empty() { None } else { Some(desc) };
    (title, description)
}

fn parse_skill_md(md: &str, source_default_models: &[String]) -> (String, String, Vec<String>) {
    let normalized = normalize_skill_markdown_for_parse(md);
    let (frontmatter, mut content) = split_frontmatter_block(&normalized);
    let mut name_from_frontmatter = None;
    let mut description_from_frontmatter = None;
    let mut models = parse_models(&normalized, source_default_models);
    if let Some(front) = frontmatter {
        models = parse_models(front, source_default_models);
        name_from_frontmatter = parse_frontmatter_value(front, "name");
        description_from_frontmatter = parse_frontmatter_value(front, "description");
        content = content.trim_start_matches('\n');
    }

    let (title_from_content, paragraph_from_content) = parse_title_and_description(content);
    let name = title_from_content
        .or(name_from_frontmatter)
        .unwrap_or_else(|| "Unnamed Skill".to_string());
    let desc = description_from_frontmatter
        .or(paragraph_from_content)
        .unwrap_or_else(|| "No description".to_string());
    (name, desc, models)
}

fn validate_frontmatter_name_as_dir(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err("skills/invalid_frontmatter_name".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("skills/invalid_frontmatter_name".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err("skills/invalid_frontmatter_name".to_string());
    }
    Ok(name.to_string())
}

fn parse_required_skill_dir_name(md: &str) -> Result<String, String> {
    let normalized = normalize_skill_markdown_for_parse(md);
    let (frontmatter, _) = split_frontmatter_block(&normalized);
    let frontmatter = frontmatter.ok_or("skills/invalid_frontmatter_name".to_string())?;
    let raw_name = parse_frontmatter_value(frontmatter, "name")
        .ok_or("skills/invalid_frontmatter_name".to_string())?;
    validate_frontmatter_name_as_dir(&raw_name)
}

fn read_required_skill_dir_name(skill_dir: &Path) -> Result<String, String> {
    let raw = fs::read_to_string(skill_dir.join("SKILL.md"))
        .map_err(|_| "skills/invalid_skill_dir".to_string())?;
    parse_required_skill_dir_name(&raw)
}

fn normalized_record_dir_name(record: &SkillRecord) -> String {
    let name = record.dir_name.trim();
    if name.is_empty() {
        record.id.clone()
    } else {
        name.to_string()
    }
}

fn normalized_repo_dir_name(repo: &RepositoryRecord) -> String {
    let name = repo.dir_name.trim();
    if name.is_empty() {
        repo.skill_id.clone()
    } else {
        name.to_string()
    }
}

fn record_project_root(record: &SkillRecord) -> Option<String> {
    record
        .project_root
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn record_target_root(record: &SkillRecord) -> Result<PathBuf, String> {
    let scope = record_scope(record);
    let project_root = record_project_root(record);
    let (root, _) = resolve_skill_target_dir(&record.model, &scope, project_root.as_deref())?;
    Ok(root)
}

fn locate_existing_record_local_dir(record: &SkillRecord) -> Result<PathBuf, String> {
    let root = record_target_root(record)?;
    let mut candidates = vec![];
    let dir_name = record.dir_name.trim();
    if !dir_name.is_empty() {
        candidates.push(dir_name.to_string());
    }
    candidates.push(record.id.clone());
    candidates.dedup();

    for candidate in &candidates {
        let path = root.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    let fallback = candidates
        .first()
        .cloned()
        .unwrap_or_else(|| record.id.clone());
    Ok(root.join(fallback))
}

fn has_dir_name_conflict(
    state: &SkillsLocalState,
    model: &str,
    scope: &str,
    project_root: Option<&str>,
    dir_name: &str,
    ignore_skill_id: Option<&str>,
) -> bool {
    state.skills.iter().any(|s| {
        if s.model != model {
            return false;
        }
        if record_scope(s) != scope {
            return false;
        }
        let s_root = record_project_root(s);
        let target_root = project_root
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if s_root != target_root {
            return false;
        }
        if ignore_skill_id.is_some() && ignore_skill_id == Some(s.id.as_str()) {
            return false;
        }
        normalized_record_dir_name(s) == dir_name
    })
}

fn ensure_model_dir_name_available(
    state: &SkillsLocalState,
    model: &str,
    scope: &str,
    project_root: Option<&str>,
    dir_name: &str,
    ignore_skill_id: Option<&str>,
) -> Result<(), String> {
    if has_dir_name_conflict(state, model, scope, project_root, dir_name, ignore_skill_id) {
        return Err("skills/dir_name_conflict".to_string());
    }
    let (root, _) = resolve_skill_target_dir(model, scope, project_root)?;
    let dest = root.join(dir_name);
    ensure_within(&root, &dest)?;
    if dest.exists() {
        if let Some(skill_id) = ignore_skill_id {
            if let Some(existing) = state.skills.iter().find(|s| {
                s.model == model
                    && s.id == skill_id
                    && record_scope(s) == scope
                    && record_project_root(s)
                        == project_root
                            .map(|v| v.trim().to_string())
                            .filter(|v| !v.is_empty())
            }) {
                let existing_path = locate_existing_record_local_dir(existing)?;
                if existing_path == dest {
                    return Ok(());
                }
            }
        }
        return Err("skills/dir_name_conflict".to_string());
    }
    Ok(())
}

fn remove_existing_record_dir_if_moved(
    state: &SkillsLocalState,
    model: &str,
    scope: &str,
    project_root: Option<&str>,
    skill_id: &str,
    new_dest: &Path,
) -> Result<(), String> {
    let Some(existing) = state.skills.iter().find(|s| {
        s.model == model
            && s.id == skill_id
            && record_scope(s) == scope
            && record_project_root(s)
                == project_root
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
    }) else {
        return Ok(());
    };
    let old_dir = locate_existing_record_local_dir(existing)?;
    if old_dir != new_dest && old_dir.exists() {
        fs::remove_dir_all(old_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn upsert_repo_dir_name(
    state: &mut SkillsState,
    source_id: &str,
    source_rel_path: &str,
    skill_id: &str,
    dir_name: &str,
) -> bool {
    let repo_key = make_repo_key(source_id, source_rel_path);
    let mut changed = false;
    for repo in &mut state.repositories {
        if repo.repo_key == repo_key || repo.skill_id == skill_id {
            if repo.dir_name != dir_name {
                repo.dir_name = dir_name.to_string();
                repo.updated_at = Some(now_ts());
                changed = true;
            }
        }
    }
    changed
}

fn parse_frontmatter_value(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        if !k.trim().eq_ignore_ascii_case(key) {
            continue;
        }
        let value = v
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn find_skill_dirs(base: &Path, current: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
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
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                let rel = path
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_path_buf();
                out.push(rel);
            } else {
                find_skill_dirs(base, &path, out)?;
            }
        }
    }
    Ok(())
}

fn find_local_skill_dirs(base: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = vec![];
    if base.join("SKILL.md").exists() {
        out.push(PathBuf::from("."));
    }
    find_skill_dirs(base, base, &mut out)?;
    out.sort_by(|a, b| normalize_rel_path(a).cmp(&normalize_rel_path(b)));
    Ok(out)
}

fn scan_local_candidates(root_can: &Path) -> Result<Vec<LocalSkillCandidate>, String> {
    let source_id = local_source_id(root_can);
    let skill_dirs = find_local_skill_dirs(root_can)?;
    let mut out = vec![];
    for rel in skill_dirs {
        let rel_str = normalize_rel_path(&rel);
        let abs = if rel_str == "." {
            root_can.to_path_buf()
        } else {
            root_can.join(&rel)
        };
        let md = abs.join("SKILL.md");
        let md_content = fs::read_to_string(&md).map_err(|e| e.to_string())?;
        let (name, description, declared_models) = parse_skill_md(&md_content, &[]);
        let dir_name = parse_required_skill_dir_name(&md_content).unwrap_or_default();
        out.push(LocalSkillCandidate {
            rel_path: rel_str.clone(),
            skill_id: local_skill_id(&source_id, &rel_str),
            dir_name,
            source_id: source_id.clone(),
            name,
            description,
            declared_models,
        });
    }
    Ok(out)
}

fn copy_dir_secure_internal(
    src_root: &Path,
    src: &Path,
    dst_root: &Path,
    dst: &Path,
) -> Result<(), String> {
    if !src.exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }
    let src_root_can =
        fs::canonicalize(src_root).map_err(|_| "skills/path_out_of_root".to_string())?;
    let src_can = fs::canonicalize(src).map_err(|_| "skills/path_out_of_root".to_string())?;
    if !src_can.starts_with(&src_root_can) {
        return Err("skills/path_out_of_root".to_string());
    }
    ensure_within(dst_root, dst)?;
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
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
        let target = dst.join(entry.file_name());
        if meta.is_dir() {
            copy_dir_secure_internal(src_root, &path, dst_root, &target)?;
        } else if meta.is_file() {
            if is_duplicate_clone_file(&path) {
                continue;
            }
            ensure_within(dst_root, &target)?;
            fs::copy(&path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn copy_dir_secure(src: &Path, dst: &Path) -> Result<(), String> {
    let dst_root = dst
        .parent()
        .map(|v| v.to_path_buf())
        .unwrap_or_else(|| dst.to_path_buf());
    copy_dir_secure_internal(src, src, &dst_root, dst)
}

fn replace_dir_atomic(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }
    let parent = dst.parent().ok_or("invalid destination")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let stage = parent.join(format!(".stage-{}", now_ts()));
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    }
    copy_dir_secure_internal(src, src, parent, &stage)?;

    let backup = parent.join(format!(".backup-{}", now_ts()));
    if dst.exists() {
        fs::rename(dst, &backup).map_err(|e| e.to_string())?;
    }
    fs::rename(&stage, dst).map_err(|e| e.to_string())?;
    if backup.exists() {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

fn get_source<'a>(cfg: &'a StorageConfig, source_id: &str) -> Option<&'a SkillSourceConfig> {
    cfg.skills_sources.iter().find(|s| s.id == source_id)
}

fn source_base_dir(source: &SkillSourceConfig) -> String {
    let raw = source.base_dir.clone().unwrap_or_else(|| "/".to_string());
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn source_branch(source: &SkillSourceConfig) -> String {
    let b = source.branch.clone().unwrap_or_else(|| "main".to_string());
    if b.trim().is_empty() {
        "main".to_string()
    } else {
        b
    }
}

fn git_run(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut cmd = crate::get_git_command();
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    // Never block on interactive auth prompts in background sync.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("GIT_ASKPASS", "echo");
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn sync_source_repo(source: &SkillSourceConfig) -> Result<PathBuf, String> {
    let cache_root = skills_cache_root()?;
    let repo_dir = cache_root.join(&source.id);
    let branch = source_branch(source);

    if repo_dir.join(".git").exists() {
        let _ = git_run(
            Some(&repo_dir),
            &["fetch", "--depth", "1", "origin", &branch],
        );
        let _ = git_run(Some(&repo_dir), &["checkout", &branch]);
        let _ = git_run(
            Some(&repo_dir),
            &["reset", "--hard", &format!("origin/{}", branch)],
        );
    } else {
        if repo_dir.exists() {
            fs::remove_dir_all(&repo_dir).map_err(|e| e.to_string())?;
        }
        let repo_dir_str = repo_dir.to_string_lossy().to_string();
        git_run(
            None,
            &[
                "clone",
                "--depth",
                "1",
                "--branch",
                &branch,
                &source.repo_url,
                &repo_dir_str,
            ],
        )?;
    }

    Ok(repo_dir)
}

fn source_scan_root(repo_dir: &Path, source: &SkillSourceConfig) -> Result<PathBuf, String> {
    let base_dir = source_base_dir(source);
    let rel = base_dir.trim_start_matches('/');
    let root = if rel.is_empty() {
        repo_dir.to_path_buf()
    } else {
        repo_dir.join(rel)
    };
    if !root.exists() {
        return Err("skills/source_fetch_failed".to_string());
    }
    Ok(root)
}

fn scan_source_catalog(
    repo_dir: &Path,
    source: &SkillSourceConfig,
) -> Result<Vec<CatalogSkill>, String> {
    let scan_root = source_scan_root(&repo_dir, source)?;
    let mut skill_dirs = vec![];
    find_skill_dirs(&scan_root, &scan_root, &mut skill_dirs)?;
    let mut catalog = vec![];
    for rel in skill_dirs {
        let abs = scan_root.join(&rel);
        let md = abs.join("SKILL.md");
        let md_content = fs::read_to_string(&md).map_err(|e| e.to_string())?;
        // Keep declared/all models in catalog; source allow-list is applied at query time.
        let (name, description, models) = parse_skill_md(&md_content, &[]);
        let rel_str = rel.to_string_lossy().to_string();
        let dir_name = parse_required_skill_dir_name(&md_content)
            .unwrap_or_else(|_| rel_str.rsplit('/').next().unwrap_or_default().to_string());
        let id = safe_slug(&format!("{}-{}", source.id, rel_str));
        let remote_hash = hash_dir(&abs)?;
        catalog.push(CatalogSkill {
            source_id: source.id.clone(),
            id,
            rel_path: rel_str,
            dir_name,
            name,
            description,
            models,
            remote_hash,
            icon_seed: source.id.clone(),
            first_seen_at: None,
        });
    }
    Ok(catalog)
}

fn assign_catalog_first_seen(
    previous_catalog: &[CatalogSkill],
    mut scanned_catalog: Vec<CatalogSkill>,
) -> Vec<CatalogSkill> {
    let previous_map = previous_catalog
        .iter()
        .map(|c| (make_repo_key(&c.source_id, &c.rel_path), c.first_seen_at))
        .collect::<HashMap<_, _>>();
    let now = now_ts();
    for item in &mut scanned_catalog {
        let key = make_repo_key(&item.source_id, &item.rel_path);
        item.first_seen_at = previous_map.get(&key).copied().unwrap_or(Some(now));
    }
    scanned_catalog
}

fn source_skill_abs_path(source: &SkillSourceConfig, rel_path: &str) -> Result<PathBuf, String> {
    let repo_dir = skills_cache_root()?.join(&source.id);
    let root = source_scan_root(&repo_dir, source)?;
    let rel = PathBuf::from(rel_path);
    if has_path_traversal(&rel) {
        return Err("skills/path_out_of_root".to_string());
    }
    let p = root.join(rel);
    ensure_within(&root, &p)?;
    Ok(p)
}

fn read_repository_skill_markdown(repo: &RepositoryRecord, cfg: &StorageConfig) -> Option<String> {
    if let Ok(snapshot) = repo_storage_dir(&repo.repo_key) {
        let md = snapshot.join("SKILL.md");
        if md.exists() {
            if let Ok(content) = fs::read_to_string(&md) {
                return Some(content);
            }
        }
    }

    if let Some(src) = repo.source_path.as_ref() {
        let md = PathBuf::from(src).join("SKILL.md");
        if md.exists() {
            if let Ok(content) = fs::read_to_string(&md) {
                return Some(content);
            }
        }
    }

    if repo.source_type == "remote" {
        if let Some(source) = get_source(cfg, &repo.source_id) {
            if let Ok(path) = source_skill_abs_path(source, &repo.source_rel_path) {
                let md = path.join("SKILL.md");
                if md.exists() {
                    if let Ok(content) = fs::read_to_string(&md) {
                        return Some(content);
                    }
                }
            }
        }
    }

    None
}

fn refresh_repository_metadata_from_snapshots(
    state: &mut SkillsState,
    cfg: &StorageConfig,
) -> bool {
    let mut changed = false;
    for repo in &mut state.repositories {
        if repo.source_type == "remote" {
            continue;
        }
        let Some(markdown) = read_repository_skill_markdown(repo, cfg) else {
            continue;
        };
        let (name, description, _models) = parse_skill_md(&markdown, &[]);
        let parsed_dir_name = parse_required_skill_dir_name(&markdown).ok();
        if repo.name != name
            || repo.description != description
            || parsed_dir_name
                .as_ref()
                .map(|dir| repo.dir_name != *dir)
                .unwrap_or(false)
        {
            repo.name = name;
            repo.description = description;
            if let Some(dir_name) = parsed_dir_name {
                repo.dir_name = dir_name;
            }
            repo.updated_at = Some(now_ts());
            changed = true;
        }
    }
    changed
}
