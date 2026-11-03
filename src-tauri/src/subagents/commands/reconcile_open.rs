use crate::config::{self};
use crate::subagents::{
    acquire_job_key, api_ok, combined_revision, current_installed_subagents, ensure_dir,
    ensure_within, find_current_installed_subagent, find_definition_markdown, get_source, hash_dir,
    hash_source_entry, hydrate_local_records_from_catalog, is_ignored_name, job_lock,
    load_local_subagents_state, load_subagents_state, load_sync_state, local_source_id,
    local_subagent_id, locate_existing_record_local_dir, make_repo_key, mirror_dir, model_dir,
    normalize_install_scope, normalize_project_root_for_scope, normalized_record_dir_name,
    normalized_repo_dir_name, now_ts, parse_required_subagent_dir_name,
    parse_subagent_frontmatter_meta, parse_subagent_md, project_primary_dir,
    prune_codex_project_managed_entries, read_required_subagent_dir_name_from_entry,
    record_local_dir, record_scope, refresh_repository_record_from_snapshot, replace_dir_atomic,
    replace_source_entry_atomic, repo_index_baseline_dir, repo_storage_dir,
    resolve_effective_models, resolve_subagent_target_dir, save_local_subagents_state,
    save_subagents_state, snapshot_repository_index_baseline, source_entry_exists,
    source_entry_markdown_path, source_subagent_abs_path, trigger_storage_sync,
    update_record_remote_flags, upsert_codex_project_agent_entry, upsert_repository_from_dir,
    ApiOk, CatalogOpenFolderResult, CatalogSubagentKeyInput, SubagentKeyInput, SubagentRecord,
    SubagentsLocalState, SubagentsState, INSTALL_SCOPE_GLOBAL, INSTALL_SCOPE_PROJECT, MODELS,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(in crate::subagents) fn reconcile_one_model(
    model: &str,
    scope: &str,
    project_root: Option<&str>,
) -> Result<(), String> {
    if scope == INSTALL_SCOPE_PROJECT {
        let root = project_root.ok_or("subagents/project_root_required")?;
        let project_root_path = PathBuf::from(root);
        let primary = project_primary_dir(model, &project_root_path)?;
        let (_, compat_roots) = resolve_subagent_target_dir(model, scope, Some(root))?;
        for compat in compat_roots {
            ensure_dir(&compat)?;
            let mut primary_map: HashMap<String, PathBuf> = HashMap::new();
            for entry in fs::read_dir(&primary).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let p = entry.path();
                if p.is_dir() {
                    primary_map.insert(entry.file_name().to_string_lossy().to_string(), p);
                }
            }
            let mut compat_names = HashSet::new();
            for entry in fs::read_dir(&compat).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let p = entry.path();
                if p.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    compat_names.insert(name.clone());
                    if let Some(src) = primary_map.get(&name) {
                        let dst = compat.join(&name);
                        if hash_dir(src)? != hash_dir(&dst)? {
                            replace_dir_atomic(src, &dst)?;
                        }
                    } else {
                        fs::remove_dir_all(p).map_err(|e| e.to_string())?;
                    }
                }
            }
            for (name, src) in primary_map {
                if !compat_names.contains(&name) {
                    replace_dir_atomic(&src, &compat.join(name))?;
                }
            }
        }

        if model == "codex" {
            let local_state = load_local_subagents_state()?;
            let sync_state = load_sync_state()?;
            let cfg = config::get_storage_config()?;
            let scanned = current_installed_subagents(
                &local_state,
                &sync_state,
                &cfg,
                Some("codex"),
                INSTALL_SCOPE_PROJECT,
                Some(root),
            )?;
            let mut keep_dir_names = HashSet::new();
            for record in &scanned {
                let dir_name = normalized_record_dir_name(record);
                keep_dir_names.insert(dir_name.clone());
                let local_dir = locate_existing_record_local_dir(record)?;
                let Some(markdown_path) = find_definition_markdown(&local_dir) else {
                    continue;
                };
                let markdown = fs::read_to_string(&markdown_path).unwrap_or_default();
                let (meta_model, meta_tools) = parse_subagent_frontmatter_meta(&markdown);
                let display_name = if record.name.trim().is_empty() {
                    dir_name.clone()
                } else {
                    record.name.clone()
                };
                upsert_codex_project_agent_entry(
                    root,
                    &dir_name,
                    &display_name,
                    meta_model.as_deref(),
                    &meta_tools,
                    &markdown,
                )?;
            }
            prune_codex_project_managed_entries(root, &keep_dir_names)?;
        }
        return Ok(());
    }

    let sot = model_dir(model)?;
    let mirror = mirror_dir(model)?;

    let mut sot_map: HashMap<String, PathBuf> = HashMap::new();
    for entry in fs::read_dir(&sot).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            sot_map.insert(name, p);
        }
    }

    let mut mirror_names = HashSet::new();
    for entry in fs::read_dir(&mirror).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            mirror_names.insert(name.clone());
            if let Some(src) = sot_map.get(&name) {
                let dst = mirror.join(&name);
                if hash_dir(src)? != hash_dir(&dst)? {
                    replace_dir_atomic(src, &dst)?;
                }
            } else {
                fs::remove_dir_all(p).map_err(|e| e.to_string())?;
            }
        }
    }

    for (name, src) in sot_map {
        if !mirror_names.contains(&name) {
            let dst = mirror.join(name);
            replace_dir_atomic(&src, &dst)?;
        }
    }

    Ok(())
}

pub(in crate::subagents) fn reconcile_internal(
    model: Option<&str>,
    scope: Option<&str>,
    project_root: Option<&str>,
) -> Result<(), String> {
    let target_scope = normalize_install_scope(scope);
    match model {
        Some(m) => reconcile_one_model(m, &target_scope, project_root),
        None => {
            for m in MODELS {
                let _ = reconcile_one_model(m, &target_scope, project_root);
            }
            Ok(())
        }
    }
}

pub(in crate::subagents) fn rebuild_local_installed_from_models(
    state: &mut SubagentsLocalState,
) -> Result<(), String> {
    let mut existing = HashSet::new();
    for model in MODELS {
        let root = model_dir(model)?;
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let Some(md) = find_definition_markdown(&p) else {
                continue;
            };
            let content = fs::read_to_string(&md).unwrap_or_default();
            let (name, desc, models) = parse_subagent_md(&content, &[]);
            let hash = hash_dir(&p)?;
            existing.insert((model.to_string(), dir_name.clone()));

            if let Some(record) = state.subagents.iter_mut().find(|s| {
                s.model == model
                    && record_scope(s) == INSTALL_SCOPE_GLOBAL
                    && normalized_record_dir_name(s) == dir_name
            }) {
                record.dir_name = dir_name.clone();
                record.name = name.clone();
                record.description = desc.clone();
                record.models = models.clone();
                record.local_hash = hash.clone();
                record.has_update = false;
                record.scope = INSTALL_SCOPE_GLOBAL.to_string();
                record.project_root = None;
                record.target_path = Some(p.to_string_lossy().to_string());
            } else {
                state.subagents.push(SubagentRecord {
                    id: dir_name.clone(),
                    dir_name: dir_name.clone(),
                    model: model.to_string(),
                    models: models.clone(),
                    name: name.clone(),
                    description: desc.clone(),
                    source_id: "local".to_string(),
                    source_rel_path: dir_name.clone(),
                    installed_at: now_ts(),
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: hash.clone(),
                    remote_hash: None,
                    has_update: false,
                    icon_seed: dir_name.clone(),
                    scope: INSTALL_SCOPE_GLOBAL.to_string(),
                    project_root: None,
                    target_path: Some(p.to_string_lossy().to_string()),
                });
            }
        }
    }

    state.subagents.retain(|s| {
        if record_scope(s) != INSTALL_SCOPE_GLOBAL {
            return true;
        }
        existing.contains(&(s.model.clone(), normalized_record_dir_name(s)))
    });
    state.last_rescan_at = Some(now_ts());
    Ok(())
}

struct DiscoveredLocalSubagent {
    path: PathBuf,
    dir_name: String,
    name: String,
    description: String,
    models: Vec<String>,
    model: Option<String>,
    tools: Vec<String>,
    hash: String,
}

fn local_subagent_scan_roots() -> Result<Vec<PathBuf>, String> {
    let mut roots = vec![];
    let mut seen = HashSet::new();
    for model in MODELS {
        let root = mirror_dir(model)?;
        let canonical = fs::canonicalize(&root).unwrap_or(root);
        if seen.insert(canonical.clone()) {
            roots.push(canonical);
        }
    }
    Ok(roots)
}

fn scan_local_subagents_in_root(root: &Path) -> Result<Vec<DiscoveredLocalSubagent>, String> {
    let entries = fs::read_dir(root).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().to_string();
        if is_ignored_name(&entry_name) {
            continue;
        }
        let Some(markdown_path) = source_entry_markdown_path(&path) else {
            continue;
        };
        let markdown = fs::read_to_string(&markdown_path).unwrap_or_default();
        let Ok(dir_name) = parse_required_subagent_dir_name(&markdown) else {
            continue;
        };
        let (name, description, models) = parse_subagent_md(&markdown, &[]);
        let (model, tools) = parse_subagent_frontmatter_meta(&markdown);
        let hash = hash_source_entry(&path)?;
        out.push(DiscoveredLocalSubagent {
            path: fs::canonicalize(&path).unwrap_or(path),
            dir_name,
            name,
            description,
            models,
            model,
            tools,
            hash,
        });
    }
    Ok(out)
}

fn discover_local_subagents() -> Result<(Vec<PathBuf>, Vec<DiscoveredLocalSubagent>), String> {
    let roots = local_subagent_scan_roots()?;
    let mut seen = HashSet::new();
    let mut out = vec![];
    for root in &roots {
        for item in scan_local_subagents_in_root(root)? {
            if seen.insert(item.dir_name.clone()) {
                out.push(item);
            }
        }
    }
    out.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
    Ok((roots, out))
}

fn import_local_subagents_into_model_dirs() -> Result<(), String> {
    for model in MODELS {
        let mirror_root = mirror_dir(model)?;
        let model_root = model_dir(model)?;
        for item in scan_local_subagents_in_root(&mirror_root)? {
            let dest = model_root.join(&item.dir_name);
            ensure_within(&model_root, &dest)?;
            replace_source_entry_atomic(&item.path, &dest)?;
        }
    }
    Ok(())
}

fn sync_local_repositories(
    shared_state: &mut SubagentsState,
    discovered: &[DiscoveredLocalSubagent],
    roots: &[PathBuf],
) -> Result<bool, String> {
    let remote_names = shared_state
        .repositories
        .iter()
        .filter(|repo| repo.source_type == "remote")
        .map(normalized_repo_dir_name)
        .collect::<HashSet<_>>();
    let mut changed = false;

    let stale_keys = shared_state
        .repositories
        .iter()
        .filter(|repo| {
            if repo.source_type != "local_import" {
                return false;
            }
            let Some(source_path) = repo.source_path.as_deref() else {
                return false;
            };
            let source_path = Path::new(source_path);
            roots.iter().any(|root| source_path.starts_with(root)) && !source_path.exists()
        })
        .map(|repo| repo.repo_key.clone())
        .collect::<Vec<_>>();
    for repo_key in stale_keys {
        if let Ok(path) = repo_storage_dir(&repo_key) {
            if path.exists() {
                let _ = fs::remove_dir_all(path);
            }
        }
        if let Ok(path) = repo_index_baseline_dir(&repo_key) {
            if path.exists() {
                let _ = fs::remove_dir_all(path);
            }
        }
        let before = shared_state.repositories.len();
        shared_state
            .repositories
            .retain(|repo| repo.repo_key != repo_key);
        if shared_state.repositories.len() != before {
            changed = true;
        }
    }

    for item in discovered {
        if remote_names.contains(&item.dir_name) {
            continue;
        }
        let item_path = item.path.to_string_lossy().to_string();
        if shared_state.repositories.iter().any(|repo| {
            repo.source_type == "local_import"
                && normalized_repo_dir_name(repo) == item.dir_name
                && repo.source_path.as_deref() != Some(item_path.as_str())
        }) {
            continue;
        }
        let source_id = local_source_id(&item.path);
        let repo_key = make_repo_key(&source_id, ".");
        let unchanged = shared_state
            .repositories
            .iter()
            .find(|repo| repo.repo_key == repo_key)
            .map(|repo| {
                repo.source_type == "local_import"
                    && repo.dir_name == item.dir_name
                    && repo.name == item.name
                    && repo.description == item.description
                    && repo.models == item.models
                    && repo.model == item.model
                    && repo.tools == item.tools
                    && repo.source_path.as_deref() == Some(item_path.as_str())
                    && repo.hash.as_deref() == Some(item.hash.as_str())
                    && repo.ever_installed
            })
            .unwrap_or(false);
        if unchanged {
            continue;
        }
        let subagent_id = local_subagent_id(&source_id, ".");
        upsert_repository_from_dir(
            shared_state,
            &item.path,
            &source_id,
            ".",
            &subagent_id,
            &item.dir_name,
            "local_import",
            &item.name,
            &item.description,
            &item.models,
            item.model.as_deref(),
            &item.tools,
            &source_id,
            Some(item_path),
            Some(item.hash.clone()),
            true,
        )?;
        if let Some(repo) = shared_state
            .repositories
            .iter_mut()
            .find(|repo| repo.repo_key == repo_key)
        {
            if repo.hash.as_deref() != Some(item.hash.as_str()) {
                repo.hash = Some(item.hash.clone());
                repo.updated_at = Some(now_ts());
            }
        }
        changed = true;
    }

    Ok(changed)
}

pub(in crate::subagents) fn subagents_sync_local_blocking(
) -> Result<(SubagentsState, SubagentsLocalState, bool), String> {
    let (roots, discovered) = discover_local_subagents()?;
    import_local_subagents_into_model_dirs()?;

    let mut shared_state = load_subagents_state()?;
    let mut local_state = load_local_subagents_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;

    rebuild_local_installed_from_models(&mut local_state)?;
    let repositories_changed = sync_local_repositories(&mut shared_state, &discovered, &roots)?;
    hydrate_local_records_from_catalog(&mut local_state, &sync_state);
    update_record_remote_flags(&mut local_state, &sync_state, &cfg);
    local_state = save_local_subagents_state(local_state)?;
    if repositories_changed {
        shared_state = save_subagents_state(shared_state)?;
    }

    Ok((shared_state, local_state, repositories_changed))
}

#[tauri::command]
pub async fn subagents_sync_local(
    app: tauri::AppHandle,
) -> Result<ApiOk<Vec<SubagentRecord>>, String> {
    let _job = match acquire_job_key("sync:local")? {
        Some(value) => value,
        None => {
            let local_state = load_local_subagents_state()?;
            let shared_state = load_subagents_state()?;
            return api_ok(
                local_state.subagents.clone(),
                combined_revision(&shared_state, &local_state),
            );
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let (shared_state, local_state, repositories_changed) = subagents_sync_local_blocking()?;
    if repositories_changed {
        trigger_storage_sync(app, "subagents_sync_local");
    }

    api_ok(
        local_state.subagents.clone(),
        combined_revision(&shared_state, &local_state),
    )
}

#[tauri::command]
pub async fn subagents_reconcile(
    _app: tauri::AppHandle,
    model: Option<String>,
    scope: Option<String>,
    project_root: Option<String>,
) -> Result<ApiOk<bool>, String> {
    let target_scope = normalize_install_scope(scope.as_deref());
    let target_project_root =
        normalize_project_root_for_scope(&target_scope, project_root.as_deref())?;
    let dedupe_key = format!(
        "reconcile:{}:{}:{}",
        model.clone().unwrap_or_else(|| "all".to_string()),
        target_scope,
        target_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let state = load_local_subagents_state()?;
            return api_ok(true, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    reconcile_internal(
        model.as_deref(),
        Some(target_scope.as_str()),
        target_project_root.as_deref(),
    )
    .map_err(|_| "subagents/mirror_apply_failed".to_string())?;
    let state = load_local_subagents_state()?;
    api_ok(true, state.revision)
}

pub(in crate::subagents) fn open_folder_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn subagents_catalog_open_folder(
    app: tauri::AppHandle,
    input: CatalogSubagentKeyInput,
) -> Result<ApiOk<CatalogOpenFolderResult>, String> {
    let dedupe_key = format!(
        "catalog_open_folder:{}:{}",
        input.source_id, input.subagent_ref
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_subagents_state()?;
            let local_state = load_local_subagents_state()?;
            let result = CatalogOpenFolderResult {
                repo_key: make_repo_key(&input.source_id, &input.subagent_ref),
                opened_path: String::new(),
            };
            return api_ok(result, combined_revision(&shared_state, &local_state));
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut shared_state = load_subagents_state()?;
    let local_state = load_local_subagents_state()?;
    let mut shared_changed = false;

    let existing_repo = shared_state
        .repositories
        .iter()
        .find(|r| {
            r.source_id == input.source_id
                && (r.source_rel_path == input.subagent_ref || r.subagent_id == input.subagent_ref)
        })
        .cloned();

    let (repo_key, _repo_snapshot) = if let Some(repo) = existing_repo {
        let repo_key = repo.repo_key.clone();
        let repo_snapshot = repo_storage_dir(&repo_key)?;
        if !repo_snapshot.exists() {
            let mut materialized = false;

            if let Some(src) = repo.source_path.as_ref() {
                let src_path = PathBuf::from(src);
                if source_entry_exists(&src_path) {
                    replace_source_entry_atomic(&src_path, &repo_snapshot)?;
                    snapshot_repository_index_baseline(&repo_key, &repo_snapshot)?;
                    materialized = true;
                }
            }

            if !materialized {
                if let Some(local_record) = local_state.subagents.iter().find(|s| {
                    s.source_id == repo.source_id
                        && (s.source_rel_path == repo.source_rel_path || s.id == repo.subagent_id)
                }) {
                    let local_dir = record_local_dir(local_record)?;
                    if find_definition_markdown(&local_dir).is_some() {
                        replace_dir_atomic(&local_dir, &repo_snapshot)?;
                        snapshot_repository_index_baseline(&repo_key, &repo_snapshot)?;
                        materialized = true;
                    }
                }
            }

            if !materialized && repo.source_type == "remote" {
                if let Ok(cfg) = config::get_storage_config() {
                    if let Some(source) = get_source(&cfg, &repo.source_id) {
                        if let Ok(source_path) =
                            source_subagent_abs_path(source, &repo.source_rel_path)
                        {
                            if source_entry_exists(&source_path) {
                                replace_source_entry_atomic(&source_path, &repo_snapshot)?;
                                snapshot_repository_index_baseline(&repo_key, &repo_snapshot)?;
                                materialized = true;
                            }
                        }
                    }
                }
            }

            if materialized {
                if let Some(repo_mut) = shared_state
                    .repositories
                    .iter_mut()
                    .find(|r| r.repo_key == repo_key)
                {
                    refresh_repository_record_from_snapshot(repo_mut)?;
                    shared_changed = true;
                }
            } else {
                return Err("repository_snapshot_missing".to_string());
            }
        }
        (repo_key, repo_snapshot)
    } else {
        let cfg = config::get_storage_config()?;
        let source = get_source(&cfg, &input.source_id).ok_or("source not found")?;
        let sync_state = load_sync_state()?;
        let catalog = sync_state
            .catalog
            .iter()
            .find(|c| {
                c.source_id == input.source_id
                    && (c.rel_path == input.subagent_ref || c.id == input.subagent_ref)
            })
            .cloned()
            .ok_or("catalog subagent not found")?;
        let effective_models = resolve_effective_models(&catalog.models, &source.default_models);
        if effective_models.is_empty() {
            return Err("catalog subagent not found".to_string());
        }
        let src = source_subagent_abs_path(source, &catalog.rel_path)?;
        if !source_entry_exists(&src) {
            return Err("subagents/invalid_subagent_dir".to_string());
        }
        let catalog_dir_name =
            read_required_subagent_dir_name_from_entry(&src).unwrap_or_else(|_| catalog.id.clone());
        let repo_key = make_repo_key(&catalog.source_id, &catalog.rel_path);
        let repo_snapshot = repo_storage_dir(&repo_key)?;
        if !repo_snapshot.exists() {
            let _ = upsert_repository_from_dir(
                &mut shared_state,
                &src,
                &catalog.source_id,
                &catalog.rel_path,
                &catalog.id,
                &catalog_dir_name,
                "remote",
                &catalog.name,
                &catalog.description,
                &effective_models,
                catalog.model.as_deref(),
                &catalog.tools,
                &catalog.icon_seed,
                Some(src.to_string_lossy().to_string()),
                Some(catalog.remote_hash.clone()),
                false,
            )?;
            shared_changed = true;
        }
        (repo_key, repo_snapshot)
    };

    if shared_changed {
        shared_state = save_subagents_state(shared_state)?;
        trigger_storage_sync(app, "subagents_catalog_open_folder");
    }

    let open_path = repo_storage_dir(&repo_key)?;
    open_folder_path(&open_path)?;
    let result = CatalogOpenFolderResult {
        repo_key,
        opened_path: open_path.to_string_lossy().to_string(),
    };
    api_ok(result, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub fn subagents_open_folder(input: SubagentKeyInput) -> Result<ApiOk<bool>, String> {
    let open_scope = normalize_install_scope(input.scope.as_deref());
    let open_project_root =
        normalize_project_root_for_scope(&open_scope, input.project_root.as_deref())?;
    let state = load_local_subagents_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let subagent = find_current_installed_subagent(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.subagent_id,
        &open_scope,
        open_project_root.as_deref(),
    )?;
    let path = record_local_dir(&subagent)?;
    open_folder_path(&path)?;

    api_ok(true, state.revision)
}

pub fn subagents_reconcile_for_tool(
    tool: &str,
    scope: Option<&str>,
    project_root: Option<&str>,
) -> Result<(), String> {
    if !MODELS.contains(&tool) {
        return Ok(());
    }
    let normalized_scope = normalize_install_scope(scope);
    let normalized_project_root =
        normalize_project_root_for_scope(&normalized_scope, project_root)?;
    let key = format!(
        "reconcile:{}:{}:{}",
        tool,
        normalized_scope,
        normalized_project_root.clone().unwrap_or_default()
    );
    let _job = match acquire_job_key(key)? {
        Some(v) => v,
        None => return Ok(()),
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    reconcile_internal(
        Some(tool),
        Some(normalized_scope.as_str()),
        normalized_project_root.as_deref(),
    )
    .map_err(|_| "subagents/mirror_apply_failed".to_string())
}

pub fn subagents_installed_asset_count_all_scopes() -> Result<usize, String> {
    let state = load_local_subagents_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let mut unique = state
        .subagents
        .iter()
        .cloned()
        .map(|item| format!("{}::{}::{}", item.source_id, item.source_rel_path, item.id))
        .collect::<HashSet<_>>();
    for project_root in crate::workspaces::workspace_roots()? {
        for item in current_installed_subagents(
            &state,
            &sync_state,
            &cfg,
            None,
            INSTALL_SCOPE_PROJECT,
            Some(project_root.as_str()),
        )? {
            unique.insert(format!(
                "{}::{}::{}",
                item.source_id, item.source_rel_path, item.id
            ));
        }
    }
    Ok(unique.len())
}
