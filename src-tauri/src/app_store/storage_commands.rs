use super::{
    api_error, api_ok, cli_has_system_config, detect_cli_installation,
    extract_active_map_from_snapshot, extract_providers_from_snapshot,
    filter_sessions_by_history_window, get_meta, install_guide_for, is_managed_tool,
    load_launcher_state, load_outbox_state, load_service_providers_state, load_sessions_state,
    normalize_device_label, parse_json_array_len, provider_snapshot_candidates,
    provider_snapshot_quality_score, read_provider_snapshot_value,
    resolve_claude_config_dir_for_provider_id, run_migration_impl, save_service_providers_internal,
    service_providers_to_legacy_view, session_to_legacy, validate_provider_uuid_param, ApiErr,
    ApiMeta, ApiOk, AppSnapshot, CliEnvProbeResult, DashboardCounts, ServiceProviderRecord,
    StorageEngine, SyncedDeviceProvidersView,
};
use crate::{config, storage, workspaces};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::{self};
use std::path::PathBuf;

#[tauri::command]
pub fn storage_get_snapshot() -> Result<ApiOk<AppSnapshot>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let providers = service_providers_to_legacy_view(
        &load_service_providers_state().map_err(|e| api_error("io_error", e))?,
    );
    let sessions = load_sessions_state().map_err(|e| api_error("io_error", e))?;
    let cfg = config::get_storage_config().map_err(|e| api_error("config_error", e))?;
    let schema = StorageEngine::load_schema().map_err(|e| api_error("io_error", e))?;
    let outbox = load_outbox_state().map_err(|e| api_error("io_error", e))?;

    api_ok(
        AppSnapshot {
            providers: serde_json::to_value(providers)
                .map_err(|e| api_error("serialize_error", e.to_string()))?,
            sessions: Value::Array(sessions.sessions.iter().map(session_to_legacy).collect()),
            config: serde_json::to_value(cfg)
                .map_err(|e| api_error("serialize_error", e.to_string()))?,
            schema,
            outbox,
        },
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

pub fn list_synced_device_providers() -> Result<Vec<SyncedDeviceProvidersView>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let cfg = config::get_storage_config().map_err(|e| api_error("config_error", e))?;

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(root) = config::resolve_shared_storage_root(&cfg) {
        roots.push(root);
    }
    if let Ok(shared) = config::get_shared_data_dir_for(&cfg) {
        if !roots.iter().any(|p| p == &shared) {
            roots.push(shared);
        }
    }

    let current_device = normalize_device_label(&crate::get_hostname());
    let mut seen_devices: HashSet<String> = HashSet::new();
    let mut devices: Vec<SyncedDeviceProvidersView> = Vec::new();
    let skip_dirs: HashSet<&str> = [
        "shared", "profile", "content", "meta", "data", "backup", "backups", ".git",
    ]
    .into_iter()
    .collect();

    for root in roots {
        if !root.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let device_id = entry.file_name().to_string_lossy().trim().to_string();
            if device_id.is_empty() {
                continue;
            }
            let normalized = normalize_device_label(&device_id);
            if normalized.is_empty()
                || normalized == current_device
                || skip_dirs.contains(normalized.as_str())
                || seen_devices.contains(&normalized)
            {
                continue;
            }

            let mut matched: Option<(usize, SyncedDeviceProvidersView)> = None;
            for candidate in provider_snapshot_candidates(&path) {
                if !candidate.exists() {
                    continue;
                }
                let Some(value) = read_provider_snapshot_value(&candidate) else {
                    continue;
                };
                let Some(root_obj) = value.as_object() else {
                    continue;
                };
                let providers = extract_providers_from_snapshot(root_obj);
                if providers.is_empty() {
                    continue;
                }
                let active = extract_active_map_from_snapshot(root_obj);
                let score = provider_snapshot_quality_score(&providers, &active);
                let view = SyncedDeviceProvidersView {
                    device_id: device_id.clone(),
                    active,
                    providers,
                };
                match &matched {
                    Some((best_score, _)) if *best_score >= score => {}
                    _ => matched = Some((score, view)),
                }
            }

            if let Some((_, view)) = matched {
                seen_devices.insert(normalized);
                devices.push(view);
            }
        }
    }

    devices.sort_by(|a, b| a.device_id.to_lowercase().cmp(&b.device_id.to_lowercase()));
    Ok(devices)
}

#[tauri::command]
pub async fn dashboard_counts() -> Result<ApiOk<DashboardCounts>, ApiErr> {
    run_migration_impl().map_err(|e| api_error("migration_failed", e))?;
    let counts = tauri::async_runtime::spawn_blocking(compute_dashboard_counts)
        .await
        .map_err(|e| api_error("task_join_error", e.to_string()))?
        .map_err(|e| api_error("io_error", e))?;
    api_ok(counts, get_meta().map_err(|e| api_error("io_error", e))?)
}

pub(in crate::app_store) fn compute_dashboard_counts() -> Result<DashboardCounts, String> {
    let launcher = load_launcher_state().map(|s| s.items.len())?;
    let workspaces = workspaces::workspace_count_fast().unwrap_or(0);
    let sessions_state = load_sessions_state()?;
    let sessions = filter_sessions_by_history_window(sessions_state.sessions.iter()).len();

    let environments = load_service_providers_state().map(|s| s.providers.len())?;

    let ssh = crate::get_ssh_hosts().map(|hosts| hosts.len()).unwrap_or(0);
    let snippets = storage::read_snippets()
        .map(|raw| parse_json_array_len(&raw))
        .unwrap_or(0);
    let bookmarks = storage::read_bookmarks()
        .map(|raw| parse_json_array_len(&raw))
        .unwrap_or(0);
    let notes = storage::read_notes()
        .map(|raw| parse_json_array_len(&raw))
        .unwrap_or(0);
    let ai_news = crate::ai_news::ai_news_count_fast().unwrap_or(0);
    let skills = crate::skills::skills_installed_count_all_scopes().unwrap_or(0);
    let subagents = crate::subagents::subagents_installed_asset_count_all_scopes().unwrap_or(0);
    let mcp_servers = crate::mcp_servers::get_mcp_servers_count_fast().unwrap_or(0);
    let storage_type = config::get_storage_config()
        .ok()
        .map(|cfg| cfg.storage_type);

    Ok(DashboardCounts {
        launcher,
        workspaces,
        sessions,
        ssh,
        snippets,
        bookmarks,
        notes,
        ai_news,
        environments,
        skills,
        subagents,
        mcp_servers,
        storage_type,
    })
}

#[tauri::command]
pub async fn cli_env_probe(tool: String) -> Result<ApiOk<CliEnvProbeResult>, ApiErr> {
    let probe_tool = tool.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let (installed, version) = detect_cli_installation(&probe_tool);
        let configured = cli_has_system_config(&probe_tool);
        CliEnvProbeResult {
            tool: probe_tool.clone(),
            installed,
            version,
            configured,
            importable: is_managed_tool(&probe_tool) && installed && configured,
            install_guide: install_guide_for(&probe_tool),
        }
    })
    .await
    .map_err(|e| api_error("task_join_error", e.to_string()))?;

    api_ok(result, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub fn claude_profile_list() -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let profiles = crate::claude_profiles::list_claude_profiles_sp(&state);
    api_ok(
        serde_json::to_value(profiles).map_err(|e| api_error("serialize_error", e.to_string()))?,
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn claude_profile_resolve(query: String) -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let profile = crate::claude_profiles::resolve_claude_profile_sp(&state, &query)
        .ok_or_else(|| api_error("not_found", format!("Claude profile not found: {query}")))?;
    let config_dir = crate::claude_profiles::get_claude_profiles_dir()
        .map(|d| d.join(crate::claude_profiles::resolve_claude_dir_name_sp(&profile)))
        .map_err(|e| api_error("io_error", e))?;
    let mut obj = serde_json::to_value(&profile)
        .map_err(|e| api_error("serialize_error", e.to_string()))?
        .as_object()
        .cloned()
        .unwrap_or_default();
    obj.insert(
        "claude_config_dir".to_string(),
        Value::String(config_dir.to_string_lossy().to_string()),
    );
    api_ok(
        Value::Object(obj),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn claude_profile_set_default(profile_id: String) -> Result<ApiOk<Value>, ApiErr> {
    validate_provider_uuid_param(&profile_id).map_err(|e| api_error("invalid_payload", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let exists = state
        .providers
        .iter()
        .any(|p| p.id == profile_id && p.tool == "claude");
    if !exists {
        return Err(api_error(
            "invalid_payload",
            format!("Claude service provider not found: {profile_id}"),
        ));
    }
    state
        .active
        .insert("claude".to_string(), profile_id.clone());
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    api_ok(
        json!({ "profile_id": profile_id, "set_default": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn get_claude_config_dir(provider_id: String) -> Result<String, String> {
    validate_provider_uuid_param(&provider_id)?;
    resolve_claude_config_dir_for_provider_id(&provider_id).map(|d| d.to_string_lossy().to_string())
}

pub(crate) fn materialize_isolated_claude_profile(
    provider: &ServiceProviderRecord,
) -> Result<PathBuf, String> {
    if provider.tool != "claude" {
        return Err(format!(
            "Claude profile materialization only supports Claude providers: {}",
            provider.tool
        ));
    }

    let profile_dir = crate::claude_profiles::get_claude_profiles_dir()?
        .join(crate::claude_profiles::resolve_claude_dir_name_sp(provider));
    crate::claude_profiles::materialize_claude_settings_sp(provider, &profile_dir)?;
    Ok(profile_dir)
}

pub(crate) async fn materialize_isolated_claude_profile_async(
    provider: &ServiceProviderRecord,
) -> Result<PathBuf, String> {
    if provider.tool != "claude" {
        return Err(format!(
            "Claude profile materialization only supports Claude providers: {}",
            provider.tool
        ));
    }

    let profile_dir = crate::claude_profiles::get_claude_profiles_dir()?
        .join(crate::claude_profiles::resolve_claude_dir_name_sp(provider));
    crate::claude_profiles::materialize_claude_settings_sp_async(provider, &profile_dir).await?;
    Ok(profile_dir)
}

pub(crate) fn resolve_claude_profile_config_dir(query: &str) -> Result<PathBuf, String> {
    let state = load_service_providers_state()?;
    let provider = state
        .providers
        .iter()
        .find(|p| {
            p.tool == "claude"
                && (p.id == query || p.name == query || p.code.as_deref() == Some(query))
        })
        .cloned()
        .ok_or_else(|| format!("Claude profile not found: {query}"))?;
    materialize_isolated_claude_profile(&provider)
}

#[tauri::command]
pub fn claude_profile_materialize(provider_id: String) -> Result<ApiOk<Value>, ApiErr> {
    validate_provider_uuid_param(&provider_id).map_err(|e| api_error("invalid_payload", e))?;
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == "claude")
        .cloned()
        .ok_or_else(|| {
            api_error(
                "not_found",
                format!("Claude service provider not found: {provider_id}"),
            )
        })?;
    let profile_dir = materialize_isolated_claude_profile(&provider)
        .map_err(|e| api_error("profile_failed", e))?;
    api_ok(
        json!({ "materialized": true, "config_dir": profile_dir.to_string_lossy().to_string() }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}
