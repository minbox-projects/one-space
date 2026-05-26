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
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let s = s.to_lowercase();
    let s: String = s
        .chars()
        .fold(String::new(), |mut acc, c| {
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

pub(crate) fn materialize_claude_settings(
    provider: &ProviderRecord,
    profile_dir: &Path,
) -> Result<(), String> {
    fs::create_dir_all(profile_dir).map_err(|e| format!("Failed to create profile dir: {e}"))?;

    let mut settings = read_claude_settings(profile_dir)?;

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

    for (src, dst) in [
        ("claude_default_model", "ANTHROPIC_MODEL"),
        ("claude_reasoning_model", "ANTHROPIC_REASONING_MODEL"),
        ("claude_haiku_model", "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
        ("claude_sonnet_model", "ANTHROPIC_DEFAULT_SONNET_MODEL"),
        ("claude_opus_model", "ANTHROPIC_DEFAULT_OPUS_MODEL"),
        ("claude_reasoning_effort", "CLAUDE_CODE_EFFORT_LEVEL"),
    ] {
        if let Some(v) = provider.tool_config.get(src).and_then(|v| v.as_str()) {
            if !v.is_empty() {
                env.insert(dst.to_string(), Value::String(v.to_string()));
            }
        } else {
            env.remove(dst);
        }
    }

    settings.insert("env".to_string(), Value::Object(env));

    let content =
        serde_json::to_string_pretty(&Value::Object(settings)).map_err(|e| e.to_string())?;

    fs::write(profile_dir.join("settings.json"), &content)
        .map_err(|e| format!("Failed to write settings.json: {e}"))?;

    Ok(())
}

pub(crate) fn read_claude_settings(
    profile_dir: &Path,
) -> Result<Map<String, Value>, String> {
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
    pub config_dir: String,
    pub is_default: bool,
    pub auth_type: String,
    pub model: Option<String>,
    pub tool_config: Map<String, Value>,
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
                && (p.core.id == query || p.core.name == query)
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
    state.active.insert("claude".to_string(), profile_id.to_string());
    Ok(())
}

pub(crate) fn list_claude_profiles(state: &ProvidersState) -> Vec<ClaudeProfileSummary> {
    let default_id = state.active.get("claude").cloned();
    state
        .providers
        .iter()
        .filter(|p| p.core.tool == "claude")
        .map(|p| {
            let config_dir = claude_profile_dir(&p.core.id)
                .map(|d| d.to_string_lossy().to_string())
                .unwrap_or_default();
            let auth_type = if p.core.api_key.is_empty() {
                "oauth"
            } else {
                "api_key"
            };
            ClaudeProfileSummary {
                id: p.core.id.clone(),
                name: p.core.name.clone(),
                config_dir,
                is_default: default_id.as_deref() == Some(&p.core.id),
                auth_type: auth_type.to_string(),
                model: p.core.model.clone(),
                tool_config: p.tool_config.clone(),
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::fs;

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
        provider.tool_config = serde_json::from_str(r#"{"dangerously_skip_permissions": true}"#).unwrap();

        materialize_claude_settings(&provider, &dir).unwrap();

        let settings_path = dir.join("settings.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let obj = parsed.as_object().unwrap();

        assert_eq!(obj["dangerouslySkipPermissions"], Value::Bool(true));
        assert!(obj["env"].is_object());
        let env = obj["env"].as_object().unwrap();
        assert_eq!(env["ANTHROPIC_API_KEY"], Value::String("sk-ant-test123".to_string()));
        assert_eq!(env["ANTHROPIC_BASE_URL"], Value::String("https://example.com".to_string()));
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
        fs::write(&settings_path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        let provider = make_provider("test-claude", "Test Claude", "sk-ant-new-key");

        materialize_claude_settings(&provider, &dir).unwrap();

        let content = fs::read_to_string(&settings_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let obj = parsed.as_object().unwrap();

        assert_eq!(obj["oauthToken"], Value::String("existing-oauth-token".to_string()));
        assert_eq!(obj["someHistory"], json!(["entry1"]));

        let env = obj["env"].as_object().unwrap();
        assert_eq!(
            env["ANTHROPIC_API_KEY"],
            Value::String("sk-ant-new-key".to_string())
        );
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
        state.providers.push(make_provider("work-claude", "Work Claude", "sk-1"));
        state.providers.push(make_provider("personal-claude", "Personal Claude", "sk-2"));

        let found = resolve_claude_profile(&state, "work-claude").unwrap();
        assert_eq!(found.core.id, "work-claude");
        assert_eq!(found.core.name, "Work Claude");
    }

    #[test]
    fn test_resolve_claude_profile_by_name() {
        let mut state = ProvidersState::default();
        state.providers.push(make_provider("work-claude", "Work Claude", "sk-1"));

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
        state.providers.push(make_provider("work-claude", "Work Claude", "sk-1"));

        set_default_claude_profile(&mut state, "work-claude").unwrap();
        assert_eq!(state.active.get("claude").map(|s| s.as_str()), Some("work-claude"));
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
        state.providers.push(make_provider("work-claude", "Work Claude", "sk-1"));
        state.providers.push(make_provider("personal-claude", "Personal Claude", "sk-2"));

        let profiles = list_claude_profiles(&state);
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].id, "work-claude");
        assert_eq!(profiles[0].auth_type, "api_key");
        assert!(!profiles[0].is_default);
    }

    #[test]
    fn test_list_claude_profiles_with_default() {
        let mut state = ProvidersState::default();
        state.providers.push(make_provider("work-claude", "Work Claude", "sk-1"));
        state.active.insert("claude".to_string(), "work-claude".to_string());

        let profiles = list_claude_profiles(&state);
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].is_default);
    }

    #[test]
    fn test_list_claude_profiles_filters_non_claude() {
        let mut state = ProvidersState::default();
        state.providers.push(make_provider("work-claude", "Work Claude", "sk-1"));
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
}
