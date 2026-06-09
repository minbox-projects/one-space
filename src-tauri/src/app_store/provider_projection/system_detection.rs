use crate::app_store::{
    claude_model_env_keys_for_family, generate_provider_uuid, parse_supported_capabilities_csv,
    resolve_claude_default_model_from_settings, split_claude_1m_suffix, ClaudeModelMapping,
    CliInstallCommand, CliInstallGuide, ProviderRecord, MANAGED_TOOLS,
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::fs::{self};
use std::path::Path;

pub(in crate::app_store) fn is_managed_tool(tool: &str) -> bool {
    MANAGED_TOOLS.contains(&tool)
}

pub(in crate::app_store) fn provider_env_managed(provider: &ProviderRecord) -> bool {
    if !is_managed_tool(&provider.core.tool) {
        return true;
    }
    provider
        .tool_config
        .get("env_managed")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Read the `_onespace_source_profile` marker from `~/.claude/settings.json`.
/// Returns the profile ID that is currently applied to the global Claude config.
pub(crate) fn read_global_claude_profile_id() -> Option<String> {
    let home_dir = dirs::home_dir()?;
    let path = home_dir.join(".claude").join("settings.json");
    let settings: Map<String, Value> = read_json_object(&path)?;
    settings
        .get("_onespace_source_profile")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub(in crate::app_store) fn cli_cmd_name(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

pub(in crate::app_store) fn detect_cli_installation(tool: &str) -> (bool, String) {
    let Some(cmd_name) = cli_cmd_name(tool) else {
        return (false, String::new());
    };

    let probe = crate::cli_probe::probe_cli_version(cmd_name);
    (probe.installed, probe.version)
}

pub(in crate::app_store) fn read_json_object(path: &Path) -> Option<Map<String, Value>> {
    let content = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<Value>(&content).ok()?;
    value.as_object().cloned()
}

pub(in crate::app_store) fn parse_first_json_value<T: DeserializeOwned>(
    content: &str,
) -> Option<T> {
    let mut stream = serde_json::Deserializer::from_str(content).into_iter::<Value>();
    let first = stream.next()?.ok()?;
    serde_json::from_value::<T>(first).ok()
}

pub(in crate::app_store) fn cli_has_system_config(tool: &str) -> bool {
    let Some(home_dir) = dirs::home_dir() else {
        return false;
    };

    match tool {
        "claude" => {
            let path = home_dir.join(".claude").join("settings.json");
            let Some(settings) = read_json_object(&path) else {
                return false;
            };
            if let Some(env) = settings.get("env").and_then(|v| v.as_object()) {
                return env.contains_key("ANTHROPIC_API_KEY")
                    || env.contains_key("ANTHROPIC_AUTH_TOKEN")
                    || env.contains_key("ANTHROPIC_BASE_URL")
                    || env.contains_key("ANTHROPIC_MODEL");
            }
            false
        }
        "codex" => {
            let auth_path = home_dir.join(".codex").join("auth.json");
            if let Some(auth) = read_json_object(&auth_path) {
                if auth
                    .get("OPENAI_API_KEY")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                {
                    return true;
                }
            }
            let cfg_path = home_dir.join(".codex").join("config.toml");
            if let Ok(content) = fs::read_to_string(cfg_path) {
                if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                    return doc.get("base_url").is_some()
                        || doc.get("model").is_some()
                        || doc.get("model_provider").is_some()
                        || doc.get("forced_login_method").is_some()
                        || doc.get("approval_policy").is_some()
                        || doc.get("sandbox_mode").is_some();
                }
            }
            false
        }
        "gemini" => {
            let env_path = home_dir.join(".gemini").join(".env");
            if let Ok(content) = fs::read_to_string(env_path) {
                let has_key = content.lines().any(|line| {
                    let line = line.trim();
                    line.starts_with("GEMINI_API_KEY=")
                        || line.starts_with("GOOGLE_GEMINI_BASE_URL=")
                        || line.starts_with("GEMINI_MODEL=")
                });
                if has_key {
                    return true;
                }
            }
            let settings_path = home_dir.join(".gemini").join("settings.json");
            if let Some(settings) = read_json_object(&settings_path) {
                return settings.get("security").is_some() || settings.get("general").is_some();
            }
            false
        }
        "opencode" => {
            let path = home_dir
                .join(".config")
                .join("opencode")
                .join("opencode.json");
            if let Some(settings) = read_json_object(&path) {
                return settings
                    .get("provider")
                    .and_then(|v| v.as_object())
                    .map(|m| !m.is_empty())
                    .unwrap_or(false);
            }
            false
        }
        _ => false,
    }
}

pub(in crate::app_store) fn install_guide_for(tool: &str) -> CliInstallGuide {
    match tool {
        "claude" => CliInstallGuide {
            docs_url: "https://docs.anthropic.com/en/docs/claude-code".to_string(),
            commands: vec![CliInstallCommand {
                label: "Recommended".to_string(),
                command: "curl -fsSL https://claude.ai/install.sh | bash".to_string(),
            }],
        },
        "codex" => CliInstallGuide {
            docs_url: "https://github.com/openai/codex".to_string(),
            commands: vec![CliInstallCommand {
                label: "Recommended".to_string(),
                command: "bun install -g @openai/codex".to_string(),
            }],
        },
        "gemini" => CliInstallGuide {
            docs_url: "https://github.com/google-gemini/gemini-cli".to_string(),
            commands: vec![CliInstallCommand {
                label: "Recommended".to_string(),
                command: "npm install -g @google/gemini-cli".to_string(),
            }],
        },
        "opencode" => CliInstallGuide {
            docs_url: "https://opencode.ai/docs".to_string(),
            commands: vec![CliInstallCommand {
                label: "Recommended".to_string(),
                command: "curl -fsSL https://opencode.ai/install | bash".to_string(),
            }],
        },
        _ => CliInstallGuide {
            docs_url: String::new(),
            commands: vec![],
        },
    }
}

pub(in crate::app_store) fn read_system_provider(tool: &str) -> Option<ProviderRecord> {
    if !is_managed_tool(tool) {
        return None;
    }
    let home_dir = dirs::home_dir()?;
    read_system_provider_at_home(tool, &home_dir)
}

pub(in crate::app_store) fn read_system_provider_at_home(
    tool: &str,
    home_dir: &Path,
) -> Option<ProviderRecord> {
    if !is_managed_tool(tool) {
        return None;
    }
    let mut provider = ProviderRecord::default();
    provider.core.id = generate_provider_uuid();
    provider.core.tool = tool.to_string();
    provider.core.code = Some(format!("default-{}", tool));
    provider.core.name = match tool {
        "claude" => "Imported Claude Config".to_string(),
        "codex" => "Imported Codex Config".to_string(),
        "gemini" => "Imported Gemini Config".to_string(),
        _ => "Imported Config".to_string(),
    };
    provider
        .tool_config
        .insert("env_managed".to_string(), Value::Bool(true));

    match tool {
        "claude" => {
            let path = home_dir.join(".claude").join("settings.json");
            let settings = read_json_object(&path)?;
            let normalized_default_model = resolve_claude_default_model_from_settings(&settings);
            if let Some(env) = settings.get("env").and_then(|v| v.as_object()) {
                if let Some(key) = env
                    .get("ANTHROPIC_API_KEY")
                    .and_then(|v| v.as_str())
                    .or_else(|| env.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()))
                {
                    provider.core.api_key = key.to_string();
                }
                if let Some(v) = env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()) {
                    provider.core.base_url = Some(v.to_string());
                }
                let mut claude_model_mappings = Vec::new();
                for family in ["haiku", "sonnet", "opus"] {
                    let Some((model_key, name_key, capabilities_key)) =
                        claude_model_env_keys_for_family(family)
                    else {
                        continue;
                    };
                    let raw_model = env.get(model_key).and_then(|v| v.as_str()).unwrap_or("");
                    let (upstream_model, supports_1m) = split_claude_1m_suffix(raw_model);
                    let display_name = env
                        .get(name_key)
                        .and_then(|v| v.as_str())
                        .unwrap_or(match family {
                            "haiku" => "Haiku",
                            "sonnet" => "Sonnet",
                            "opus" => "Opus",
                            _ => "",
                        })
                        .to_string();
                    let supported_capabilities = env
                        .get(capabilities_key)
                        .and_then(|v| v.as_str())
                        .and_then(parse_supported_capabilities_csv);
                    if !upstream_model.is_empty()
                        || !display_name.is_empty()
                        || supported_capabilities.is_some()
                    {
                        claude_model_mappings.push(ClaudeModelMapping {
                            family: family.to_string(),
                            display_name,
                            upstream_model,
                            supports_1m: Some(supports_1m && family != "haiku"),
                            supported_capabilities,
                        });
                    }
                }
                if !claude_model_mappings.is_empty() {
                    provider.tool_config.insert(
                        "claude_model_mappings".to_string(),
                        serde_json::to_value(&claude_model_mappings)
                            .unwrap_or_else(|_| Value::Array(vec![])),
                    );
                }
                for (src, dst) in [
                    ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "claude_haiku_model"),
                    ("ANTHROPIC_DEFAULT_SONNET_MODEL", "claude_sonnet_model"),
                    ("ANTHROPIC_DEFAULT_OPUS_MODEL", "claude_opus_model"),
                    ("CLAUDE_CODE_EFFORT_LEVEL", "claude_reasoning_effort"),
                ] {
                    if let Some(v) = env.get(src).and_then(|v| v.as_str()) {
                        provider
                            .tool_config
                            .insert(dst.to_string(), Value::String(v.to_string()));
                    }
                }
            }
            provider.core.model = normalized_default_model.clone();
            if let Some(model) = normalized_default_model {
                provider
                    .tool_config
                    .insert("claude_default_model".to_string(), Value::String(model));
            } else {
                provider.tool_config.remove("claude_default_model");
            }
            for (src, dst) in [
                ("dangerouslySkipPermissions", "dangerously_skip_permissions"),
                ("enableAllMemoryFeatures", "enable_all_memory_features"),
                ("enableMcp", "enable_mcp"),
            ] {
                if let Some(v) = settings.get(src).and_then(|v| v.as_bool()) {
                    provider.tool_config.insert(dst.to_string(), Value::Bool(v));
                }
            }
            for (src, dst) in [
                ("allowedTools", "allowed_tools"),
                ("blockedTools", "blocked_tools"),
            ] {
                if let Some(v) = settings.get(src) {
                    provider.tool_config.insert(dst.to_string(), v.clone());
                }
            }
            if let Some(v) = settings.get("maxSessionTurns").and_then(|v| v.as_u64()) {
                provider
                    .tool_config
                    .insert("max_session_turns".to_string(), Value::Number(v.into()));
            }
        }
        "codex" => {
            let auth_path = home_dir.join(".codex").join("auth.json");
            if let Some(auth) = read_json_object(&auth_path) {
                if let Some(v) = auth.get("OPENAI_API_KEY").and_then(|v| v.as_str()) {
                    provider.core.api_key = v.to_string();
                }
            }
            let config_path = home_dir.join(".codex").join("config.toml");
            if let Ok(content) = fs::read_to_string(config_path) {
                if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                    let active_model_provider = doc
                        .get("model_provider")
                        .and_then(|v| v.as_str())
                        .and_then(|id| {
                            doc.get("model_providers")
                                .and_then(|v| v.as_table())
                                .and_then(|table| table.get(id.trim()))
                                .and_then(|v| v.as_table())
                        });
                    if let Some(active_provider) = active_model_provider {
                        if let Some(v) = active_provider.get("base_url").and_then(|v| v.as_str()) {
                            provider.core.base_url = Some(v.to_string());
                        }
                        if let Some(wire_api) =
                            active_provider.get("wire_api").and_then(|v| v.as_str())
                        {
                            provider.tool_config.insert(
                                "wire_api".to_string(),
                                Value::String(wire_api.to_string()),
                            );
                        }
                    }
                    if let Some(v) = doc.get("base_url").and_then(|v| v.as_str()) {
                        if provider.core.base_url.is_none() {
                            provider.core.base_url = Some(v.to_string());
                        }
                    }
                    if let Some(v) = doc.get("model").and_then(|v| v.as_str()) {
                        provider.core.model = Some(v.to_string());
                    }
                    if let Some(v) = doc.get("forced_login_method").and_then(|v| v.as_str()) {
                        provider
                            .tool_config
                            .insert("codex_auth_mode".to_string(), Value::String(v.to_string()));
                    }
                    for k in [
                        "disable_response_storage",
                        "personality",
                        "model_reasoning_effort",
                        "model_reasoning_summary",
                        "approval_policy",
                        "sandbox_mode",
                    ] {
                        if let Some(v) = doc.get(k) {
                            if let Some(b) = v.as_bool() {
                                provider.tool_config.insert(k.to_string(), Value::Bool(b));
                            } else if let Some(s) = v.as_str() {
                                provider
                                    .tool_config
                                    .insert(k.to_string(), Value::String(s.to_string()));
                            }
                        }
                    }
                    if let Some(mp) = doc.get("model_providers").and_then(|v| v.as_table()) {
                        if let Some(default) = mp.get("default") {
                            if let Some(wire_api) = default.get("wire_api").and_then(|v| v.as_str())
                            {
                                provider
                                    .tool_config
                                    .entry("wire_api".to_string())
                                    .or_insert(Value::String(wire_api.to_string()));
                            }
                        }
                    }
                }
            }
        }
        "gemini" => {
            let env_path = home_dir.join(".gemini").join(".env");
            if let Ok(content) = fs::read_to_string(env_path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        let key = k.trim();
                        let val = v.trim().to_string();
                        match key {
                            "GEMINI_API_KEY" => provider.core.api_key = val,
                            "GOOGLE_GEMINI_BASE_URL" => provider.core.base_url = Some(val),
                            "GEMINI_MODEL" => provider.core.model = Some(val),
                            _ => {}
                        }
                    }
                }
            }
            let settings_path = home_dir.join(".gemini").join("settings.json");
            if let Some(settings) = read_json_object(&settings_path) {
                if let Some(v) = settings.get("theme") {
                    provider.tool_config.insert("theme".to_string(), v.clone());
                }
                if let Some(general) = settings.get("general").and_then(|v| v.as_object()) {
                    if let Some(v) = general.get("vimMode").and_then(|v| v.as_bool()) {
                        provider
                            .tool_config
                            .insert("vim_mode".to_string(), Value::Bool(v));
                    }
                    if let Some(v) = general.get("defaultApprovalMode").and_then(|v| v.as_str()) {
                        provider.tool_config.insert(
                            "default_approval_mode".to_string(),
                            Value::String(v.to_string()),
                        );
                    }
                }
                if let Some(auth_type) = settings
                    .get("security")
                    .and_then(|v| v.as_object())
                    .and_then(|s| s.get("auth"))
                    .and_then(|v| v.as_object())
                    .and_then(|a| a.get("selectedType"))
                    .and_then(|v| v.as_str())
                {
                    provider.tool_config.insert(
                        "gemini_auth_type".to_string(),
                        Value::String(auth_type.to_string()),
                    );
                }
            }
        }
        _ => return None,
    }

    Some(provider)
}
