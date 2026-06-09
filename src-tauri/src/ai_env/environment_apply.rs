use super::AiProvider;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

pub(in crate::ai_env) fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let temp_path = path.with_extension("tmp");
    let mut file = File::create(&temp_path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&temp_path, path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn remove_ai_environment(provider: AiProvider) -> Result<(), String> {
    if provider.tool != "opencode" {
        return Ok(());
    }
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let opencode_dir = home_dir.join(".config").join("opencode");
    let settings_path = opencode_dir.join("opencode.json");
    if !settings_path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
    let mut settings: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;
    if let Some(providers) = settings.get_mut("provider").and_then(|v| v.as_object_mut()) {
        let target_id = provider
            .provider_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "OpenCode provider_key is required".to_string())?;
        providers.remove(target_id);
        atomic_write(
            &settings_path,
            &serde_json::to_string_pretty(&settings).unwrap(),
        )?;
    }
    Ok(())
}

pub async fn apply_ai_environment(provider: AiProvider) -> Result<(), String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    if provider.tool == "opencode" && provider.is_enabled == Some(false) {
        return remove_ai_environment(provider);
    }
    match provider.tool.as_str() {
        "claude" => {
            let claude_dir = home_dir.join(".claude");
            let settings_path = claude_dir.join("settings.json");
            let mut settings = serde_json::Map::new();
            if settings_path.exists() {
                let content =
                    fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                    settings = map;
                }
            }
            if let Some(skip) = provider.dangerously_skip_permissions {
                settings.insert(
                    "dangerouslySkipPermissions".to_string(),
                    serde_json::Value::Bool(skip),
                );
            } else {
                settings.remove("dangerouslySkipPermissions");
            }
            if let Some(memory) = provider.enable_all_memory_features {
                settings.insert(
                    "enableAllMemoryFeatures".to_string(),
                    serde_json::Value::Bool(memory),
                );
            } else {
                settings.remove("enableAllMemoryFeatures");
            }
            if let Some(mcp) = provider.enable_mcp {
                settings.insert("enableMcp".to_string(), serde_json::Value::Bool(mcp));
            } else {
                settings.remove("enableMcp");
            }
            if let Some(allowed) = &provider.allowed_tools {
                if !allowed.is_empty() {
                    settings.insert(
                        "allowedTools".to_string(),
                        serde_json::Value::Array(
                            allowed
                                .iter()
                                .map(|s| serde_json::Value::String(s.clone()))
                                .collect(),
                        ),
                    );
                } else {
                    settings.remove("allowedTools");
                }
            } else {
                settings.remove("allowedTools");
            }
            if let Some(blocked) = &provider.blocked_tools {
                if !blocked.is_empty() {
                    settings.insert(
                        "blockedTools".to_string(),
                        serde_json::Value::Array(
                            blocked
                                .iter()
                                .map(|s| serde_json::Value::String(s.clone()))
                                .collect(),
                        ),
                    );
                } else {
                    settings.remove("blockedTools");
                }
            } else {
                settings.remove("blockedTools");
            }
            if let Some(turns) = provider.max_session_turns {
                settings.insert(
                    "maxSessionTurns".to_string(),
                    serde_json::Value::Number(turns.into()),
                );
            } else {
                settings.remove("maxSessionTurns");
            }
            if !settings.contains_key("env") {
                settings.insert(
                    "env".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            let default_model = crate::app_store::normalize_claude_default_model_value(
                provider
                    .claude_default_model
                    .as_deref()
                    .or(provider.model.as_deref()),
            );
            if let Some(ref default_model) = default_model {
                settings.insert(
                    "model".to_string(),
                    serde_json::Value::String(default_model.clone()),
                );
            } else {
                settings.remove("model");
            }
            if let Some(serde_json::Value::Object(ref mut env)) = settings.get_mut("env") {
                // Set API key and remove AUTH_TOKEN to avoid conflict
                env.insert(
                    "ANTHROPIC_API_KEY".to_string(),
                    serde_json::Value::String(provider.api_key.clone()),
                );
                env.remove("ANTHROPIC_AUTH_TOKEN"); // Remove to avoid auth conflict

                if let Some(base_url) = provider.base_url {
                    if !base_url.is_empty() {
                        env.insert(
                            "ANTHROPIC_BASE_URL".to_string(),
                            serde_json::Value::String(base_url),
                        );
                    } else {
                        env.remove("ANTHROPIC_BASE_URL");
                    }
                } else {
                    env.remove("ANTHROPIC_BASE_URL");
                }
                if let Some(ref default_model) = default_model {
                    env.insert(
                        "ANTHROPIC_MODEL".to_string(),
                        serde_json::Value::String(default_model.clone()),
                    );
                } else {
                    env.remove("ANTHROPIC_MODEL");
                }
                let claude_model_mappings =
                    provider.claude_model_mappings.clone().unwrap_or_else(|| {
                        let mut tool_config = serde_json::Map::new();
                        if let Some(ref value) = provider.claude_haiku_model {
                            tool_config.insert(
                                "claude_haiku_model".to_string(),
                                serde_json::Value::String(value.clone()),
                            );
                        }
                        if let Some(ref value) = provider.claude_sonnet_model {
                            tool_config.insert(
                                "claude_sonnet_model".to_string(),
                                serde_json::Value::String(value.clone()),
                            );
                        }
                        if let Some(ref value) = provider.claude_opus_model {
                            tool_config.insert(
                                "claude_opus_model".to_string(),
                                serde_json::Value::String(value.clone()),
                            );
                        }
                        crate::app_store::resolved_claude_model_mappings(&tool_config)
                    });
                for family in ["haiku", "sonnet", "opus"] {
                    let Some((model_key, name_key, capabilities_key)) =
                        crate::app_store::claude_model_env_keys_for_family(family)
                    else {
                        continue;
                    };
                    if let Some(mapping) = claude_model_mappings
                        .iter()
                        .find(|mapping| mapping.family == family)
                    {
                        let mut upstream_model = mapping.upstream_model.clone();
                        if mapping.supports_1m.unwrap_or(false)
                            && family != "haiku"
                            && !upstream_model.contains("[1m]")
                        {
                            upstream_model.push_str("[1m]");
                        }
                        if upstream_model.is_empty() {
                            env.remove(model_key);
                        } else {
                            env.insert(
                                model_key.to_string(),
                                serde_json::Value::String(upstream_model),
                            );
                        }
                        if mapping.display_name.is_empty() {
                            env.remove(name_key);
                        } else {
                            env.insert(
                                name_key.to_string(),
                                serde_json::Value::String(mapping.display_name.clone()),
                            );
                        }
                        if let Some(capabilities) =
                            mapping.supported_capabilities.as_ref().and_then(|values| {
                                crate::app_store::join_supported_capabilities_csv(values)
                            })
                        {
                            env.insert(
                                capabilities_key.to_string(),
                                serde_json::Value::String(capabilities),
                            );
                        } else {
                            env.remove(capabilities_key);
                        }
                    } else {
                        env.remove(model_key);
                        env.remove(name_key);
                        env.remove(capabilities_key);
                    }
                }
                if let Some(ref effort) = provider.claude_reasoning_effort {
                    if !effort.is_empty() {
                        env.insert(
                            "CLAUDE_CODE_EFFORT_LEVEL".to_string(),
                            serde_json::Value::String(effort.clone()),
                        );
                    } else {
                        env.remove("CLAUDE_CODE_EFFORT_LEVEL");
                    }
                } else {
                    env.remove("CLAUDE_CODE_EFFORT_LEVEL");
                }
            }
            atomic_write(
                &settings_path,
                &serde_json::to_string_pretty(&settings).unwrap(),
            )?;
        }
        "codex" => {
            let codex_dir = home_dir.join(".codex");
            let auth_path = codex_dir.join("auth.json");
            let mut auth = serde_json::Map::new();
            if auth_path.exists() {
                let content = fs::read_to_string(&auth_path).unwrap_or_else(|_| "{}".to_string());
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                    auth = map;
                }
            }
            auth.insert(
                "OPENAI_API_KEY".to_string(),
                serde_json::Value::String(provider.api_key),
            );
            atomic_write(&auth_path, &serde_json::to_string_pretty(&auth).unwrap())?;
            let config_path = codex_dir.join("config.toml");
            let mut toml_str = String::new();
            if config_path.exists() {
                toml_str = fs::read_to_string(&config_path).unwrap_or_default();
            }
            let mut doc = toml_str
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| e.to_string())?;

            if let Some(disable) = provider.disable_response_storage {
                doc["disable_response_storage"] = toml_edit::value(disable);
            } else {
                doc.remove("disable_response_storage");
            }

            if let Some(ref personality) = provider.personality {
                doc["personality"] = toml_edit::value(personality.clone());
            } else {
                doc.remove("personality");
            }

            if let Some(base_url) = provider.base_url {
                if !base_url.is_empty() {
                    doc["base_url"] = toml_edit::value(base_url);
                } else {
                    doc.remove("base_url");
                }
            } else {
                doc.remove("base_url");
            }
            if let Some(model) = provider.model {
                if !model.is_empty() {
                    doc["model"] = toml_edit::value(model);
                } else {
                    doc.remove("model");
                }
            } else {
                doc.remove("model");
            }

            if let Some(ref wire_api) = provider.wire_api {
                let model_provider_name = "default";
                if !doc.contains_key("model_providers") {
                    doc["model_providers"] = toml_edit::Item::Table(toml_edit::Table::new());
                }
                if let Some(providers) = doc["model_providers"].as_table_mut() {
                    if !providers.contains_key(model_provider_name) {
                        providers[model_provider_name] =
                            toml_edit::Item::Table(toml_edit::Table::new());
                    }
                    if let Some(provider_table) = providers[model_provider_name].as_table_mut() {
                        provider_table.insert("wire_api", toml_edit::value(wire_api.clone()));
                    }
                }
            } else {
                doc.remove("wire_api");
                if let Some(providers) = doc["model_providers"].as_table_mut() {
                    if let Some(default_provider) = providers.get_mut("default") {
                        if let Some(provider_table) = default_provider.as_table_mut() {
                            provider_table.remove("wire_api");
                        }
                    }
                }
            }

            // Codex 新增配置参数
            if let Some(ref effort) = provider.model_reasoning_effort {
                doc["model_reasoning_effort"] = toml_edit::value(effort.clone());
            } else {
                doc.remove("model_reasoning_effort");
            }

            if let Some(ref summary) = provider.model_reasoning_summary {
                doc["model_reasoning_summary"] = toml_edit::value(summary.clone());
            } else {
                doc.remove("model_reasoning_summary");
            }

            if let Some(ref policy) = provider.approval_policy {
                doc["approval_policy"] = toml_edit::value(policy.clone());
            } else {
                doc.remove("approval_policy");
            }

            if let Some(ref sandbox) = provider.sandbox_mode {
                doc["sandbox_mode"] = toml_edit::value(sandbox.clone());
            } else {
                doc.remove("sandbox_mode");
            }

            atomic_write(&config_path, &doc.to_string())?;
        }
        "gemini" => {
            let gemini_dir = home_dir.join(".gemini");
            let env_path = gemini_dir.join(".env");
            let mut env_map = std::collections::BTreeMap::new();
            if env_path.exists() {
                let content = fs::read_to_string(&env_path).unwrap_or_default();
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        env_map.insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
            }
            env_map.insert("GEMINI_API_KEY".to_string(), provider.api_key);
            if let Some(base_url) = provider.base_url {
                if !base_url.is_empty() {
                    env_map.insert("GOOGLE_GEMINI_BASE_URL".to_string(), base_url);
                } else {
                    env_map.remove("GOOGLE_GEMINI_BASE_URL");
                }
            } else {
                env_map.remove("GOOGLE_GEMINI_BASE_URL");
            }
            if let Some(model) = provider.model {
                if !model.is_empty() {
                    env_map.insert("GEMINI_MODEL".to_string(), model);
                } else {
                    env_map.remove("GEMINI_MODEL");
                }
            } else {
                env_map.remove("GEMINI_MODEL");
            }
            let mut env_content = String::new();
            for (k, v) in env_map {
                env_content.push_str(&format!("{}={}\n", k, v));
            }
            atomic_write(&env_path, &env_content)?;

            let settings_path = gemini_dir.join("settings.json");
            let mut settings = serde_json::Map::new();

            if settings_path.exists() {
                let content =
                    fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                    settings = map;
                }
            }

            if let Some(ref auth_type) = provider.gemini_auth_type {
                if !settings.contains_key("security") {
                    settings.insert(
                        "security".to_string(),
                        serde_json::Value::Object(serde_json::Map::new()),
                    );
                }
                if let Some(security_val) = settings.get_mut("security") {
                    if let Some(security) = security_val.as_object_mut() {
                        if !security.contains_key("auth") {
                            security.insert(
                                "auth".to_string(),
                                serde_json::Value::Object(serde_json::Map::new()),
                            );
                        }
                        if let Some(auth_val) = security.get_mut("auth") {
                            if let Some(auth) = auth_val.as_object_mut() {
                                auth.insert(
                                    "selectedType".to_string(),
                                    serde_json::Value::String(auth_type.clone()),
                                );
                            }
                        }
                    }
                }
            } else {
                if let Some(security_val) = settings.get_mut("security") {
                    if let Some(security) = security_val.as_object_mut() {
                        if let Some(auth_val) = security.get_mut("auth") {
                            if let Some(auth) = auth_val.as_object_mut() {
                                auth.remove("selectedType");
                            }
                        }
                    }
                }
            }

            // Gemini 新增配置参数
            if let Some(ref theme) = provider.theme {
                settings.insert(
                    "theme".to_string(),
                    serde_json::Value::String(theme.clone()),
                );
            } else {
                settings.remove("theme");
            }

            // general.vimMode
            if let Some(vim) = provider.vim_mode {
                if !settings.contains_key("general") {
                    settings.insert(
                        "general".to_string(),
                        serde_json::Value::Object(serde_json::Map::new()),
                    );
                }
                if let Some(general_val) = settings.get_mut("general") {
                    if let Some(general) = general_val.as_object_mut() {
                        general.insert("vimMode".to_string(), serde_json::Value::Bool(vim));
                    }
                }
            }

            // general.defaultApprovalMode
            if let Some(ref mode) = provider.default_approval_mode {
                if !settings.contains_key("general") {
                    settings.insert(
                        "general".to_string(),
                        serde_json::Value::Object(serde_json::Map::new()),
                    );
                }
                if let Some(general_val) = settings.get_mut("general") {
                    if let Some(general) = general_val.as_object_mut() {
                        general.insert(
                            "defaultApprovalMode".to_string(),
                            serde_json::Value::String(mode.clone()),
                        );
                    }
                }
            }

            if !settings.is_empty() {
                atomic_write(
                    &settings_path,
                    &serde_json::to_string_pretty(&settings).unwrap(),
                )?;
            }
        }
        "opencode" => {
            let opencode_dir = home_dir.join(".config").join("opencode");
            let settings_path = opencode_dir.join("opencode.json");
            let mut settings = serde_json::Map::new();
            if settings_path.exists() {
                let content =
                    fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                    settings = map;
                }
            }
            if !settings.contains_key("$schema") {
                settings.insert(
                    "$schema".to_string(),
                    serde_json::Value::String("https://opencode.ai/config.json".to_string()),
                );
            }

            if let Some(ref default_model) = provider.opencode_default_model {
                if !default_model.is_empty() {
                    settings.insert(
                        "model".to_string(),
                        serde_json::Value::String(default_model.clone()),
                    );
                } else {
                    settings.remove("model");
                }
            } else {
                settings.remove("model");
            }

            if let Some(ref default_agent) = provider.opencode_default_agent {
                if !default_agent.is_empty() {
                    if !settings.contains_key("agent") {
                        settings.insert(
                            "agent".to_string(),
                            serde_json::Value::Object(serde_json::Map::new()),
                        );
                    }
                    if let Some(agent_val) = settings.get_mut("agent") {
                        if let Some(agent) = agent_val.as_object_mut() {
                            agent.insert(
                                "default".to_string(),
                                serde_json::Value::String(default_agent.clone()),
                            );
                        }
                    }
                } else {
                    if let Some(agent_val) = settings.get_mut("agent") {
                        if let Some(agent) = agent_val.as_object_mut() {
                            agent.remove("default");
                            if agent.is_empty() {
                                settings.remove("agent");
                            }
                        }
                    }
                }
            } else {
                if let Some(agent_val) = settings.get_mut("agent") {
                    if let Some(agent) = agent_val.as_object_mut() {
                        agent.remove("default");
                        if agent.is_empty() {
                            settings.remove("agent");
                        }
                    }
                }
            }

            if let Some(ref sessions_dir) = provider.opencode_sessions_dir {
                if !sessions_dir.is_empty() {
                    if !settings.contains_key("sessions") {
                        settings.insert(
                            "sessions".to_string(),
                            serde_json::Value::Object(serde_json::Map::new()),
                        );
                    }
                    if let Some(sessions_val) = settings.get_mut("sessions") {
                        if let Some(sessions) = sessions_val.as_object_mut() {
                            sessions.insert(
                                "dir".to_string(),
                                serde_json::Value::String(sessions_dir.clone()),
                            );
                        }
                    }
                } else {
                    if let Some(sessions_val) = settings.get_mut("sessions") {
                        if let Some(sessions) = sessions_val.as_object_mut() {
                            sessions.remove("dir");
                            if sessions.is_empty() {
                                settings.remove("sessions");
                            }
                        }
                    }
                }
            } else {
                if let Some(sessions_val) = settings.get_mut("sessions") {
                    if let Some(sessions) = sessions_val.as_object_mut() {
                        sessions.remove("dir");
                        if sessions.is_empty() {
                            settings.remove("sessions");
                        }
                    }
                }
            }

            // OpenCode 新增配置参数
            if let Some(ref small_model) = provider.small_model {
                if !small_model.is_empty() {
                    settings.insert(
                        "small_model".to_string(),
                        serde_json::Value::String(small_model.clone()),
                    );
                } else {
                    settings.remove("small_model");
                }
            } else {
                settings.remove("small_model");
            }

            if let Some(timeout) = provider.timeout {
                settings.insert(
                    "timeout".to_string(),
                    serde_json::Value::Number(timeout.into()),
                );
            } else {
                settings.remove("timeout");
            }

            // share mode
            if let Some(ref share_mode) = provider.share_mode {
                if !share_mode.is_empty() {
                    let share_obj = serde_json::json!({
                        "mode": share_mode
                    });
                    settings.insert("share".to_string(), share_obj);
                } else {
                    settings.remove("share");
                }
            } else {
                settings.remove("share");
            }

            if !settings.contains_key("provider") {
                settings.insert(
                    "provider".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            if let Some(serde_json::Value::Object(ref mut providers)) = settings.get_mut("provider")
            {
                let target_id = provider
                    .provider_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "OpenCode provider_key is required".to_string())?;
                let mut full_provider_json = serde_json::to_value(&provider).unwrap();
                if let Some(obj) = full_provider_json.as_object_mut() {
                    obj.remove("id");
                    obj.remove("tool");
                    obj.remove("is_enabled");
                    obj.remove("provider_key");
                    obj.remove("history");
                    obj.remove("api_key");
                    obj.remove("base_url");
                    obj.remove("model");
                    obj.remove("claude_reasoning_model");
                    obj.remove("claude_haiku_model");
                    obj.remove("claude_sonnet_model");
                    obj.remove("claude_opus_model");
                    obj.remove("dangerously_skip_permissions");
                    obj.remove("enable_all_memory_features");
                    obj.remove("enable_mcp");
                    obj.remove("allowed_tools");
                    obj.remove("blocked_tools");
                    obj.remove("max_session_turns");
                    obj.remove("disable_response_storage");
                    obj.remove("personality");
                    obj.remove("wire_api");
                    obj.remove("gemini_auth_type");
                    obj.remove("opencode_default_model");
                    obj.remove("opencode_default_agent");
                    obj.remove("opencode_sessions_dir");
                }
                providers.insert(target_id.to_string(), full_provider_json);
            }
            atomic_write(
                &settings_path,
                &serde_json::to_string_pretty(&settings).unwrap(),
            )?;
        }
        _ => return Err(format!("Unknown tool: {}", provider.tool)),
    }
    Ok(())
}
