use crate::config::{self};
use crate::skills::{
    acquire_job_key, api_ok, combined_revision, ensure_dir, ensure_within,
    find_current_installed_skill, get_source, hash_dir, job_lock, load_local_skills_state,
    load_skills_state, load_sync_state, local_skill_id, make_repo_key, mirror_dir, model_dir,
    normalize_install_scope, normalize_project_root_for_scope, normalized_record_dir_name, now_ts,
    parse_required_skill_dir_name, parse_skill_md, project_compat_dirs, project_primary_dir,
    read_required_skill_dir_name, record_local_dir, record_scope,
    refresh_repository_record_from_snapshot, replace_dir_atomic, repo_storage_dir,
    resolve_effective_models, save_local_skills_state, save_skills_state,
    scan_project_installed_skills_for_model, snapshot_repository_index_baseline,
    source_skill_abs_path, trigger_storage_sync, upsert_repository_from_dir, ApiOk,
    CatalogOpenFolderResult, CatalogSkillKeyInput, RepositoryRecord, SkillKeyInput, SkillRecord,
    SkillsLocalState, INSTALL_SCOPE_GLOBAL, INSTALL_SCOPE_PROJECT, MODELS,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(in crate::skills) fn reconcile_one_model(
    model: &str,
    scope: &str,
    project_root: Option<&str>,
) -> Result<(), String> {
    if scope == INSTALL_SCOPE_PROJECT {
        let root = project_root.ok_or("skills/project_root_required")?;
        let project_root_path = PathBuf::from(root);
        let primary = project_primary_dir(model, &project_root_path)?;
        for compat in project_compat_dirs(model, &project_root_path) {
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

pub(in crate::skills) fn reconcile_internal(
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

pub(in crate::skills) fn rebuild_local_installed_from_models(
    state: &mut SkillsLocalState,
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
            let md = p.join("SKILL.md");
            if !md.exists() {
                continue;
            }
            let content = fs::read_to_string(&md).unwrap_or_default();
            let (name, desc, models) = parse_skill_md(&content, &[]);
            let hash = hash_dir(&p)?;
            existing.insert((model.to_string(), dir_name.clone()));

            if let Some(record) = state.skills.iter_mut().find(|s| {
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
                state.skills.push(SkillRecord {
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

    state.skills.retain(|s| {
        if record_scope(s) != INSTALL_SCOPE_GLOBAL {
            return true;
        }
        existing.contains(&(s.model.clone(), normalized_record_dir_name(s)))
    });
    state.last_rescan_at = Some(now_ts());
    Ok(())
}

#[tauri::command]
pub async fn skills_reconcile(
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
            let state = load_local_skills_state()?;
            return api_ok(true, state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    reconcile_internal(
        model.as_deref(),
        Some(target_scope.as_str()),
        target_project_root.as_deref(),
    )
    .map_err(|_| "skills/mirror_apply_failed".to_string())?;
    let state = load_local_skills_state()?;
    api_ok(true, state.revision)
}

#[tauri::command]
pub async fn skills_rescan_local(
    _app: tauri::AppHandle,
) -> Result<ApiOk<Vec<SkillRecord>>, String> {
    let _job = match acquire_job_key("rescan:local")? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            return api_ok(state.skills.clone(), state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;
    let mut state = load_local_skills_state()?;
    rebuild_local_installed_from_models(&mut state)?;
    let state = save_local_skills_state(state)?;
    api_ok(state.skills.clone(), state.revision)
}

#[tauri::command]
pub async fn skills_rescan_mirror(
    _app: tauri::AppHandle,
) -> Result<ApiOk<Vec<SkillRecord>>, String> {
    let _job = match acquire_job_key("rescan:mirror")? {
        Some(v) => v,
        None => {
            let state = load_local_skills_state()?;
            return api_ok(state.skills.clone(), state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    for model in MODELS {
        if let Ok(mirror_root) = mirror_dir(model) {
            if let Ok(model_root) = model_dir(model) {
                if let Ok(entries) = fs::read_dir(&mirror_root) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if !p.is_dir() {
                            continue;
                        }
                        let id = entry.file_name().to_string_lossy().to_string();
                        let md = p.join("SKILL.md");
                        if !md.exists() {
                            continue;
                        }
                        let sot_dir = model_root.join(&id);
                        if let Ok(()) = ensure_within(&model_root, &sot_dir) {
                            let _ = replace_dir_atomic(&p, &sot_dir);
                        }
                    }
                }
            }
        }
    }

    // 关键修复：同步仓库记录
    let mut state = load_skills_state()?;

    for model in MODELS {
        let root = model_dir(model)?;
        let entries = fs::read_dir(&root).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let md = p.join("SKILL.md");
            if !md.exists() {
                continue;
            }
            let content = fs::read_to_string(&md).unwrap_or_default();
            let (name, desc, models) = parse_skill_md(&content, &[]);
            let dir_name = parse_required_skill_dir_name(&content)
                .unwrap_or_else(|_| entry.file_name().to_string_lossy().to_string());

            // 尝试匹配或创建仓库记录
            let source_id = "local".to_string(); // 本地扫描的统一标识
            let rel_path = entry.file_name().to_string_lossy().to_string();
            let repo_key = make_repo_key(&source_id, &rel_path);

            if !state.repositories.iter().any(|r| r.repo_key == repo_key) {
                state.repositories.push(RepositoryRecord {
                    repo_key,
                    skill_id: local_skill_id(&source_id, &rel_path),
                    dir_name,
                    source_id,
                    source_rel_path: rel_path,
                    source_type: "local_import".to_string(),
                    source_path: Some(p.to_string_lossy().to_string()),
                    name,
                    description: desc,
                    models,
                    icon_seed: "local".to_string(),
                    hash: Some(hash_dir(&p)?),
                    created_at: now_ts(),
                    updated_at: Some(now_ts()),
                    ever_installed: true,
                });
            }
        }
    }

    save_skills_state(state)?;

    let mut local_state = load_local_skills_state()?;
    rebuild_local_installed_from_models(&mut local_state)?;
    let local_state = save_local_skills_state(local_state)?;
    api_ok(local_state.skills.clone(), local_state.revision)
}

pub(in crate::skills) fn open_folder_path(path: &Path) -> Result<(), String> {
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
pub async fn skills_catalog_open_folder(
    app: tauri::AppHandle,
    input: CatalogSkillKeyInput,
) -> Result<ApiOk<CatalogOpenFolderResult>, String> {
    let dedupe_key = format!(
        "catalog_open_folder:{}:{}",
        input.source_id, input.skill_ref
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            let result = CatalogOpenFolderResult {
                repo_key: make_repo_key(&input.source_id, &input.skill_ref),
                opened_path: String::new(),
            };
            return api_ok(result, combined_revision(&shared_state, &local_state));
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let mut shared_changed = false;

    let existing_repo = shared_state
        .repositories
        .iter()
        .find(|r| {
            r.source_id == input.source_id
                && (r.source_rel_path == input.skill_ref || r.skill_id == input.skill_ref)
        })
        .cloned();

    let (repo_key, _repo_snapshot) = if let Some(repo) = existing_repo {
        let repo_key = repo.repo_key.clone();
        let repo_snapshot = repo_storage_dir(&repo_key)?;
        if !repo_snapshot.exists() {
            let mut materialized = false;

            if let Some(src) = repo.source_path.as_ref() {
                let src_path = PathBuf::from(src);
                if src_path.join("SKILL.md").exists() {
                    replace_dir_atomic(&src_path, &repo_snapshot)?;
                    snapshot_repository_index_baseline(&repo_key, &repo_snapshot)?;
                    materialized = true;
                }
            }

            if !materialized {
                if let Some(local_record) = local_state.skills.iter().find(|s| {
                    s.source_id == repo.source_id
                        && (s.source_rel_path == repo.source_rel_path || s.id == repo.skill_id)
                }) {
                    let local_dir = record_local_dir(local_record)?;
                    if local_dir.join("SKILL.md").exists() {
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
                            source_skill_abs_path(source, &repo.source_rel_path)
                        {
                            if source_path.join("SKILL.md").exists() {
                                replace_dir_atomic(&source_path, &repo_snapshot)?;
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
                    && (c.rel_path == input.skill_ref || c.id == input.skill_ref)
            })
            .cloned()
            .ok_or("catalog skill not found")?;
        let effective_models = resolve_effective_models(&catalog.models, &source.default_models);
        if effective_models.is_empty() {
            return Err("catalog skill not found".to_string());
        }
        let src = source_skill_abs_path(source, &catalog.rel_path)?;
        if !src.join("SKILL.md").exists() {
            return Err("skills/invalid_skill_dir".to_string());
        }
        let catalog_dir_name =
            read_required_skill_dir_name(&src).unwrap_or_else(|_| catalog.id.clone());
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
        shared_state = save_skills_state(shared_state)?;
        trigger_storage_sync(app, "skills_catalog_open_folder");
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
pub fn skills_open_folder(input: SkillKeyInput) -> Result<ApiOk<bool>, String> {
    let open_scope = normalize_install_scope(input.scope.as_deref());
    let open_project_root =
        normalize_project_root_for_scope(&open_scope, input.project_root.as_deref())?;
    let state = load_local_skills_state()?;
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    let skill = find_current_installed_skill(
        &state,
        &sync_state,
        &cfg,
        &input.model,
        &input.skill_id,
        &open_scope,
        open_project_root.as_deref(),
    )?;
    let path = record_local_dir(&skill)?;
    open_folder_path(&path)?;

    api_ok(true, state.revision)
}

pub fn skills_reconcile_for_tool(
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
    .map_err(|_| "skills/mirror_apply_failed".to_string())
}

pub fn skills_installed_count_all_scopes() -> Result<usize, String> {
    let mut total = 0usize;
    for model in MODELS {
        if let Ok(root) = model_dir(model) {
            if let Ok(entries) = fs::read_dir(&root) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() && entry.path().join("SKILL.md").exists() {
                        total += 1;
                    }
                }
            }
        }
    }
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    for project_root in crate::workspaces::workspace_roots()? {
        for model in MODELS {
            total +=
                scan_project_installed_skills_for_model(model, &project_root, &sync_state, &cfg)
                    .map(|v| v.len())
                    .unwrap_or(0);
        }
    }
    Ok(total)
}
