use super::{
    ensure_repositories_migrated, record_scope, replace_dir_atomic, ApiMeta, ApiOk,
    RepositoryRecord, SubagentsLocalState, SubagentsState, SubagentsSyncState,
    CODEX_ONESPACE_DIR_KEY, CODEX_ONESPACE_MANAGED_KEY, IGNORE_NAMES, INSTALL_SCOPE_GLOBAL,
    INSTALL_SCOPE_PROJECT, MODELS,
};
use crate::config::{self};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use toml_edit::{self, DocumentMut, Item, Table};

pub(in crate::subagents) fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(in crate::subagents) fn bool_true() -> bool {
    true
}

pub(in crate::subagents) fn subagents_root() -> Result<PathBuf, String> {
    let p = crate::get_data_dir()?.join("data").join("subagents");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn subagents_local_root() -> Result<PathBuf, String> {
    let p = subagents_local_cache_base_root()?.join("local_state");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn subagents_models_root() -> Result<PathBuf, String> {
    let p = subagents_local_root()?.join("models");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn subagents_meta_root() -> Result<PathBuf, String> {
    let p = subagents_root()?.join("meta");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn subagents_local_cache_base_root() -> Result<PathBuf, String> {
    let p = if let Some(home) = dirs::home_dir() {
        home.join(".config").join("onespace").join("subagents")
    } else {
        // Fallback to app-local config directory if home is unavailable.
        config::get_app_dir()?.join("subagents")
    };
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn subagents_cache_root() -> Result<PathBuf, String> {
    // Remote git source caches are always local to reduce iCloud/git sync pressure.
    let p = subagents_local_cache_base_root()?.join("remote_cache");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn subagents_state_path() -> Result<PathBuf, String> {
    Ok(subagents_meta_root()?.join("state.json"))
}

pub(in crate::subagents) fn subagents_local_meta_root() -> Result<PathBuf, String> {
    let p = subagents_local_root()?.join("meta");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn subagents_local_state_path() -> Result<PathBuf, String> {
    Ok(subagents_local_meta_root()?.join("installed_state.json"))
}

pub(in crate::subagents) fn sync_state_path() -> Result<PathBuf, String> {
    Ok(subagents_meta_root()?.join("sync_state.json"))
}

pub(in crate::subagents) fn model_dir(model: &str) -> Result<PathBuf, String> {
    if !MODELS.contains(&model) {
        return Err(format!("unsupported model: {}", model));
    }
    let p = subagents_models_root()?.join(model);
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

pub(in crate::subagents) fn project_primary_dir(
    model: &str,
    project_root: &Path,
) -> Result<PathBuf, String> {
    let p = project_scan_root(model, project_root)?;
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn project_scan_root(
    model: &str,
    project_root: &Path,
) -> Result<PathBuf, String> {
    Ok(match model {
        "claude" => project_root.join(".claude").join("agents"),
        "codex" => project_root.join(".codex").join("agents"),
        "gemini" => project_root.join(".gemini").join("agents"),
        "opencode" => project_root.join(".opencode").join("agents"),
        _ => return Err(format!("unsupported model: {}", model)),
    })
}

pub(in crate::subagents) fn mirror_dir(model: &str) -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("home directory not found")?;
    let p = match model {
        "claude" => home.join(".claude").join("agents"),
        "codex" => home.join(".codex").join("agents"),
        "gemini" => home.join(".gemini").join("agents"),
        "opencode" => home.join(".config").join("opencode").join("agents"),
        _ => return Err(format!("unsupported model: {}", model)),
    };
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(fs::canonicalize(&p).unwrap_or(p))
}

pub(in crate::subagents) fn resolve_subagent_target_dir(
    model: &str,
    scope: &str,
    project_root: Option<&str>,
) -> Result<(PathBuf, Vec<PathBuf>), String> {
    if scope == INSTALL_SCOPE_PROJECT {
        let root = project_root.ok_or("subagents/project_root_required")?;
        let primary = project_primary_dir(model, &PathBuf::from(root))?;
        return Ok((primary, vec![]));
    }
    let primary = model_dir(model)?;
    let mirror = mirror_dir(model)?;
    Ok((primary, vec![mirror]))
}

pub(in crate::subagents) fn codex_project_config_path(project_root: &str) -> PathBuf {
    PathBuf::from(project_root)
        .join(".codex")
        .join("config.toml")
}

pub(in crate::subagents) fn codex_agent_key(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

pub(in crate::subagents) fn codex_managed_agent_key(dir_name: &str) -> String {
    let slug = codex_agent_key(dir_name);
    if slug.is_empty() {
        "onespace_agent".to_string()
    } else {
        format!("onespace_{}", slug)
    }
}

pub(in crate::subagents) fn upsert_codex_project_agent_entry(
    project_root: &str,
    dir_name: &str,
    display_name: &str,
    model: Option<&str>,
    tools: &[String],
    prompt: &str,
) -> Result<(), String> {
    let path = codex_project_config_path(project_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut doc = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.parse::<DocumentMut>().ok())
            .unwrap_or_default()
    } else {
        DocumentMut::new()
    };

    if !doc["agents"].is_table() {
        doc["agents"] = Item::Table(Table::new());
    }
    let agents = doc["agents"]
        .as_table_mut()
        .ok_or("invalid codex config agents table")?;
    let mut agent = Table::new();
    agent["description"] = toml_edit::value(display_name.to_string());
    let prompt_text = prompt.trim();
    if !prompt_text.is_empty() {
        agent["prompt"] = toml_edit::value(prompt_text.to_string());
    }
    if let Some(model_value) = model.map(|v| v.trim()).filter(|v| !v.is_empty()) {
        agent["model"] = toml_edit::value(model_value.to_string());
    }
    if !tools.is_empty() {
        let mut arr = toml_edit::Array::new();
        for tool in tools {
            if !tool.trim().is_empty() {
                arr.push(tool.trim());
            }
        }
        if !arr.is_empty() {
            agent["tools"] = Item::Value(toml_edit::Value::Array(arr));
        }
    }
    agent[CODEX_ONESPACE_MANAGED_KEY] = toml_edit::value(true);
    agent[CODEX_ONESPACE_DIR_KEY] = toml_edit::value(dir_name.to_string());
    agents.insert(&codex_managed_agent_key(dir_name), Item::Table(agent));
    crate::atomic_write_string(&path, &doc.to_string()).map_err(|e| e.to_string())?;
    Ok(())
}

pub(in crate::subagents) fn remove_codex_project_agent_entry(
    project_root: &str,
    dir_name: &str,
) -> Result<(), String> {
    let path = codex_project_config_path(project_root);
    if !path.exists() {
        return Ok(());
    }
    let mut doc = fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.parse::<DocumentMut>().ok())
        .unwrap_or_default();
    if let Some(agents) = doc["agents"].as_table_mut() {
        let dir_name_trimmed = dir_name.trim();
        let mut remove_keys = vec![];
        for (key, item) in agents.iter() {
            let Some(table) = item.as_table() else {
                continue;
            };
            let managed = table
                .get(CODEX_ONESPACE_MANAGED_KEY)
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !managed {
                continue;
            }
            let managed_dir = table
                .get(CODEX_ONESPACE_DIR_KEY)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if managed_dir == dir_name_trimmed || key == codex_managed_agent_key(dir_name_trimmed) {
                remove_keys.push(key.to_string());
            }
        }
        for key in remove_keys {
            agents.remove(&key);
        }
    }
    crate::atomic_write_string(&path, &doc.to_string()).map_err(|e| e.to_string())?;
    Ok(())
}

pub(in crate::subagents) fn prune_codex_project_managed_entries(
    project_root: &str,
    keep_dir_names: &HashSet<String>,
) -> Result<(), String> {
    let path = codex_project_config_path(project_root);
    if !path.exists() {
        return Ok(());
    }
    let mut doc = fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.parse::<DocumentMut>().ok())
        .unwrap_or_default();
    let mut changed = false;
    if let Some(agents) = doc["agents"].as_table_mut() {
        let mut remove_keys = vec![];
        for (key, item) in agents.iter() {
            let Some(table) = item.as_table() else {
                continue;
            };
            let managed = table
                .get(CODEX_ONESPACE_MANAGED_KEY)
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !managed {
                continue;
            }
            let managed_dir = table
                .get(CODEX_ONESPACE_DIR_KEY)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !keep_dir_names.contains(managed_dir) {
                remove_keys.push(key.to_string());
            }
        }
        if !remove_keys.is_empty() {
            changed = true;
        }
        for key in remove_keys {
            agents.remove(&key);
        }
    }
    if changed {
        crate::atomic_write_string(&path, &doc.to_string()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(in crate::subagents) fn make_repo_key(source_id: &str, source_rel_path: &str) -> String {
    format!("{}::{}", source_id, source_rel_path)
}

pub(in crate::subagents) fn repo_storage_root() -> Result<PathBuf, String> {
    let p = subagents_root()?.join("repository");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn repo_storage_dir(repo_key: &str) -> Result<PathBuf, String> {
    let digest = sha256_hex(repo_key);
    Ok(repo_storage_root()?.join(digest))
}

pub(in crate::subagents) fn repo_index_baseline_root() -> Result<PathBuf, String> {
    let p = subagents_root()?.join("index_baselines");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::subagents) fn repo_index_baseline_dir(repo_key: &str) -> Result<PathBuf, String> {
    let digest = sha256_hex(repo_key);
    Ok(repo_index_baseline_root()?.join(digest))
}

pub(in crate::subagents) fn snapshot_repository_index_baseline(
    repo_key: &str,
    source_dir: &Path,
) -> Result<(), String> {
    let baseline = repo_index_baseline_dir(repo_key)?;
    replace_dir_atomic(source_dir, &baseline)
}

pub(in crate::subagents) fn safe_slug(input: &str) -> String {
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

pub(in crate::subagents) fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(in crate::subagents) fn normalize_rel_path(rel: &Path) -> String {
    if rel == Path::new(".") {
        return ".".to_string();
    }
    rel.to_string_lossy().replace('\\', "/")
}

pub(in crate::subagents) fn resolve_scan_root(root_path: &str) -> Result<PathBuf, String> {
    let raw = root_path.trim();
    if raw.is_empty() {
        return Err("subagents/invalid_scan_root".to_string());
    }
    let root = PathBuf::from(raw);
    if !root.exists() {
        return Err("subagents/invalid_scan_root".to_string());
    }
    if !root.is_dir() {
        return Err("subagents/invalid_scan_root".to_string());
    }
    fs::canonicalize(&root).map_err(|_| "subagents/invalid_scan_root".to_string())
}

pub(in crate::subagents) fn local_source_id(root_can: &Path) -> String {
    let digest = sha256_hex(&root_can.to_string_lossy());
    format!("local-{}", &digest[..8])
}

pub(in crate::subagents) fn local_subagent_id(source_id: &str, rel_path: &str) -> String {
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

pub(in crate::subagents) fn has_path_traversal(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

pub(in crate::subagents) fn is_ignored_name(name: &str) -> bool {
    IGNORE_NAMES.contains(&name)
}

pub(in crate::subagents) fn parse_duplicate_file_name(name: &str) -> Option<String> {
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

pub(in crate::subagents) fn is_duplicate_clone_file(path: &Path) -> bool {
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

pub(in crate::subagents) fn ensure_within(root: &Path, target: &Path) -> Result<(), String> {
    if has_path_traversal(target) {
        return Err("subagents/path_out_of_root".to_string());
    }
    let root_can = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let target_can = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    if !target_can.starts_with(&root_can) {
        return Err("subagents/path_out_of_root".to_string());
    }
    Ok(())
}

pub(in crate::subagents) fn read_json_or_default<T: for<'de> Deserialize<'de> + Default>(
    path: &Path,
) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

pub(in crate::subagents) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    crate::atomic_write_string(&tmp, &raw).map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}

pub(in crate::subagents) fn merge_repository_record(
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
    if keep.model.as_deref().unwrap_or("").is_empty()
        && !candidate.model.as_deref().unwrap_or("").is_empty()
    {
        keep.model = candidate.model.clone();
    }
    if keep.tools.is_empty() && !candidate.tools.is_empty() {
        keep.tools = candidate.tools.clone();
    }
    if keep.created_at == 0 {
        keep.created_at = candidate.created_at;
    } else if candidate.created_at > 0 {
        keep.created_at = keep.created_at.min(candidate.created_at);
    }
    keep.ever_installed = keep.ever_installed || candidate.ever_installed;
    keep
}

pub(in crate::subagents) fn is_local_duplicate_repository(
    a: &RepositoryRecord,
    b: &RepositoryRecord,
) -> bool {
    if a.source_type != "local_import" || b.source_type != "local_import" {
        return false;
    }
    if a.subagent_id == b.subagent_id {
        return true;
    }
    let ah = a.hash.as_deref().unwrap_or("").trim();
    let bh = b.hash.as_deref().unwrap_or("").trim();
    if ah.is_empty() || bh.is_empty() || ah != bh {
        return false;
    }
    a.name.trim().eq_ignore_ascii_case(b.name.trim())
}

pub(in crate::subagents) fn normalize_repositories(state: &mut SubagentsState) -> bool {
    let mut changed = false;
    let before_len = state.repositories.len();
    state
        .repositories
        .retain(|repo| !(repo.source_type == "local_import" && repo.source_id == "local"));
    if state.repositories.len() != before_len {
        changed = true;
    }

    for repo in &mut state.repositories {
        if repo.source_type == "mirror" {
            repo.source_type = "local_import".to_string();
            changed = true;
        }
        if repo.repo_key.trim().is_empty()
            && !repo.source_id.trim().is_empty()
            && !repo.source_rel_path.trim().is_empty()
        {
            repo.repo_key = make_repo_key(&repo.source_id, &repo.source_rel_path);
            changed = true;
        }
        if repo.dir_name.trim().is_empty() {
            repo.dir_name = repo.subagent_id.clone();
            changed = true;
        }
        if repo.created_at == 0 {
            repo.created_at = repo.updated_at.unwrap_or_else(now_ts);
            changed = true;
        }
    }

    let mut deduped_by_repo_key: Vec<RepositoryRecord> = vec![];
    for repo in state.repositories.drain(..) {
        let repo_key = repo.repo_key.trim();
        if !repo_key.is_empty() {
            if let Some(idx) = deduped_by_repo_key
                .iter()
                .position(|existing| existing.repo_key == repo.repo_key)
            {
                let merged = merge_repository_record(deduped_by_repo_key[idx].clone(), repo);
                deduped_by_repo_key[idx] = merged;
                changed = true;
                continue;
            }
        }
        deduped_by_repo_key.push(repo);
    }

    let mut deduped: Vec<RepositoryRecord> = vec![];
    for repo in deduped_by_repo_key {
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

pub(in crate::subagents) fn load_subagents_state() -> Result<SubagentsState, String> {
    let mut state: SubagentsState = read_json_or_default(&subagents_state_path()?)?;
    let mut changed = false;
    if ensure_repositories_migrated(&mut state)? {
        changed = true;
    }
    if normalize_repositories(&mut state) {
        changed = true;
    }
    if !state.subagents.is_empty() {
        // Installed records must not be persisted in shared storage.
        state.subagents.clear();
        changed = true;
    }
    if changed {
        state = save_subagents_state(state)?;
    }
    Ok(state)
}

pub(in crate::subagents) fn save_subagents_state(
    mut state: SubagentsState,
) -> Result<SubagentsState, String> {
    state.subagents.clear();
    state.revision = state.revision.saturating_add(1);
    write_json(&subagents_state_path()?, &state)?;
    Ok(state)
}

pub(in crate::subagents) fn load_local_subagents_state() -> Result<SubagentsLocalState, String> {
    let mut state: SubagentsLocalState = read_json_or_default(&subagents_local_state_path()?)?;
    if normalize_local_subagents_state(&mut state) {
        state = save_local_subagents_state(state)?;
    }
    Ok(state)
}

pub(in crate::subagents) fn save_local_subagents_state(
    mut state: SubagentsLocalState,
) -> Result<SubagentsLocalState, String> {
    state.revision = state.revision.saturating_add(1);
    write_json(&subagents_local_state_path()?, &state)?;
    Ok(state)
}

pub(in crate::subagents) fn normalize_local_subagents_state(
    state: &mut SubagentsLocalState,
) -> bool {
    let before_len = state.subagents.len();
    state
        .subagents
        .retain(|subagent| record_scope(subagent) == INSTALL_SCOPE_GLOBAL);
    before_len != state.subagents.len()
}

pub(in crate::subagents) fn combined_revision(
    shared: &SubagentsState,
    local: &SubagentsLocalState,
) -> u64 {
    shared.revision.max(local.revision)
}

pub(in crate::subagents) fn load_sync_state() -> Result<SubagentsSyncState, String> {
    read_json_or_default(&sync_state_path()?)
}

pub(in crate::subagents) fn save_sync_state(state: &SubagentsSyncState) -> Result<(), String> {
    write_json(&sync_state_path()?, state)
}

pub(in crate::subagents) fn api_ok<T: Serialize>(
    data: T,
    revision: u64,
) -> Result<ApiOk<T>, String> {
    Ok(ApiOk {
        ok: true,
        data,
        meta: ApiMeta {
            revision,
            ts: now_ts(),
        },
    })
}
