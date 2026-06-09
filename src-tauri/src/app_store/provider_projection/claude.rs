use crate::app_store::{
    claude_model_env_keys_for_family, join_supported_capabilities_csv,
    resolve_claude_default_model, resolve_claude_reasoning_effort, resolved_claude_model_mappings,
    ProviderRecord,
};
use serde_json::{Map, Value};
use std::fs::{self};
use std::path::{Path, PathBuf};

pub(in crate::app_store) fn render_claude_to_dir(
    provider: &ProviderRecord,
    target_dir: &Path,
) -> Result<Vec<(PathBuf, String)>, String> {
    let settings_path = target_dir.join("settings.json");
    let is_global_dir = target_dir.ends_with(".claude");
    let mut settings = Map::new();

    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    let bool_fields = [
        ("dangerously_skip_permissions", "dangerouslySkipPermissions"),
        ("enable_all_memory_features", "enableAllMemoryFeatures"),
        ("enable_mcp", "enableMcp"),
    ];

    for (src, dst) in bool_fields {
        if let Some(v) = provider.tool_config.get(src).and_then(|v| v.as_bool()) {
            settings.insert(dst.to_string(), Value::Bool(v));
        } else {
            settings.remove(dst);
        }
    }

    for (src, dst) in [
        ("allowed_tools", "allowedTools"),
        ("blocked_tools", "blockedTools"),
    ] {
        if let Some(v) = provider.tool_config.get(src) {
            settings.insert(dst.to_string(), v.clone());
        } else {
            settings.remove(dst);
        }
    }

    if let Some(turns) = provider
        .tool_config
        .get("max_session_turns")
        .and_then(|v| v.as_u64())
    {
        settings.insert("maxSessionTurns".to_string(), Value::Number(turns.into()));
    }

    let mut env = settings
        .remove("env")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    env.insert(
        "ANTHROPIC_API_KEY".to_string(),
        Value::String(provider.core.api_key.clone()),
    );
    env.remove("ANTHROPIC_AUTH_TOKEN");

    if let Some(base_url) = &provider.core.base_url {
        if !base_url.is_empty() {
            env.insert(
                "ANTHROPIC_BASE_URL".to_string(),
                Value::String(base_url.clone()),
            );
        }
    } else {
        env.remove("ANTHROPIC_BASE_URL");
    }

    if let Some(v) =
        resolve_claude_default_model(provider.core.model.as_deref(), &provider.tool_config)
    {
        settings.insert("model".to_string(), Value::String(v.clone()));
        env.insert("ANTHROPIC_MODEL".to_string(), Value::String(v));
    } else {
        settings.remove("model");
        env.remove("ANTHROPIC_MODEL");
    }

    let claude_model_mappings = resolved_claude_model_mappings(&provider.tool_config);
    for family in ["haiku", "sonnet", "opus"] {
        let Some((model_key, name_key, capabilities_key)) =
            claude_model_env_keys_for_family(family)
        else {
            continue;
        };
        let mapping = claude_model_mappings
            .iter()
            .find(|mapping| mapping.family == family);
        if let Some(mapping) = mapping {
            let mut upstream_model = mapping.upstream_model.clone();
            if mapping.supports_1m.unwrap_or(false)
                && family != "haiku"
                && !upstream_model.contains("[1m]")
            {
                upstream_model.push_str("[1m]");
            }
            if upstream_model.trim().is_empty() {
                env.remove(model_key);
            } else {
                env.insert(model_key.to_string(), Value::String(upstream_model));
            }
            if mapping.display_name.trim().is_empty() {
                env.remove(name_key);
            } else {
                env.insert(
                    name_key.to_string(),
                    Value::String(mapping.display_name.clone()),
                );
            }
            if let Some(capabilities) = mapping
                .supported_capabilities
                .as_ref()
                .and_then(|values| join_supported_capabilities_csv(values))
            {
                env.insert(capabilities_key.to_string(), Value::String(capabilities));
            } else {
                env.remove(capabilities_key);
            }
        } else {
            env.remove(model_key);
            env.remove(name_key);
            env.remove(capabilities_key);
        }
    }

    if let Some(effort) = resolve_claude_reasoning_effort(&provider.tool_config) {
        env.insert(
            "CLAUDE_CODE_EFFORT_LEVEL".to_string(),
            Value::String(effort),
        );
    } else {
        env.remove("CLAUDE_CODE_EFFORT_LEVEL");
    }

    settings.insert("env".to_string(), Value::Object(env));

    // Internal marker: track which onespace profile is applied to the global Claude config.
    // Only written to ~/.claude, not to profile-specific directories.
    if is_global_dir {
        settings.insert(
            "_onespace_source_profile".to_string(),
            Value::String(provider.core.id.clone()),
        );
    } else {
        settings.remove("_onespace_source_profile");
    }

    let content =
        serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;
    Ok(vec![(settings_path, content)])
}

pub(in crate::app_store) fn render_claude(
    provider: &ProviderRecord,
) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    render_claude_to_dir(provider, &home_dir.join(".claude"))
}

pub(in crate::app_store) fn render_claude_reset_to_unmanaged(
) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let settings_path = home_dir.join(".claude").join("settings.json");
    let mut settings = Map::new();

    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    // Remove the onespace source marker when resetting global config.
    settings.remove("_onespace_source_profile");

    for key in [
        "dangerouslySkipPermissions",
        "enableAllMemoryFeatures",
        "enableMcp",
        "allowedTools",
        "blockedTools",
        "maxSessionTurns",
    ] {
        settings.remove(key);
    }

    if let Some(env) = settings.get_mut("env").and_then(|v| v.as_object_mut()) {
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_SUPPORTED_CAPABILITIES",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_SUPPORTED_CAPABILITIES",
            "CLAUDE_CODE_EFFORT_LEVEL",
        ] {
            env.remove(key);
        }
        if env.is_empty() {
            settings.remove("env");
        }
    }

    let content =
        serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;
    Ok(vec![(settings_path, content)])
}
