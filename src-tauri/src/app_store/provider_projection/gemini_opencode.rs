use crate::app_store::{
    provider_env_managed, read_json_object, render_claude, render_claude_reset_to_unmanaged,
    render_codex, render_codex_reset_to_unmanaged, ProviderRecord, StorageEngine,
};
use serde_json::{json, Map, Value};
use std::fs::{self};
use std::path::PathBuf;

pub(in crate::app_store) fn render_gemini(
    provider: &ProviderRecord,
) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let gemini_dir = home_dir.join(".gemini");
    let env_path = gemini_dir.join(".env");
    let settings_path = gemini_dir.join("settings.json");

    let mut env_map = std::collections::BTreeMap::new();
    env_map.insert("GEMINI_API_KEY".to_string(), provider.core.api_key.clone());
    if let Some(v) = &provider.core.base_url {
        env_map.insert("GOOGLE_GEMINI_BASE_URL".to_string(), v.clone());
    }
    if let Some(v) = &provider.core.model {
        env_map.insert("GEMINI_MODEL".to_string(), v.clone());
    }

    let mut env_content = String::new();
    for (k, v) in env_map {
        env_content.push_str(&format!("{}={}\n", k, v));
    }

    let mut settings = Map::new();
    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    for field in ["theme"] {
        if let Some(v) = provider.tool_config.get(field) {
            settings.insert(field.to_string(), v.clone());
        }
    }

    if let Some(v) = provider
        .tool_config
        .get("vim_mode")
        .and_then(|v| v.as_bool())
    {
        let mut general = settings
            .remove("general")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        general.insert("vimMode".to_string(), Value::Bool(v));
        if let Some(mode) = provider
            .tool_config
            .get("default_approval_mode")
            .and_then(|v| v.as_str())
        {
            general.insert(
                "defaultApprovalMode".to_string(),
                Value::String(mode.to_string()),
            );
        }
        settings.insert("general".to_string(), Value::Object(general));
    }

    if let Some(auth_type) = provider
        .tool_config
        .get("gemini_auth_type")
        .and_then(|v| v.as_str())
    {
        let mut security = settings
            .remove("security")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let mut auth = security
            .remove("auth")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        auth.insert(
            "selectedType".to_string(),
            Value::String(auth_type.to_string()),
        );
        security.insert("auth".to_string(), Value::Object(auth));
        settings.insert("security".to_string(), Value::Object(security));
    }

    Ok(vec![
        (env_path, env_content),
        (
            settings_path,
            serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?,
        ),
    ])
}

pub(in crate::app_store) fn render_gemini_reset_to_unmanaged(
) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let gemini_dir = home_dir.join(".gemini");
    let env_path = gemini_dir.join(".env");
    let settings_path = gemini_dir.join("settings.json");
    let mut outputs = Vec::new();

    if env_path.exists() {
        let content = fs::read_to_string(&env_path).unwrap_or_default();
        let mut env_map = std::collections::BTreeMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim();
                if key == "GEMINI_API_KEY"
                    || key == "GOOGLE_GEMINI_BASE_URL"
                    || key == "GEMINI_MODEL"
                {
                    continue;
                }
                env_map.insert(key.to_string(), v.trim().to_string());
            }
        }
        let mut new_content = String::new();
        for (k, v) in env_map {
            new_content.push_str(&format!("{}={}\n", k, v));
        }
        outputs.push((env_path, new_content));
    }

    if settings_path.exists() {
        let mut settings = read_json_object(&settings_path).unwrap_or_default();
        settings.remove("theme");

        if let Some(general) = settings.get_mut("general").and_then(|v| v.as_object_mut()) {
            general.remove("vimMode");
            general.remove("defaultApprovalMode");
            if general.is_empty() {
                settings.remove("general");
            }
        }

        if let Some(security) = settings.get_mut("security").and_then(|v| v.as_object_mut()) {
            if let Some(auth) = security.get_mut("auth").and_then(|v| v.as_object_mut()) {
                auth.remove("selectedType");
                if auth.is_empty() {
                    security.remove("auth");
                }
            }
            if security.is_empty() {
                settings.remove("security");
            }
        }

        outputs.push((
            settings_path,
            serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?,
        ));
    }

    Ok(outputs)
}

pub(in crate::app_store) fn render_opencode(
    provider: &ProviderRecord,
) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let path = home_dir
        .join(".config")
        .join("opencode")
        .join("opencode.json");

    let mut settings = Map::new();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
                settings = map;
            }
        }
    }

    settings
        .entry("$schema".to_string())
        .or_insert(Value::String("https://opencode.ai/config.json".to_string()));

    if let Some(v) = provider
        .tool_config
        .get("opencode_default_model")
        .and_then(|v| v.as_str())
    {
        settings.insert("model".to_string(), Value::String(v.to_string()));
    }

    if let Some(v) = provider
        .tool_config
        .get("opencode_default_agent")
        .and_then(|v| v.as_str())
    {
        let mut agent = settings
            .remove("agent")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        agent.insert("default".to_string(), Value::String(v.to_string()));
        settings.insert("agent".to_string(), Value::Object(agent));
    }

    if let Some(v) = provider
        .tool_config
        .get("opencode_sessions_dir")
        .and_then(|v| v.as_str())
    {
        let mut sessions = settings
            .remove("sessions")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        sessions.insert("dir".to_string(), Value::String(v.to_string()));
        settings.insert("sessions".to_string(), Value::Object(sessions));
    }

    let mut providers = settings
        .remove("provider")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    let provider_key = provider
        .provider_key
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OpenCode provider_key is required".to_string())?;

    let mut provider_obj = provider.tool_config.clone();
    provider_obj.insert(
        "name".to_string(),
        Value::String(provider.core.name.clone()),
    );
    providers.insert(provider_key, Value::Object(provider_obj));

    settings.insert("provider".to_string(), Value::Object(providers));

    Ok(vec![(
        path,
        serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?,
    )])
}

pub(in crate::app_store) fn render_projection(
    provider: &ProviderRecord,
) -> Result<Vec<(PathBuf, String)>, String> {
    if !provider_env_managed(provider) {
        return match provider.core.tool.as_str() {
            "claude" => render_claude_reset_to_unmanaged(),
            "codex" => render_codex_reset_to_unmanaged(),
            "gemini" => render_gemini_reset_to_unmanaged(),
            _ => Err(format!(
                "Unsupported tool for unmanaged reset: {}",
                provider.core.tool
            )),
        };
    }

    match provider.core.tool.as_str() {
        "claude" => render_claude(provider),
        "codex" => render_codex(provider),
        "gemini" => render_gemini(provider),
        "opencode" => render_opencode(provider),
        other => Err(format!("Unsupported tool: {}", other)),
    }
}

pub(in crate::app_store) fn apply_projection(provider: &ProviderRecord) -> Result<(), String> {
    let renders = render_projection(provider)?;
    for (path, content) in renders {
        StorageEngine::atomic_write(&path, &content)?;
    }
    Ok(())
}

pub(in crate::app_store) fn build_projection_diff(
    provider: &ProviderRecord,
) -> Result<Vec<Value>, String> {
    let renders = render_projection(provider)?;
    let mut diffs = Vec::new();

    for (path, desired) in renders {
        let current = if path.exists() {
            fs::read_to_string(&path).unwrap_or_default()
        } else {
            String::new()
        };
        if current != desired {
            diffs.push(json!({
                "path": path.to_string_lossy(),
                "current": current,
                "desired": desired
            }));
        }
    }

    Ok(diffs)
}
