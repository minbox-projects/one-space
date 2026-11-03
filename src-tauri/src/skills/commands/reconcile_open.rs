use crate::config::{self};
use crate::skills::{
    acquire_job_key, api_ok, combined_revision, find_current_installed_skill, get_source,
    global_skills_dir, hash_dir, hydrate_local_records_from_catalog, is_ignored_name, job_lock,
    load_local_skills_state, load_skills_state, load_sync_state, local_skill_id, make_repo_key,
    metadata_timestamp, normalize_install_scope, normalize_project_root_for_scope, now_ts,
    parse_required_skill_dir_name, parse_skill_md, read_required_skill_dir_name, record_local_dir,
    record_scope, refresh_repository_record_from_snapshot, replace_dir_atomic,
    repo_index_baseline_dir, repo_storage_dir, resolve_effective_models, save_local_skills_state,
    save_skills_state, scan_project_installed_skills_for_model, snapshot_repository_index_baseline,
    source_skill_abs_path, trigger_storage_sync, update_record_remote_flags,
    upsert_repository_from_dir, upsert_repository_record, ApiOk, CatalogOpenFolderResult,
    CatalogSkillKeyInput, RepositoryRecord, SkillKeyInput, SkillRecord, SkillsLocalState,
    SkillsState, GLOBAL_SKILL_MODEL, INSTALL_SCOPE_GLOBAL, MODELS,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn local_discovery_roots() -> Result<Vec<PathBuf>, String> {
    let home = dirs::home_dir().ok_or("home directory not found")?;
    let candidates = [
        home.join(".agents").join("skills"),
        home.join(".claude").join("skills"),
        home.join(".codex").join("skills"),
        home.join(".gemini").join("config").join("skills"),
        home.join(".config").join("opencode").join("skills"),
    ];
    let mut roots = vec![];
    let mut seen = HashSet::new();
    for path in candidates {
        if !path.is_dir() {
            continue;
        }
        let canonical = fs::canonicalize(&path).unwrap_or(path);
        if seen.insert(canonical.clone()) {
            roots.push(canonical);
        }
    }
    Ok(roots)
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.is_dir() && !meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn scan_immediate_skill_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(root) {
        Ok(value) => value,
        Err(_) => return Ok(vec![]),
    };
    let mut out = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ignored_name(&name) || !is_real_dir(&path) {
            continue;
        }
        if path.join("SKILL.md").is_file() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

struct DiscoveredLocalSkill {
    folder_name: String,
    dir_name: String,
    path: PathBuf,
    name: String,
    description: String,
    models: Vec<String>,
    hash: String,
}

fn discover_local_skills() -> Result<Vec<DiscoveredLocalSkill>, String> {
    let mut out = vec![];
    let mut seen = HashSet::new();
    for root in local_discovery_roots()? {
        for path in scan_immediate_skill_dirs(&root)? {
            let folder_name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default();
            let markdown = fs::read_to_string(path.join("SKILL.md")).unwrap_or_default();
            let Ok(dir_name) = parse_required_skill_dir_name(&markdown) else {
                continue;
            };
            if !seen.insert(dir_name.clone()) {
                continue;
            }
            let (name, description, models) = parse_skill_md(&markdown, &[]);
            let hash = hash_dir(&path)?;
            out.push(DiscoveredLocalSkill {
                folder_name,
                dir_name,
                path,
                name,
                description,
                models,
                hash,
            });
        }
    }
    Ok(out)
}

fn installed_local_skill_folders(state: &SkillsLocalState) -> HashSet<String> {
    state
        .skills
        .iter()
        .map(|skill| {
            skill
                .source_rel_path
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string()
        })
        .filter(|value| !value.is_empty())
        .collect()
}

pub(in crate::skills) fn rebuild_global_installed_from_unified(
    state: &mut SkillsLocalState,
) -> Result<bool, String> {
    let root = global_skills_dir()?;
    let mut records = vec![];
    for path in scan_immediate_skill_dirs(&root)? {
        let folder_name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        let markdown = fs::read_to_string(path.join("SKILL.md")).unwrap_or_default();
        let (name, description, models) = parse_skill_md(&markdown, &[]);
        let existing = state.skills.iter().find(|skill| {
            record_scope(skill) == INSTALL_SCOPE_GLOBAL
                && (skill.dir_name == folder_name
                    || skill.source_rel_path.rsplit('/').next() == Some(folder_name.as_str()))
        });
        records.push(SkillRecord {
            id: existing
                .map(|skill| skill.id.clone())
                .unwrap_or_else(|| local_skill_id("local", &folder_name)),
            dir_name: folder_name.clone(),
            model: GLOBAL_SKILL_MODEL.to_string(),
            models,
            name,
            description,
            source_id: existing
                .map(|skill| skill.source_id.clone())
                .unwrap_or_else(|| "local".to_string()),
            source_rel_path: existing
                .map(|skill| skill.source_rel_path.clone())
                .unwrap_or_else(|| folder_name.clone()),
            installed_at: metadata_timestamp(&path),
            updated_at: Some(metadata_timestamp(&path.join("SKILL.md"))),
            last_synced_at: existing.and_then(|skill| skill.last_synced_at),
            local_hash: hash_dir(&path)?,
            remote_hash: existing.and_then(|skill| skill.remote_hash.clone()),
            has_update: existing.map(|skill| skill.has_update).unwrap_or(false),
            icon_seed: existing
                .map(|skill| skill.icon_seed.clone())
                .unwrap_or_else(|| "local".to_string()),
            scope: INSTALL_SCOPE_GLOBAL.to_string(),
            project_root: None,
            target_path: Some(path.to_string_lossy().to_string()),
        });
    }
    records.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
    let changed = state.skills.len() != records.len()
        || state
            .skills
            .iter()
            .zip(records.iter())
            .any(|(left, right)| {
                left.id != right.id
                    || left.dir_name != right.dir_name
                    || left.local_hash != right.local_hash
                    || left.name != right.name
                    || left.description != right.description
                    || left.models != right.models
                    || left.target_path != right.target_path
            });
    if changed {
        state.skills = records;
        state.last_rescan_at = Some(now_ts());
    }
    Ok(changed)
}

pub(in crate::skills) fn sync_local_repositories(
    shared_state: &mut SkillsState,
    local_state: &SkillsLocalState,
) -> Result<bool, String> {
    let discovered = discover_local_skills()?;
    let installed = installed_local_skill_folders(local_state);
    let remote_names = shared_state
        .repositories
        .iter()
        .filter(|repo| repo.source_type == "remote")
        .map(|repo| {
            let name = repo.dir_name.trim();
            if name.is_empty() {
                repo.skill_id.clone()
            } else {
                name.to_string()
            }
        })
        .collect::<HashSet<_>>();
    let discovered_by_folder = discovered
        .iter()
        .map(|item| (item.folder_name.clone(), item))
        .collect::<HashMap<_, _>>();
    let mut changed = false;

    let stale = shared_state
        .repositories
        .iter()
        .filter(|repo| {
            repo.source_id == "local" && !discovered_by_folder.contains_key(&repo.source_rel_path)
        })
        .map(|repo| repo.repo_key.clone())
        .collect::<Vec<_>>();
    for repo_key in stale {
        if let Some(repo) = shared_state
            .repositories
            .iter()
            .find(|repo| repo.repo_key == repo_key)
        {
            if let Ok(path) = repo_storage_dir(&repo.repo_key) {
                let _ = fs::remove_dir_all(path);
            }
            if let Ok(path) = repo_index_baseline_dir(&repo.repo_key) {
                let _ = fs::remove_dir_all(path);
            }
            shared_state
                .repositories
                .retain(|item| item.repo_key != repo_key);
            changed = true;
        }
    }

    for item in discovered {
        if remote_names.contains(&item.dir_name) {
            let before = shared_state.repositories.len();
            shared_state.repositories.retain(|repo| {
                !(repo.source_id == "local" && repo.source_rel_path == item.folder_name)
            });
            if before != shared_state.repositories.len() {
                changed = true;
            }
            continue;
        }
        let repo_key = make_repo_key("local", &item.folder_name);
        let existing = shared_state
            .repositories
            .iter()
            .find(|repo| repo.repo_key == repo_key)
            .cloned();
        let ever_installed = existing
            .as_ref()
            .map(|repo| repo.ever_installed)
            .unwrap_or(false)
            || installed.contains(&item.folder_name);
        let unchanged = existing
            .as_ref()
            .map(|repo| {
                repo.source_type == "local_import"
                    && repo.dir_name == item.dir_name
                    && repo.source_path.as_deref() == Some(item.path.to_string_lossy().as_ref())
                    && repo.name == item.name
                    && repo.description == item.description
                    && repo.models == item.models
                    && repo.hash.as_deref() == Some(item.hash.as_str())
                    && repo.ever_installed == ever_installed
            })
            .unwrap_or(false);
        if unchanged {
            continue;
        }
        let repo_snapshot = repo_storage_dir(&repo_key)?;
        if repo_snapshot.exists() {
            replace_dir_atomic(&item.path, &repo_snapshot)?;
            snapshot_repository_index_baseline(&repo_key, &repo_snapshot)?;
        }
        let record = RepositoryRecord {
            repo_key: repo_key.clone(),
            skill_id: local_skill_id("local", &item.folder_name),
            dir_name: item.dir_name,
            source_id: "local".to_string(),
            source_rel_path: item.folder_name,
            source_type: "local_import".to_string(),
            source_path: Some(item.path.to_string_lossy().to_string()),
            name: item.name,
            description: item.description,
            models: item.models,
            icon_seed: "local".to_string(),
            hash: Some(item.hash),
            created_at: existing.as_ref().map(|repo| repo.created_at).unwrap_or(0),
            updated_at: Some(now_ts()),
            ever_installed,
        };
        upsert_repository_record(&mut shared_state.repositories, record);
        changed = true;
    }

    Ok(changed)
}

#[tauri::command]
pub async fn skills_sync_local(app: tauri::AppHandle) -> Result<ApiOk<Vec<SkillRecord>>, String> {
    let _job = match acquire_job_key("sync:local")? {
        Some(value) => value,
        None => {
            let state = load_local_skills_state()?;
            return api_ok(state.skills.clone(), state.revision);
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut shared_state = load_skills_state()?;
    let mut local_state = load_local_skills_state()?;
    let mut installed_changed = rebuild_global_installed_from_unified(&mut local_state)?;
    let sync_state = load_sync_state()?;
    let before_sources = local_state
        .skills
        .iter()
        .map(|skill| {
            (
                skill.id.clone(),
                skill.source_id.clone(),
                skill.source_rel_path.clone(),
                skill.remote_hash.clone(),
                skill.has_update,
                skill.icon_seed.clone(),
            )
        })
        .collect::<Vec<_>>();
    hydrate_local_records_from_catalog(&mut local_state, &sync_state);
    update_record_remote_flags(&mut local_state, &sync_state);
    let hydrated_changed = local_state
        .skills
        .iter()
        .map(|skill| {
            (
                skill.id.clone(),
                skill.source_id.clone(),
                skill.source_rel_path.clone(),
                skill.remote_hash.clone(),
                skill.has_update,
                skill.icon_seed.clone(),
            )
        })
        .collect::<Vec<_>>()
        != before_sources;
    installed_changed = installed_changed || hydrated_changed;
    let repositories_changed = sync_local_repositories(&mut shared_state, &local_state)?;

    if installed_changed {
        local_state = save_local_skills_state(local_state)?;
    }
    if repositories_changed {
        shared_state = save_skills_state(shared_state)?;
        trigger_storage_sync(app, "skills_sync_local");
    }
    api_ok(
        local_state.skills.clone(),
        combined_revision(&shared_state, &local_state),
    )
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

pub fn skills_installed_count_all_scopes() -> Result<usize, String> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let root = global_skills_dir()?;
    for path in scan_immediate_skill_dirs(&root)? {
        seen.insert(fs::canonicalize(&path).unwrap_or(path));
    }
    let sync_state = load_sync_state()?;
    let cfg = config::get_storage_config()?;
    for project_root in crate::workspaces::workspace_roots()? {
        for model in MODELS {
            if let Ok(records) =
                scan_project_installed_skills_for_model(model, &project_root, &sync_state, &cfg)
            {
                for record in records {
                    if let Some(target) = record.target_path {
                        let path = PathBuf::from(target);
                        seen.insert(fs::canonicalize(&path).unwrap_or(path));
                    }
                }
            }
        }
    }
    Ok(seen.len())
}
