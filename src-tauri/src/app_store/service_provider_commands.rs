use super::{
    api_error, api_ok, apply_opencode_remove_projection, apply_provider_id_map_to_dependent_state,
    auto_import_system_provider_into_service_state, cli_has_system_config, detect_cli_installation,
    enqueue_sync_event, expand_home_dir_path, generate_provider_uuid, get_meta,
    infer_claude_api_format, infer_protocol_router_wire_api, is_managed_tool, is_uuid_v4,
    list_synced_device_providers, load_service_providers_state,
    materialize_isolated_claude_profile_async, normalize_protocol_router_wire_api,
    normalize_service_provider_ids, normalize_service_provider_record, now_ts, process_sync_queue,
    provider_import_key, read_system_provider, run_migration_impl, save_service_providers_internal,
    service_provider_matches_system_default, service_provider_to_legacy,
    validate_provider_uuid_param, validate_service_provider_reference, ApiErr, ApiMeta, ApiOk,
    ProviderHistoryEntry, ProviderImportDecision, ProviderImportPreviewItem,
    ProvidersImportPreview, ServiceProviderRecord, ServiceProvidersState, StorageEngine,
    PROVIDERS_EXPORT_VERSION, PROVIDER_HISTORY_LIMIT,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

// ─── Service Providers commands (new unified domain) ───────────────────────────

static SERVICE_PROVIDER_OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn lock_service_provider_operation() -> Result<MutexGuard<'static, ()>, String> {
    SERVICE_PROVIDER_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "service_provider_operation_unavailable".to_string())
}

pub(in crate::app_store) fn service_provider_to_value(sp: &ServiceProviderRecord) -> Value {
    let mut obj = json!({
        "id": sp.id,
        "name": sp.name,
        "tool": sp.tool,
        "api_key": sp.api_key,
    });
    if let Some(ref v) = sp.icon {
        obj["icon"] = json!(v);
    }
    if let Some(ref v) = sp.base_url {
        obj["base_url"] = json!(v);
    }
    if let Some(ref v) = sp.model {
        obj["model"] = json!(v);
        if sp.tool == "claude" {
            obj["claude_default_model"] = json!(v);
        }
    } else if sp.tool == "claude" {
        obj["claude_default_model"] = Value::Null;
    }
    if let Some(ref v) = sp.code {
        obj["code"] = json!(v);
    }
    if let Some(ref v) = sp.is_enabled {
        obj["is_enabled"] = json!(v);
    }
    if let Some(ref v) = sp.provider_key {
        obj["provider_key"] = json!(v);
    }
    if let Some(ref v) = sp.env_managed {
        obj["env_managed"] = json!(v);
    }
    if let Some(v) = sp.favorite_at {
        obj["favorite_at"] = json!(v);
    }
    if !sp.claude_model_mappings.is_empty() {
        obj["claude_model_mappings"] =
            serde_json::to_value(&sp.claude_model_mappings).unwrap_or(json!([]));
    }
    if let Some(ref v) = sp.claude_enable_tool_search {
        obj["claude_enable_tool_search"] = json!(v);
    }
    if let Some(ref v) = sp.claude_auto_memory_enabled {
        obj["claude_auto_memory_enabled"] = json!(v);
    }
    if let Some(ref v) = sp.claude_always_thinking_enabled {
        obj["claude_always_thinking_enabled"] = json!(v);
    }
    if let Some(ref v) = sp.claude_away_summary_enabled {
        obj["claude_away_summary_enabled"] = json!(v);
    }
    if let Some(ref v) = sp.claude_include_git_instructions {
        obj["claude_include_git_instructions"] = json!(v);
    }
    if let Some(ref v) = sp.claude_enable_attribution {
        obj["claude_enable_attribution"] = json!(v);
    }
    if sp.claude_api_format != "anthropic_messages" {
        obj["claude_api_format"] = json!(sp.claude_api_format);
    }
    if sp.claude_connection_mode != "native_anthropic" {
        obj["claude_connection_mode"] = json!(sp.claude_connection_mode);
    }
    if let Some(ref v) = sp.protocol_router_upstream_provider_id {
        obj["protocol_router_upstream_provider_id"] = json!(v);
    }
    if sp.protocol_router_wire_api != "open_ai_chat" {
        obj["protocol_router_wire_api"] = json!(sp.protocol_router_wire_api);
    }
    if sp.claude_auth_env_key != "ANTHROPIC_API_KEY" {
        obj["claude_auth_env_key"] = json!(sp.claude_auth_env_key);
    }
    if let Some(ref v) = sp.fetched_models {
        obj["fetched_models"] = json!(v);
    }
    // Pass through tool_config for backward compatibility
    if !sp.tool_config.is_empty() {
        obj["tool_config"] = serde_json::to_value(&sp.tool_config).unwrap_or(json!({}));
    }
    obj
}

fn redact_provider_secrets(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if matches!(key.as_str(), "api_key" | "apiKey") {
                    if child
                        .as_str()
                        .map(|value| !value.is_empty())
                        .unwrap_or(false)
                    {
                        *child = Value::String("********".to_string());
                    }
                } else {
                    redact_provider_secrets(child);
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                redact_provider_secrets(child);
            }
        }
        _ => {}
    }
}

pub(in crate::app_store) fn normalize_provider_value_for_history(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };

    for key in ["history", "favorite_at", "env_managed", "fetched_models"] {
        obj.remove(key);
    }

    if let Some(tool_config) = obj.get_mut("tool_config").and_then(|v| v.as_object_mut()) {
        for key in ["env_managed", "favorite_at", "history", "fetched_models"] {
            tool_config.remove(key);
        }
        if tool_config.is_empty() {
            obj.remove("tool_config");
        }
    }
}

pub(in crate::app_store) fn service_provider_history_snapshot(sp: &ServiceProviderRecord) -> Value {
    let mut snapshot = service_provider_to_value(sp);
    if let Some(obj) = snapshot.as_object_mut() {
        for (k, v) in &sp.tool_config {
            obj.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in &sp.extra {
            obj.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
    snapshot
}

pub(in crate::app_store) fn service_provider_history_comparison_value(
    sp: &ServiceProviderRecord,
) -> Value {
    let mut value = service_provider_history_snapshot(sp);
    normalize_provider_value_for_history(&mut value);
    value
}

pub(in crate::app_store) fn normalize_provider_history(history: &mut Vec<ProviderHistoryEntry>) {
    history.sort_by(|a, b| b.ts.cmp(&a.ts));
    history.truncate(PROVIDER_HISTORY_LIMIT);
}

pub(in crate::app_store) fn provider_history_from_value(
    value: Option<&Value>,
) -> Vec<ProviderHistoryEntry> {
    value
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<ProviderHistoryEntry>>(value).ok())
        .map(|mut history| {
            normalize_provider_history(&mut history);
            history
        })
        .unwrap_or_default()
}

pub(in crate::app_store) fn append_provider_history_if_changed(
    existing: Option<&ServiceProviderRecord>,
    next: &mut ServiceProviderRecord,
    action: &str,
) -> bool {
    let Some(old) = existing else {
        normalize_provider_history(&mut next.history);
        return false;
    };

    next.history = old.history.clone();
    if service_provider_history_comparison_value(old)
        == service_provider_history_comparison_value(next)
    {
        normalize_provider_history(&mut next.history);
        return false;
    }

    next.history.insert(
        0,
        ProviderHistoryEntry {
            ts: now_ts(),
            action: action.to_string(),
            snapshot: Some(service_provider_history_snapshot(old)),
            content: None,
            summary: Some(format!("{} provider changed", old.tool)),
        },
    );
    normalize_provider_history(&mut next.history);
    true
}

pub(in crate::app_store) fn merge_imported_service_provider(
    state: &mut ServiceProvidersState,
    mut record: ServiceProviderRecord,
) -> bool {
    normalize_service_provider_record(&mut record);
    if let Some(pos) = state.providers.iter().position(|p| p.id == record.id) {
        let existing = state.providers[pos].clone();
        let changed = append_provider_history_if_changed(Some(&existing), &mut record, "import");
        state.providers[pos] = record;
        changed
    } else {
        normalize_provider_history(&mut record.history);
        state.providers.push(record);
        false
    }
}

pub(in crate::app_store) fn service_provider_from_value(
    val: Value,
    existing: Option<&ServiceProviderRecord>,
) -> ServiceProviderRecord {
    let obj = val.as_object().cloned().unwrap_or_default();
    let explicit_empty_claude_default_model = obj
        .get("claude_default_model")
        .map(|value| match value {
            Value::String(s) => s.trim().is_empty(),
            Value::Null => true,
            _ => false,
        })
        .unwrap_or(false);
    let mut tool_config = existing.map(|e| e.tool_config.clone()).unwrap_or_default();
    // Merge any fields from the input that aren't top-level fields
    let top_level_keys: HashSet<&str> = [
        "id",
        "name",
        "tool",
        "api_key",
        "icon",
        "base_url",
        "model",
        "code",
        "is_enabled",
        "provider_key",
        "env_managed",
        "favorite_at",
        "history",
        "claude_api_format",
        "claude_connection_mode",
        "protocol_router_upstream_provider_id",
        "protocol_router_wire_api",
        "claude_auth_env_key",
        "claude_default_model",
        "claude_reasoning_effort",
        "claude_model_mappings",
        "claude_enable_tool_search",
        "claude_auto_memory_enabled",
        "claude_always_thinking_enabled",
        "claude_away_summary_enabled",
        "claude_include_git_instructions",
        "claude_enable_attribution",
        "fetched_models",
        "tool_config",
    ]
    .into_iter()
    .collect();
    for (k, v) in &obj {
        if !top_level_keys.contains(k.as_str()) {
            tool_config.insert(k.clone(), v.clone());
        }
    }
    // If tool_config was explicitly provided, merge it
    if let Some(tc) = obj.get("tool_config").and_then(|v| v.as_object()) {
        for (k, v) in tc {
            tool_config.insert(k.clone(), v.clone());
        }
    }
    for mirrored_key in ["claude_default_model", "claude_reasoning_effort"] {
        if let Some(value) = obj.get(mirrored_key) {
            let keep_value = match value {
                Value::String(s) => !s.trim().is_empty(),
                Value::Null => false,
                _ => true,
            };
            if keep_value {
                tool_config.insert(mirrored_key.to_string(), value.clone());
            } else {
                tool_config.remove(mirrored_key);
            }
        }
    }

    let mut record = ServiceProviderRecord {
        id: obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        name: obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        tool: obj
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        icon: obj
            .get("icon")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        api_key: obj
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        base_url: obj
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model: obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        claude_api_format: infer_claude_api_format(
            obj.get("claude_api_format")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("claude_api_format"))
                        .and_then(|v| v.as_str())
                }),
            obj.get("claude_connection_mode")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("claude_connection_mode"))
                        .and_then(|v| v.as_str())
                }),
            obj.get("protocol_router_wire_api")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("wire_api").and_then(|v| v.as_str()))
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("wire_api"))
                        .and_then(|v| v.as_str())
                })
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("protocol_router_wire_api"))
                        .and_then(|v| v.as_str())
                }),
        ),
        claude_connection_mode: "native_anthropic".to_string(),
        protocol_router_upstream_provider_id: obj
            .get("protocol_router_upstream_provider_id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        protocol_router_wire_api: infer_protocol_router_wire_api(
            obj.get("protocol_router_wire_api")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("wire_api").and_then(|v| v.as_str()))
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("wire_api"))
                        .and_then(|v| v.as_str())
                })
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("protocol_router_wire_api"))
                        .and_then(|v| v.as_str())
                }),
            obj.get("claude_api_format")
                .and_then(|v| v.as_str())
                .unwrap_or("anthropic_messages"),
            obj.get("claude_connection_mode")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    obj.get("tool_config")
                        .and_then(|v| v.as_object())
                        .and_then(|tc| tc.get("claude_connection_mode"))
                        .and_then(|v| v.as_str())
                }),
        ),
        claude_auth_env_key: obj
            .get("claude_auth_env_key")
            .and_then(|v| v.as_str())
            .unwrap_or("ANTHROPIC_API_KEY")
            .to_string(),
        claude_model_mappings: obj
            .get("claude_model_mappings")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        claude_enable_tool_search: obj
            .get("claude_enable_tool_search")
            .and_then(|v| v.as_bool()),
        claude_auto_memory_enabled: obj
            .get("claude_auto_memory_enabled")
            .and_then(|v| v.as_bool()),
        claude_always_thinking_enabled: obj
            .get("claude_always_thinking_enabled")
            .and_then(|v| v.as_bool()),
        claude_away_summary_enabled: obj
            .get("claude_away_summary_enabled")
            .and_then(|v| v.as_bool()),
        claude_include_git_instructions: obj
            .get("claude_include_git_instructions")
            .and_then(|v| v.as_bool()),
        claude_enable_attribution: obj
            .get("claude_enable_attribution")
            .and_then(|v| v.as_bool()),
        code: obj
            .get("code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        is_enabled: obj.get("is_enabled").and_then(|v| v.as_bool()),
        provider_key: obj
            .get("provider_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        env_managed: obj.get("env_managed").and_then(|v| v.as_bool()),
        favorite_at: obj
            .get("favorite_at")
            .and_then(|v| v.as_u64())
            .or_else(|| existing.and_then(|e| e.favorite_at)),
        tool_config,
        history: existing
            .map(|e| e.history.clone())
            .unwrap_or_else(|| provider_history_from_value(obj.get("history"))),
        extra: existing.map(|e| e.extra.clone()).unwrap_or_default(),
        fetched_models: obj
            .get("fetched_models")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
    };
    if explicit_empty_claude_default_model {
        record.model = None;
        record.tool_config.remove("claude_default_model");
    }
    normalize_service_provider_record(&mut record);
    record
}

#[tauri::command]
pub fn service_providers_list() -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let providers: Vec<Value> = state
        .providers
        .iter()
        .map(|provider| {
            let mut value = service_provider_to_legacy(provider);
            redact_provider_secrets(&mut value);
            value
        })
        .collect();
    let payload = json!({
        "active": state.active,
        "active_claude": state.active.get("claude"),
        "active_codex": state.active.get("codex"),
        "active_gemini": state.active.get("gemini"),
        "active_opencode": state.active_opencode,
        "providers": providers,
    });
    api_ok(payload, get_meta().map_err(|e| api_error("io_error", e))?)
}

fn read_opencode_provider_config_at_home(
    home_dir: &Path,
    provider_key: &str,
) -> Result<Value, String> {
    let path = home_dir
        .join(".config")
        .join("opencode")
        .join("opencode.json");
    let content =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let root: Value = serde_json::from_str(&content)
        .map_err(|e| format!("invalid JSON in {}: {e}", path.display()))?;
    let provider_map = root
        .as_object()
        .ok_or_else(|| "OpenCode config root must be an object".to_string())?
        .get("provider")
        .and_then(Value::as_object)
        .ok_or_else(|| "OpenCode config provider must be an object".to_string())?;
    let provider = provider_map
        .get(provider_key)
        .ok_or_else(|| format!("OpenCode provider key not found: {provider_key}"))?;
    if !provider.is_object() {
        return Err(format!(
            "OpenCode provider '{provider_key}' must be an object"
        ));
    }
    Ok(provider.clone())
}

#[tauri::command]
pub fn service_provider_read_opencode_config(provider_key: String) -> Result<ApiOk<Value>, ApiErr> {
    if provider_key.is_empty() {
        return Err(api_error("invalid_payload", "provider_key is required"));
    }
    let home_dir =
        dirs::home_dir().ok_or_else(|| api_error("io_error", "home directory not found"))?;
    let provider = read_opencode_provider_config_at_home(&home_dir, &provider_key)
        .map_err(|e| api_error("opencode_config_read_failed", e))?;
    api_ok(provider, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn service_providers_upsert(
    app: tauri::AppHandle,
    provider: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    let response = service_providers_upsert_inner(provider).await?;
    enqueue_sync_event("service_providers", "service_providers_upsert")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    Ok(response)
}

pub(in crate::app_store) async fn service_providers_upsert_inner(
    provider: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    let operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let obj = provider
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "provider must be object"))?;
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tool = obj
        .get("tool")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if id.is_empty() || tool.is_empty() {
        return Err(api_error("invalid_payload", "provider id/tool required"));
    }

    // Validate code for Claude
    if tool == "claude" {
        if let Some(ref code) = obj.get("code").and_then(|v| v.as_str()) {
            let code_trim = code.trim().to_lowercase();
            if code_trim.is_empty() {
                return Err(api_error("invalid_payload", "code is required"));
            }
            if !code_trim
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(api_error(
                    "invalid_payload",
                    "code can only contain ASCII letters, numbers, hyphens, and underscores",
                ));
            }
        }
    }

    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;

    // Validate code uniqueness
    if tool == "claude" {
        if let Some(ref code) = obj.get("code").and_then(|v| v.as_str()) {
            let code_trim = code.trim().to_lowercase();
            let duplicate = state.providers.iter().any(|p| {
                p.tool == "claude"
                    && p.id != id
                    && p.code.as_ref().map(|c| c.trim().to_lowercase()) == Some(code_trim.clone())
            });
            if duplicate {
                return Err(api_error(
                    "invalid_payload",
                    format!(
                        "code '{}' is already used by another Claude service provider",
                        code_trim
                    ),
                ));
            }
        }
    }

    let existing = state.providers.iter().find(|p| p.id == id).cloned();
    let final_id = existing
        .as_ref()
        .map(|provider| provider.id.clone())
        .unwrap_or_else(|| {
            if is_uuid_v4(&id) {
                id.clone()
            } else {
                generate_provider_uuid()
            }
        });
    let mut normalized_obj = obj.clone();
    normalized_obj.insert("id".to_string(), Value::String(final_id.clone()));

    // Handle secret placeholder: if api_key is ******** and existing has a real key, preserve it
    let mut record = service_provider_from_value(Value::Object(normalized_obj), existing.as_ref());
    if record.api_key == "********" {
        if let Some(ref ex) = existing {
            if !ex.api_key.is_empty() && ex.api_key != "********" {
                record.api_key = ex.api_key.clone();
            }
        }
    }

    // Ensure default claude fields
    if record.claude_api_format.is_empty() {
        record.claude_api_format = "anthropic_messages".to_string();
    }
    if record.claude_connection_mode.is_empty() {
        record.claude_connection_mode = "native_anthropic".to_string();
    }
    if record.tool == "claude"
        && (record.claude_api_format == "open_ai_chat"
            || record.claude_api_format == "open_ai_responses")
    {
        record.claude_connection_mode = "protocol_router".to_string();
        record.protocol_router_wire_api =
            normalize_protocol_router_wire_api(&record.claude_api_format);
    }
    record.protocol_router_wire_api =
        normalize_protocol_router_wire_api(&record.protocol_router_wire_api);
    if record.claude_auth_env_key.is_empty() {
        record.claude_auth_env_key = "ANTHROPIC_API_KEY".to_string();
    }
    append_provider_history_if_changed(existing.as_ref(), &mut record, "upsert");

    if let Some(pos) = state.providers.iter().position(|p| p.id == record.id) {
        state.providers[pos] = record.clone();
    } else {
        state.providers.push(record.clone());
    }

    let (id_map, normalized_ids) = normalize_service_provider_ids(&mut state);
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    if normalized_ids {
        apply_provider_id_map_to_dependent_state(&id_map).map_err(|e| api_error("io_error", e))?;
    }
    drop(operation);
    if record.tool == "claude" {
        materialize_isolated_claude_profile_async(&record)
            .await
            .map_err(|e| api_error("profile_failed", e))?;
    }

    api_ok(
        service_provider_to_legacy(&record),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_providers_delete(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    validate_provider_uuid_param(&provider_id).map_err(|e| api_error("invalid_payload", e))?;
    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let original = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    if !original.providers.iter().any(|p| p.id == provider_id) {
        return Err(api_error("not_found", "service provider not found"));
    }
    let mut next = original;
    next.providers.retain(|provider| provider.id != provider_id);
    next.active.retain(|_, active_id| *active_id != provider_id);
    next.active_opencode
        .retain(|active_id| *active_id != provider_id);
    let schema = save_service_providers_internal(&next).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_delete")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_providers_set_active(
    app: tauri::AppHandle,
    tool: String,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    validate_service_provider_reference(&tool, &provider_id)
        .map_err(|e| api_error("invalid_payload", e))?;
    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    if tool == "opencode" {
        if !state.active_opencode.contains(&provider_id) {
            state.active_opencode.push(provider_id.clone());
        }
    } else {
        state.active.insert(tool.clone(), provider_id.clone());
    }
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_set_active")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "tool": tool, "provider_id": provider_id }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_providers_set_inactive(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    validate_provider_uuid_param(&provider_id).map_err(|e| api_error("invalid_payload", e))?;
    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let provider = state
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.tool == "opencode")
        .cloned()
        .ok_or_else(|| api_error("not_found", "opencode provider not found"))?;
    state.active_opencode.retain(|id| id != &provider_id);
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;

    apply_opencode_remove_projection(&provider).map_err(|e| api_error("projection_failed", e))?;

    enqueue_sync_event("service_providers", "service_providers_set_inactive")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "provider_id": provider_id, "inactive": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_providers_set_env_managed(
    app: tauri::AppHandle,
    provider_id: String,
    env_managed: bool,
) -> Result<ApiOk<Value>, ApiErr> {
    validate_provider_uuid_param(&provider_id).map_err(|e| api_error("invalid_payload", e))?;
    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let p = state
        .providers
        .iter_mut()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| api_error("not_found", "service provider not found"))?;
    p.env_managed = Some(env_managed);
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_set_env_managed")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        json!({ "provider_id": provider_id, "env_managed": env_managed }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

pub(in crate::app_store) fn set_service_provider_favorite_impl(
    state: &mut ServiceProvidersState,
    provider_id: &str,
    favorite: bool,
) -> Result<ServiceProviderRecord, ApiErr> {
    let record = state
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| api_error("not_found", "service provider not found"))?;

    if favorite {
        if record.favorite_at.is_none() {
            record.favorite_at = Some(now_ts());
        }
    } else {
        record.favorite_at = None;
    }

    Ok(record.clone())
}

#[tauri::command]
pub async fn service_providers_set_favorite(
    app: tauri::AppHandle,
    provider_id: String,
    favorite: bool,
) -> Result<ApiOk<Value>, ApiErr> {
    validate_provider_uuid_param(&provider_id).map_err(|e| api_error("invalid_payload", e))?;
    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let updated = set_service_provider_favorite_impl(&mut state, &provider_id, favorite)?;
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_set_favorite")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });
    api_ok(
        service_provider_to_legacy(&updated),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn service_providers_export(output_path: String) -> Result<ApiOk<Value>, ApiErr> {
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let providers: Vec<Value> = state
        .providers
        .iter()
        .map(service_provider_to_value)
        .collect();
    let payload = json!({
        "format": "onespace-service-providers",
        "version": PROVIDERS_EXPORT_VERSION,
        "exported_at": now_ts(),
        "active": state.active,
        "providers": providers,
    });
    let content = serde_json::to_string_pretty(&payload)
        .map_err(|e| api_error("serialize_error", e.to_string()))?;
    let expanded_output_path =
        expand_home_dir_path(&output_path).map_err(|e| api_error("io_error", e))?;
    let final_output_path = if expanded_output_path.is_dir() {
        expanded_output_path.join("onespace-service-providers-export.json")
    } else {
        expanded_output_path
    };
    StorageEngine::atomic_write(&final_output_path, &content)
        .map_err(|e| api_error("io_error", e))?;
    api_ok(
        json!({
            "path": final_output_path.to_string_lossy().to_string(),
            "count": payload.get("providers").and_then(|v| v.as_array()).map(|arr| arr.len()).unwrap_or(0)
        }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn service_providers_import_preview(import_path: String) -> Result<ApiOk<Value>, ApiErr> {
    let expanded =
        expand_home_dir_path(&import_path).map_err(|e| api_error("invalid_payload", e))?;
    let content =
        fs::read_to_string(&expanded).map_err(|e| api_error("io_error", e.to_string()))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| api_error("invalid_payload", e.to_string()))?;
    let imported = parse_service_providers_import_value(value)?;
    let state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let preview = build_service_providers_import_preview(&state, &imported);
    api_ok(
        serde_json::to_value(preview).map_err(|e| api_error("serialize_error", e.to_string()))?,
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn service_providers_import_apply(
    app: tauri::AppHandle,
    import_path: String,
    decisions: Vec<ProviderImportDecision>,
) -> Result<ApiOk<Value>, ApiErr> {
    let expanded =
        expand_home_dir_path(&import_path).map_err(|e| api_error("invalid_payload", e))?;
    let content =
        fs::read_to_string(&expanded).map_err(|e| api_error("io_error", e.to_string()))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| api_error("invalid_payload", e.to_string()))?;

    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    let imported = parse_service_providers_import_value(value)?;
    let candidates = service_provider_import_candidates(&state, &imported);
    let decision_map =
        decisions
            .into_iter()
            .try_fold(HashMap::<String, String>::new(), |mut acc, decision| {
                let action = decision.action.trim().to_lowercase();
                if action != "overwrite" && action != "new" {
                    return Err(api_error(
                        "invalid_payload",
                        format!("invalid import action: {}", decision.action),
                    ));
                }
                acc.insert(decision.import_key, action);
                Ok(acc)
            })?;

    let mut id_map = HashMap::<String, String>::new();
    let mut overwritten = 0usize;
    let mut created = 0usize;
    for candidate in candidates {
        let mut record = candidate.record;
        let action = if candidate.conflict_existing_id.is_some() {
            decision_map
                .get(&candidate.import_key)
                .map(|v| v.as_str())
                .unwrap_or("overwrite")
        } else {
            "new"
        };
        let original_id = record.id.clone();
        if let Some(existing_id) = candidate.conflict_existing_id {
            if action == "overwrite" {
                record.id = existing_id.clone();
                merge_imported_service_provider(&mut state, record);
                id_map.insert(candidate.import_key, existing_id);
                overwritten = overwritten.saturating_add(1);
            } else {
                record.id = generate_provider_uuid();
                let new_id = record.id.clone();
                merge_imported_service_provider(&mut state, record);
                id_map.insert(candidate.import_key, new_id);
                created = created.saturating_add(1);
            }
        } else {
            if state
                .providers
                .iter()
                .any(|provider| provider.id == record.id)
            {
                record.id = generate_provider_uuid();
            }
            let final_id = record.id.clone();
            merge_imported_service_provider(&mut state, record);
            id_map.insert(candidate.import_key, final_id);
            created = created.saturating_add(1);
        }
        id_map
            .entry(provider_import_key("", &original_id))
            .or_insert_with(String::new);
    }

    let mut active_restored = 0usize;
    for (tool, provider_id) in imported.active {
        let key = provider_import_key(&tool, &provider_id);
        if let Some(final_id) = id_map.get(&key).filter(|value| !value.is_empty()) {
            state.active.insert(tool, final_id.clone());
            active_restored = active_restored.saturating_add(1);
        }
    }

    let (normalized_id_map, _) = normalize_service_provider_ids(&mut state);
    if !normalized_id_map.is_empty() {
        apply_provider_id_map_to_dependent_state(&normalized_id_map)
            .map_err(|e| api_error("io_error", e))?;
    }
    state.active.retain(|tool, provider_id| {
        state
            .providers
            .iter()
            .any(|provider| provider.tool == *tool && provider.id == *provider_id)
    });

    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("service_providers", "service_providers_import_apply")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({
            "imported": overwritten.saturating_add(created),
            "overwritten": overwritten,
            "created": created,
            "active_restored": active_restored,
            "total": state.providers.len(),
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

struct ServiceProviderImportCandidate {
    import_key: String,
    record: ServiceProviderRecord,
    conflict_existing_id: Option<String>,
    conflict_existing_name: Option<String>,
    conflict_reason: Option<String>,
}

fn parse_service_providers_import_value(value: Value) -> Result<ServiceProvidersState, ApiErr> {
    if let Ok(imported) = serde_json::from_value::<ServiceProvidersState>(value.clone()) {
        return Ok(imported);
    }
    let Some(obj) = value.as_object() else {
        return Err(api_error(
            "invalid_payload",
            "import payload must be a service providers object",
        ));
    };
    let providers = obj
        .get("providers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| api_error("invalid_payload", "import payload must contain providers"))?
        .iter()
        .enumerate()
        .map(|(idx, value)| {
            let Some(provider_obj) = value.as_object() else {
                return Err(api_error(
                    "invalid_payload",
                    format!("provider #{} must be an object", idx.saturating_add(1)),
                ));
            };
            if provider_obj.contains_key("core") {
                return Err(api_error(
                    "invalid_payload",
                    format!(
                        "provider #{} uses old ProvidersState core schema",
                        idx.saturating_add(1)
                    ),
                ));
            }
            for key in ["id", "name", "tool"] {
                let valid = provider_obj
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false);
                if !valid {
                    return Err(api_error(
                        "invalid_payload",
                        format!("provider #{} missing {key}", idx.saturating_add(1)),
                    ));
                }
            }
            Ok(service_provider_from_value(value.clone(), None))
        })
        .collect::<Result<Vec<_>, ApiErr>>()?;
    let active = obj
        .get("active")
        .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
        .unwrap_or_default();
    Ok(ServiceProvidersState {
        active,
        active_opencode: Vec::new(),
        providers,
    })
}

fn service_provider_import_candidates(
    state: &ServiceProvidersState,
    imported: &ServiceProvidersState,
) -> Vec<ServiceProviderImportCandidate> {
    imported
        .providers
        .iter()
        .map(|record| {
            let by_id = state
                .providers
                .iter()
                .find(|existing| existing.id == record.id);
            let by_name = state.providers.iter().find(|existing| {
                existing.tool == record.tool
                    && !record.name.trim().is_empty()
                    && existing
                        .name
                        .trim()
                        .eq_ignore_ascii_case(record.name.trim())
            });
            let conflict = by_id.or(by_name);
            ServiceProviderImportCandidate {
                import_key: provider_import_key(&record.tool, &record.id),
                record: record.clone(),
                conflict_existing_id: conflict.map(|existing| existing.id.clone()),
                conflict_existing_name: conflict.map(|existing| existing.name.clone()),
                conflict_reason: if by_id.is_some() {
                    Some("id".to_string())
                } else if by_name.is_some() {
                    Some("name".to_string())
                } else {
                    None
                },
            }
        })
        .collect()
}

fn build_service_providers_import_preview(
    state: &ServiceProvidersState,
    imported: &ServiceProvidersState,
) -> ProvidersImportPreview {
    let candidates = service_provider_import_candidates(state, imported);
    let items = candidates
        .into_iter()
        .map(|candidate| ProviderImportPreviewItem {
            import_key: candidate.import_key,
            id: candidate.record.id,
            name: candidate.record.name,
            tool: candidate.record.tool,
            model: candidate.record.model,
            conflict: candidate.conflict_existing_id.is_some(),
            conflict_reason: candidate.conflict_reason,
            existing_id: candidate.conflict_existing_id,
            existing_name: candidate.conflict_existing_name,
        })
        .collect::<Vec<_>>();
    ProvidersImportPreview {
        active: imported.active.clone(),
        total: items.len(),
        conflicts: items.iter().filter(|item| item.conflict).count(),
        items,
    }
}

#[tauri::command]
pub fn service_providers_list_synced_other_devices() -> Result<ApiOk<Vec<Value>>, ApiErr> {
    let devices = list_synced_device_providers()?
        .into_iter()
        .map(|device| serde_json::to_value(device).unwrap_or(Value::Null))
        .collect();
    api_ok(devices, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn service_providers_auto_import_from_system(
    app: tauri::AppHandle,
    tool: String,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    if !is_managed_tool(&tool) {
        return Err(api_error(
            "invalid_tool",
            "tool does not support env managed import",
        ));
    }

    let (installed, _) = detect_cli_installation(&tool);
    if !installed {
        return api_ok(
            json!({ "imported": false, "reason": "not_installed" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }
    if !cli_has_system_config(&tool) {
        return api_ok(
            json!({ "imported": false, "reason": "not_configured" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let _operation = lock_service_provider_operation().map_err(|e| api_error("io_error", e))?;
    let mut state = load_service_providers_state().map_err(|e| api_error("io_error", e))?;
    if state
        .providers
        .iter()
        .any(|provider| service_provider_matches_system_default(provider, &tool))
    {
        return api_ok(
            json!({ "imported": false, "reason": "provider_exists" }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let provider = read_system_provider(&tool).ok_or_else(|| {
        api_error(
            "import_failed",
            "failed to parse system config for selected tool",
        )
    })?;
    let outcome = auto_import_system_provider_into_service_state(&mut state, &tool, provider)
        .map_err(|e| api_error("import_failed", e))?;
    if !outcome.imported {
        return api_ok(
            json!({
                "imported": false,
                "reason": outcome.reason.unwrap_or("skipped")
            }),
            get_meta().map_err(|e| api_error("io_error", e))?,
        );
    }

    let (id_map, changed_ids) = normalize_service_provider_ids(&mut state);
    let schema = save_service_providers_internal(&state).map_err(|e| api_error("io_error", e))?;
    if changed_ids {
        apply_provider_id_map_to_dependent_state(&id_map).map_err(|e| api_error("io_error", e))?;
    }
    enqueue_sync_event("service_providers", "auto_import_system_config")
        .map_err(|e| api_error("sync_error", e))?;

    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        json!({
            "imported": true,
            "provider_id": outcome.provider_id.unwrap_or_default(),
            "tool": tool,
            "activated": outcome.activated,
            "missing_fields": outcome.missing_fields
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[cfg(test)]
mod opencode_config_read_tests {
    use super::*;

    fn with_opencode_config(name: &str, content: &str, test: impl FnOnce(&Path)) {
        let home = std::env::temp_dir().join(format!(
            "onespace-opencode-config-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        let path = home.join(".config").join("opencode").join("opencode.json");
        fs::create_dir_all(path.parent().expect("config parent")).expect("create config dir");
        fs::write(path, content).expect("write OpenCode config");
        test(&home);
        fs::remove_dir_all(home).expect("remove temp home");
    }

    #[test]
    fn reads_only_the_requested_opencode_provider_and_preserves_all_fields() {
        with_opencode_config(
            "read-provider",
            r#"{
                    "provider": {
                        "target": {
                            "name": "Latest",
                            "options": {"apiKey": "new-key", "nested": {"keep": true}},
                            "models": {"latest-model": {"limit": {"context": 200000}}},
                            "unknownTopLevel": [1, 2, 3]
                        },
                        "other": {"name": "Untouched"}
                    }
                }"#,
            |home| {
                let provider = read_opencode_provider_config_at_home(home, "target")
                    .expect("read target provider");

                assert_eq!(provider["name"], "Latest");
                assert_eq!(provider["options"]["nested"]["keep"], true);
                assert_eq!(
                    provider["models"]["latest-model"]["limit"]["context"],
                    200000
                );
                assert_eq!(provider["unknownTopLevel"], json!([1, 2, 3]));
                assert!(provider.get("other").is_none());
            },
        );
    }

    #[test]
    fn rejects_missing_provider_key_and_invalid_provider_structure() {
        with_opencode_config(
            "invalid-provider",
            r#"{"provider":{"valid":{"name":"Valid"},"invalid":"not-an-object"}}"#,
            |home| {
                let missing = read_opencode_provider_config_at_home(home, "missing")
                    .expect_err("missing key must fail");
                assert!(missing.contains("provider key not found: missing"));

                let invalid = read_opencode_provider_config_at_home(home, "invalid")
                    .expect_err("non-object provider must fail");
                assert!(invalid.contains("provider 'invalid' must be an object"));
            },
        );
    }

    #[test]
    fn provider_list_redaction_covers_top_level_nested_and_history_secrets() {
        let plaintext = "gateway-plaintext-fixture";
        let mut value = json!({
            "api_key": plaintext,
            "options": { "apiKey": plaintext },
            "history": [{ "snapshot": { "api_key": plaintext } }]
        });
        redact_provider_secrets(&mut value);
        assert_eq!(value["api_key"], "********");
        assert_eq!(value["options"]["apiKey"], "********");
        assert_eq!(value["history"][0]["snapshot"]["api_key"], "********");
        assert!(!serde_json::to_string(&value).unwrap().contains(plaintext));
    }
}

// ─── End service_providers commands ────────────────────────────────────────────
