use super::{
    api_error, api_ok, enqueue_sync_event, get_meta, now_ts, process_sync_queue, ApiErr, ApiMeta,
    ApiOk, SchemaMeta, StorageEngine,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ServiceProviderPresetEndpoints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini_base_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ServiceProviderPresetRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub endpoints: ServiceProviderPresetEndpoints,
    #[serde(default)]
    pub template: Map<String, Value>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ServiceProviderPresetsState {
    #[serde(default)]
    pub presets: Vec<ServiceProviderPresetRecord>,
    #[serde(default)]
    pub builtin_seed_version: u32,
}

const BUILTIN_PRESET_SEED_VERSION: u32 = 2;

const INSTANCE_FIELD_KEYS: &[&str] = &[
    "id",
    "tool",
    "api_key",
    "code",
    "provider_key",
    "is_enabled",
    "env_managed",
    "favorite_at",
    "history",
    "fetched_models",
    "models",
    "options",
];

pub(in crate::app_store) fn local_provider_presets_path() -> Result<PathBuf, String> {
    StorageEngine::provider_presets_path()
}

fn trim_string_option(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn sanitize_endpoints(endpoints: ServiceProviderPresetEndpoints) -> ServiceProviderPresetEndpoints {
    ServiceProviderPresetEndpoints {
        openai_base_url: trim_string_option(endpoints.openai_base_url),
        anthropic_base_url: trim_string_option(endpoints.anthropic_base_url),
        gemini_base_url: trim_string_option(endpoints.gemini_base_url),
    }
}

fn strip_sensitive_template_fields(template: &mut Map<String, Value>) {
    for key in INSTANCE_FIELD_KEYS {
        template.remove(*key);
    }
    template.remove("base_url");
    template.remove("baseURL");
    template.remove("claude_base_url");
    template.remove("openai_base_url");
    template.remove("anthropic_base_url");
    template.remove("gemini_base_url");
    template.retain(|key, _| {
        let lower = key.to_ascii_lowercase();
        !(lower.contains("key")
            || lower.contains("token")
            || lower.contains("secret")
            || lower.contains("password")
            || lower.contains("auth"))
    });
}

fn sanitize_claude_model_mappings_value(value: &mut Value) -> bool {
    let Value::Array(items) = value else {
        return false;
    };

    let mut sanitized = Vec::new();
    for item in items.iter() {
        let Value::Object(obj) = item else {
            continue;
        };

        let family = obj
            .get("family")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        if family.is_empty() {
            continue;
        }

        let display_name = obj
            .get("display_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        let upstream_model = obj
            .get("upstream_model")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        let supports_1m = obj.get("supports_1m").and_then(Value::as_bool);
        let supported_capabilities = obj
            .get("supported_capabilities")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| Value::String(value.to_string()))
                    .collect::<Vec<_>>()
            })
            .filter(|values| !values.is_empty());

        if upstream_model.is_empty()
            && !supports_1m.unwrap_or(false)
            && supported_capabilities.is_none()
        {
            continue;
        }

        let mut mapping = Map::new();
        mapping.insert("family".to_string(), Value::String(family));
        mapping.insert("display_name".to_string(), Value::String(display_name));
        mapping.insert("upstream_model".to_string(), Value::String(upstream_model));
        if let Some(value) = supports_1m {
            mapping.insert("supports_1m".to_string(), Value::Bool(value));
        }
        if let Some(values) = supported_capabilities {
            mapping.insert("supported_capabilities".to_string(), Value::Array(values));
        }
        sanitized.push(Value::Object(mapping));
    }

    *items = sanitized;
    !items.is_empty()
}

fn sanitize_claude_template_fields(template: &mut Map<String, Value>) {
    if let Some(value) = template.get_mut("claude_default_model") {
        match value.as_str().map(str::trim).filter(|value| !value.is_empty()) {
            Some(trimmed) => *value = Value::String(trimmed.to_string()),
            None => {
                template.remove("claude_default_model");
            }
        }
    }
    if let Some(value) = template.get_mut("claude_reasoning_effort") {
        match value.as_str().map(str::trim).filter(|value| !value.is_empty()) {
            Some(trimmed) => *value = Value::String(trimmed.to_string()),
            None => {
                template.remove("claude_reasoning_effort");
            }
        }
    }
    if let Some(value) = template.get_mut("claude_model_mappings") {
        if !sanitize_claude_model_mappings_value(value) {
            template.remove("claude_model_mappings");
        }
    }
}

pub(in crate::app_store) fn sanitize_provider_preset(
    mut preset: ServiceProviderPresetRecord,
    existing: Option<&ServiceProviderPresetRecord>,
) -> Result<ServiceProviderPresetRecord, String> {
    preset.id = preset.id.trim().to_string();
    preset.name = preset.name.trim().to_string();
    if preset.id.is_empty() {
        preset.id = format!("preset-{}", uuid::Uuid::new_v4());
    }
    if preset.name.is_empty() {
        return Err("preset name required".to_string());
    }

    preset.description = trim_string_option(preset.description);
    preset.icon = trim_string_option(preset.icon);
    preset.endpoints = sanitize_endpoints(preset.endpoints);
    strip_sensitive_template_fields(&mut preset.template);
    sanitize_claude_template_fields(&mut preset.template);

    let now = now_ts();
    preset.created_at = existing
        .map(|item| item.created_at)
        .filter(|value| *value > 0)
        .unwrap_or_else(|| {
            if preset.created_at > 0 {
                preset.created_at
            } else {
                now
            }
        });
    preset.updated_at = now;
    Ok(preset)
}

pub(in crate::app_store) fn default_provider_presets() -> ServiceProviderPresetsState {
    let now = now_ts();
    let preset = |id: &str,
                  name: &str,
                  description: &str,
                  icon: &str,
                  openai_base_url: Option<&str>,
                  anthropic_base_url: Option<&str>| {
        ServiceProviderPresetRecord {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(description.to_string()),
            icon: Some(icon.to_string()),
            endpoints: ServiceProviderPresetEndpoints {
                openai_base_url: openai_base_url.map(str::to_string),
                anthropic_base_url: anthropic_base_url.map(str::to_string),
                gemini_base_url: None,
            },
            template: Map::new(),
            created_at: now,
            updated_at: now,
        }
    };
    ServiceProviderPresetsState {
        presets: vec![
            preset(
                "openai",
                "OpenAI",
                "OpenAI API endpoints",
                "builtin:chatgpt",
                Some("https://api.openai.com/v1"),
                None,
            ),
            preset(
                "anthropic",
                "Anthropic",
                "Anthropic native API endpoint",
                "builtin:claude",
                None,
                Some("https://api.anthropic.com"),
            ),
            preset(
                "alibaba-bailian",
                "阿里百炼",
                "Alibaba Cloud Model Studio OpenAI-compatible endpoint",
                "builtin:bailian",
                Some("https://dashscope.aliyuncs.com/compatible-mode/v1"),
                None,
            ),
            preset(
                "volcengine-ark",
                "火山方舟",
                "Volcengine Ark OpenAI and Anthropic-compatible endpoints",
                "builtin:volcengine",
                Some("https://ark.cn-beijing.volces.com/api/v3"),
                Some("https://ark.cn-beijing.volces.com/api/compatible"),
            ),
            preset(
                "deepseek",
                "DeepSeek",
                "DeepSeek OpenAI and Anthropic-compatible endpoints",
                "builtin:deepseek",
                Some("https://api.deepseek.com"),
                Some("https://api.deepseek.com/anthropic"),
            ),
            preset(
                "opencode-go",
                "OpenCode Go",
                "OpenCode Go OpenAI and Anthropic-compatible endpoints",
                "builtin:opencode",
                Some("https://opencode.ai/zen/go/v1"),
                Some("https://opencode.ai/zen/go/v1"),
            ),
        ],
        builtin_seed_version: BUILTIN_PRESET_SEED_VERSION,
    }
}

fn builtin_preset_by_id(id: &str) -> Option<ServiceProviderPresetRecord> {
    default_provider_presets()
        .presets
        .into_iter()
        .find(|preset| preset.id == id)
}

fn merge_builtin_preset_updates(state: &mut ServiceProviderPresetsState) -> bool {
    if state.builtin_seed_version >= BUILTIN_PRESET_SEED_VERSION {
        return false;
    }

    for id in ["alibaba-bailian", "volcengine-ark", "opencode-go"] {
        if !state.presets.iter().any(|preset| preset.id == id) {
            if let Some(preset) = builtin_preset_by_id(id) {
                state.presets.push(preset);
            }
        }
    }

    if let Some(deepseek) = state
        .presets
        .iter_mut()
        .find(|preset| preset.id == "deepseek")
    {
        if deepseek.endpoints.anthropic_base_url.as_deref()
            != Some("https://api.deepseek.com/anthropic")
        {
            deepseek.endpoints.anthropic_base_url =
                Some("https://api.deepseek.com/anthropic".to_string());
            if deepseek.icon.as_deref().unwrap_or("").trim().is_empty() {
                deepseek.icon = Some("builtin:deepseek".to_string());
            }
            deepseek.updated_at = now_ts();
        }
    } else if let Some(preset) = builtin_preset_by_id("deepseek") {
        state.presets.push(preset);
    }

    state.builtin_seed_version = BUILTIN_PRESET_SEED_VERSION;
    true
}

pub(in crate::app_store) fn load_service_provider_presets_state(
) -> Result<ServiceProviderPresetsState, String> {
    let path = StorageEngine::provider_presets_path()?;
    if !path.exists() {
        let state = default_provider_presets();
        save_service_provider_presets_state(&state)?;
        return Ok(state);
    }
    let mut state: ServiceProviderPresetsState = StorageEngine::read_json(&path)?;
    for preset in &mut state.presets {
        preset.endpoints = sanitize_endpoints(preset.endpoints.clone());
        strip_sensitive_template_fields(&mut preset.template);
    }
    if merge_builtin_preset_updates(&mut state) {
        save_service_provider_presets_state(&state)?;
    }
    Ok(state)
}

pub(in crate::app_store) fn save_service_provider_presets_state(
    state: &ServiceProviderPresetsState,
) -> Result<SchemaMeta, String> {
    StorageEngine::write_json(&StorageEngine::provider_presets_path()?, state)?;
    StorageEngine::bump_revision()
}

pub(in crate::app_store) fn export_local_provider_presets_to_shared(
    path: &Path,
) -> Result<(), String> {
    let state = load_service_provider_presets_state()?;
    StorageEngine::write_json(path, &state)
}

pub(in crate::app_store) fn import_shared_provider_presets_to_local(
    path: &Path,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let incoming: ServiceProviderPresetsState = StorageEngine::read_json(path)?;
    let existing = if StorageEngine::provider_presets_path()?.exists() {
        load_service_provider_presets_state()?
    } else {
        ServiceProviderPresetsState::default()
    };

    let mut sanitized = ServiceProviderPresetsState::default();
    for preset in incoming.presets {
        let matched = existing.presets.iter().find(|item| item.id == preset.id);
        sanitized
            .presets
            .push(sanitize_provider_preset(preset, matched)?);
    }
    StorageEngine::write_json(&StorageEngine::provider_presets_path()?, &sanitized)?;
    let _ = StorageEngine::bump_revision()?;
    Ok(())
}

pub(in crate::app_store) fn sync_provider_presets_profile(
    cfg: &crate::config::StorageConfig,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let local = local_provider_presets_path()?;
    let shared = super::shared_profile_path(cfg, "provider_presets.json")?;
    let local_ts = super::file_modified_ts(&local);
    let shared_ts = super::file_modified_ts(&shared);
    let shared_pending_download = shared_ts.is_none() && super::placeholder_for(&shared).exists();

    match (local_ts, shared_ts) {
        (Some(l), Some(s)) if s > l => import_shared_provider_presets_to_local(&shared)?,
        (Some(l), Some(s)) if l > s => export_local_provider_presets_to_shared(&shared)?,
        (None, Some(_)) => import_shared_provider_presets_to_local(&shared)?,
        (Some(_), None) => {
            if shared_pending_download {
                warnings.push(
                    "provider_presets: skip export while shared file is pending download"
                        .to_string(),
                );
            } else {
                export_local_provider_presets_to_shared(&shared)?;
            }
        }
        (None, None) => {
            if !shared_pending_download {
                export_local_provider_presets_to_shared(&shared)?;
            }
        }
        _ => {}
    }

    if shared_pending_download {
        warnings.push(format!(
            "provider_presets: shared file pending download ({})",
            super::placeholder_for(&shared).display()
        ));
    }

    Ok(())
}

#[tauri::command]
pub fn service_provider_presets_list() -> Result<ApiOk<ServiceProviderPresetsState>, ApiErr> {
    let state = load_service_provider_presets_state().map_err(|e| api_error("io_error", e))?;
    api_ok(state, get_meta().map_err(|e| api_error("io_error", e))?)
}

#[tauri::command]
pub async fn service_provider_presets_upsert(
    app: tauri::AppHandle,
    preset: ServiceProviderPresetRecord,
) -> Result<ApiOk<ServiceProviderPresetRecord>, ApiErr> {
    let mut state = load_service_provider_presets_state().map_err(|e| api_error("io_error", e))?;
    let existing = state
        .presets
        .iter()
        .find(|item| item.id == preset.id)
        .cloned();
    let sanitized = sanitize_provider_preset(preset, existing.as_ref())
        .map_err(|e| api_error("invalid_payload", e))?;

    if let Some(pos) = state
        .presets
        .iter()
        .position(|item| item.id == sanitized.id)
    {
        state.presets[pos] = sanitized.clone();
    } else {
        state.presets.push(sanitized.clone());
    }

    let schema =
        save_service_provider_presets_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("provider_presets", "service_provider_presets_upsert")
        .map_err(|e| api_error("sync_error", e))?;
    tauri::async_runtime::spawn(async move {
        let _ = process_sync_queue(app).await;
    });

    api_ok(
        sanitized,
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn service_provider_presets_delete(
    app: tauri::AppHandle,
    preset_id: String,
) -> Result<ApiOk<Value>, ApiErr> {
    let mut state = load_service_provider_presets_state().map_err(|e| api_error("io_error", e))?;
    let before = state.presets.len();
    state.presets.retain(|item| item.id != preset_id);
    if state.presets.len() == before {
        return Err(api_error("not_found", "service provider preset not found"));
    }
    let schema =
        save_service_provider_presets_state(&state).map_err(|e| api_error("io_error", e))?;
    enqueue_sync_event("provider_presets", "service_provider_presets_delete")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_provider_preset_strips_sensitive_and_instance_fields() {
        let mut template = Map::new();
        template.insert("api_key".to_string(), Value::String("secret".to_string()));
        template.insert("provider_key".to_string(), Value::String("pk".to_string()));
        template.insert("history".to_string(), Value::Array(vec![json!({"x": 1})]));
        template.insert("fetched_models".to_string(), Value::Array(vec![json!("m")]));
        template.insert(
            "base_url".to_string(),
            Value::String("https://wrong".to_string()),
        );
        template.insert("model".to_string(), Value::String("gpt-4.1".to_string()));

        let preset = sanitize_provider_preset(
            ServiceProviderPresetRecord {
                id: " custom ".to_string(),
                name: " Custom ".to_string(),
                endpoints: ServiceProviderPresetEndpoints {
                    openai_base_url: Some(" https://openai.example/v1 ".to_string()),
                    anthropic_base_url: Some("".to_string()),
                    gemini_base_url: None,
                },
                template,
                ..ServiceProviderPresetRecord::default()
            },
            None,
        )
        .expect("sanitize preset");

        assert_eq!(preset.id, "custom");
        assert_eq!(preset.name, "Custom");
        assert_eq!(
            preset.endpoints.openai_base_url.as_deref(),
            Some("https://openai.example/v1")
        );
        assert!(preset.endpoints.anthropic_base_url.is_none());
        assert!(!preset.template.contains_key("api_key"));
        assert!(!preset.template.contains_key("provider_key"));
        assert!(!preset.template.contains_key("history"));
        assert!(!preset.template.contains_key("fetched_models"));
        assert!(!preset.template.contains_key("base_url"));
        assert_eq!(
            preset.template.get("model").and_then(Value::as_str),
            Some("gpt-4.1")
        );
    }
}
