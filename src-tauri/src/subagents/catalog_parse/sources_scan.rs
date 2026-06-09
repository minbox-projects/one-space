use crate::config::{StorageConfig, SubagentSourceConfig};
use crate::subagents::{
    diagnose_frontmatter_name_error, ensure_within, find_catalog_subagent_entries,
    has_path_traversal, hash_source_entry, make_repo_key, normalize_rel_path, now_ts,
    parse_required_subagent_dir_name, parse_subagent_frontmatter_meta, parse_subagent_md,
    read_markdown_from_source_entry, repo_storage_dir, safe_slug, subagents_cache_root,
    CatalogSubagent, RepositoryRecord, SubagentSourceDiagnoseResult,
    SubagentSourceDiagnoseSkippedSample, SubagentsState,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(in crate::subagents) fn get_source<'a>(
    cfg: &'a StorageConfig,
    source_id: &str,
) -> Option<&'a SubagentSourceConfig> {
    cfg.subagents_sources.iter().find(|s| s.id == source_id)
}

pub(in crate::subagents) fn source_base_dir(source: &SubagentSourceConfig) -> String {
    let raw = source.base_dir.clone().unwrap_or_else(|| "/".to_string());
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(in crate::subagents) fn source_branch(source: &SubagentSourceConfig) -> String {
    let b = source.branch.clone().unwrap_or_else(|| "main".to_string());
    if b.trim().is_empty() {
        "main".to_string()
    } else {
        b
    }
}

pub(in crate::subagents) fn git_run(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
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

pub(in crate::subagents) fn sync_source_repo(
    source: &SubagentSourceConfig,
) -> Result<PathBuf, String> {
    let cache_root = subagents_cache_root()?;
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

pub(in crate::subagents) fn source_scan_root(
    repo_dir: &Path,
    source: &SubagentSourceConfig,
) -> Result<PathBuf, String> {
    let base_dir = source_base_dir(source);
    let rel = base_dir.trim_start_matches('/');
    let root = if rel.is_empty() {
        repo_dir.to_path_buf()
    } else {
        repo_dir.join(rel)
    };
    if !root.exists() {
        return Err("subagents/source_fetch_failed".to_string());
    }
    Ok(root)
}

pub(in crate::subagents) fn scan_source_catalog(
    repo_dir: &Path,
    source: &SubagentSourceConfig,
) -> Result<Vec<CatalogSubagent>, String> {
    let (catalog, _) = scan_source_catalog_with_diagnostics(repo_dir, source)?;
    Ok(catalog)
}

pub(in crate::subagents) fn scan_source_catalog_with_diagnostics(
    repo_dir: &Path,
    source: &SubagentSourceConfig,
) -> Result<(Vec<CatalogSubagent>, SubagentSourceDiagnoseResult), String> {
    let scan_root = source_scan_root(&repo_dir, source)?;
    let catalog_entries = find_catalog_subagent_entries(&scan_root)?;
    let mut diagnostics = SubagentSourceDiagnoseResult {
        source_id: source.id.clone(),
        scan_root: scan_root.to_string_lossy().to_string(),
        last_commit_sha: None,
        total_entries: 0,
        accepted_entries: 0,
        skipped_entries: 0,
        skipped_missing_frontmatter: 0,
        skipped_missing_name: 0,
        skipped_invalid_name: 0,
        skipped_read_error: 0,
        skipped_other: 0,
        skipped_samples: vec![],
    };
    let mut catalog = vec![];
    for rel in catalog_entries {
        diagnostics.total_entries = diagnostics.total_entries.saturating_add(1);
        let abs = scan_root.join(&rel);
        let rel_str = normalize_rel_path(&rel);

        let md_content = match read_markdown_from_source_entry(&abs) {
            Ok(content) => content,
            Err(_) => {
                diagnostics.skipped_read_error = diagnostics.skipped_read_error.saturating_add(1);
                diagnostics.skipped_entries = diagnostics.skipped_entries.saturating_add(1);
                if diagnostics.skipped_samples.len() < 12 {
                    diagnostics
                        .skipped_samples
                        .push(SubagentSourceDiagnoseSkippedSample {
                            rel_path: rel_str.clone(),
                            reason: "read_error".to_string(),
                        });
                }
                continue;
            }
        };

        let dir_name = match parse_required_subagent_dir_name(&md_content) {
            Ok(name) => name,
            Err(_) => {
                let reason = diagnose_frontmatter_name_error(&md_content)
                    .unwrap_or_else(|| "missing_frontmatter".to_string());
                match reason.as_str() {
                    "missing_frontmatter" => {
                        diagnostics.skipped_missing_frontmatter =
                            diagnostics.skipped_missing_frontmatter.saturating_add(1);
                    }
                    "missing_name" => {
                        diagnostics.skipped_missing_name =
                            diagnostics.skipped_missing_name.saturating_add(1);
                    }
                    "invalid_name" => {
                        diagnostics.skipped_invalid_name =
                            diagnostics.skipped_invalid_name.saturating_add(1);
                    }
                    _ => {
                        diagnostics.skipped_other = diagnostics.skipped_other.saturating_add(1);
                    }
                }
                diagnostics.skipped_entries = diagnostics.skipped_entries.saturating_add(1);
                if diagnostics.skipped_samples.len() < 12 {
                    diagnostics
                        .skipped_samples
                        .push(SubagentSourceDiagnoseSkippedSample {
                            rel_path: rel_str.clone(),
                            reason,
                        });
                }
                continue;
            }
        };

        // Keep declared/all models in catalog; source allow-list is applied at query time.
        let (name, description, models) = parse_subagent_md(&md_content, &[]);
        let (model, tools) = parse_subagent_frontmatter_meta(&md_content);
        let id = safe_slug(&format!("{}-{}", source.id, rel_str));
        let remote_hash = hash_source_entry(&abs)?;
        catalog.push(CatalogSubagent {
            source_id: source.id.clone(),
            id,
            rel_path: rel_str,
            dir_name,
            name,
            description,
            models,
            model,
            tools,
            remote_hash,
            icon_seed: source.id.clone(),
            first_seen_at: None,
        });
        diagnostics.accepted_entries = diagnostics.accepted_entries.saturating_add(1);
    }
    Ok((catalog, diagnostics))
}

pub(in crate::subagents) fn assign_catalog_first_seen(
    previous_catalog: &[CatalogSubagent],
    mut scanned_catalog: Vec<CatalogSubagent>,
) -> Vec<CatalogSubagent> {
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

pub(in crate::subagents) fn source_subagent_abs_path(
    source: &SubagentSourceConfig,
    rel_path: &str,
) -> Result<PathBuf, String> {
    let repo_dir = subagents_cache_root()?.join(&source.id);
    let root = source_scan_root(&repo_dir, source)?;
    let rel = PathBuf::from(rel_path);
    if has_path_traversal(&rel) {
        return Err("subagents/path_out_of_root".to_string());
    }
    let p = root.join(rel);
    ensure_within(&root, &p)?;
    Ok(p)
}

pub(in crate::subagents) fn read_repository_subagent_markdown(
    repo: &RepositoryRecord,
    cfg: &StorageConfig,
) -> Option<String> {
    if let Ok(snapshot) = repo_storage_dir(&repo.repo_key) {
        let md = snapshot.join("AGENT.md");
        if md.exists() {
            if let Ok(content) = fs::read_to_string(&md) {
                return Some(content);
            }
        }
    }

    if let Some(src) = repo.source_path.as_ref() {
        if let Ok(content) = read_markdown_from_source_entry(&PathBuf::from(src)) {
            return Some(content);
        }
    }

    if repo.source_type == "remote" {
        if let Some(source) = get_source(cfg, &repo.source_id) {
            if let Ok(path) = source_subagent_abs_path(source, &repo.source_rel_path) {
                if let Ok(content) = read_markdown_from_source_entry(&path) {
                    return Some(content);
                }
            }
        }
    }

    None
}

pub(in crate::subagents) fn refresh_repository_metadata_from_snapshots(
    state: &mut SubagentsState,
    cfg: &StorageConfig,
) -> bool {
    let mut changed = false;
    for repo in &mut state.repositories {
        let Some(markdown) = read_repository_subagent_markdown(repo, cfg) else {
            continue;
        };
        let (name, description, _models) = parse_subagent_md(&markdown, &[]);
        let parsed_dir_name = parse_required_subagent_dir_name(&markdown).ok();
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
