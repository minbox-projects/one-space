use crate::app_store::{ProviderRecord, ProvidersState};
use crate::config;
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn get_claude_profiles_dir() -> Result<PathBuf, String> {
    Ok(config::get_app_dir()?.join("claude_profiles"))
}

pub(crate) fn claude_profile_dir(profile_id_or_name: &str) -> Result<PathBuf, String> {
    Ok(get_claude_profiles_dir()?.join(safe_dir_name(profile_id_or_name)))
}

pub(crate) fn safe_dir_name(raw: &str) -> String {
    if raw.is_empty() {
        return "profile".to_string();
    }
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.to_lowercase();
    let s: String = s.chars().fold(String::new(), |mut acc, c| {
        if acc.ends_with('_') && c == '_' {
            // skip consecutive underscores
        } else {
            acc.push(c);
        }
        acc
    });
    let s = s.trim_matches('_').to_string();
    if s.is_empty() || s == "-" {
        "profile".to_string()
    } else {
        s
    }
}

/// 解析 Claude profile 的目录名。优先使用 `code`，无 code 时回退到 id。
pub(crate) fn resolve_claude_dir_name(provider: &ProviderRecord) -> String {
    if let Some(ref code) = provider.core.code {
        if !code.trim().is_empty() {
            return safe_dir_name(code);
        }
    }
    safe_dir_name(&provider.core.id)
}

/// Legacy materialize that accepts ProviderRecord for backward compatibility.
pub(crate) fn materialize_claude_settings(
    provider: &ProviderRecord,
    profile_dir: &Path,
) -> Result<(), String> {
    // Convert to ServiceProviderRecord and delegate
    let sp = provider_to_service_provider_record(provider);
    materialize_claude_settings_sp(&sp, profile_dir)
}

fn provider_to_service_provider_record(p: &ProviderRecord) -> crate::app_store::ServiceProviderRecord {
    use crate::app_store::{ServiceProviderRecord, ClaudeModelMapping};
    // Migrate old haiku/sonnet/opus fields to claude_model_mappings
    let haiku_model = p.tool_config.get("claude_haiku_model").and_then(|v| v.as_str()).unwrap_or("claude-haiku-4-3-20250514");
    let sonnet_model = p.tool_config.get("claude_sonnet_model").and_then(|v| v.as_str()).unwrap_or("claude-sonnet-4-20250514");
    let opus_model = p.tool_config.get("claude_opus_model").and_then(|v| v.as_str()).unwrap_or("claude-opus-4-20250514");
    let mappings = vec![
        ClaudeModelMapping { family: "haiku".to_string(), display_name: "Haiku".to_string(), upstream_model: haiku_model.to_string(), supports_1m: Some(false) },
        ClaudeModelMapping { family: "sonnet".to_string(), display_name: "Sonnet".to_string(), upstream_model: sonnet_model.to_string(), supports_1m: Some(false) },
        ClaudeModelMapping { family: "opus".to_string(), display_name: "Opus".to_string(), upstream_model: opus_model.to_string(), supports_1m: Some(false) },
    ];
    let auth_env = "ANTHROPIC_API_KEY"; // Keep legacy behavior: always use ANTHROPIC_API_KEY for migrated records
    ServiceProviderRecord {
        id: p.core.id.clone(),
        name: p.core.name.clone(),
        tool: p.core.tool.clone(),
        icon: None,
        api_key: p.core.api_key.clone(),
        base_url: p.core.base_url.clone(),
        model: p.core.model.clone(),
        claude_api_format: "anthropic_messages".to_string(),
        claude_auth_env_key: auth_env.to_string(),
        claude_model_mappings: mappings,
        claude_enable_tool_search: p.tool_config.get("enable_tool_search").and_then(|v| v.as_bool()),
        claude_auto_memory_enabled: p
            .tool_config
            .get("claude_auto_memory_enabled")
            .and_then(|v| v.as_bool()),
        claude_always_thinking_enabled: p
            .tool_config
            .get("claude_always_thinking_enabled")
            .and_then(|v| v.as_bool()),
        claude_away_summary_enabled: p
            .tool_config
            .get("claude_away_summary_enabled")
            .and_then(|v| v.as_bool()),
        claude_include_git_instructions: p
            .tool_config
            .get("claude_include_git_instructions")
            .and_then(|v| v.as_bool()),
        claude_enable_attribution: p.tool_config.get("enable_attribution").and_then(|v| v.as_bool()),
        code: p.core.code.clone(),
        is_enabled: p.is_enabled,
        provider_key: p.provider_key.clone(),
        env_managed: None,
        tool_config: p.tool_config.clone(),
        history: p.history.clone(),
        extra: p.extra.clone(),
        fetched_models: None,
    }
}

/// Materialize Claude settings from a ServiceProviderRecord.
pub(crate) fn materialize_claude_settings_sp(
    provider: &crate::app_store::ServiceProviderRecord,
    profile_dir: &Path,
) -> Result<(), String> {
    use serde_json::Map;
    fs::create_dir_all(profile_dir).map_err(|e| format!("Failed to create profile dir: {e}"))?;

    let mut settings = read_claude_settings_sp(profile_dir)?;

    // Legacy tool_config fields pass-through
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
    for (value, key) in [
        (provider.claude_auto_memory_enabled, "autoMemoryEnabled"),
        (provider.claude_always_thinking_enabled, "alwaysThinkingEnabled"),
        (provider.claude_away_summary_enabled, "awaySummaryEnabled"),
        (
            provider.claude_include_git_instructions,
            "includeGitInstructions",
        ),
    ] {
        if let Some(v) = value {
            settings.insert(key.to_string(), Value::Bool(v));
        } else {
            settings.remove(key);
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
    if let Some(turns) = provider.tool_config.get("max_session_turns").and_then(|v| v.as_u64()) {
        settings.insert("maxSessionTurns".to_string(), Value::Number(turns.into()));
    }

    let mut env = settings.remove("env").and_then(|v| v.as_object().cloned()).unwrap_or_default();

    // Determine API format and auth
    let api_format = if provider.claude_api_format.is_empty() {
        "anthropic_messages"
    } else {
        &provider.claude_api_format
    };

    // Also check legacy tool_config fields for protocol proxy trigger
    let legacy_use_proxy = provider.tool_config.get("model_source").and_then(|v| v.as_str()) == Some("protocol_proxy")
        || provider.tool_config.get("protocol_proxy_route_id").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
    let use_protocol_proxy = api_format == "open_ai_chat" || api_format == "open_ai_responses" || legacy_use_proxy;

    if use_protocol_proxy {
        // Check if this is the new api_format-based proxy or legacy tool_config-based proxy
        let is_new_format = api_format == "open_ai_chat" || api_format == "open_ai_responses";
        if is_new_format {
            // Create/update protocol proxy route
            let route_id = crate::protocol_proxy::ensure_route_for_service_provider(
                &provider.id,
                &provider.name,
                &provider.base_url.clone().unwrap_or_default(),
                &provider.api_key,
                if api_format == "open_ai_chat" {
                    crate::protocol_proxy::WireApi::OpenAiChat
                } else {
                    crate::protocol_proxy::WireApi::OpenAiResponses
                },
                provider.model.as_deref(),
            )?;
            tauri::async_runtime::block_on(crate::protocol_proxy::protocol_proxy_start())
                .map_err(|e| format!("failed to start protocol proxy: {e}"))?;
            env.insert(
                "ANTHROPIC_API_KEY".to_string(),
                Value::String(crate::protocol_proxy::proxy_token()?),
            );
            env.insert(
                "ANTHROPIC_BASE_URL".to_string(),
                Value::String(crate::protocol_proxy::proxy_base_url_for_route(&route_id)?),
            );
        } else {
            // Legacy model_source=protocol_proxy mode: use existing route
            let route_id = provider.tool_config.get("protocol_proxy_route_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "protocol proxy route id is required".to_string())?;
            env.insert(
                "ANTHROPIC_API_KEY".to_string(),
                Value::String(crate::protocol_proxy::proxy_token()?),
            );
            env.insert(
                "ANTHROPIC_BASE_URL".to_string(),
                Value::String(crate::protocol_proxy::proxy_base_url_for_route(&route_id)?),
            );
            // Model from tool_config
            if let Some(model) = provider.tool_config.get("protocol_proxy_claude_model").and_then(|v| v.as_str()) {
                if !model.is_empty() {
                    env.insert("ANTHROPIC_MODEL".to_string(), Value::String(model.to_string()));
                }
            }
        }
    } else {
        // Anthropic native mode
        let auth_key = if provider.claude_auth_env_key.is_empty() {
            "ANTHROPIC_AUTH_TOKEN"
        } else {
            &provider.claude_auth_env_key
        };
        env.insert(auth_key.to_string(), Value::String(provider.api_key.clone()));
        // Remove the other auth key
        if auth_key == "ANTHROPIC_AUTH_TOKEN" {
            env.remove("ANTHROPIC_API_KEY");
        } else {
            env.remove("ANTHROPIC_AUTH_TOKEN");
        }

        if let Some(ref base_url) = provider.base_url {
            if !base_url.is_empty() {
                env.insert("ANTHROPIC_BASE_URL".to_string(), Value::String(base_url.clone()));
            } else {
                env.remove("ANTHROPIC_BASE_URL");
            }
        } else {
            env.remove("ANTHROPIC_BASE_URL");
        }
    }

    // Model mappings: haiku/sonnet/opus -> ANTHROPIC_DEFAULT_*_MODEL + _NAME
    for m in &provider.claude_model_mappings {
        let (env_key, name_key) = match m.family.as_str() {
            "haiku" => ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME"),
            "sonnet" => ("ANTHROPIC_DEFAULT_SONNET_MODEL", "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"),
            "opus" => ("ANTHROPIC_DEFAULT_OPUS_MODEL", "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"),
            _ => continue,
        };
        let mut model_val = m.upstream_model.clone();
        let supports_1m = m.supports_1m.unwrap_or(false);
        // 1M suffix: only Sonnet/Opus support it per Claude Code docs
        if supports_1m && m.family != "haiku" {
            if !model_val.contains("[1m]") {
                model_val = format!("{}[1m]", model_val);
            }
        }
        if !model_val.is_empty() {
            env.insert(env_key.to_string(), Value::String(model_val.clone()));
            if !m.display_name.is_empty() {
                env.insert(name_key.to_string(), Value::String(m.display_name.clone()));
            }
        }
    }

    // Hide attribution: write empty attribution object when claude_enable_attribution is false/None
    let enable_attribution = provider.claude_enable_attribution.unwrap_or(false);
    if !enable_attribution {
        let empty_attribution = serde_json::from_str::<Value>(r#"{"commit":"","pr":""}"#).unwrap_or(Value::Object(Map::new()));
        settings.insert("attribution".to_string(), empty_attribution);
    } else {
        settings.remove("attribution");
    }

    // Tool Search
    if provider.claude_enable_tool_search.unwrap_or(false) {
        env.insert("ENABLE_TOOL_SEARCH".to_string(), Value::String("true".to_string()));
    } else {
        env.remove("ENABLE_TOOL_SEARCH");
    }

    // Pass through legacy tool_config fields
    if let Some(v) = provider.tool_config.get("claude_default_model").and_then(|v| v.as_str()) {
        env.insert("ANTHROPIC_MODEL".to_string(), Value::String(v.to_string()));
    }
    if let Some(v) = provider.tool_config.get("claude_reasoning_model").and_then(|v| v.as_str()) {
        env.insert("ANTHROPIC_REASONING_MODEL".to_string(), Value::String(v.to_string()));
    }
    if let Some(v) = provider.tool_config.get("claude_reasoning_effort").and_then(|v| v.as_str()) {
        env.insert("CLAUDE_CODE_EFFORT_LEVEL".to_string(), Value::String(v.to_string()));
    }

    settings.insert("env".to_string(), Value::Object(env));

    let content = serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;
    fs::write(profile_dir.join("settings.json"), &content)
        .map_err(|e| format!("Failed to write settings.json: {e}"))?;

    Ok(())
}

fn read_claude_settings_sp(profile_dir: &Path) -> Result<Map<String, Value>, String> {
    read_claude_settings(profile_dir)
}

pub(crate) fn read_claude_settings(profile_dir: &Path) -> Result<Map<String, Value>, String> {
    let settings_path = profile_dir.join("settings.json");
    if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&content) {
            return Ok(map);
        }
    }
    Ok(Map::new())
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ClaudeProfileSummary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub code: Option<String>,
    pub config_dir: String,
    pub is_default: bool,
    pub is_global: bool,
    pub auth_type: String,
    pub model: Option<String>,
    pub tool_config: Map<String, Value>,
    pub raw_api_key: String,
    pub raw_base_url: Option<String>,
    pub tilde_config_dir: String,
}

pub(crate) fn resolve_claude_profile(
    state: &ProvidersState,
    query: &str,
) -> Option<ProviderRecord> {
    state
        .providers
        .iter()
        .find(|p| {
            p.core.tool == "claude"
                && (p.core.id == query
                    || p.core.name == query
                    || p.core.code.as_deref() == Some(query))
        })
        .cloned()
}

pub(crate) fn set_default_claude_profile(
    state: &mut ProvidersState,
    profile_id: &str,
) -> Result<(), String> {
    // Verify the profile exists and is a Claude provider
    let exists = state
        .providers
        .iter()
        .any(|p| p.core.id == profile_id && p.core.tool == "claude");
    if !exists {
        return Err(format!("Claude profile not found: {profile_id}"));
    }
    state
        .active
        .insert("claude".to_string(), profile_id.to_string());
    Ok(())
}

pub(crate) fn list_claude_profiles(state: &ProvidersState) -> Vec<ClaudeProfileSummary> {
    let default_id = state.active.get("claude").cloned();
    let global_profile_id = crate::app_store::read_global_claude_profile_id();
    let profiles_dir = get_claude_profiles_dir();
    let home_prefix = dirs::home_dir().map(|d| d.to_string_lossy().to_string() + "/");
    state
        .providers
        .iter()
        .filter(|p| p.core.tool == "claude")
        .map(|p| {
            let dir_name = resolve_claude_dir_name(p);
            let config_dir = profiles_dir
                .as_ref()
                .map(|d| d.join(&dir_name).to_string_lossy().to_string())
                .unwrap_or_default();
            let tilde_config_dir = home_prefix
                .as_ref()
                .map(|hp| {
                    if config_dir.starts_with(hp) {
                        format!("~/{}", &config_dir[hp.len()..])
                    } else {
                        config_dir.clone()
                    }
                })
                .unwrap_or_else(|| config_dir.clone());
            let auth_type = if p.core.api_key.is_empty() {
                "oauth"
            } else {
                "api_key"
            };
            ClaudeProfileSummary {
                id: p.core.id.clone(),
                name: p.core.name.clone(),
                icon: p
                    .tool_config
                    .get("icon")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                code: p.core.code.clone(),
                config_dir,
                is_default: default_id.as_deref() == Some(&p.core.id),
                is_global: global_profile_id.as_deref() == Some(&p.core.id),
                auth_type: auth_type.to_string(),
                model: p.core.model.clone(),
                tool_config: p.tool_config.clone(),
                raw_api_key: p.core.api_key.clone(),
                raw_base_url: p.core.base_url.clone(),
                tilde_config_dir,
            }
        })
        .collect()
}

pub(crate) fn get_claude_config_dir(profile_id: &str) -> Result<String, String> {
    let dir = claude_profile_dir(profile_id)?;
    Ok(dir.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_store::{ProviderCore, ProviderRuntimePolicy, ProvidersState};
    use serde_json::json;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("claude_profiles_test_{}", id));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_provider(id: &str, name: &str, api_key: &str) -> ProviderRecord {
        ProviderRecord {
            core: ProviderCore {
                id: id.to_string(),
                name: name.to_string(),
                tool: "claude".to_string(),
                api_key: api_key.to_string(),
                code: None,
                base_url: None,
                model: None,
            },
            runtime_policy: ProviderRuntimePolicy {
                approval_policy: None,
                sandbox_mode: None,
            },
            tool_config: Map::new(),
            history: vec![],
            extra: Map::new(),
            is_enabled: None,
            provider_key: None,
        }
    }

    #[test]
    fn test_get_claude_profiles_dir() {
        let dir = get_claude_profiles_dir().unwrap();
        assert!(dir.ends_with("claude_profiles"));
        assert!(dir.to_string_lossy().contains(".config/onespace"));
    }

    #[test]
    fn test_claude_profile_dir() {
        let dir = claude_profile_dir("work").unwrap();
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(name, "work");
        assert!(dir.to_string_lossy().contains("claude_profiles"));
    }

    #[test]
    fn test_safe_dir_name_empty() {
        assert_eq!(safe_dir_name(""), "profile");
    }

    #[test]
    fn test_safe_dir_name_normal() {
        assert_eq!(safe_dir_name("My Work!"), "my_work");
    }

    #[test]
    fn test_safe_dir_name_chinese() {
        assert_eq!(safe_dir_name("中文测试"), "profile");
    }

    #[test]
    fn test_safe_dir_name_path_traversal() {
        assert_eq!(safe_dir_name("../etc"), "etc");
        assert!(!safe_dir_name("../etc").contains(".."));
    }

    #[test]
    fn test_safe_dir_name_special_chars() {
        assert_eq!(safe_dir_name("Hello@World#2024"), "hello_world_2024");
    }

    #[test]
    fn test_safe_dir_name_just_dash() {
        assert_eq!(safe_dir_name("-"), "profile");
    }

    #[test]
    fn test_read_claude_settings_nonexistent() {
        let dir = temp_dir();
        let settings = read_claude_settings(&dir).unwrap();
        assert!(settings.is_empty());
    }

    #[test]
    fn test_materialize_claude_settings_creates_settings() {
        let dir = temp_dir();
        let mut provider = make_provider("test-claude", "Test Claude", "sk-ant-test123");
        provider.core.base_url = Some("https://example.com".to_string());
        provider.core.model = Some("claude-sonnet-4".to_string());
        provider.tool_config =
            serde_json::from_str(r#"{"dangerously_skip_permissions": true}"#).unwrap();

        materialize_claude_settings(&provider, &dir).unwrap();

        let settings_path = dir.join("settings.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let obj = parsed.as_object().unwrap();

        assert_eq!(obj["dangerouslySkipPermissions"], Value::Bool(true));
        assert!(obj["env"].is_object());
        let env = obj["env"].as_object().unwrap();
        assert_eq!(
            env["ANTHROPIC_API_KEY"],
            Value::String("sk-ant-test123".to_string())
        );
        assert_eq!(
            env["ANTHROPIC_BASE_URL"],
            Value::String("https://example.com".to_string())
        );
    }

    #[test]
    fn test_materialize_claude_settings_uses_protocol_proxy() {
        let original_home = std::env::var("HOME").ok();
        let home = temp_dir();
        std::env::set_var("HOME", &home);
        let dir = temp_dir();
        let mut provider = make_provider("proxy-claude", "Proxy Claude", "sk-upstream");
        provider.tool_config = serde_json::from_str(
            r#"{
                "model_source": "protocol_proxy",
                "protocol_proxy_route_id": "opencode-go",
                "protocol_proxy_claude_model": "sonnet"
            }"#,
        )
        .unwrap();

        materialize_claude_settings(&provider, &dir).unwrap();

        let content = fs::read_to_string(dir.join("settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let env = parsed["env"].as_object().unwrap();
        assert_eq!(
            env["ANTHROPIC_BASE_URL"],
            Value::String("http://127.0.0.1:17687/anthropic/opencode-go/v1".to_string())
        );
        assert_eq!(env["ANTHROPIC_MODEL"], Value::String("sonnet".to_string()));
        assert_ne!(
            env["ANTHROPIC_API_KEY"],
            Value::String("sk-upstream".to_string())
        );

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_materialize_claude_settings_preserves_existing() {
        let dir = temp_dir();
        let settings_path = dir.join("settings.json");

        let existing = json!({
            "oauthToken": "existing-oauth-token",
            "someHistory": ["entry1"],
            "env": {
                "ANTHROPIC_API_KEY": "old-key"
            }
        });
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let provider = make_provider("test-claude", "Test Claude", "sk-ant-new-key");

        materialize_claude_settings(&provider, &dir).unwrap();

        let content = fs::read_to_string(&settings_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let obj = parsed.as_object().unwrap();

        assert_eq!(
            obj["oauthToken"],
            Value::String("existing-oauth-token".to_string())
        );
        assert_eq!(obj["someHistory"], json!(["entry1"]));

        let env = obj["env"].as_object().unwrap();
        assert_eq!(
            env["ANTHROPIC_API_KEY"],
            Value::String("sk-ant-new-key".to_string())
        );
    }

    #[test]
    fn test_materialize_claude_settings_writes_and_removes_new_boolean_settings() {
        let dir = temp_dir();
        let settings_path = dir.join("settings.json");

        let existing = json!({
            "autoMemoryEnabled": true,
            "alwaysThinkingEnabled": true,
            "awaySummaryEnabled": true,
            "includeGitInstructions": true
        });
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let mut provider = crate::app_store::ServiceProviderRecord {
            id: "test-claude".to_string(),
            name: "Test Claude".to_string(),
            tool: "claude".to_string(),
            icon: None,
            api_key: "sk-ant-test123".to_string(),
            base_url: None,
            model: None,
            claude_api_format: "anthropic_messages".to_string(),
            claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
            claude_model_mappings: vec![],
            claude_enable_tool_search: Some(false),
            claude_auto_memory_enabled: Some(false),
            claude_always_thinking_enabled: Some(true),
            claude_away_summary_enabled: None,
            claude_include_git_instructions: Some(false),
            claude_enable_attribution: Some(false),
            code: None,
            is_enabled: None,
            provider_key: None,
            env_managed: None,
            tool_config: Map::new(),
            history: vec![],
            extra: Map::new(),
            fetched_models: None,
        };

        materialize_claude_settings_sp(&provider, &dir).unwrap();

        let content = fs::read_to_string(&settings_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let obj = parsed.as_object().unwrap();

        assert_eq!(obj["autoMemoryEnabled"], Value::Bool(false));
        assert_eq!(obj["alwaysThinkingEnabled"], Value::Bool(true));
        assert!(obj.get("awaySummaryEnabled").is_none());
        assert_eq!(obj["includeGitInstructions"], Value::Bool(false));

        provider.claude_auto_memory_enabled = None;
        provider.claude_always_thinking_enabled = None;
        provider.claude_away_summary_enabled = Some(true);
        provider.claude_include_git_instructions = None;

        materialize_claude_settings_sp(&provider, &dir).unwrap();

        let content = fs::read_to_string(&settings_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let obj = parsed.as_object().unwrap();

        assert!(obj.get("autoMemoryEnabled").is_none());
        assert!(obj.get("alwaysThinkingEnabled").is_none());
        assert_eq!(obj["awaySummaryEnabled"], Value::Bool(true));
        assert!(obj.get("includeGitInstructions").is_none());
    }

    #[test]
    fn test_materialize_with_empty_api_key() {
        let dir = temp_dir();
        let provider = make_provider("oauth-claude", "OAuth Claude", "");

        materialize_claude_settings(&provider, &dir).unwrap();

        let settings_path = dir.join("settings.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let env = parsed["env"].as_object().unwrap();
        assert_eq!(env["ANTHROPIC_API_KEY"], Value::String(String::new()));
    }

    #[test]
    fn test_resolve_claude_profile_by_id() {
        let mut state = ProvidersState::default();
        state
            .providers
            .push(make_provider("work-claude", "Work Claude", "sk-1"));
        state
            .providers
            .push(make_provider("personal-claude", "Personal Claude", "sk-2"));

        let found = resolve_claude_profile(&state, "work-claude").unwrap();
        assert_eq!(found.core.id, "work-claude");
        assert_eq!(found.core.name, "Work Claude");
    }

    #[test]
    fn test_resolve_claude_profile_by_name() {
        let mut state = ProvidersState::default();
        state
            .providers
            .push(make_provider("work-claude", "Work Claude", "sk-1"));

        let found = resolve_claude_profile(&state, "Work Claude").unwrap();
        assert_eq!(found.core.id, "work-claude");
    }

    #[test]
    fn test_resolve_claude_profile_not_found() {
        let state = ProvidersState::default();
        let found = resolve_claude_profile(&state, "nonexistent");
        assert!(found.is_none());
    }

    #[test]
    fn test_set_default_claude_profile() {
        let mut state = ProvidersState::default();
        state
            .providers
            .push(make_provider("work-claude", "Work Claude", "sk-1"));

        set_default_claude_profile(&mut state, "work-claude").unwrap();
        assert_eq!(
            state.active.get("claude").map(|s| s.as_str()),
            Some("work-claude")
        );
    }

    #[test]
    fn test_set_default_claude_profile_not_found() {
        let mut state = ProvidersState::default();
        let result = set_default_claude_profile(&mut state, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_claude_profiles() {
        let mut state = ProvidersState::default();
        state
            .providers
            .push(make_provider("work-claude", "Work Claude", "sk-1"));
        state
            .providers
            .push(make_provider("personal-claude", "Personal Claude", "sk-2"));

        let profiles = list_claude_profiles(&state);
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].id, "work-claude");
        assert_eq!(profiles[0].auth_type, "api_key");
        assert!(!profiles[0].is_default);
    }

    #[test]
    fn test_list_claude_profiles_with_default() {
        let mut state = ProvidersState::default();
        state
            .providers
            .push(make_provider("work-claude", "Work Claude", "sk-1"));
        state
            .active
            .insert("claude".to_string(), "work-claude".to_string());

        let profiles = list_claude_profiles(&state);
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].is_default);
    }

    #[test]
    fn test_list_claude_profiles_filters_non_claude() {
        let mut state = ProvidersState::default();
        state
            .providers
            .push(make_provider("work-claude", "Work Claude", "sk-1"));
        // Add a non-Claude provider that should be filtered out
        let mut codex = make_provider("work-codex", "Work Codex", "");
        codex.core.tool = "codex".to_string();
        state.providers.push(codex);

        let profiles = list_claude_profiles(&state);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, "work-claude");
    }

    #[test]
    fn test_get_claude_config_dir() {
        let dir = get_claude_config_dir("work").unwrap();
        assert!(dir.contains("claude_profiles"));
        assert!(dir.contains("work"));
    }

    #[test]
    fn test_resolve_claude_dir_name_with_code() {
        let mut provider = make_provider("my-id", "My Provider", "sk-1");
        provider.core.code = Some("work".to_string());
        assert_eq!(resolve_claude_dir_name(&provider), "work");
    }

    #[test]
    fn test_resolve_claude_dir_name_without_code() {
        let provider = make_provider("my-work-id", "My Work", "sk-1");
        assert_eq!(resolve_claude_dir_name(&provider), "my-work-id");
    }

    #[test]
    fn test_resolve_claude_dir_name_empty_code_fallback() {
        let mut provider = make_provider("fallback-id", "Fallback", "sk-1");
        provider.core.code = Some("".to_string());
        assert_eq!(resolve_claude_dir_name(&provider), "fallback-id");

        provider.core.code = Some("   ".to_string());
        assert_eq!(resolve_claude_dir_name(&provider), "fallback-id");
    }

    #[test]
    fn test_resolve_claude_dir_name_code_special_chars() {
        let mut provider = make_provider("some-id", "Some", "sk-1");
        provider.core.code = Some("My-Work!@#".to_string());
        assert_eq!(resolve_claude_dir_name(&provider), "my-work");
    }

    #[test]
    fn test_resolve_claude_profile_by_code() {
        let mut state = ProvidersState::default();
        let mut provider = make_provider("work-claude", "Work Claude", "sk-1");
        provider.core.code = Some("work".to_string());
        state.providers.push(provider);

        let found = resolve_claude_profile(&state, "work").unwrap();
        assert_eq!(found.core.id, "work-claude");
    }

    #[test]
    fn test_list_claude_profiles_includes_code() {
        let mut state = ProvidersState::default();
        let mut p1 = make_provider("p1", "Profile One", "sk-1");
        p1.core.code = Some("my-code".to_string());
        state.providers.push(p1);
        let p2 = make_provider("p2", "Profile Two", "sk-2");
        state.providers.push(p2);

        let profiles = list_claude_profiles(&state);
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].code, Some("my-code".to_string()));
        assert_eq!(profiles[1].code, None);
        // config_dir should use code for p1, id for p2
        assert!(profiles[0].config_dir.ends_with("my-code"));
        assert!(profiles[1].config_dir.ends_with("p2"));
    }
}
