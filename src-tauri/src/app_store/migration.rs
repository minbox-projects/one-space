use super::{
    apply_provider_id_map_to_dependent_state, detect_cli_installation, is_managed_tool,
    load_migration_state, load_service_providers_state_with_id_map, local_workflow_presets_path,
    local_workflow_runs_path, migrate_providers_to_service_providers, normalize_service_provider_ids,
    now_ts, resolved_claude_model_mappings, save_migration_state, save_outbox_state,
    save_service_providers_internal, shared_profile_path, strip_legacy_claude_model_keys,
    CryptoService, EncryptedBlob, MigrationReport, MigrationState, OutboxState, ProviderCore,
    ProviderHistoryEntry, ProviderRecord, ProviderRuntimePolicy, ProvidersState, SessionRecord,
    SessionsState, StorageEngine, SCHEMA_VERSION,
};
use crate::{ai_env, ai_sessions, config, mcp_servers, secrets, storage};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs::{self};
use std::path::{Path, PathBuf};

pub(in crate::app_store) fn copy_if_exists(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(src, dst).map_err(|e| e.to_string())?;
    Ok(())
}

pub(in crate::app_store) fn backup_legacy_files(backup_id: &str) -> Result<PathBuf, String> {
    let backup_root = StorageEngine::backup_root()?.join(backup_id);
    fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;

    let data_dir = crate::get_data_dir()?;
    let app_dir = config::get_app_dir()?;

    let files = vec![
        data_dir.join("ai_providers.json"),
        data_dir.join("ai_sessions.json"),
        data_dir.join("secrets.json"),
        data_dir.join("snippets.json"),
        data_dir.join("bookmarks.json"),
        data_dir.join("notes.json"),
        data_dir.join("mcp_servers.json"),
        app_dir.join("config.json"),
    ];

    for file in files {
        if file.exists() {
            let rel = file
                .file_name()
                .ok_or("invalid file")?
                .to_string_lossy()
                .to_string();
            copy_if_exists(&file, &backup_root.join(rel))?;
        }
    }

    Ok(backup_root)
}

pub(in crate::app_store) fn build_new_providers_from_legacy() -> Result<ProvidersState, String> {
    let legacy = ai_env::get_ai_providers()?;
    let mut active = HashMap::new();
    if let Some(v) = legacy.active_claude {
        active.insert("claude".to_string(), v);
    }
    if let Some(v) = legacy.active_codex {
        active.insert("codex".to_string(), v);
    }
    if let Some(v) = legacy.active_antigravity {
        active.insert("antigravity".to_string(), v);
    }
    if let Some(v) = legacy.active_opencode {
        active.insert("opencode".to_string(), v);
    }

    let mut providers = Vec::new();
    for p in legacy.providers {
        let value = serde_json::to_value(&p).map_err(|e| e.to_string())?;
        let obj = value.as_object().cloned().unwrap_or_default();

        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let tool = obj
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        // Defensive: legacy loaders may still synthesize a `gemini` record even
        // though the persisted store was rewritten above.
        let tool = if tool == LEGACY_TOOL_ID {
            ANTIGRAVITY_TOOL_ID.to_string()
        } else {
            tool
        };
        let api_key = obj
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let base_url = obj
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let model = obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let is_enabled = obj.get("is_enabled").and_then(|v| v.as_bool());
        let provider_key = obj
            .get("provider_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Skip legacy auto-imported managed default providers when the corresponding CLI
        // binary is not installed on this machine.
        let default_import_name = match tool.as_str() {
            "claude" => "Imported Claude Config",
            "codex" => "Imported Codex Config",
            "antigravity" => "Imported Antigravity Config",
            _ => "",
        };
        let looks_like_empty_system_import = is_managed_tool(&tool)
            && name == default_import_name
            && api_key.trim().is_empty()
            && base_url.as_deref().unwrap_or("").trim().is_empty()
            && model.as_deref().unwrap_or("").trim().is_empty();
        if (id == format!("default-{}", tool) || looks_like_empty_system_import)
            && is_managed_tool(&tool)
        {
            let (installed, _) = detect_cli_installation(&tool);
            if !installed {
                continue;
            }
        }

        let mut tool_config = Map::new();
        for (k, v) in &obj {
            match k.as_str() {
                "id"
                | "name"
                | "tool"
                | "api_key"
                | "base_url"
                | "model"
                | "is_enabled"
                | "provider_key"
                | "history"
                | "claude_haiku_model"
                | "claude_sonnet_model"
                | "claude_opus_model" => {}
                _ => {
                    tool_config.insert(k.clone(), v.clone());
                }
            }
        }

        if tool == "claude" {
            if !tool_config.contains_key("claude_model_mappings") {
                let mut legacy_tool_config = tool_config.clone();
                for legacy_key in [
                    "claude_haiku_model",
                    "claude_sonnet_model",
                    "claude_opus_model",
                ] {
                    if let Some(value) = obj.get(legacy_key) {
                        legacy_tool_config.insert(legacy_key.to_string(), value.clone());
                    }
                }
                let mappings = resolved_claude_model_mappings(&legacy_tool_config);
                if mappings
                    .iter()
                    .any(|mapping| !mapping.upstream_model.trim().is_empty())
                {
                    tool_config.insert(
                        "claude_model_mappings".to_string(),
                        serde_json::to_value(&mappings).unwrap_or_else(|_| Value::Array(vec![])),
                    );
                }
            }
            strip_legacy_claude_model_keys(&mut tool_config);
        }

        let mut history = Vec::new();
        if let Some(Value::Array(arr)) = obj.get("history") {
            for item in arr {
                if let Some(ts) = item.get("timestamp").and_then(|v| v.as_u64()) {
                    let summary = item
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    history.push(ProviderHistoryEntry {
                        ts: ts / 1000,
                        action: "legacy-import".to_string(),
                        snapshot: None,
                        content: summary.clone(),
                        summary,
                    });
                }
            }
        }

        providers.push(ProviderRecord {
            core: ProviderCore {
                id,
                name,
                tool,
                api_key,
                code: None,
                base_url,
                model,
            },
            runtime_policy: ProviderRuntimePolicy {
                approval_policy: tool_config
                    .get("approval_policy")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                sandbox_mode: tool_config
                    .get("sandbox_mode")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            },
            favorite_at: obj.get("favorite_at").and_then(|v| v.as_u64()),
            tool_config,
            history,
            extra: Map::new(),
            is_enabled,
            provider_key,
        });
    }

    // Keep active bindings only when the provider still exists after filtering.
    active.retain(|tool, provider_id| {
        providers
            .iter()
            .any(|p| p.core.tool == *tool && p.core.id == *provider_id)
    });

    Ok(ProvidersState { active, providers })
}

pub(in crate::app_store) fn build_new_sessions_from_legacy() -> Result<SessionsState, String> {
    let legacy = ai_sessions::get_ai_sessions()?;
    let sessions = legacy
        .into_iter()
        .map(|s| SessionRecord {
            id: s.id,
            name: s.name,
            working_dir: s.working_dir,
            tool: s.model_type,
            tool_session_id: s.tool_session_id,
            model_name: None,
            name_source: "manual".to_string(),
            runtime_mode: "shared".to_string(),
            runtime_profile_id: None,
            preset_id: None,
            created_at: s.created_at,
            last_used_at: s.created_at,
            status: "active".to_string(),
            favorited_at: None,
            provider_id: None,
        })
        .collect();
    Ok(SessionsState {
        sessions,
        ..SessionsState::default()
    })
}

pub(in crate::app_store) fn migrate_content_file(
    read: fn() -> Result<String, String>,
    name: &str,
) -> Result<(), String> {
    let content = read()?;
    let parsed: Value = serde_json::from_str(&content).unwrap_or_else(|_| Value::Array(vec![]));
    let encrypted = CryptoService::encrypt_json(&parsed)?;
    StorageEngine::write_json(&StorageEngine::content_path(name)?, &encrypted)
}

pub(in crate::app_store) fn migrate_secrets() -> Result<(), String> {
    let data_dir = crate::get_data_dir()?;
    let legacy_path = data_dir.join("secrets.json");
    if !legacy_path.exists() {
        let empty = CryptoService::encrypt_json(&json!({}))?;
        return StorageEngine::write_json(&StorageEngine::secrets_path()?, &empty);
    }

    let content = fs::read_to_string(&legacy_path).map_err(|e| e.to_string())?;
    let mut legacy: secrets::Secrets = serde_json::from_str(&content).unwrap_or_default();

    let mut map = Map::new();
    if legacy.is_encrypted {
        for (k, v) in legacy.values.drain() {
            let dec = CryptoService::decrypt(&v).unwrap_or(v);
            map.insert(k, Value::String(dec));
        }
    } else {
        for (k, v) in legacy.values.drain() {
            map.insert(k, Value::String(v));
        }
    }

    let encrypted = CryptoService::encrypt_json(&Value::Object(map))?;
    StorageEngine::write_json(&StorageEngine::secrets_path()?, &encrypted)
}

pub(in crate::app_store) fn migrate_mcp() -> Result<(), String> {
    let mut state = mcp_servers::get_mcp_servers().unwrap_or_default();
    state.is_encrypted = true;
    for server in state.servers.iter_mut() {
        let _ = mcp_servers::encrypt_sensitive_data(server);
    }
    let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    StorageEngine::write_json(&StorageEngine::mcp_path()?, &value)
}

pub(in crate::app_store) fn migrate_config_shadow() -> Result<(), String> {
    let mut cfg = config::get_config()?;
    cfg.http_token = None;
    if let Some(ref mut proxy) = cfg.proxy {
        proxy.proxy_password = None;
    }
    let value = serde_json::to_value(cfg).map_err(|e| e.to_string())?;
    let path = StorageEngine::meta_dir()?.join("config_shadow.json");
    StorageEngine::write_json(&path, &value)
}

pub(in crate::app_store) fn write_migration_report(report: &MigrationReport) -> Result<(), String> {
    StorageEngine::write_json(&StorageEngine::migration_report_path()?, report)
}

const LEGACY_TOOL_ID: &str = "gemini";
const ANTIGRAVITY_TOOL_ID: &str = "antigravity";
const LEGACY_TOOL_KEY_PREFIX: &str = "gemini_";
const ANTIGRAVITY_TOOL_KEY_PREFIX: &str = "antigravity_";

/// Rewrite a single terminal-tool discriminator value. Only an exact `gemini`
/// identifier is replaced, so `google-gemini` protocol values and `gemini-*`
/// model slugs are preserved.
fn replace_legacy_tool_value(value: &mut Value) -> bool {
    if value.as_str() == Some(LEGACY_TOOL_ID) {
        *value = Value::String(ANTIGRAVITY_TOOL_ID.to_string());
        return true;
    }
    false
}

/// Rewrite only structured tool discriminator fields, never free-form text.
fn rewrite_tool_identifier(obj: &mut Map<String, Value>) -> bool {
    let mut changed = false;
    for key in ["tool", "model_type"] {
        if let Some(value) = obj.get_mut(key) {
            changed |= replace_legacy_tool_value(value);
        }
    }
    changed
}

/// Rename structured `gemini_*` keys (e.g. `gemini_auth_type`, `gemini_base_url`)
/// to `antigravity_*`. Never applied to free-form user text.
fn rename_legacy_tool_keys(obj: &mut Map<String, Value>) -> bool {
    let legacy_keys: Vec<String> = obj
        .keys()
        .filter(|key| key.starts_with(LEGACY_TOOL_KEY_PREFIX))
        .cloned()
        .collect();
    let mut changed = false;
    for key in legacy_keys {
        let renamed = format!(
            "{}{}",
            ANTIGRAVITY_TOOL_KEY_PREFIX,
            &key[LEGACY_TOOL_KEY_PREFIX.len()..]
        );
        if let Some(value) = obj.remove(&key) {
            changed = true;
            if !obj.contains_key(&renamed) {
                obj.insert(renamed, value);
            }
        }
    }
    changed
}

/// Rename a `gemini` key inside a tool-keyed map (active maps, launch commands,
/// permission modes) while leaving its value untouched.
fn rename_legacy_tool_map_key(obj: &mut Map<String, Value>) -> bool {
    let Some(value) = obj.remove(LEGACY_TOOL_ID) else {
        return false;
    };
    if !obj.contains_key(ANTIGRAVITY_TOOL_ID) {
        obj.insert(ANTIGRAVITY_TOOL_ID.to_string(), value);
    }
    true
}

fn replace_legacy_tool_in_array(value: &mut Value) -> bool {
    let Some(items) = value.as_array_mut() else {
        return false;
    };
    let mut changed = false;
    for item in items.iter_mut() {
        changed |= replace_legacy_tool_value(item);
    }
    changed
}

fn rewrite_provider_object(value: &mut Value) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let mut changed = rewrite_tool_identifier(obj);
    changed |= rename_legacy_tool_keys(obj);
    if let Some(tool_config) = obj
        .get_mut("tool_config")
        .and_then(|v| v.as_object_mut())
    {
        changed |= rename_legacy_tool_keys(tool_config);
    }
    if let Some(core) = obj.get_mut("core").and_then(|v| v.as_object_mut()) {
        changed |= rewrite_tool_identifier(core);
        changed |= rename_legacy_tool_keys(core);
    }
    changed
}

fn rewrite_legacy_providers_value(value: &mut Value) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if let Some(active) = obj.remove("active_gemini") {
        changed = true;
        if !obj.contains_key("active_antigravity") {
            obj.insert("active_antigravity".to_string(), active);
        }
    }
    if let Some(active) = obj.get_mut("active").and_then(|v| v.as_object_mut()) {
        changed |= rename_legacy_tool_map_key(active);
    }
    if let Some(providers) = obj.get_mut("providers").and_then(|v| v.as_array_mut()) {
        for provider in providers.iter_mut() {
            changed |= rewrite_provider_object(provider);
        }
    }
    changed
}

fn rewrite_service_providers_value(value: &mut Value) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if let Some(active) = obj.get_mut("active").and_then(|v| v.as_object_mut()) {
        changed |= rename_legacy_tool_map_key(active);
    }
    if let Some(providers) = obj.get_mut("providers").and_then(|v| v.as_array_mut()) {
        for provider in providers.iter_mut() {
            changed |= rewrite_provider_object(provider);
        }
    }
    changed
}

fn remove_legacy_tool_from_sessions(value: &mut Value) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if let Some(sessions) = obj.get_mut("sessions").and_then(|v| v.as_array_mut()) {
        let before = sessions.len();
        sessions.retain(|session| {
            session
                .get("tool")
                .and_then(|v| v.as_str())
                .map(|tool| tool != LEGACY_TOOL_ID)
                .unwrap_or(true)
        });
        changed |= sessions.len() != before;
    }
    if let Some(tools) = obj
        .get_mut("history_sync")
        .and_then(|v| v.as_object_mut())
        .and_then(|history| history.get_mut("tools"))
        .and_then(|v| v.as_object_mut())
    {
        changed |= tools.remove(LEGACY_TOOL_ID).is_some();
    }
    if let Some(tombstones) = obj.get_mut("tombstones").and_then(|v| v.as_array_mut()) {
        let before = tombstones.len();
        let prefix = format!("{}::", LEGACY_TOOL_ID);
        tombstones.retain(|entry| {
            entry
                .as_str()
                .map(|value| !value.starts_with(&prefix))
                .unwrap_or(true)
        });
        changed |= tombstones.len() != before;
    }
    changed
}

fn remove_legacy_tool_from_legacy_sessions(value: &mut Value) -> bool {
    let Some(items) = value.as_array_mut() else {
        return false;
    };
    let before = items.len();
    items.retain(|item| {
        item.get("model_type")
            .and_then(|v| v.as_str())
            .map(|tool| tool != LEGACY_TOOL_ID)
            .unwrap_or(true)
    });
    items.len() != before
}

fn rewrite_provider_presets_value(value: &mut Value) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if let Some(presets) = obj.get_mut("presets").and_then(|v| v.as_array_mut()) {
        for preset in presets.iter_mut() {
            let Some(preset_obj) = preset.as_object_mut() else {
                continue;
            };
            changed |= rewrite_tool_identifier(preset_obj);
            if let Some(endpoints) = preset_obj
                .get_mut("endpoints")
                .and_then(|v| v.as_object_mut())
            {
                changed |= rename_legacy_tool_keys(endpoints);
            }
            if let Some(template) = preset_obj
                .get_mut("template")
                .and_then(|v| v.as_object_mut())
            {
                changed |= rewrite_tool_identifier(template);
                changed |= rename_legacy_tool_keys(template);
            }
        }
    }
    changed
}

fn rewrite_workflow_records_value(value: &mut Value) -> bool {
    let Some(items) = value.as_array_mut() else {
        return false;
    };
    let mut changed = false;
    for item in items.iter_mut() {
        let Some(obj) = item.as_object_mut() else {
            continue;
        };
        changed |= rewrite_tool_identifier(obj);
        if let Some(default_models) = obj.get_mut("default_models") {
            changed |= replace_legacy_tool_in_array(default_models);
        }
    }
    changed
}

fn rewrite_workspaces_value(value: &mut Value) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if let Some(workspaces) = obj.get_mut("workspaces").and_then(|v| v.as_array_mut()) {
        for workspace in workspaces.iter_mut() {
            let Some(workspace_obj) = workspace.as_object_mut() else {
                continue;
            };
            changed |= rewrite_tool_identifier(workspace_obj);
            if let Some(default_models) = workspace_obj.get_mut("default_models") {
                changed |= replace_legacy_tool_in_array(default_models);
            }
        }
    }
    if let Some(bindings) = obj.get_mut("mcp_bindings").and_then(|v| v.as_array_mut()) {
        for binding in bindings.iter_mut() {
            let Some(binding_obj) = binding.as_object_mut() else {
                continue;
            };
            if let Some(enabled_models) = binding_obj.get_mut("enabled_models") {
                changed |= replace_legacy_tool_in_array(enabled_models);
            }
        }
    }
    changed
}

fn rewrite_source_default_models(obj: &mut Map<String, Value>, key: &str) -> bool {
    let Some(sources) = obj.get_mut(key).and_then(|v| v.as_array_mut()) else {
        return false;
    };
    let mut changed = false;
    for source in sources.iter_mut() {
        let Some(source_obj) = source.as_object_mut() else {
            continue;
        };
        if let Some(default_models) = source_obj.get_mut("default_models") {
            changed |= replace_legacy_tool_in_array(default_models);
        }
    }
    changed
}

fn rewrite_config_value(value: &mut Value) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if let Some(model) = obj.get_mut("default_ai_model") {
        changed |= replace_legacy_tool_value(model);
    }
    // Rename only the map keys; command/permission values remain user-authored.
    for key in ["ai_model_launch_commands", "ai_model_permission_modes"] {
        if let Some(map) = obj.get_mut(key).and_then(|v| v.as_object_mut()) {
            changed |= rename_legacy_tool_map_key(map);
        }
    }
    changed |= rewrite_source_default_models(obj, "skills_sources");
    changed |= rewrite_source_default_models(obj, "subagents_sources");
    changed
}

fn rewrite_shared_profile_value(value: &mut Value) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let mut changed = rewrite_source_default_models(obj, "skills_sources");
    changed |= rewrite_source_default_models(obj, "subagents_sources");
    changed
}

/// Read a plain or encrypted JSON file, run `rewrite`, and persist it in the
/// same form only when the rewrite changed something.
fn rewrite_json_file<F>(path: &Path, mut rewrite: F) -> Result<bool, String>
where
    F: FnMut(&mut Value) -> bool,
{
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(false);
    }

    let encrypted_blob = serde_json::from_str::<EncryptedBlob>(&content).ok();
    let mut value = if let Some(blob) = encrypted_blob.as_ref() {
        CryptoService::decrypt_json(blob)?
    } else {
        match serde_json::from_str::<Value>(&content) {
            Ok(value) => value,
            Err(_) => return Ok(false),
        }
    };

    if !rewrite(&mut value) {
        return Ok(false);
    }

    if encrypted_blob.is_some() {
        let blob = CryptoService::encrypt_json(&value)?;
        StorageEngine::write_json(path, &blob)?;
    } else {
        let serialized = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        StorageEngine::atomic_write(path, &serialized)?;
    }
    Ok(true)
}

/// Idempotent rewrite of stored `gemini` terminal-tool identifiers to
/// `antigravity` across legacy and canonical stores. Free-form user text,
/// `google-gemini` protocol values and `gemini-*` model slugs are untouched.
pub(in crate::app_store) fn migrate_gemini_identifiers_to_antigravity() -> Result<(), String> {
    let data_dir = crate::get_data_dir()?;

    // Legacy plain stores, rewritten before the typed legacy loaders read them.
    let _ = rewrite_json_file(
        &data_dir.join("ai_providers.json"),
        rewrite_legacy_providers_value,
    )?;
    let _ = rewrite_json_file(
        &data_dir.join("ai_sessions.json"),
        remove_legacy_tool_from_legacy_sessions,
    )?;

    // Canonical encrypted stores.
    let _ = rewrite_json_file(
        &StorageEngine::providers_path()?,
        rewrite_service_providers_value,
    )?;
    let _ = rewrite_json_file(
        &StorageEngine::sessions_path()?,
        remove_legacy_tool_from_sessions,
    )?;

    // Canonical plain stores.
    let _ = rewrite_json_file(
        &StorageEngine::provider_presets_path()?,
        rewrite_provider_presets_value,
    )?;
    let _ = rewrite_json_file(
        &local_workflow_presets_path()?,
        rewrite_workflow_records_value,
    )?;
    let _ = rewrite_json_file(
        &local_workflow_runs_path()?,
        rewrite_workflow_records_value,
    )?;
    let workspaces_path = data_dir.join("data").join("workspaces").join("state.json");
    let _ = rewrite_json_file(&workspaces_path, rewrite_workspaces_value)?;

    // Device/shared configuration.
    let config_path = config::get_app_dir()?.join("config.json");
    let _ = rewrite_json_file(&config_path, rewrite_config_value)?;
    if let Ok(shared_profile) = config::shared_profile_local_path() {
        let _ = rewrite_json_file(&shared_profile, rewrite_shared_profile_value)?;
    }

    if let Ok(cfg) = config::get_config() {
        for name in [
            "providers.json",
            "provider_presets.json",
            "workflow_presets.json",
        ] {
            let Ok(path) = shared_profile_path(&cfg, name) else {
                continue;
            };
            let result = match name {
                "providers.json" => rewrite_json_file(&path, rewrite_service_providers_value),
                "provider_presets.json" => {
                    rewrite_json_file(&path, rewrite_provider_presets_value)
                }
                _ => rewrite_json_file(&path, rewrite_workflow_records_value),
            };
            let _ = result?;
        }
    }

    Ok(())
}

pub(in crate::app_store) fn run_migration_impl() -> Result<MigrationState, String> {
    let mut state = load_migration_state().unwrap_or_default();
    let schema = StorageEngine::load_schema().unwrap_or_default();
    if state.migrated && schema.schema_version == SCHEMA_VERSION {
        return Ok(state);
    }

    state.in_progress = true;
    state.last_error = None;
    save_migration_state(&state)?;

    let started = now_ts();
    let backup_id = format!("backup-{}", started);
    let mut steps = Vec::new();

    let result = (|| -> Result<(), String> {
        let _backup_dir = backup_legacy_files(&backup_id)?;
        steps.push("backup".to_string());

        migrate_gemini_identifiers_to_antigravity()?;
        steps.push("antigravity-identifiers".to_string());

        migrate_config_shadow()?;
        steps.push("config".to_string());

        let provider_id_map = if StorageEngine::providers_path()?.exists() {
            let (_service_state, id_map) = load_service_providers_state_with_id_map()?;
            steps.push("providers".to_string());
            id_map
        } else {
            let providers = build_new_providers_from_legacy()?;
            let mut service_state = migrate_providers_to_service_providers(providers);
            let (id_map, _) = normalize_service_provider_ids(&mut service_state);
            save_service_providers_internal(&service_state)?;
            steps.push("providers".to_string());
            id_map
        };

        let sessions = build_new_sessions_from_legacy()?;
        let sessions_blob = CryptoService::encrypt_json(
            &serde_json::to_value(&sessions).map_err(|e| e.to_string())?,
        )?;
        StorageEngine::write_json(&StorageEngine::sessions_path()?, &sessions_blob)?;
        steps.push("sessions".to_string());

        migrate_secrets()?;
        steps.push("secrets".to_string());

        migrate_mcp()?;
        steps.push("mcp".to_string());

        apply_provider_id_map_to_dependent_state(&provider_id_map)?;
        if !provider_id_map.is_empty() {
            steps.push("provider-id-remap".to_string());
        }

        migrate_content_file(storage::read_snippets, "snippets")?;
        migrate_content_file(storage::read_bookmarks, "bookmarks")?;
        migrate_content_file(storage::read_notes, "notes")?;
        steps.push("content".to_string());

        let mut schema = StorageEngine::load_schema()?;
        schema.schema_version = SCHEMA_VERSION;
        schema.last_migrated_at = now_ts();
        schema.revision = schema.revision.saturating_add(1);
        StorageEngine::write_json(&StorageEngine::schema_path()?, &schema)?;

        let outbox = OutboxState::default();
        save_outbox_state(&outbox)?;

        Ok(())
    })();

    let finished = now_ts();

    match result {
        Ok(_) => {
            state.migrated = true;
            state.schema_version = SCHEMA_VERSION;
            state.last_migrated_at = Some(finished);
            state.last_backup_id = Some(backup_id.clone());
            state.in_progress = false;
            state.last_error = None;
            save_migration_state(&state)?;

            write_migration_report(&MigrationReport {
                started_at: started,
                finished_at: finished,
                success: true,
                backup_id,
                steps,
                error: None,
            })?;
            Ok(state)
        }
        Err(err) => {
            state.in_progress = false;
            state.last_error = Some(err.clone());
            save_migration_state(&state)?;

            let _ = write_migration_report(&MigrationReport {
                started_at: started,
                finished_at: finished,
                success: false,
                backup_id,
                steps,
                error: Some(err.clone()),
            });
            Err(err)
        }
    }
}

pub(in crate::app_store) fn rollback_from_backup(backup_id: &str) -> Result<(), String> {
    let backup_dir = StorageEngine::backup_root()?.join(backup_id);
    if !backup_dir.exists() {
        return Err("Backup not found".to_string());
    }

    let data_dir = crate::get_data_dir()?;
    let app_dir = config::get_app_dir()?;

    for file in [
        "ai_providers.json",
        "ai_sessions.json",
        "secrets.json",
        "snippets.json",
        "bookmarks.json",
        "notes.json",
        "mcp_servers.json",
    ] {
        let src = backup_dir.join(file);
        let dst = data_dir.join(file);
        if src.exists() {
            copy_if_exists(&src, &dst)?;
        }
    }

    let cfg = backup_dir.join("config.json");
    if cfg.exists() {
        copy_if_exists(&cfg, &app_dir.join("config.json"))?;
    }

    Ok(())
}

pub(in crate::app_store) fn cleanup_legacy_root_files() -> Result<(), String> {
    let data_dir = crate::get_data_dir()?;
    let checks = vec![
        (
            data_dir.join("ai_providers.json"),
            StorageEngine::providers_path()?,
        ),
        (
            data_dir.join("ai_sessions.json"),
            StorageEngine::sessions_path()?,
        ),
        (
            data_dir.join("secrets.json"),
            StorageEngine::secrets_path()?,
        ),
        (
            data_dir.join("snippets.json"),
            StorageEngine::content_path("snippets")?,
        ),
        (
            data_dir.join("bookmarks.json"),
            StorageEngine::content_path("bookmarks")?,
        ),
        (
            data_dir.join("notes.json"),
            StorageEngine::content_path("notes")?,
        ),
        (
            data_dir.join("mcp_servers.json"),
            StorageEngine::mcp_path()?,
        ),
    ];

    for (legacy, new_path) in checks {
        if legacy.exists() && new_path.exists() {
            let _ = fs::remove_file(legacy);
        }
    }
    Ok(())
}

pub(in crate::app_store) fn rotate_encrypted_blob_file(
    path: &Path,
    old_pass: &str,
    new_pass: &str,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(());
    }

    let plain_json = if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if blob.is_encrypted {
            match crate::crypto::decrypt(&blob.data, old_pass) {
                Ok(plain) => plain,
                Err(err) => {
                    // Do not fail the whole password-change flow for one incompatible/corrupted file.
                    // If payload itself looks like JSON, treat it as legacy plain content; otherwise skip.
                    if serde_json::from_str::<Value>(&blob.data).is_ok() {
                        eprintln!(
                            "rotate_encrypted_blob_file: decrypt failed but blob data is plain JSON, path={}, err={}",
                            path.display(),
                            err
                        );
                        blob.data
                    } else {
                        eprintln!(
                            "rotate_encrypted_blob_file: skip file due to decrypt failure, path={}, err={}",
                            path.display(),
                            err
                        );
                        return Ok(());
                    }
                }
            }
        } else {
            blob.data
        }
    } else {
        content
    };

    let parsed: Value = match serde_json::from_str(&plain_json) {
        Ok(v) => v,
        Err(err) => {
            eprintln!(
                "rotate_encrypted_blob_file: skip file due to invalid json, path={}, err={}",
                path.display(),
                err
            );
            return Ok(());
        }
    };
    let encrypted = crate::crypto::encrypt(&parsed.to_string(), new_pass)?;
    let blob = EncryptedBlob {
        is_encrypted: true,
        data: encrypted,
    };
    StorageEngine::write_json(path, &blob)
}

pub(in crate::app_store) fn rotate_mcp_state_password(
    old_pass: &str,
    new_pass: &str,
) -> Result<(), String> {
    let path = StorageEngine::mcp_path()?;
    if !path.exists() {
        return Ok(());
    }
    let mut state: mcp_servers::MCPServersState = StorageEngine::read_json(&path)?;
    if state.servers.is_empty() {
        return Ok(());
    }

    for server in state.servers.iter_mut() {
        if let Some(ref mut env) = server.env {
            for (_, value) in env.iter_mut() {
                if value.is_empty() || value.starts_with('$') || value.starts_with("${") {
                    continue;
                }
                let plain =
                    crate::crypto::decrypt(value, old_pass).unwrap_or_else(|_| value.clone());
                *value = crate::crypto::encrypt(&plain, new_pass)?;
            }
        }

        if let Some(ref mut headers) = server.headers {
            for (key, value) in headers.iter_mut() {
                let k = key.to_lowercase();
                if !(k.contains("auth")
                    || k.contains("key")
                    || k.contains("token")
                    || k.contains("secret"))
                {
                    continue;
                }
                if value.is_empty() || value.starts_with('$') || value.starts_with("${") {
                    continue;
                }
                let plain =
                    crate::crypto::decrypt(value, old_pass).unwrap_or_else(|_| value.clone());
                *value = crate::crypto::encrypt(&plain, new_pass)?;
            }
        }
    }
    state.is_encrypted = true;
    StorageEngine::write_json(&path, &state)
}

pub fn rotate_master_password_data(old_pass: &str, new_pass: &str) -> Result<(), String> {
    rotate_encrypted_blob_file(&StorageEngine::providers_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::sessions_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::launcher_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(&StorageEngine::secrets_path()?, old_pass, new_pass)?;
    rotate_encrypted_blob_file(
        &StorageEngine::content_path("snippets")?,
        old_pass,
        new_pass,
    )?;
    rotate_encrypted_blob_file(
        &StorageEngine::content_path("bookmarks")?,
        old_pass,
        new_pass,
    )?;
    rotate_encrypted_blob_file(&StorageEngine::content_path("notes")?, old_pass, new_pass)?;
    rotate_mcp_state_password(old_pass, new_pass)?;
    Ok(())
}

pub fn ensure_migrated_on_startup() -> Result<(), String> {
    run_migration_impl().map(|_| ())?;
    // Keep startup side-effects minimal: do not create a new local key before onboarding.
    let key_path = crate::crypto::get_local_key_path()?;
    if key_path.exists() {
        let pass = crate::crypto::get_or_init_master_password()?;
        rotate_master_password_data(&pass, &pass)?;
    }
    cleanup_legacy_root_files()?;
    Ok(())
}
