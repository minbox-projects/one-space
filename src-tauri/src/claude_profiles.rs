use crate::app_store::{ProviderRecord, ProvidersState};
use crate::config;
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn get_claude_profiles_dir() -> Result<PathBuf, String> {
    Ok(config::get_app_dir()?.join("claude_profiles"))
}

#[allow(dead_code)]
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
#[allow(dead_code)]
pub(crate) fn materialize_claude_settings(
    provider: &ProviderRecord,
    profile_dir: &Path,
) -> Result<(), String> {
    // Convert to ServiceProviderRecord and delegate
    let sp = provider_to_service_provider_record(provider);
    materialize_claude_settings_sp(&sp, profile_dir)
}

#[allow(dead_code)]
fn provider_to_service_provider_record(
    p: &ProviderRecord,
) -> crate::app_store::ServiceProviderRecord {
    use crate::app_store::{ClaudeModelMapping, ServiceProviderRecord};
    // Migrate old haiku/sonnet/opus fields to claude_model_mappings
    let mappings = {
        let mappings = crate::app_store::resolved_claude_model_mappings(&p.tool_config);
        if mappings
            .iter()
            .any(|mapping| !mapping.upstream_model.trim().is_empty())
        {
            mappings
        } else {
            vec![
                ClaudeModelMapping {
                    family: "haiku".to_string(),
                    display_name: "Haiku".to_string(),
                    upstream_model: "claude-haiku-4-3-20250514".to_string(),
                    supports_1m: Some(false),
                    supported_capabilities: None,
                },
                ClaudeModelMapping {
                    family: "sonnet".to_string(),
                    display_name: "Sonnet".to_string(),
                    upstream_model: "claude-sonnet-4-20250514".to_string(),
                    supports_1m: Some(false),
                    supported_capabilities: None,
                },
                ClaudeModelMapping {
                    family: "opus".to_string(),
                    display_name: "Opus".to_string(),
                    upstream_model: "claude-opus-4-20250514".to_string(),
                    supports_1m: Some(false),
                    supported_capabilities: None,
                },
            ]
        }
    };
    let auth_env = "ANTHROPIC_API_KEY"; // Keep legacy behavior: always use ANTHROPIC_API_KEY for migrated records
    let claude_api_format = p
        .tool_config
        .get("claude_api_format")
        .and_then(|v| v.as_str())
        .unwrap_or("anthropic_messages")
        .to_string();
    let claude_connection_mode = p
        .tool_config
        .get("claude_connection_mode")
        .and_then(|v| v.as_str())
        .unwrap_or(
            if claude_api_format == "open_ai_chat" || claude_api_format == "open_ai_responses" {
                "protocol_router"
            } else {
                "native_anthropic"
            },
        )
        .to_string();
    ServiceProviderRecord {
        id: p.core.id.clone(),
        name: p.core.name.clone(),
        tool: p.core.tool.clone(),
        icon: None,
        api_key: p.core.api_key.clone(),
        base_url: p.core.base_url.clone(),
        model: p.core.model.clone(),
        claude_api_format,
        claude_connection_mode,
        protocol_router_upstream_provider_id: None,
        protocol_router_wire_api: "open_ai_chat".to_string(),
        claude_auth_env_key: auth_env.to_string(),
        claude_model_mappings: mappings,
        claude_enable_tool_search: p
            .tool_config
            .get("enable_tool_search")
            .and_then(|v| v.as_bool()),
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
        claude_enable_attribution: p
            .tool_config
            .get("enable_attribution")
            .and_then(|v| v.as_bool()),
        code: p.core.code.clone(),
        is_enabled: p.is_enabled,
        provider_key: p.provider_key.clone(),
        env_managed: None,
        favorite_at: p.favorite_at,
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
    materialize_claude_settings_sp_sync(provider, profile_dir)
}

pub(crate) async fn materialize_claude_settings_sp_async(
    provider: &crate::app_store::ServiceProviderRecord,
    profile_dir: &Path,
) -> Result<(), String> {
    materialize_claude_settings_sp_async_inner(provider, profile_dir).await
}

fn materialize_claude_settings_sp_sync(
    provider: &crate::app_store::ServiceProviderRecord,
    profile_dir: &Path,
) -> Result<(), String> {
    materialize_claude_settings_sp_with_router_start(provider, profile_dir, None)
}

async fn materialize_claude_settings_sp_async_inner(
    provider: &crate::app_store::ServiceProviderRecord,
    profile_dir: &Path,
) -> Result<(), String> {
    materialize_claude_settings_sp_with_router_start(
        provider,
        profile_dir,
        Some(crate::protocol_router::protocol_router_start().await),
    )
}

fn materialize_claude_settings_sp_with_router_start(
    provider: &crate::app_store::ServiceProviderRecord,
    profile_dir: &Path,
    async_router_start: Option<Result<crate::protocol_router::ProtocolRouterStatus, String>>,
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
        (
            provider.claude_always_thinking_enabled,
            "alwaysThinkingEnabled",
        ),
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
    let default_model = crate::app_store::resolve_claude_default_model(
        provider.model.as_deref(),
        &provider.tool_config,
    );

    let connection_mode = if provider.claude_connection_mode.is_empty() {
        "native_anthropic"
    } else {
        &provider.claude_connection_mode
    };
    let legacy_use_router = provider
        .tool_config
        .get("model_source")
        .and_then(|v| v.as_str())
        == Some("protocol_proxy")
        || provider
            .tool_config
            .get("protocol_proxy_route_id")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false)
        || provider.claude_api_format == "open_ai_chat"
        || provider.claude_api_format == "open_ai_responses";
    let use_protocol_router = connection_mode == "protocol_router" || legacy_use_router;

    if use_protocol_router {
        if let Some(result) = async_router_start {
            result.map_err(|e| format!("failed to start protocol router: {e}"))?;
        } else {
            tauri::async_runtime::block_on(crate::protocol_router::protocol_router_start())
                .map_err(|e| format!("failed to start protocol router: {e}"))?;
        }
        env.insert(
            "ANTHROPIC_API_KEY".to_string(),
            Value::String(crate::protocol_router::router_token()?),
        );
        env.remove("ANTHROPIC_AUTH_TOKEN");
        env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            Value::String(crate::protocol_router::router_base_url_for_claude_provider(
                &provider.id,
            )?),
        );
    } else {
        // Anthropic native mode
        let auth_key = if provider.claude_auth_env_key.is_empty() {
            "ANTHROPIC_AUTH_TOKEN"
        } else {
            &provider.claude_auth_env_key
        };
        env.insert(
            auth_key.to_string(),
            Value::String(provider.api_key.clone()),
        );
        // Remove the other auth key
        if auth_key == "ANTHROPIC_AUTH_TOKEN" {
            env.remove("ANTHROPIC_API_KEY");
        } else {
            env.remove("ANTHROPIC_AUTH_TOKEN");
        }

        if let Some(ref base_url) = provider.base_url {
            if !base_url.is_empty() {
                env.insert(
                    "ANTHROPIC_BASE_URL".to_string(),
                    Value::String(base_url.clone()),
                );
            } else {
                env.remove("ANTHROPIC_BASE_URL");
            }
        } else {
            env.remove("ANTHROPIC_BASE_URL");
        }
    }

    if let Some(model) = &default_model {
        settings.insert("model".to_string(), Value::String(model.clone()));
        env.insert("ANTHROPIC_MODEL".to_string(), Value::String(model.clone()));
    } else {
        settings.remove("model");
        env.remove("ANTHROPIC_MODEL");
    }

    let claude_model_mappings = if provider.claude_model_mappings.is_empty() {
        crate::app_store::resolved_claude_model_mappings(&provider.tool_config)
    } else {
        provider.claude_model_mappings.clone()
    };

    // Model mappings: haiku/sonnet/opus -> ANTHROPIC_DEFAULT_*_MODEL + _NAME + _SUPPORTED_CAPABILITIES
    for m in &claude_model_mappings {
        let Some((env_key, name_key, capabilities_key)) =
            crate::app_store::claude_model_env_keys_for_family(&m.family)
        else {
            continue;
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
            } else {
                env.remove(name_key);
            }
            if let Some(capabilities) = m
                .supported_capabilities
                .as_ref()
                .and_then(|values| crate::app_store::join_supported_capabilities_csv(values))
            {
                env.insert(capabilities_key.to_string(), Value::String(capabilities));
            } else {
                env.remove(capabilities_key);
            }
        } else {
            env.remove(env_key);
            env.remove(name_key);
            env.remove(capabilities_key);
        }
    }

    // Hide attribution: write empty attribution object when claude_enable_attribution is false/None
    let enable_attribution = provider.claude_enable_attribution.unwrap_or(false);
    if !enable_attribution {
        let empty_attribution = serde_json::from_str::<Value>(r#"{"commit":"","pr":""}"#)
            .unwrap_or(Value::Object(Map::new()));
        settings.insert("attribution".to_string(), empty_attribution);
    } else {
        settings.remove("attribution");
    }

    // Tool Search
    if provider.claude_enable_tool_search.unwrap_or(false) {
        env.insert(
            "ENABLE_TOOL_SEARCH".to_string(),
            Value::String("true".to_string()),
        );
    } else {
        env.remove("ENABLE_TOOL_SEARCH");
    }

    if let Some(v) = crate::app_store::resolve_claude_reasoning_effort(&provider.tool_config) {
        env.insert("CLAUDE_CODE_EFFORT_LEVEL".to_string(), Value::String(v));
    } else {
        env.remove("CLAUDE_CODE_EFFORT_LEVEL");
    }

    settings.insert("env".to_string(), Value::Object(env));

    let content =
        serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;
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
    pub favorite_at: Option<u64>,
    pub auth_type: String,
    pub model: Option<String>,
    pub claude_api_format: String,
    pub claude_connection_mode: String,
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
                favorite_at: p.favorite_at,
                auth_type: auth_type.to_string(),
                model: p.core.model.clone(),
                claude_api_format: p
                    .tool_config
                    .get("claude_api_format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("anthropic_messages")
                    .to_string(),
                claude_connection_mode: p
                    .tool_config
                    .get("claude_connection_mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("native_anthropic")
                    .to_string(),
                tool_config: p.tool_config.clone(),
                raw_api_key: p.core.api_key.clone(),
                raw_base_url: p.core.base_url.clone(),
                tilde_config_dir,
            }
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn get_claude_config_dir(profile_id: &str) -> Result<String, String> {
    let dir = claude_profile_dir(profile_id)?;
    Ok(dir.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        claude_profile_dir, get_claude_config_dir, get_claude_profiles_dir, list_claude_profiles,
        materialize_claude_settings, materialize_claude_settings_sp, read_claude_settings,
        resolve_claude_dir_name, resolve_claude_profile, safe_dir_name, set_default_claude_profile,
        ProviderRecord,
    };
    use crate::app_store::{ProviderCore, ProviderRuntimePolicy, ProvidersState};
    use serde_json::{json, Map, Value};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
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
            favorite_at: None,
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
    fn list_claude_profiles_includes_favorite_at() {
        let mut provider = make_provider("p1", "Claude", "sk-test");
        provider.favorite_at = Some(1234);
        let state = ProvidersState {
            active: HashMap::new(),
            providers: vec![provider],
        };

        let profiles = list_claude_profiles(&state);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].favorite_at, Some(1234));
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
        assert_eq!(obj["model"], Value::String("claude-sonnet-4".to_string()));
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
    fn test_materialize_claude_settings_uses_protocol_router() {
        let _guard = crate::lock_test_home_env();
        let original_home = std::env::var("HOME").ok();
        let home = temp_dir();
        std::env::set_var("HOME", &home);
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let app_dir = home.join(".config").join("onespace");
        fs::create_dir_all(&app_dir).unwrap();
        fs::write(
            app_dir.join("protocol_router.json"),
            serde_json::to_string_pretty(&json!({
                "enabled": true,
                "port": port,
                "token": "osp_test",
                "retention_days": 30
            }))
            .unwrap(),
        )
        .unwrap();
        let dir = temp_dir();
        let mut provider = make_provider("proxy-claude", "Proxy Claude", "sk-upstream");
        provider.tool_config =
            serde_json::from_str(r#"{ "protocol_proxy_claude_model": "sonnet" }"#).unwrap();
        provider.core.base_url = Some("https://openai-compatible.example.com/v1".to_string());
        provider.tool_config.insert(
            "claude_connection_mode".to_string(),
            Value::String("protocol_router".to_string()),
        );
        provider.tool_config.insert(
            "claude_api_format".to_string(),
            Value::String("open_ai_chat".to_string()),
        );
        provider.core.model = Some("claude-sonnet-4-5".to_string());

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            materialize_claude_settings(&provider, &dir).unwrap();

            let content = fs::read_to_string(dir.join("settings.json")).unwrap();
            let parsed: Value = serde_json::from_str(&content).unwrap();
            assert_eq!(
                parsed["model"],
                Value::String("claude-sonnet-4-5".to_string())
            );
            let env = parsed["env"].as_object().unwrap();
            assert_eq!(
                env["ANTHROPIC_BASE_URL"],
                Value::String(format!(
                    "http://127.0.0.1:{port}/anthropic/service-provider-proxy-claude/v1"
                ))
            );
            assert_eq!(
                env["ANTHROPIC_MODEL"],
                Value::String("claude-sonnet-4-5".to_string())
            );
            assert_ne!(
                env["ANTHROPIC_API_KEY"],
                Value::String("sk-upstream".to_string())
            );
        }));

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
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

        assert!(obj.get("model").is_none());
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
            claude_connection_mode: "native_anthropic".to_string(),
            protocol_router_upstream_provider_id: None,
            protocol_router_wire_api: "open_ai_chat".to_string(),
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
            favorite_at: None,
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
    fn test_materialize_claude_settings_writes_effort_and_supported_capabilities() {
        let dir = temp_dir();
        let provider = crate::app_store::ServiceProviderRecord {
            id: "test-claude".to_string(),
            name: "Test Claude".to_string(),
            tool: "claude".to_string(),
            icon: None,
            api_key: "sk-ant-test123".to_string(),
            base_url: Some("https://example.com".to_string()),
            model: None,
            claude_api_format: "anthropic_messages".to_string(),
            claude_connection_mode: "native_anthropic".to_string(),
            protocol_router_upstream_provider_id: None,
            protocol_router_wire_api: "open_ai_chat".to_string(),
            claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
            claude_model_mappings: vec![
                crate::app_store::ClaudeModelMapping {
                    family: "haiku".to_string(),
                    display_name: "Haiku".to_string(),
                    upstream_model: "claude-haiku-4-5".to_string(),
                    supports_1m: Some(false),
                    supported_capabilities: Some(vec!["prompt-cache".to_string()]),
                },
                crate::app_store::ClaudeModelMapping {
                    family: "sonnet".to_string(),
                    display_name: "Sonnet".to_string(),
                    upstream_model: "claude-sonnet-4-5".to_string(),
                    supports_1m: Some(true),
                    supported_capabilities: Some(vec!["image".to_string(), "pdfs".to_string()]),
                },
            ],
            claude_enable_tool_search: Some(false),
            claude_auto_memory_enabled: None,
            claude_always_thinking_enabled: None,
            claude_away_summary_enabled: None,
            claude_include_git_instructions: None,
            claude_enable_attribution: Some(false),
            code: Some("test-claude".to_string()),
            is_enabled: Some(true),
            provider_key: None,
            favorite_at: None,
            env_managed: Some(true),
            tool_config: serde_json::from_str(
                r#"{
                    "claude_default_model": "claude-sonnet-4-5[1m]",
                    "claude_reasoning_effort": "auto"
                }"#,
            )
            .unwrap(),
            history: vec![],
            extra: Map::new(),
            fetched_models: None,
        };

        materialize_claude_settings_sp(&provider, &dir).unwrap();

        let content = fs::read_to_string(dir.join("settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed["model"],
            Value::String("claude-sonnet-4-5[1m]".to_string())
        );
        let env = parsed["env"].as_object().unwrap();
        assert_eq!(
            env["ANTHROPIC_MODEL"],
            Value::String("claude-sonnet-4-5[1m]".to_string())
        );
        assert_eq!(
            env["CLAUDE_CODE_EFFORT_LEVEL"],
            Value::String("auto".to_string())
        );
        assert_eq!(
            env["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            Value::String("claude-sonnet-4-5[1m]".to_string())
        );
        assert_eq!(
            env["ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"],
            Value::String("Sonnet".to_string())
        );
        assert_eq!(
            env["ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES"],
            Value::String("image,pdfs".to_string())
        );
        assert_eq!(
            env["ANTHROPIC_DEFAULT_HAIKU_MODEL_SUPPORTED_CAPABILITIES"],
            Value::String("prompt-cache".to_string())
        );
    }

    #[test]
    fn test_materialize_claude_settings_sp_removes_model_fields_when_default_model_missing() {
        let dir = temp_dir();
        fs::write(
            dir.join("settings.json"),
            serde_json::to_string_pretty(&json!({
                "model": "old-model",
                "env": {
                    "ANTHROPIC_MODEL": "old-model",
                    "ANTHROPIC_API_KEY": "old-key"
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let provider = crate::app_store::ServiceProviderRecord {
            id: "test-claude".to_string(),
            name: "Test Claude".to_string(),
            tool: "claude".to_string(),
            icon: None,
            api_key: "sk-ant-test123".to_string(),
            base_url: Some("https://example.com".to_string()),
            model: None,
            claude_api_format: "anthropic_messages".to_string(),
            claude_connection_mode: "native_anthropic".to_string(),
            protocol_router_upstream_provider_id: None,
            protocol_router_wire_api: "open_ai_chat".to_string(),
            claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
            claude_model_mappings: vec![],
            claude_enable_tool_search: Some(false),
            claude_auto_memory_enabled: None,
            claude_always_thinking_enabled: None,
            claude_away_summary_enabled: None,
            claude_include_git_instructions: None,
            claude_enable_attribution: Some(false),
            code: Some("test-claude".to_string()),
            is_enabled: Some(true),
            provider_key: None,
            favorite_at: None,
            env_managed: Some(true),
            tool_config: Map::new(),
            history: vec![],
            extra: Map::new(),
            fetched_models: None,
        };

        materialize_claude_settings_sp(&provider, &dir).unwrap();

        let content = fs::read_to_string(dir.join("settings.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed.get("model").is_none());
        assert!(parsed["env"]
            .as_object()
            .expect("env")
            .get("ANTHROPIC_MODEL")
            .is_none());
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
    fn test_list_claude_profiles_exposes_router_fields() {
        let mut state = ProvidersState::default();
        let mut provider = make_provider("router-claude", "Router Claude", "sk-router");
        provider.tool_config.insert(
            "claude_api_format".to_string(),
            Value::String("open_ai_responses".to_string()),
        );
        provider.tool_config.insert(
            "claude_connection_mode".to_string(),
            Value::String("protocol_router".to_string()),
        );
        state.providers.push(provider);

        let profiles = list_claude_profiles(&state);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].claude_api_format, "open_ai_responses");
        assert_eq!(profiles[0].claude_connection_mode, "protocol_router");
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
