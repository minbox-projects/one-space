use super::{
    generate_provider_uuid, is_managed_tool, is_placeholder_string,
    normalize_service_provider_record, service_provider_to_value, CryptoService, EncryptedBlob,
    LegacyProvidersView, ServiceProviderRecord, ServiceProvidersState, SyncedDeviceProviderLite,
};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self};
use std::path::{Path, PathBuf};

pub(in crate::app_store) fn service_provider_to_legacy(sp: &ServiceProviderRecord) -> Value {
    let mut map = service_provider_to_value(sp)
        .as_object()
        .cloned()
        .unwrap_or_default();
    for (k, v) in &sp.tool_config {
        map.entry(k.clone()).or_insert_with(|| v.clone());
    }
    for (k, v) in &sp.extra {
        map.entry(k.clone()).or_insert_with(|| v.clone());
    }
    if !sp.history.is_empty() {
        let arr: Vec<Value> = sp
            .history
            .iter()
            .map(|h| {
                let mut item = json!({
                    "ts": h.ts,
                    "timestamp": h.ts.saturating_mul(1000),
                    "action": h.action,
                });
                if let Some(snapshot) = &h.snapshot {
                    item["snapshot"] = snapshot.clone();
                }
                if let Some(content) = &h.content {
                    item["content"] = Value::String(content.clone());
                } else if let Some(summary) = &h.summary {
                    item["content"] = Value::String(summary.clone());
                }
                if let Some(summary) = &h.summary {
                    item["summary"] = Value::String(summary.clone());
                }
                item
            })
            .collect();
        map.insert("history".to_string(), Value::Array(arr));
    }
    Value::Object(map)
}

pub(in crate::app_store) fn service_providers_to_legacy_view(
    state: &ServiceProvidersState,
) -> LegacyProvidersView {
    LegacyProvidersView {
        active_claude: state.active.get("claude").cloned(),
        active_codex: state.active.get("codex").cloned(),
        active_gemini: state.active.get("gemini").cloned(),
        active_opencode: state.active.get("opencode").cloned(),
        providers: state
            .providers
            .iter()
            .map(service_provider_to_legacy)
            .collect(),
    }
}

pub(in crate::app_store) fn provider_import_key(tool: &str, provider_id: &str) -> String {
    format!("{}::{}", tool.trim().to_lowercase(), provider_id.trim())
}

pub(in crate::app_store) fn provider_import_key_id(import_key: &str) -> Option<&str> {
    import_key
        .split_once("::")
        .map(|(_, provider_id)| provider_id)
}

pub(in crate::app_store) fn provider_import_id_map_to_plain_id_map(
    import_id_map: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut plain = HashMap::new();
    let mut ambiguous = HashSet::new();
    for (import_key, local_id) in import_id_map {
        let Some(remote_id) = provider_import_key_id(import_key) else {
            continue;
        };
        if ambiguous.contains(remote_id) {
            continue;
        }
        if let Some(existing) = plain.get(remote_id) {
            if existing != local_id {
                plain.remove(remote_id);
                ambiguous.insert(remote_id.to_string());
            }
        } else {
            plain.insert(remote_id.to_string(), local_id.clone());
        }
    }
    plain
}

pub(in crate::app_store) fn normalize_provider_name(name: &str) -> String {
    name.trim().to_lowercase()
}

pub(in crate::app_store) fn normalize_provider_code(code: Option<&str>) -> Option<String> {
    code.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_lowercase())
}

pub(in crate::app_store) fn service_provider_records_match(
    a: &ServiceProviderRecord,
    b: &ServiceProviderRecord,
) -> bool {
    if a.tool != b.tool {
        return false;
    }
    if !a.id.trim().is_empty() && a.id == b.id {
        return true;
    }

    let a_code = normalize_provider_code(a.code.as_deref());
    let b_code = normalize_provider_code(b.code.as_deref());
    if a_code.is_some() && a_code == b_code {
        return true;
    }

    if a.tool == "opencode" {
        let a_key = a
            .provider_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let b_key = b
            .provider_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if a_key.is_some() && a_key == b_key {
            return true;
        }
    }

    let a_name = normalize_provider_name(&a.name);
    let b_name = normalize_provider_name(&b.name);
    !a_name.is_empty() && a_name == b_name
}

pub(in crate::app_store) fn service_provider_matches_system_default(
    provider: &ServiceProviderRecord,
    tool: &str,
) -> bool {
    if provider.tool != tool {
        return false;
    }

    let default_code = format!("default-{}", tool);
    normalize_provider_code(provider.code.as_deref()) == Some(default_code)
}

#[derive(Debug, Clone, Default)]
pub(in crate::app_store) struct ServiceProviderAutoImportOutcome {
    pub(in crate::app_store) imported: bool,
    pub(in crate::app_store) reason: Option<&'static str>,
    pub(in crate::app_store) provider_id: Option<String>,
    pub(in crate::app_store) activated: bool,
    pub(in crate::app_store) missing_fields: Vec<&'static str>,
}

pub(in crate::app_store) fn auto_import_system_provider_into_service_state(
    state: &mut ServiceProvidersState,
    tool: &str,
    provider: ServiceProviderRecord,
) -> Result<ServiceProviderAutoImportOutcome, String> {
    if !is_managed_tool(tool) {
        return Err("tool does not support env managed import".to_string());
    }

    if state
        .providers
        .iter()
        .any(|provider| service_provider_matches_system_default(provider, tool))
    {
        return Ok(ServiceProviderAutoImportOutcome {
            imported: false,
            reason: Some("provider_exists"),
            ..ServiceProviderAutoImportOutcome::default()
        });
    }

    let mut record = provider;
    record.id = generate_provider_uuid();
    record.tool = tool.to_string();
    record.code = Some(format!("default-{}", tool));
    record.env_managed = Some(true);
    record
        .tool_config
        .insert("env_managed".to_string(), Value::Bool(true));
    normalize_service_provider_record(&mut record);

    let api_key = record.api_key.trim();
    let base_url = record.base_url.as_deref().map(str::trim).unwrap_or("");
    let mut missing_fields: Vec<&'static str> = Vec::new();
    if api_key.is_empty() {
        missing_fields.push("api_key");
    }
    if base_url.is_empty() {
        missing_fields.push("base_url");
    }
    let activated = missing_fields.is_empty() && !state.active.contains_key(tool);
    let provider_id = record.id.clone();
    state.providers.push(record);
    if activated {
        state.active.insert(tool.to_string(), provider_id.clone());
    }

    Ok(ServiceProviderAutoImportOutcome {
        imported: true,
        reason: None,
        provider_id: Some(provider_id),
        activated,
        missing_fields,
    })
}

pub(in crate::app_store) fn api_key_has_value(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !is_placeholder_string(trimmed)
}

pub(in crate::app_store) fn expand_home_dir_path(path: &str) -> Result<PathBuf, String> {
    if path == "~" {
        return dirs::home_dir().ok_or_else(|| "home directory not found".to_string());
    }
    if let Some(stripped) = path.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| "home directory not found".to_string())?;
        return Ok(home.join(stripped));
    }
    Ok(PathBuf::from(path))
}

pub(in crate::app_store) fn normalize_device_label(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub(in crate::app_store) fn provider_snapshot_candidates(device_dir: &Path) -> Vec<PathBuf> {
    vec![
        device_dir.join("data").join("providers").join("state.json"),
        device_dir
            .join("shared")
            .join("profile")
            .join("providers.json"),
        device_dir.join("profile").join("providers.json"),
    ]
}

pub(in crate::app_store) fn read_provider_snapshot_value(path: &Path) -> Option<Value> {
    let content = fs::read_to_string(path).ok()?;
    if content.trim().is_empty() {
        return None;
    }

    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(&content) {
        if blob.is_encrypted {
            if let Ok(value) = CryptoService::decrypt_json(&blob) {
                return Some(value);
            }
            return None;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&blob.data) {
            return Some(value);
        }
    }

    serde_json::from_str::<Value>(&content).ok()
}

pub(in crate::app_store) fn extract_active_map_from_snapshot(
    root: &Map<String, Value>,
) -> HashMap<String, String> {
    const TOOLS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];
    let mut active = HashMap::new();

    if let Some(active_obj) = root.get("active").and_then(|v| v.as_object()) {
        for tool in TOOLS {
            if let Some(provider_id) = active_obj.get(tool).and_then(|v| v.as_str()) {
                if !provider_id.trim().is_empty() {
                    active.insert(tool.to_string(), provider_id.to_string());
                }
            }
        }
    }

    for tool in TOOLS {
        let key = format!("active_{}", tool);
        if let Some(provider_id) = root.get(&key).and_then(|v| v.as_str()) {
            if !provider_id.trim().is_empty() {
                active.insert(tool.to_string(), provider_id.to_string());
            }
        }
    }

    active
}

pub(in crate::app_store) fn extract_providers_from_snapshot(
    root: &Map<String, Value>,
) -> Vec<SyncedDeviceProviderLite> {
    let mut providers = Vec::new();
    let Some(items) = root.get("providers").and_then(|v| v.as_array()) else {
        return providers;
    };

    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let core = obj.get("core").and_then(|v| v.as_object());
        let field = |name: &str| -> Option<String> {
            core.and_then(|c| c.get(name).and_then(|v| v.as_str()))
                .or_else(|| obj.get(name).and_then(|v| v.as_str()))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        let Some(id) = field("id") else { continue };
        let Some(name) = field("name") else { continue };
        let Some(tool) = field("tool") else { continue };
        if !matches!(tool.as_str(), "claude" | "codex" | "gemini" | "opencode") {
            continue;
        }
        let mut api_key = field("api_key").unwrap_or_default();
        if is_placeholder_string(&api_key) {
            api_key.clear();
        }
        let base_url = field("base_url").filter(|v| !is_placeholder_string(v));
        let model = field("model");
        let provider_key = field("provider_key");
        let is_enabled = core
            .and_then(|c| c.get("is_enabled").and_then(|v| v.as_bool()))
            .or_else(|| obj.get("is_enabled").and_then(|v| v.as_bool()));

        providers.push(SyncedDeviceProviderLite {
            id,
            name,
            tool,
            api_key,
            base_url,
            model,
            provider_key,
            is_enabled,
        });
    }

    providers.sort_by(|a, b| {
        a.tool
            .cmp(&b.tool)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    providers
}

pub(in crate::app_store) fn provider_snapshot_quality_score(
    providers: &[SyncedDeviceProviderLite],
    active: &HashMap<String, String>,
) -> usize {
    let with_key = providers
        .iter()
        .filter(|p| !p.api_key.trim().is_empty())
        .count();
    let with_model = providers.iter().filter(|p| p.model.is_some()).count();
    let with_base_url = providers.iter().filter(|p| p.base_url.is_some()).count();
    let with_provider_key = providers
        .iter()
        .filter(|p| p.provider_key.is_some())
        .count();

    // Prefer snapshots that include decrypted api_key first, then richer metadata.
    with_key.saturating_mul(10000)
        + with_model.saturating_mul(500)
        + with_base_url.saturating_mul(100)
        + with_provider_key.saturating_mul(20)
        + active.len().saturating_mul(5)
        + providers.len()
}
