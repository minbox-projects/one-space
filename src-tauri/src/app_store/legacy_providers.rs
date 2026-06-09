use super::{
    extract_fields, generate_provider_uuid, is_managed_tool, is_placeholder_string,
    migrate_providers_to_service_providers, normalize_service_provider_record,
    service_provider_to_value, strip_legacy_claude_model_keys, CryptoService, EncryptedBlob,
    LegacyProvidersView, ProviderCore, ProviderImportCandidate, ProviderImportConflictMatch,
    ProviderImportPreviewItem, ProviderInput, ProviderRecord, ProviderRuntimePolicy,
    ProvidersImportPreview, ProvidersState, ServiceProviderRecord, ServiceProvidersState,
    SyncedDeviceProviderLite, MANAGED_TOOLS,
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

pub(in crate::app_store) fn service_providers_to_provider_state(
    state: &ServiceProvidersState,
) -> ProvidersState {
    ProvidersState {
        active: state.active.clone(),
        providers: state
            .providers
            .iter()
            .map(service_provider_to_provider_record)
            .collect(),
    }
}

pub(in crate::app_store) fn service_provider_to_provider_record(
    sp: &ServiceProviderRecord,
) -> ProviderRecord {
    let mut tool_config = sp.tool_config.clone();
    if let Some(v) = &sp.icon {
        tool_config.insert("icon".to_string(), Value::String(v.clone()));
    }
    tool_config.insert(
        "claude_api_format".to_string(),
        Value::String(sp.claude_api_format.clone()),
    );
    tool_config.insert(
        "claude_connection_mode".to_string(),
        Value::String(sp.claude_connection_mode.clone()),
    );
    tool_config.insert(
        "claude_auth_env_key".to_string(),
        Value::String(sp.claude_auth_env_key.clone()),
    );
    if !sp.claude_model_mappings.is_empty() {
        if let Ok(value) = serde_json::to_value(&sp.claude_model_mappings) {
            tool_config.insert("claude_model_mappings".to_string(), value);
        }
    }
    strip_legacy_claude_model_keys(&mut tool_config);
    if let Some(v) = sp.claude_enable_tool_search {
        tool_config.insert("claude_enable_tool_search".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.claude_auto_memory_enabled {
        tool_config.insert("claude_auto_memory_enabled".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.claude_always_thinking_enabled {
        tool_config.insert("claude_always_thinking_enabled".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.claude_away_summary_enabled {
        tool_config.insert("claude_away_summary_enabled".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.claude_include_git_instructions {
        tool_config.insert(
            "claude_include_git_instructions".to_string(),
            Value::Bool(v),
        );
    }
    if let Some(v) = sp.claude_enable_attribution {
        tool_config.insert("claude_enable_attribution".to_string(), Value::Bool(v));
    }
    if let Some(v) = sp.env_managed {
        tool_config.insert("env_managed".to_string(), Value::Bool(v));
    }

    ProviderRecord {
        core: ProviderCore {
            id: sp.id.clone(),
            name: sp.name.clone(),
            tool: sp.tool.clone(),
            api_key: sp.api_key.clone(),
            code: sp.code.clone(),
            base_url: sp.base_url.clone(),
            model: sp.model.clone(),
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
        favorite_at: sp.favorite_at,
        tool_config,
        history: sp.history.clone(),
        extra: sp.extra.clone(),
        is_enabled: sp.is_enabled,
        provider_key: sp.provider_key.clone(),
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

pub(in crate::app_store) fn provider_record_matches_service_provider(
    provider: &ProviderRecord,
    service_provider: &ServiceProviderRecord,
) -> bool {
    if provider.core.tool != service_provider.tool {
        return false;
    }
    if !provider.core.id.trim().is_empty() && provider.core.id == service_provider.id {
        return true;
    }

    let provider_code = normalize_provider_code(provider.core.code.as_deref());
    let service_code = normalize_provider_code(service_provider.code.as_deref());
    if provider_code.is_some() && provider_code == service_code {
        return true;
    }

    let provider_name = normalize_provider_name(&provider.core.name);
    let service_name = normalize_provider_name(&service_provider.name);
    !provider_name.is_empty() && provider_name == service_name
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
    provider: ProviderRecord,
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

    let mut record = migrate_providers_to_service_providers(ProvidersState {
        active: HashMap::new(),
        providers: vec![provider],
    })
    .providers
    .into_iter()
    .next()
    .ok_or_else(|| "system provider was empty".to_string())?;

    record.id = generate_provider_uuid();
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

pub(in crate::app_store) fn provider_input_from_value(
    value: &Value,
) -> Result<ProviderInput, String> {
    let obj = value
        .as_object()
        .cloned()
        .ok_or_else(|| "provider must be object".to_string())?;

    let mut input = ProviderInput {
        id: obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        name: obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        tool: obj
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_lowercase(),
        api_key: obj
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        base_url: obj
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        model: obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        is_enabled: obj.get("is_enabled").and_then(|v| v.as_bool()),
        provider_key: obj
            .get("provider_key")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        favorite_at: obj.get("favorite_at").and_then(|v| v.as_u64()),
        code: obj
            .get("code")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_lowercase().to_string())
            .filter(|s| !s.is_empty()),
        fields: extract_fields(&Value::Object(obj.clone())),
    };

    if input.id.is_empty() {
        return Err("provider id required".to_string());
    }
    if input.name.is_empty() {
        return Err("provider name required".to_string());
    }
    if input.tool.is_empty() {
        return Err("provider tool required".to_string());
    }
    if !MANAGED_TOOLS.contains(&input.tool.as_str()) {
        return Err(format!("unsupported provider tool: {}", input.tool));
    }

    input.fields.remove("history");
    Ok(input)
}

pub(in crate::app_store) fn parse_providers_import_payload(
    import_path: &str,
) -> Result<(HashMap<String, String>, Vec<Value>), String> {
    let raw = fs::read_to_string(import_path).map_err(|e| e.to_string())?;
    let parsed: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let providers = parsed
        .get("providers")
        .and_then(|v| v.as_array().cloned())
        .or_else(|| parsed.as_array().cloned())
        .ok_or_else(|| "import payload must contain providers array".to_string())?;

    let active = parsed
        .as_object()
        .map(extract_active_map_from_snapshot)
        .unwrap_or_default();

    Ok((active, providers))
}

pub(in crate::app_store) fn find_provider_import_conflict(
    state: &ProvidersState,
    input: &ProviderInput,
) -> Option<ProviderImportConflictMatch> {
    if let Some(existing) = state.providers.iter().find(|p| p.core.id == input.id) {
        return Some(ProviderImportConflictMatch {
            existing_id: existing.core.id.clone(),
            existing_name: existing.core.name.clone(),
            reason: "id".to_string(),
        });
    }

    let normalized_name = normalize_provider_name(&input.name);
    if normalized_name.is_empty() {
        return None;
    }

    state
        .providers
        .iter()
        .find(|p| {
            p.core.tool == input.tool && normalize_provider_name(&p.core.name) == normalized_name
        })
        .map(|existing| ProviderImportConflictMatch {
            existing_id: existing.core.id.clone(),
            existing_name: existing.core.name.clone(),
            reason: "name".to_string(),
        })
}

pub(in crate::app_store) fn collect_provider_import_candidates(
    state: &ProvidersState,
    provider_values: &[Value],
) -> Result<Vec<ProviderImportCandidate>, String> {
    let mut seen_import_keys: HashSet<String> = HashSet::new();
    let mut candidates = Vec::new();

    for (idx, value) in provider_values.iter().enumerate() {
        let input = provider_input_from_value(value)
            .map_err(|e| format!("provider #{}: {}", idx.saturating_add(1), e))?;
        let import_key = provider_import_key(&input.tool, &input.id);
        if !seen_import_keys.insert(import_key.clone()) {
            return Err(format!(
                "duplicate provider in import file: {} ({})",
                input.name, import_key
            ));
        }
        let conflict = find_provider_import_conflict(state, &input);
        candidates.push(ProviderImportCandidate {
            import_key,
            input,
            conflict,
        });
    }

    Ok(candidates)
}

pub(in crate::app_store) fn make_imported_provider_id(state: &ProvidersState) -> String {
    loop {
        let candidate = generate_provider_uuid();
        if !state.providers.iter().any(|p| p.core.id == candidate) {
            return candidate;
        }
    }
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

pub(in crate::app_store) fn providers_import_preview_from_candidates(
    active: HashMap<String, String>,
    candidates: &[ProviderImportCandidate],
) -> ProvidersImportPreview {
    let items = candidates
        .iter()
        .map(|candidate| ProviderImportPreviewItem {
            import_key: candidate.import_key.clone(),
            id: candidate.input.id.clone(),
            name: candidate.input.name.clone(),
            tool: candidate.input.tool.clone(),
            model: candidate.input.model.clone(),
            conflict: candidate.conflict.is_some(),
            conflict_reason: candidate.conflict.as_ref().map(|item| item.reason.clone()),
            existing_id: candidate
                .conflict
                .as_ref()
                .map(|item| item.existing_id.clone()),
            existing_name: candidate
                .conflict
                .as_ref()
                .map(|item| item.existing_name.clone()),
        })
        .collect::<Vec<_>>();
    let conflicts = items.iter().filter(|item| item.conflict).count();

    ProvidersImportPreview {
        active,
        total: items.len(),
        conflicts,
        items,
    }
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
        device_dir.join("providers.json"),
        device_dir.join("ai_providers.json"),
        device_dir.join("data").join("providers.json"),
        device_dir.join("data").join("ai_providers.json"),
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
