use crate::subagents::{
    ensure_within, is_duplicate_clone_file, is_ignored_name, is_markdown_file, local_source_id,
    local_subagent_id, make_repo_key, normalize_rel_path, normalized_project_root_value, now_ts,
    parse_required_subagent_dir_name, parse_subagent_md, record_scope, resolve_subagent_target_dir,
    source_entry_markdown_path, LocalSubagentCandidate, RepositoryRecord, SubagentRecord,
    SubagentsLocalState, SubagentsState,
};
use std::fs;
use std::path::{Path, PathBuf};

pub(in crate::subagents) fn read_required_subagent_dir_name(
    subagent_dir: &Path,
) -> Result<String, String> {
    let raw = fs::read_to_string(subagent_dir.join("AGENT.md"))
        .map_err(|_| "subagents/invalid_subagent_dir".to_string())?;
    parse_required_subagent_dir_name(&raw)
}

pub(in crate::subagents) fn normalized_record_dir_name(record: &SubagentRecord) -> String {
    let name = record.dir_name.trim();
    if name.is_empty() {
        record.id.clone()
    } else {
        name.to_string()
    }
}

pub(in crate::subagents) fn normalized_repo_dir_name(repo: &RepositoryRecord) -> String {
    let name = repo.dir_name.trim();
    if name.is_empty() {
        repo.subagent_id.clone()
    } else {
        name.to_string()
    }
}

pub(in crate::subagents) fn record_project_root(record: &SubagentRecord) -> Option<String> {
    record
        .project_root
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(in crate::subagents) fn record_target_root(record: &SubagentRecord) -> Result<PathBuf, String> {
    let scope = record_scope(record);
    let project_root = record_project_root(record);
    let (root, _) = resolve_subagent_target_dir(&record.model, &scope, project_root.as_deref())?;
    Ok(root)
}

pub(in crate::subagents) fn scope_project_match(
    record: &SubagentRecord,
    scope: &str,
    project_root: Option<&str>,
) -> bool {
    if record_scope(record) != scope {
        return false;
    }
    record_project_root(record) == normalized_project_root_value(project_root)
}

pub(in crate::subagents) fn locate_existing_record_local_dir(
    record: &SubagentRecord,
) -> Result<PathBuf, String> {
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

pub(in crate::subagents) fn has_dir_name_conflict(
    state: &SubagentsLocalState,
    model: &str,
    scope: &str,
    project_root: Option<&str>,
    dir_name: &str,
    ignore_subagent_id: Option<&str>,
) -> bool {
    state.subagents.iter().any(|s| {
        if s.model != model {
            return false;
        }
        if record_scope(s) != scope {
            return false;
        }
        if record_project_root(s) != normalized_project_root_value(project_root) {
            return false;
        }
        if ignore_subagent_id.is_some() && ignore_subagent_id == Some(s.id.as_str()) {
            return false;
        }
        normalized_record_dir_name(s) == dir_name
    })
}

pub(in crate::subagents) fn ensure_model_dir_name_available(
    state: &SubagentsLocalState,
    model: &str,
    scope: &str,
    project_root: Option<&str>,
    dir_name: &str,
    ignore_subagent_id: Option<&str>,
) -> Result<(), String> {
    if has_dir_name_conflict(
        state,
        model,
        scope,
        project_root,
        dir_name,
        ignore_subagent_id,
    ) {
        return Err("subagents/dir_name_conflict".to_string());
    }
    let (root, _) = resolve_subagent_target_dir(model, scope, project_root)?;
    let dest = root.join(dir_name);
    ensure_within(&root, &dest)?;
    if dest.exists() {
        if let Some(subagent_id) = ignore_subagent_id {
            if let Some(existing) = state.subagents.iter().find(|s| {
                s.model == model
                    && s.id == subagent_id
                    && record_scope(s) == scope
                    && record_project_root(s) == normalized_project_root_value(project_root)
            }) {
                let existing_path = locate_existing_record_local_dir(existing)?;
                if existing_path == dest {
                    return Ok(());
                }
            }
        }
        return Err("subagents/dir_name_conflict".to_string());
    }
    Ok(())
}

pub(in crate::subagents) fn remove_existing_record_dir_if_moved(
    state: &SubagentsLocalState,
    model: &str,
    scope: &str,
    project_root: Option<&str>,
    subagent_id: &str,
    new_dest: &Path,
) -> Result<(), String> {
    let Some(existing) = state.subagents.iter().find(|s| {
        s.model == model
            && s.id == subagent_id
            && record_scope(s) == scope
            && record_project_root(s) == normalized_project_root_value(project_root)
    }) else {
        return Ok(());
    };
    let old_dir = locate_existing_record_local_dir(existing)?;
    if old_dir != new_dest && old_dir.exists() {
        fs::remove_dir_all(old_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(in crate::subagents) fn upsert_repo_dir_name(
    state: &mut SubagentsState,
    source_id: &str,
    source_rel_path: &str,
    subagent_id: &str,
    dir_name: &str,
) -> bool {
    let repo_key = make_repo_key(source_id, source_rel_path);
    let mut changed = false;
    for repo in &mut state.repositories {
        if repo.repo_key == repo_key || repo.subagent_id == subagent_id {
            if repo.dir_name != dir_name {
                repo.dir_name = dir_name.to_string();
                repo.updated_at = Some(now_ts());
                changed = true;
            }
        }
    }
    changed
}

pub(in crate::subagents) fn parse_frontmatter_value(
    frontmatter: &str,
    key: &str,
) -> Option<String> {
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

pub(in crate::subagents) fn find_subagent_dirs(
    base: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
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
            let subagent_md = path.join("AGENT.md");
            if subagent_md.exists() {
                let rel = path
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_path_buf();
                out.push(rel);
            } else {
                find_subagent_dirs(base, &path, out)?;
            }
        }
    }
    Ok(())
}

pub(in crate::subagents) fn find_local_subagent_dirs(base: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = vec![];
    if base.join("AGENT.md").exists() {
        out.push(PathBuf::from("."));
    }
    find_subagent_dirs(base, base, &mut out)?;
    out.sort_by(|a, b| normalize_rel_path(a).cmp(&normalize_rel_path(b)));
    Ok(out)
}

pub(in crate::subagents) fn find_catalog_entries(
    base: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
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
            let subagent_md = path.join("AGENT.md");
            if subagent_md.exists() {
                let rel = path
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_path_buf();
                out.push(rel);
            } else {
                find_catalog_entries(base, &path, out)?;
            }
            continue;
        }
        if meta.is_file() && is_markdown_file(&path) && !name.eq_ignore_ascii_case("AGENT.md") {
            let rel = path
                .strip_prefix(base)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            out.push(rel);
        }
    }
    Ok(())
}

pub(in crate::subagents) fn find_catalog_subagent_entries(
    base: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut out = vec![];
    if base.join("AGENT.md").exists() {
        out.push(PathBuf::from("."));
    }
    find_catalog_entries(base, base, &mut out)?;
    out.sort_by(|a, b| normalize_rel_path(a).cmp(&normalize_rel_path(b)));
    Ok(out)
}

pub(in crate::subagents) fn scan_local_candidates(
    root_can: &Path,
) -> Result<Vec<LocalSubagentCandidate>, String> {
    let source_id = local_source_id(root_can);
    let subagent_dirs = find_local_subagent_dirs(root_can)?;
    let mut out = vec![];
    for rel in subagent_dirs {
        let rel_str = normalize_rel_path(&rel);
        let abs = if rel_str == "." {
            root_can.to_path_buf()
        } else {
            root_can.join(&rel)
        };
        let md = abs.join("AGENT.md");
        let md_content = fs::read_to_string(&md).map_err(|e| e.to_string())?;
        let (name, description, declared_models) = parse_subagent_md(&md_content, &[]);
        let dir_name = parse_required_subagent_dir_name(&md_content).unwrap_or_default();
        out.push(LocalSubagentCandidate {
            rel_path: rel_str.clone(),
            subagent_id: local_subagent_id(&source_id, &rel_str),
            dir_name,
            source_id: source_id.clone(),
            name,
            description,
            declared_models,
        });
    }
    Ok(out)
}

pub(in crate::subagents) fn copy_dir_secure_internal(
    src_root: &Path,
    src: &Path,
    dst_root: &Path,
    dst: &Path,
) -> Result<(), String> {
    if !src.exists() {
        return Err("subagents/invalid_subagent_dir".to_string());
    }
    let src_root_can =
        fs::canonicalize(src_root).map_err(|_| "subagents/path_out_of_root".to_string())?;
    let src_can = fs::canonicalize(src).map_err(|_| "subagents/path_out_of_root".to_string())?;
    if !src_can.starts_with(&src_root_can) {
        return Err("subagents/path_out_of_root".to_string());
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
pub(in crate::subagents) fn copy_dir_secure(src: &Path, dst: &Path) -> Result<(), String> {
    let dst_root = dst
        .parent()
        .map(|v| v.to_path_buf())
        .unwrap_or_else(|| dst.to_path_buf());
    copy_dir_secure_internal(src, src, &dst_root, dst)
}

pub(in crate::subagents) fn replace_dir_atomic(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err("subagents/invalid_subagent_dir".to_string());
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

pub(in crate::subagents) fn replace_source_entry_atomic(
    src_entry: &Path,
    dst: &Path,
) -> Result<(), String> {
    if src_entry.is_dir() {
        return replace_dir_atomic(src_entry, dst);
    }

    let src_md = source_entry_markdown_path(src_entry)
        .ok_or("subagents/invalid_subagent_dir".to_string())?;
    let parent = dst.parent().ok_or("invalid destination")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let stage = parent.join(format!(".stage-file-{}", now_ts()));
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    fs::copy(&src_md, stage.join("AGENT.md")).map_err(|e| e.to_string())?;

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
