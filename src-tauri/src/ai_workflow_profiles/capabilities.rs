use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

use super::storage::{read_optional_config, resolve_home_dir};
use super::types::{AgentMatrixRow, ColumnModelSource, ModelCapability, ModelSourcesResult};

/// Appends a trimmed raw effort token, skipping blanks and duplicates.
fn push_capability_effort(efforts: &mut Vec<String>, raw: &str) {
    let token = raw.trim();
    if token.is_empty() {
        return;
    }
    if !efforts.iter().any(|existing| existing == token) {
        efforts.push(token.to_string());
    }
}

/// Extracts Codex per-slug raw efforts from `models_cache.json`.
/// A slug with no non-empty declared effort is unknown and omitted.
fn codex_capabilities_from_value(root: &serde_json::Value) -> BTreeMap<String, ModelCapability> {
    let mut capabilities = BTreeMap::new();
    let Some(models) = root.get("models").and_then(|value| value.as_array()) else {
        return capabilities;
    };
    for model in models {
        let Some(slug) = model.get("slug").and_then(|value| value.as_str()) else {
            continue;
        };
        let slug = slug.trim();
        if slug.is_empty() {
            continue;
        }
        let mut efforts = Vec::new();
        if let Some(levels) = model
            .get("supported_reasoning_levels")
            .and_then(|value| value.as_array())
        {
            for level in levels {
                if let Some(effort) = level.get("effort").and_then(|value| value.as_str()) {
                    push_capability_effort(&mut efforts, effort);
                }
            }
        }
        if efforts.is_empty() {
            continue;
        }
        let default_effort = model
            .get("default_reasoning_level")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        capabilities
            .entry(slug.to_string())
            .or_insert(ModelCapability {
                efforts,
                default_effort,
            });
    }
    capabilities
}

/// A model's explicit `variants` declaration, if any.
/// Declared-but-all-disabled yields a known entry with an empty effort list;
/// a variant without a string `reasoningEffort` is not a declaration.
fn opencode_declared_capability(model: &serde_json::Value) -> Option<ModelCapability> {
    let variants = model.get("variants")?.as_object()?;
    let mut efforts = Vec::new();
    let mut declared = false;
    for variant in variants.values() {
        let Some(raw) = variant
            .get("reasoningEffort")
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        declared = true;
        let disabled = variant
            .get("disabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !disabled {
            push_capability_effort(&mut efforts, raw);
        }
    }
    if !declared {
        return None;
    }
    Some(ModelCapability {
        efforts,
        default_effort: None,
    })
}

/// Resolves one provider object from the optional local catalog.
/// A provider may sit under a top-level `providers` wrapper or directly at the
/// root; a missing or non-object provider resolves to None.
fn opencode_catalog_provider<'a>(
    catalog: &'a serde_json::Value,
    provider: &str,
) -> Option<&'a serde_json::Value> {
    catalog
        .get("providers")
        .and_then(|value| value.as_object())
        .and_then(|providers| providers.get(provider))
        .or_else(|| catalog.get(provider))
}

/// A dynamically discoverable self-operated model is free exactly when its
/// declared `input` and `output` costs are both the number zero. A missing or
/// non-object cost, a missing side, a non-zero value or an unparseable value
/// (including a string) is not free. A `status` of `deprecated` excludes it.
fn opencode_catalog_model_is_free(model: &serde_json::Value) -> bool {
    if model.get("status").and_then(|value| value.as_str()) == Some("deprecated") {
        return false;
    }
    let Some(cost) = model.get("cost").and_then(|value| value.as_object()) else {
        return false;
    };
    ["input", "output"]
        .iter()
        .all(|key| cost.get(*key).and_then(|value| value.as_f64()) == Some(0.0))
}

/// Exact provider/model effort metadata from the optional local catalog.
/// Only `type: "effort"` array values supply capabilities.
fn opencode_catalog_capability(
    catalog: &serde_json::Value,
    provider: &str,
    model: &str,
) -> Option<ModelCapability> {
    let provider_value = opencode_catalog_provider(catalog, provider)?;
    let model_value = provider_value.get("models")?.get(model)?;
    let options = model_value.get("reasoning_options")?.as_array()?;
    let mut efforts = Vec::new();
    for option in options {
        let is_effort = option.get("type").and_then(|value| value.as_str()) == Some("effort");
        if !is_effort {
            continue;
        }
        if let Some(values) = option.get("values").and_then(|value| value.as_array()) {
            for value in values {
                if let Some(raw) = value.as_str() {
                    push_capability_effort(&mut efforts, raw);
                }
            }
        }
    }
    if efforts.is_empty() {
        return None;
    }
    Some(ModelCapability {
        efforts,
        default_effort: None,
    })
}

/// Reloads local capabilities for `home` and rejects a populated known model
/// whose selected effort is not among its declared raw efforts.
pub(super) fn enforce_declared_capabilities(
    home: &Path,
    matrix: &[AgentMatrixRow],
) -> Result<(), String> {
    let sources = get_model_sources(Some(home))?;
    for row in matrix {
        let columns = [
            ("codex", &sources.codex, &row.codex),
            ("claude", &sources.claude, &row.claude),
            ("opencode", &sources.opencode, &row.opencode),
        ];
        for (host, column, effort) in columns {
            let Some(effort) = effort else {
                continue;
            };
            let model = effort.model.trim();
            if model.is_empty() {
                continue;
            }
            let Some(capability) = column.model_capabilities.get(model) else {
                continue;
            };
            let selected = effort.reasoning_effort.trim();
            if !capability.efforts.iter().any(|declared| declared == selected) {
                return Err(format!(
                    "Unsupported reasoning_effort '{}' for role '{}' and host '{}' model '{}': declared efforts are {:?}",
                    selected, row.role, host, model, capability.efforts
                ));
            }
        }
    }
    Ok(())
}

/// Gathers model source candidate lists for opencode, codex, and claude.
/// Individual column errors degrade that column while keeping others available.
pub fn get_model_sources(home_override: Option<&Path>) -> Result<ModelSourcesResult, String> {
    let home = resolve_home_dir(home_override)?;

    // 1. OpenCode column
    let mut opencode = ColumnModelSource::default();
    // The optional local catalog fills effort metadata for models whose
    // configuration declares no explicit reasoningEffort variant, and also
    // supplies the current free models of the self-operated providers.
    let mut opencode_catalog: Option<serde_json::Value> = None;
    let opencode_catalog_path = home.join(".cache/opencode/models.json");
    match read_optional_config(&opencode_catalog_path) {
        Ok(Some(content)) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(value) => opencode_catalog = Some(value),
            Err(_) => {
                opencode.capability_warning =
                    Some("Failed to parse opencode model catalog".to_string());
            }
        },
        Ok(None) => {}
        Err(_) => {
            opencode.capability_warning =
                Some("Failed to read opencode model catalog".to_string());
        }
    }

    let mut models_vec: Vec<String> = Vec::new();
    let mut capabilities: BTreeMap<String, ModelCapability> = BTreeMap::new();

    let opencode_path = home.join(".config/opencode/opencode.json");
    if opencode_path.exists() {
        match fs::read_to_string(&opencode_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    if let Some(providers) = val.get("provider").and_then(|p| p.as_object()) {
                        for (provider_name, provider_data) in providers {
                            if let Some(models) =
                                provider_data.get("models").and_then(|m| m.as_object())
                            {
                                for (model_name, model_data) in models {
                                    let composite = format!("{}/{}", provider_name, model_name);
                                    models_vec.push(composite.clone());
                                    let capability =
                                        match opencode_declared_capability(model_data) {
                                            Some(capability) => Some(capability),
                                            None => {
                                                opencode_catalog.as_ref().and_then(|catalog| {
                                                    opencode_catalog_capability(
                                                        catalog,
                                                        provider_name,
                                                        model_name,
                                                    )
                                                })
                                            }
                                        };
                                    if let Some(capability) = capability {
                                        capabilities.insert(composite, capability);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    opencode.error = Some(format!("Failed to parse opencode.json: {}", e));
                }
            },
            Err(e) => {
                opencode.error = Some(format!("Failed to read opencode.json: {}", e));
            }
        }
    }

    // Dynamically merge the current free models of the two self-operated
    // providers, independent of any opencode.json declaration, so the merge
    // still applies when opencode.json is missing or unreadable. A missing
    // catalog yields no dynamic entries and no fabricated diagnostic.
    if let Some(catalog) = opencode_catalog.as_ref() {
        for provider in ["opencode", "opencode-go"] {
            let Some(models) = opencode_catalog_provider(catalog, provider)
                .and_then(|value| value.get("models"))
                .and_then(|models| models.as_object())
            else {
                continue;
            };
            for (model_name, model_data) in models {
                if !opencode_catalog_model_is_free(model_data) {
                    continue;
                }
                let composite = format!("{}/{}", provider, model_name);
                models_vec.push(composite.clone());
                // A declared model's explicit variants declaration wins; the
                // catalog only fills a capability the declaration left unset.
                if !capabilities.contains_key(&composite) {
                    if let Some(capability) =
                        opencode_catalog_capability(catalog, provider, model_name)
                    {
                        capabilities.insert(composite, capability);
                    }
                }
            }
        }
    }

    models_vec.sort();
    models_vec.dedup();
    opencode.models = models_vec;
    opencode.model_capabilities = capabilities;

    // Read existing profile models for codex and claude
    let profiles_dir = home.join(".config/ai-workflow/profiles");
    let mut profile_codex_models = HashSet::new();
    let mut profile_claude_models = HashSet::new();
    if profiles_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("yaml") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                            if let Some(agents) = val.get("agents").and_then(|a| a.as_mapping()) {
                                for (_role_key, role_val) in agents {
                                    if let Some(codex_val) =
                                        role_val.get(serde_yaml::Value::String("codex".to_string()))
                                    {
                                        if let Some(m) = codex_val
                                            .get(serde_yaml::Value::String("model".to_string()))
                                            .and_then(|v| v.as_str())
                                        {
                                            if !m.trim().is_empty() {
                                                profile_codex_models.insert(m.trim().to_string());
                                            }
                                        }
                                    }
                                    if let Some(claude_val) = role_val
                                        .get(serde_yaml::Value::String("claude".to_string()))
                                    {
                                        if let Some(m) = claude_val
                                            .get(serde_yaml::Value::String("model".to_string()))
                                            .and_then(|v| v.as_str())
                                        {
                                            if !m.trim().is_empty() {
                                                profile_claude_models.insert(m.trim().to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Codex column
    let mut codex = ColumnModelSource::default();
    let codex_path = home.join(".codex/config.toml");
    let mut codex_models = profile_codex_models;
    if codex_path.exists() {
        match fs::read_to_string(&codex_path) {
            Ok(content) => match content.parse::<DocumentMut>() {
                Ok(doc) => {
                    if let Some(item) = doc.get("model") {
                        if let Some(s) = item.as_str() {
                            if !s.trim().is_empty() {
                                codex_models.insert(s.trim().to_string());
                            }
                        }
                    }
                    if let Some(profiles_item) = doc.get("profiles") {
                        if let Some(table) = profiles_item.as_table() {
                            for (_name, profile_item) in table.iter() {
                                if let Some(m) =
                                    profile_item.get("model").and_then(|v| v.as_str())
                                {
                                    if !m.trim().is_empty() {
                                        codex_models.insert(m.trim().to_string());
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    codex.error = Some(format!("Failed to parse config.toml: {}", e));
                }
            },
            Err(e) => {
                codex.error = Some(format!("Failed to read config.toml: {}", e));
            }
        }
    }
    let mut codex_list: Vec<String> = codex_models.into_iter().collect();
    codex_list.sort();
    codex.models = codex_list;

    let codex_cache_path = home.join(".codex/models_cache.json");
    match read_optional_config(&codex_cache_path) {
        Ok(Some(content)) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(value) => {
                codex.model_capabilities = codex_capabilities_from_value(&value);
            }
            Err(e) => {
                codex.capability_warning =
                    Some(format!("Failed to parse models_cache.json: {}", e));
            }
        },
        Ok(None) => {}
        Err(e) => {
            codex.capability_warning = Some(e);
        }
    }

    // 3. Claude column
    let mut claude = ColumnModelSource::default();
    let claude_path = home.join(".claude/settings.json");
    let mut claude_models = profile_claude_models;
    if claude_path.exists() {
        match fs::read_to_string(&claude_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(val) => {
                    if let Some(env_obj) = val.get("env").and_then(|e| e.as_object()) {
                        const CLAUDE_ENV_KEYS: [&str; 4] = [
                            "ANTHROPIC_MODEL",
                            "ANTHROPIC_DEFAULT_OPUS_MODEL",
                            "ANTHROPIC_DEFAULT_SONNET_MODEL",
                            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                        ];
                        for key in CLAUDE_ENV_KEYS {
                            if let Some(model_str) = env_obj.get(key).and_then(|v| v.as_str()) {
                                if !model_str.trim().is_empty() {
                                    claude_models.insert(model_str.trim().to_string());
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    claude.error = Some(format!("Failed to parse settings.json: {}", e));
                }
            },
            Err(e) => {
                claude.error = Some(format!("Failed to read settings.json: {}", e));
            }
        }
    }
    let mut claude_list: Vec<String> = claude_models.into_iter().collect();
    claude_list.sort();
    claude.models = claude_list;

    Ok(ModelSourcesResult {
        opencode,
        codex,
        claude,
    })
}
