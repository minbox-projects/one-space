use crate::app_store::{
    infer_claude_api_format, infer_protocol_router_wire_api, lock_canonical_state_write,
    normalize_service_provider_record, now_ts, CanonicalStateWriteGuard, ProviderHistoryEntry,
    ServiceProviderRecord, ServiceProvidersState, PROVIDER_HISTORY_LIMIT,
};
use serde_json::{json, Value};
use std::collections::HashSet;

pub(crate) fn lock_service_provider_operation() -> Result<CanonicalStateWriteGuard, String> {
    lock_canonical_state_write()
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
