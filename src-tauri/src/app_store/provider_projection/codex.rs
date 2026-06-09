use crate::app_store::{read_json_object, ProviderRecord};
use serde_json::{Map, Value};
use std::fs::{self};
use std::path::{Path, PathBuf};

pub(in crate::app_store) fn sanitize_codex_model_provider_id(raw: &str) -> String {
    let mut out = String::new();
    let mut last_was_separator = false;

    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            out.push('_');
            last_was_separator = true;
        }
    }

    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "onespace_provider".to_string()
    } else {
        format!("onespace_{}", trimmed)
    }
}

pub(in crate::app_store) fn is_onespace_codex_model_provider_id(id: &str) -> bool {
    id.trim().starts_with("onespace_")
}

pub(in crate::app_store) fn codex_auth_mode(provider: &ProviderRecord) -> Option<&'static str> {
    if let Some(mode) = provider
        .tool_config
        .get("codex_auth_mode")
        .or_else(|| provider.tool_config.get("auth_mode"))
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_lowercase())
    {
        return match mode.as_str() {
            "api" | "api_key" | "apikey" => Some("api"),
            "chatgpt" | "login" => Some("chatgpt"),
            "none" | "disabled" => None,
            _ => None,
        };
    }

    if provider.core.api_key.trim().is_empty() {
        None
    } else {
        Some("api")
    }
}

pub(in crate::app_store) fn render_codex_auth(
    auth_path: &Path,
    provider: &ProviderRecord,
    auth_mode: Option<&str>,
) -> Result<Option<(PathBuf, String)>, String> {
    let Some(auth_mode) = auth_mode else {
        return Ok(None);
    };

    let mut auth = if auth_path.exists() {
        read_json_object(auth_path).unwrap_or_default()
    } else {
        Map::new()
    };

    match auth_mode {
        "api" => {
            auth.insert(
                "OPENAI_API_KEY".to_string(),
                Value::String(provider.core.api_key.clone()),
            );
        }
        "chatgpt" => {
            auth.remove("OPENAI_API_KEY");
        }
        _ => return Ok(None),
    }

    Ok(Some((
        auth_path.to_path_buf(),
        serde_json::to_string_pretty(&Value::Object(auth)).map_err(|e| e.to_string())?,
    )))
}

pub(in crate::app_store) fn set_toml_table_string(
    table: &mut toml_edit::Table,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        table[key] = toml_edit::value(value.to_string());
    } else {
        table.remove(key);
    }
}

pub(in crate::app_store) fn set_toml_table_bool(
    table: &mut toml_edit::Table,
    key: &str,
    value: Option<bool>,
) {
    if let Some(value) = value {
        table[key] = toml_edit::value(value);
    } else {
        table.remove(key);
    }
}

pub(in crate::app_store) fn render_codex_model_provider(
    doc: &mut toml_edit::DocumentMut,
    provider: &ProviderRecord,
    provider_id: &str,
    auth_mode: Option<&str>,
) {
    if !doc.contains_key("model_providers") {
        doc["model_providers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let Some(providers) = doc["model_providers"].as_table_mut() else {
        return;
    };

    if !providers.contains_key(provider_id) {
        providers.insert(provider_id, toml_edit::Item::Table(toml_edit::Table::new()));
    }

    let Some(provider_table) = providers
        .get_mut(provider_id)
        .and_then(|item| item.as_table_mut())
    else {
        return;
    };

    set_toml_table_string(provider_table, "name", Some(&provider.core.name));
    set_toml_table_string(
        provider_table,
        "base_url",
        provider.core.base_url.as_deref(),
    );
    set_toml_table_string(
        provider_table,
        "wire_api",
        provider
            .tool_config
            .get("wire_api")
            .and_then(|v| v.as_str())
            .or(Some("responses")),
    );
    set_toml_table_bool(
        provider_table,
        "requires_openai_auth",
        (auth_mode == Some("api")).then_some(true),
    );
}

pub(in crate::app_store) fn render_codex(
    provider: &ProviderRecord,
) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    render_codex_at_home(provider, &home_dir)
}

pub(in crate::app_store) fn render_codex_at_home(
    provider: &ProviderRecord,
    home_dir: &Path,
) -> Result<Vec<(PathBuf, String)>, String> {
    let codex_dir = home_dir.join(".codex");
    let auth_path = codex_dir.join("auth.json");
    let config_path = codex_dir.join("config.toml");

    let auth_mode = codex_auth_mode(provider);

    let mut toml_str = String::new();
    if config_path.exists() {
        toml_str = fs::read_to_string(&config_path).unwrap_or_default();
    }
    let mut doc = toml_str
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|_| toml_edit::DocumentMut::new());

    doc.remove("base_url");
    doc.remove("preferred_auth_method");

    match auth_mode {
        Some("api") => doc["forced_login_method"] = toml_edit::value("api"),
        Some("chatgpt") => doc["forced_login_method"] = toml_edit::value("chatgpt"),
        _ => {
            doc.remove("forced_login_method");
        }
    }

    let custom_provider_id = provider
        .core
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|_| sanitize_codex_model_provider_id(&provider.core.id));
    if let Some(provider_id) = custom_provider_id.as_deref() {
        render_codex_model_provider(&mut doc, provider, provider_id, auth_mode);
        doc["model_provider"] = toml_edit::value(provider_id.to_string());
    } else {
        doc["model_provider"] = toml_edit::value("openai");
    }

    if let Some(v) = &provider.core.model {
        doc["model"] = toml_edit::value(v.clone());
    } else {
        doc.remove("model");
    }

    for (k, toml_key) in [
        ("disable_response_storage", "disable_response_storage"),
        ("personality", "personality"),
        ("model_reasoning_effort", "model_reasoning_effort"),
        ("model_reasoning_summary", "model_reasoning_summary"),
        ("approval_policy", "approval_policy"),
        ("sandbox_mode", "sandbox_mode"),
    ] {
        if let Some(value) = provider.tool_config.get(k) {
            match value {
                Value::Bool(b) => doc[toml_key] = toml_edit::value(*b),
                Value::String(s) => doc[toml_key] = toml_edit::value(s.clone()),
                _ => {}
            }
        }
    }

    let mut outputs = Vec::new();
    if let Some(auth_output) = render_codex_auth(&auth_path, provider, auth_mode)? {
        outputs.push(auth_output);
    }
    outputs.push((config_path, doc.to_string()));
    Ok(outputs)
}

pub(in crate::app_store) fn render_codex_reset_to_unmanaged(
) -> Result<Vec<(PathBuf, String)>, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    render_codex_reset_to_unmanaged_at_home(&home_dir)
}

pub(in crate::app_store) fn render_codex_reset_to_unmanaged_at_home(
    home_dir: &Path,
) -> Result<Vec<(PathBuf, String)>, String> {
    let codex_dir = home_dir.join(".codex");
    let auth_path = codex_dir.join("auth.json");
    let config_path = codex_dir.join("config.toml");
    let mut outputs = Vec::new();

    if auth_path.exists() {
        let mut auth = read_json_object(&auth_path).unwrap_or_default();
        auth.remove("OPENAI_API_KEY");
        outputs.push((
            auth_path,
            serde_json::to_string_pretty(&Value::Object(auth)).map_err(|e| e.to_string())?,
        ));
    }

    if config_path.exists() {
        let content = fs::read_to_string(&config_path).unwrap_or_default();
        let mut doc = content
            .parse::<toml_edit::DocumentMut>()
            .unwrap_or_else(|_| toml_edit::DocumentMut::new());
        let active_model_provider = doc
            .get("model_provider")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string());
        let active_is_onespace = active_model_provider
            .as_deref()
            .map(is_onespace_codex_model_provider_id)
            .unwrap_or(false);

        for key in [
            "base_url",
            "disable_response_storage",
            "personality",
            "model_reasoning_effort",
            "model_reasoning_summary",
            "approval_policy",
            "sandbox_mode",
        ] {
            doc.remove(key);
        }

        if active_is_onespace {
            doc.remove("model");
            doc.remove("model_provider");
            doc.remove("forced_login_method");
            doc.remove("preferred_auth_method");
        }

        if let Some(providers) = doc
            .get_mut("model_providers")
            .and_then(|v| v.as_table_mut())
        {
            if let Some(provider_id) = active_model_provider.as_deref() {
                if is_onespace_codex_model_provider_id(provider_id) {
                    providers.remove(provider_id);
                }
            }
        }

        outputs.push((config_path, doc.to_string()));
    }

    Ok(outputs)
}
