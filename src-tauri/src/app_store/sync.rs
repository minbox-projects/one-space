use super::{
    api_key_has_value, apply_provider_id_map_to_plain_json_file, generate_provider_uuid,
    is_uuid_v4, load_outbox_state, load_providers_state, now_ts,
    provider_import_id_map_to_plain_id_map, provider_import_key, provider_records_match,
    remap_provider_id, save_outbox_state, save_providers_state, OutboxEvent, ProvidersState,
    StorageEngine, OUTBOX_DEDUP_WINDOW_SECS,
};
use crate::{ai_news, config, git, mcp_servers, messages};
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;
use tauri::Emitter;

pub(in crate::app_store) static SYNC_RUNNING: AtomicBool = AtomicBool::new(false);

pub(in crate::app_store) struct SyncRunningGuard;

impl Drop for SyncRunningGuard {
    fn drop(&mut self) {
        SYNC_RUNNING.store(false, Ordering::SeqCst);
    }
}

pub(in crate::app_store) fn file_modified_ts(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

pub(in crate::app_store) fn placeholder_for(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.icloud", path.to_string_lossy()))
}

pub(in crate::app_store) fn atomic_copy(src: &Path, dst: &Path) -> Result<(), String> {
    let bytes = fs::read(src).map_err(|e| e.to_string())?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = dst.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dst).map_err(|e| e.to_string())
}

pub(in crate::app_store) fn sync_file_bidirectional(
    local: &Path,
    shared: &Path,
    warnings: &mut Vec<String>,
    label: &str,
) -> Result<(), String> {
    let local_ts = file_modified_ts(local);
    let shared_ts = file_modified_ts(shared);
    let shared_pending_download = shared_ts.is_none() && placeholder_for(shared).exists();

    if shared_pending_download {
        warnings.push(format!(
            "{}: shared file pending download ({})",
            label,
            placeholder_for(shared).display()
        ));
    }

    match (local_ts, shared_ts) {
        (Some(l), Some(s)) if s > l => {
            if let Err(err) = atomic_copy(shared, local) {
                warnings.push(format!(
                    "{}: skip importing shared copy {} -> {} ({})",
                    label,
                    shared.display(),
                    local.display(),
                    err
                ));
            }
        }
        (Some(l), Some(s)) if l > s => {
            atomic_copy(local, shared)?;
        }
        (None, Some(_)) => {
            if let Err(err) = atomic_copy(shared, local) {
                warnings.push(format!(
                    "{}: skip importing shared copy {} -> {} ({})",
                    label,
                    shared.display(),
                    local.display(),
                    err
                ));
            }
        }
        (Some(_), None) => {
            if shared_pending_download {
                warnings.push(format!(
                    "{}: skip exporting while shared file is pending download",
                    label
                ));
            } else {
                atomic_copy(local, shared)?;
            }
        }
        _ => {}
    }

    Ok(())
}

pub(in crate::app_store) fn walk_files_recursive(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Ok(vec![]);
    }

    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf();
                files.push(rel);
            }
        }
    }
    Ok(files)
}

pub(in crate::app_store) fn strip_icloud_suffix(path: &Path) -> Option<PathBuf> {
    let file_name = path.file_name()?.to_string_lossy();
    let stripped = file_name.strip_suffix(".icloud")?;
    let mut out = path.to_path_buf();
    out.set_file_name(stripped);
    Some(out)
}

pub(in crate::app_store) fn sync_directory_bidirectional(
    local_root: &Path,
    shared_root: &Path,
    warnings: &mut Vec<String>,
    label: &str,
) -> Result<(), String> {
    fs::create_dir_all(local_root).map_err(|e| e.to_string())?;
    fs::create_dir_all(shared_root).map_err(|e| e.to_string())?;

    let local_files_raw = walk_files_recursive(local_root)?;
    let shared_files_raw = walk_files_recursive(shared_root)?;

    let local_files: Vec<PathBuf> = local_files_raw
        .into_iter()
        .filter(|p| strip_icloud_suffix(p).is_none())
        .collect();

    let mut shared_files = Vec::new();
    let mut shared_pending = HashSet::<PathBuf>::new();
    for rel in shared_files_raw {
        if let Some(real_rel) = strip_icloud_suffix(&rel) {
            shared_pending.insert(real_rel);
        } else {
            shared_files.push(rel);
        }
    }

    let mut union = BTreeSet::<PathBuf>::new();
    for rel in &local_files {
        union.insert(rel.clone());
    }
    for rel in &shared_files {
        union.insert(rel.clone());
    }
    for rel in &shared_pending {
        union.insert(rel.clone());
    }

    for rel in union {
        let local = local_root.join(&rel);
        let shared = shared_root.join(&rel);
        let local_ts = file_modified_ts(&local);
        let shared_ts = file_modified_ts(&shared);
        let shared_pending_download = (shared_ts.is_none() && placeholder_for(&shared).exists())
            || shared_pending.contains(&rel);

        if shared_pending_download {
            warnings.push(format!(
                "{}:{}: shared file pending download ({})",
                label,
                rel.display(),
                placeholder_for(&shared).display()
            ));
        }

        match (local_ts, shared_ts) {
            (Some(l), Some(s)) if s > l => {
                if let Err(err) = atomic_copy(&shared, &local) {
                    warnings.push(format!(
                        "{}:{}: skip importing shared copy {} -> {} ({})",
                        label,
                        rel.display(),
                        shared.display(),
                        local.display(),
                        err
                    ));
                }
            }
            (Some(l), Some(s)) if l > s => {
                atomic_copy(&local, &shared)?;
            }
            (None, Some(_)) => {
                if let Err(err) = atomic_copy(&shared, &local) {
                    warnings.push(format!(
                        "{}:{}: skip importing shared copy {} -> {} ({})",
                        label,
                        rel.display(),
                        shared.display(),
                        local.display(),
                        err
                    ));
                }
            }
            (Some(_), None) => {
                if shared_pending_download {
                    warnings.push(format!(
                        "{}:{}: skip exporting while shared file is pending download",
                        label,
                        rel.display()
                    ));
                } else {
                    atomic_copy(&local, &shared)?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

pub(in crate::app_store) fn shared_profile_path(
    cfg: &config::StorageConfig,
    name: &str,
) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("profile")
        .join(name);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

pub(in crate::app_store) fn shared_content_path(
    cfg: &config::StorageConfig,
    file_name: &str,
) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("content")
        .join(file_name);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

pub(in crate::app_store) fn shared_news_path(
    cfg: &config::StorageConfig,
    file_name: &str,
) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("news")
        .join(file_name);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

pub(in crate::app_store) fn local_workflow_presets_path() -> Result<PathBuf, String> {
    Ok(crate::get_data_dir()?.join("workflow_presets.json"))
}

pub(in crate::app_store) fn local_workflow_runs_path() -> Result<PathBuf, String> {
    Ok(crate::get_data_dir()?.join("workflow_runs.json"))
}

pub(in crate::app_store) fn local_skills_repository_root() -> Result<PathBuf, String> {
    Ok(crate::get_data_dir()?.join("data").join("skills"))
}

pub(in crate::app_store) fn local_subagents_repository_root() -> Result<PathBuf, String> {
    Ok(crate::get_data_dir()?.join("data").join("subagents"))
}

pub(in crate::app_store) fn local_ai_news_path() -> Result<PathBuf, String> {
    ai_news::ai_news_local_path()
}

pub(in crate::app_store) fn shared_skills_repository_root(
    cfg: &config::StorageConfig,
) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("profile")
        .join("skills_repository");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::app_store) fn shared_subagents_repository_root(
    cfg: &config::StorageConfig,
) -> Result<PathBuf, String> {
    let p = config::get_shared_data_dir_for(cfg)?
        .join("profile")
        .join("subagents_repository");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

pub(in crate::app_store) fn key_looks_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("key")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("auth")
}

pub(in crate::app_store) fn is_placeholder_string(value: &str) -> bool {
    value.starts_with('$') || value.starts_with("${")
}

pub(in crate::app_store) fn placeholder_for_key(key: &str) -> String {
    let normalized = key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("${}", normalized)
}

pub(in crate::app_store) fn sanitize_value_for_shared(
    key_hint: Option<&str>,
    value: &Value,
) -> Value {
    match value {
        Value::Object(obj) => {
            let mut out = Map::new();
            for (k, v) in obj {
                out.insert(k.clone(), sanitize_value_for_shared(Some(k), v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| sanitize_value_for_shared(key_hint, v))
                .collect(),
        ),
        Value::String(s) => {
            if key_hint.map(key_looks_sensitive).unwrap_or(false) && !s.is_empty() {
                Value::String(placeholder_for_key(key_hint.unwrap_or("SECRET")))
            } else {
                Value::String(s.clone())
            }
        }
        _ => value.clone(),
    }
}

pub(in crate::app_store) fn sanitize_map_for_shared(
    source: &Map<String, Value>,
) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in source {
        out.insert(k.clone(), sanitize_value_for_shared(Some(k), v));
    }
    out
}

pub(in crate::app_store) fn merge_sensitive_maps(
    incoming: &Map<String, Value>,
    existing: &Map<String, Value>,
) -> Map<String, Value> {
    let mut merged = incoming.clone();
    for (k, old_v) in existing {
        let should_restore = key_looks_sensitive(k)
            && match merged.get(k) {
                None => true,
                Some(Value::String(s)) => s.is_empty() || is_placeholder_string(s),
                Some(_) => false,
            };
        if should_restore {
            merged.insert(k.clone(), old_v.clone());
        }
    }
    merged
}

pub(in crate::app_store) fn export_local_providers_to_shared(path: &Path) -> Result<(), String> {
    let mut state = load_providers_state()?;
    for provider in &mut state.providers {
        provider.core.api_key.clear();
        provider.tool_config = sanitize_map_for_shared(&provider.tool_config);
        provider.extra = sanitize_map_for_shared(&provider.extra);
    }
    StorageEngine::write_json(path, &state)
}

pub(in crate::app_store) fn import_shared_providers_to_local(
    path: &Path,
) -> Result<HashMap<String, String>, String> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let incoming: ProvidersState = StorageEngine::read_json(path)?;

    let mut local = load_providers_state()?;
    let before = serde_json::to_value(&local).unwrap_or(Value::Null);
    let mut incoming_to_local_id: HashMap<String, String> = HashMap::new();

    for in_provider in &incoming.providers {
        if let Some(existing_pos) = local
            .providers
            .iter()
            .position(|provider| provider_records_match(provider, in_provider))
        {
            let existing = &mut local.providers[existing_pos];
            let local_id = existing.core.id.clone();
            let old_api_key = existing.core.api_key.clone();
            let old_tool_cfg = existing.tool_config.clone();
            let old_extra = existing.extra.clone();
            let old_history = existing.history.clone();
            let old_code = existing.core.code.clone();

            *existing = in_provider.clone();
            existing.core.id = local_id.clone();
            if api_key_has_value(&old_api_key) {
                existing.core.api_key = old_api_key;
            } else if !api_key_has_value(&existing.core.api_key) {
                existing.core.api_key.clear();
            }
            if existing.core.code.is_none() {
                existing.core.code = old_code;
            }
            existing.tool_config = merge_sensitive_maps(&existing.tool_config, &old_tool_cfg);
            existing.extra = merge_sensitive_maps(&existing.extra, &old_extra);
            if !old_history.is_empty() {
                existing.history = old_history;
            }
            incoming_to_local_id.insert(
                provider_import_key(&in_provider.core.tool, &in_provider.core.id),
                local_id,
            );
        } else {
            let mut inserted = in_provider.clone();
            inserted.core.id = if is_uuid_v4(&inserted.core.id)
                && !local
                    .providers
                    .iter()
                    .any(|provider| provider.core.id == inserted.core.id)
            {
                inserted.core.id
            } else {
                generate_provider_uuid()
            };
            inserted.core.api_key.clear();
            incoming_to_local_id.insert(
                provider_import_key(&in_provider.core.tool, &in_provider.core.id),
                inserted.core.id.clone(),
            );
            local.providers.push(inserted);
        }
    }

    if !incoming.active.is_empty() {
        for (tool, provider_id) in &incoming.active {
            let key = provider_import_key(&tool, &provider_id);
            if let Some(mapped_id) = incoming_to_local_id.get(&key).cloned().or_else(|| {
                local
                    .providers
                    .iter()
                    .any(|provider| provider.core.tool == *tool && provider.core.id == *provider_id)
                    .then(|| provider_id.clone())
            }) {
                local.active.insert(tool.clone(), mapped_id);
            }
        }
    }

    // Prevent active pointer drift after deletions.
    local.active.retain(|tool, provider_id| {
        local
            .providers
            .iter()
            .any(|p| p.core.tool == *tool && p.core.id == *provider_id)
    });

    // Also backfill missing active keys if shared config omitted active map.
    if incoming.active.is_empty() {
        for provider in &local.providers {
            local
                .active
                .entry(provider.core.tool.clone())
                .or_insert_with(|| provider.core.id.clone());
        }
    }

    let after = serde_json::to_value(&local).unwrap_or(Value::Null);
    if before != after {
        let _ = save_providers_state(&local)?;
    }
    Ok(incoming_to_local_id)
}

pub(in crate::app_store) fn sanitize_mcp_for_shared(
    state: &mcp_servers::MCPServersState,
) -> mcp_servers::MCPServersState {
    let mut out = state.clone();
    out.is_encrypted = false;
    for server in &mut out.servers {
        if let Some(env) = server.env.as_mut() {
            let keys: Vec<String> = env.keys().cloned().collect();
            for key in keys {
                env.insert(key.clone(), placeholder_for_key(&key));
            }
        }
        if let Some(headers) = server.headers.as_mut() {
            let keys: Vec<String> = headers.keys().cloned().collect();
            for key in keys {
                headers.insert(key.clone(), placeholder_for_key(&key));
            }
        }
    }
    out
}

pub(in crate::app_store) fn merge_sensitive_string_maps(
    incoming: &Option<HashMap<String, String>>,
    existing: &Option<HashMap<String, String>>,
) -> Option<HashMap<String, String>> {
    match (incoming, existing) {
        (None, None) => None,
        (Some(map), None) => Some(map.clone()),
        (None, Some(map)) => Some(map.clone()),
        (Some(in_map), Some(old_map)) => {
            let mut merged = in_map.clone();
            for (k, old_val) in old_map {
                let keep_old = match merged.get(k) {
                    None => true,
                    Some(v) => v.is_empty() || is_placeholder_string(v),
                };
                if keep_old {
                    merged.insert(k.clone(), old_val.clone());
                }
            }
            Some(merged)
        }
    }
}

pub(in crate::app_store) fn export_local_mcp_to_shared(path: &Path) -> Result<(), String> {
    let local_state = mcp_servers::get_mcp_servers()?;
    let shared = sanitize_mcp_for_shared(&local_state);
    StorageEngine::write_json(path, &shared)
}

pub(in crate::app_store) fn import_shared_mcp_to_local(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let incoming: mcp_servers::MCPServersState = StorageEngine::read_json(path)?;

    let mut local_state = mcp_servers::get_mcp_servers().unwrap_or_default();
    let before = serde_json::to_value(&local_state).unwrap_or(Value::Null);
    let incoming_ids: HashSet<String> = incoming.servers.iter().map(|s| s.id.clone()).collect();

    for incoming_server in incoming.servers {
        if let Some(existing) = local_state
            .servers
            .iter_mut()
            .find(|s| s.id == incoming_server.id)
        {
            existing.name = incoming_server.name.clone();
            existing.description = incoming_server.description.clone();
            existing.config_key = incoming_server.config_key.clone();
            existing.transport = incoming_server.transport.clone();
            existing.command = incoming_server.command.clone();
            existing.args = incoming_server.args.clone();
            existing.cwd = incoming_server.cwd.clone();
            existing.url = incoming_server.url.clone();
            existing.http_url = incoming_server.http_url.clone();
            existing.timeout = incoming_server.timeout;
            existing.trust = incoming_server.trust;
            existing.linked_provider_ids = incoming_server.linked_provider_ids.clone();
            existing.env = merge_sensitive_string_maps(&incoming_server.env, &existing.env);
            existing.headers =
                merge_sensitive_string_maps(&incoming_server.headers, &existing.headers);
            existing.updated_at = chrono::Utc::now();
        } else {
            let mut inserted = incoming_server.clone();
            inserted.created_at = chrono::Utc::now();
            inserted.updated_at = chrono::Utc::now();
            local_state.servers.push(inserted);
        }
    }

    // Propagate deletions from shared profile to local mirror.
    local_state
        .servers
        .retain(|server| incoming_ids.contains(&server.id));

    let after = serde_json::to_value(&local_state).unwrap_or(Value::Null);
    if before != after {
        local_state.is_encrypted = true;
        for server in &mut local_state.servers {
            let _ = mcp_servers::encrypt_sensitive_data(server);
        }
        StorageEngine::write_json(&StorageEngine::mcp_path()?, &local_state)?;
    }

    Ok(())
}

pub(in crate::app_store) fn sync_providers_profile(
    cfg: &config::StorageConfig,
    warnings: &mut Vec<String>,
) -> Result<HashMap<String, String>, String> {
    let local = StorageEngine::providers_path()?;
    let shared = shared_profile_path(cfg, "providers.json")?;
    let local_ts = file_modified_ts(&local);
    let shared_ts = file_modified_ts(&shared);
    let shared_pending_download = shared_ts.is_none() && placeholder_for(&shared).exists();
    let mut provider_id_map = HashMap::new();

    match (local_ts, shared_ts) {
        (Some(l), Some(s)) if s > l => provider_id_map = import_shared_providers_to_local(&shared)?,
        (Some(l), Some(s)) if l > s => export_local_providers_to_shared(&shared)?,
        (None, Some(_)) => provider_id_map = import_shared_providers_to_local(&shared)?,
        (Some(_), None) => {
            if shared_pending_download {
                warnings.push(
                    "providers: skip export while shared file is pending download".to_string(),
                );
            } else {
                export_local_providers_to_shared(&shared)?;
            }
        }
        _ => {}
    }

    if shared_pending_download {
        let ph = placeholder_for(&shared);
        warnings.push(format!(
            "providers: shared file pending download ({})",
            ph.display()
        ));
    }

    Ok(provider_id_map)
}

pub(in crate::app_store) fn remap_local_mcp_provider_ids(
    id_map: &HashMap<String, String>,
) -> Result<bool, String> {
    if id_map.is_empty() {
        return Ok(false);
    }
    let mut local_state = mcp_servers::get_mcp_servers().unwrap_or_default();
    let mut changed = false;
    for server in &mut local_state.servers {
        for provider_id in &mut server.linked_provider_ids {
            if let Some(next) = remap_provider_id(provider_id, id_map) {
                *provider_id = next;
                changed = true;
            }
        }
    }
    if changed {
        local_state.is_encrypted = true;
        for server in &mut local_state.servers {
            let _ = mcp_servers::encrypt_sensitive_data(server);
        }
        StorageEngine::write_json(&StorageEngine::mcp_path()?, &local_state)?;
    }
    Ok(changed)
}

pub(in crate::app_store) fn sync_mcp_profile(
    cfg: &config::StorageConfig,
    warnings: &mut Vec<String>,
    imported_provider_id_map: &HashMap<String, String>,
) -> Result<(), String> {
    let local = StorageEngine::mcp_path()?;
    let shared = shared_profile_path(cfg, "mcp.json")?;
    let local_ts = file_modified_ts(&local);
    let shared_ts = file_modified_ts(&shared);
    let shared_pending_download = shared_ts.is_none() && placeholder_for(&shared).exists();

    match (local_ts, shared_ts) {
        (Some(l), Some(s)) if s > l => import_shared_mcp_to_local(&shared)?,
        (Some(l), Some(s)) if l > s => export_local_mcp_to_shared(&shared)?,
        (None, Some(_)) => import_shared_mcp_to_local(&shared)?,
        (Some(_), None) => {
            if shared_pending_download {
                warnings.push("mcp: skip export while shared file is pending download".to_string());
            } else {
                export_local_mcp_to_shared(&shared)?;
            }
        }
        _ => {}
    }

    if shared_pending_download {
        let ph = placeholder_for(&shared);
        warnings.push(format!(
            "mcp: shared file pending download ({})",
            ph.display()
        ));
    }

    let _ = remap_local_mcp_provider_ids(imported_provider_id_map)?;

    Ok(())
}

pub(in crate::app_store) fn sync_workflow_presets_profile(
    cfg: &config::StorageConfig,
    warnings: &mut Vec<String>,
    imported_provider_id_map: &HashMap<String, String>,
) -> Result<(), String> {
    let local = local_workflow_presets_path()?;
    let shared = shared_profile_path(cfg, "workflow_presets.json")?;
    sync_file_bidirectional(&local, &shared, warnings, "workflow_presets")?;
    let _ = apply_provider_id_map_to_plain_json_file(&local, imported_provider_id_map)?;
    Ok(())
}

pub(in crate::app_store) fn run_local_shared_sync(
    cfg: &config::StorageConfig,
) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    let policy = cfg.sync_policy.clone();
    let mut imported_provider_id_map = HashMap::new();

    if policy.providers {
        let provider_import_id_map = sync_providers_profile(cfg, &mut warnings)?;
        imported_provider_id_map = provider_import_id_map_to_plain_id_map(&provider_import_id_map);
    }

    if policy.mcp {
        sync_mcp_profile(cfg, &mut warnings, &imported_provider_id_map)?;
    }

    if policy.workflow_presets {
        sync_workflow_presets_profile(cfg, &mut warnings, &imported_provider_id_map)?;
    }

    if policy.skills_sources || policy.subagents_sources {
        let local = config::shared_profile_local_path()?;
        let shared = shared_profile_path(cfg, "skills_sources.json")?;
        sync_file_bidirectional(&local, &shared, &mut warnings, "skills_sources")?;
    }

    if policy.skills_repository {
        let local = local_skills_repository_root()?;
        let shared = shared_skills_repository_root(cfg)?;
        sync_directory_bidirectional(&local, &shared, &mut warnings, "skills_repository")?;
    }

    if policy.subagents_repository {
        let local = local_subagents_repository_root()?;
        let shared = shared_subagents_repository_root(cfg)?;
        sync_directory_bidirectional(&local, &shared, &mut warnings, "subagents_repository")?;
    }

    if policy.content {
        for name in ["notes", "bookmarks", "snippets"] {
            let local = StorageEngine::content_path(name)?;
            let shared = shared_content_path(cfg, &format!("{}.enc.json", name))?;
            sync_file_bidirectional(&local, &shared, &mut warnings, name)?;
        }
    }

    if policy.ai_news {
        let local = local_ai_news_path()?;
        let shared = shared_news_path(cfg, "ai_news.json")?;
        sync_file_bidirectional(&local, &shared, &mut warnings, "ai_news")?;
    }

    Ok(warnings)
}

pub(in crate::app_store) fn emit_sync_status(
    app: &tauri::AppHandle,
    status: &str,
    message: Option<&str>,
) {
    let payload = json!({
        "status": status,
        "message": message.unwrap_or_default(),
    });
    let _ = app.emit("git-sync-status", payload);
}

pub(in crate::app_store) async fn run_sync_pipeline(
    app: &tauri::AppHandle,
    cfg: config::StorageConfig,
) -> Result<Vec<String>, String> {
    if cfg.storage_type == "git" {
        emit_sync_status(app, "pulling", Some("Pulling from shared storage..."));
        let cfg_for_pull = cfg.clone();
        tauri::async_runtime::spawn_blocking(move || git::init_or_pull_git_repo(&cfg_for_pull))
            .await
            .map_err(|e| e.to_string())??;
    } else {
        emit_sync_status(app, "pulling", Some("Syncing local mirror..."));
    }

    let warnings = run_local_shared_sync(&cfg)?;

    if cfg.storage_type == "git" {
        emit_sync_status(app, "pushing", Some("Pushing local mirror updates..."));
        let cfg_for_push = cfg.clone();
        tauri::async_runtime::spawn_blocking(move || git::commit_and_push(&cfg_for_push))
            .await
            .map_err(|e| e.to_string())??;
    }

    Ok(warnings)
}

pub(in crate::app_store) async fn process_sync_queue_impl(
    app: tauri::AppHandle,
    force_run: bool,
) -> Result<(), String> {
    if SYNC_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }
    let _guard = SyncRunningGuard;
    let mut outbox = load_outbox_state()?;
    let previous_sync_error = outbox.last_error.clone();
    outbox.running = true;
    outbox.last_status = "running".to_string();
    save_outbox_state(&outbox)?;

    let now = now_ts();
    let mut due = Vec::new();
    let mut keep = Vec::new();
    for ev in outbox.events.into_iter() {
        if ev.next_retry_at <= now {
            due.push(ev);
        } else {
            keep.push(ev);
        }
    }

    let should_run = force_run || !due.is_empty();
    let mut last_error = None;

    if should_run {
        let cfg = config::get_config()?;
        let storage_type = cfg.storage_type.clone();
        match run_sync_pipeline(&app, cfg).await {
            Ok(warnings) => {
                if !warnings.is_empty() {
                    eprintln!("sync warnings: {}", warnings.join(" | "));
                }
                emit_sync_status(&app, "success", Some("Synced successfully"));
                if previous_sync_error.is_some() {
                    messages::record_message_silent(
                        &app,
                        messages::MessageCreateInput {
                            source: "sync".to_string(),
                            category: "recovery".to_string(),
                            severity: "success".to_string(),
                            title: messages::localized("同步恢复成功", "Sync recovered"),
                            summary: Some(if messages::current_language_is_zh() {
                                format!("{} 同步已从失败状态恢复", storage_type)
                            } else {
                                format!("{} sync recovered from a failed state", storage_type)
                            }),
                            detail: previous_sync_error,
                            dedupe_key: Some(format!("sync:recovery:{}", storage_type)),
                            target: Some(messages::MessageTarget {
                                tab: "settings".to_string(),
                                section: Some("storage".to_string()),
                                entity_id: None,
                            }),
                            metadata: Some(json!({ "storage_type": storage_type })),
                        },
                    );
                }
                let _ = app.emit("refresh-ai-providers", ());
            }
            Err(err) => {
                last_error = Some(err.clone());
                emit_sync_status(&app, "error", Some(&err));
                messages::record_message_silent(
                    &app,
                    messages::MessageCreateInput {
                        source: "sync".to_string(),
                        category: "storage".to_string(),
                        severity: "error".to_string(),
                        title: messages::localized("同步失败", "Sync failed"),
                        summary: Some(err.clone()),
                        detail: Some(err.clone()),
                        dedupe_key: Some(format!("sync:error:{}", storage_type)),
                        target: Some(messages::MessageTarget {
                            tab: "settings".to_string(),
                            section: Some("storage".to_string()),
                            entity_id: None,
                        }),
                        metadata: Some(json!({ "storage_type": storage_type })),
                    },
                );
                for mut ev in due {
                    ev.attempts = ev.attempts.saturating_add(1);
                    let backoff = 2u64.saturating_pow(ev.attempts.min(8));
                    ev.next_retry_at = now_ts().saturating_add(backoff);
                    ev.last_error = Some(err.clone());
                    keep.push(ev);
                }
            }
        }
    } else {
        keep.extend(due);
    }

    outbox.events = keep;
    outbox.last_run_at = Some(now_ts());
    outbox.running = false;
    if let Some(err) = last_error {
        outbox.last_status = "error".to_string();
        outbox.last_error = Some(err);
    } else {
        outbox.last_status = "success".to_string();
        outbox.last_error = None;
    }
    save_outbox_state(&outbox)?;
    Ok(())
}

pub(in crate::app_store) async fn process_sync_queue(app: tauri::AppHandle) -> Result<(), String> {
    process_sync_queue_impl(app, false).await
}

pub(in crate::app_store) fn enqueue_sync_event(domain: &str, reason: &str) -> Result<(), String> {
    let mut outbox = load_outbox_state()?;
    let now = now_ts();

    let is_dup = outbox.events.iter().any(|e| {
        e.domain == domain
            && now.saturating_sub(e.created_at) <= OUTBOX_DEDUP_WINDOW_SECS
            && e.last_error.is_none()
    });

    if !is_dup {
        outbox.events.push(OutboxEvent {
            id: format!("evt-{}", uuid::Uuid::new_v4()),
            domain: domain.to_string(),
            reason: reason.to_string(),
            created_at: now,
            attempts: 0,
            next_retry_at: now,
            last_error: None,
        });
    }
    save_outbox_state(&outbox)
}
