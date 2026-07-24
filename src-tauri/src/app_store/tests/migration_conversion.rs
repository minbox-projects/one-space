use super::*;
use serde_json::json;

#[test]
fn launch_claude_config_dir_without_provider_id() {
    with_temp_dir("launch-claude-config-dir-without-provider", |_| {
        let record = session_record("s1", "claude", "/tmp", 100, "active");
        let options =
            tauri::async_runtime::block_on(super::launch_options_for_session_async(&record))
                .unwrap();
        assert!(options.env.is_none());
    });
}

#[test]
fn launch_claude_config_dir_non_claude_tool() {
    with_temp_dir("launch-claude-config-dir-non-claude", |_| {
        let mut record = session_record("s1", "codex", "/tmp", 100, "active");
        record.provider_id = Some("work-claude".to_string());
        let options =
            tauri::async_runtime::block_on(super::launch_options_for_session_async(&record))
                .unwrap();
        assert!(options.env.is_none());
    });
}

#[test]
fn migrate_providers_to_service_providers_basic() {
    let mut old_tool_config = Map::new();
    old_tool_config.insert(
        "claude_haiku_model".to_string(),
        Value::String("claude-haiku-latest".to_string()),
    );
    old_tool_config.insert(
        "claude_sonnet_model".to_string(),
        Value::String("claude-sonnet-latest".to_string()),
    );
    old_tool_config.insert(
        "claude_opus_model".to_string(),
        Value::String("claude-opus-latest".to_string()),
    );
    old_tool_config.insert(
        "icon".to_string(),
        Value::String("builtin:bailian".to_string()),
    );

    let mut active = HashMap::new();
    active.insert("claude".to_string(), "my-claude-id".to_string());

    let old = ProvidersState {
        active,
        providers: vec![ProviderRecord {
            core: ProviderCore {
                id: "my-claude-id".to_string(),
                name: "My Claude".to_string(),
                tool: "claude".to_string(),
                api_key: "sk-test".to_string(),
                base_url: Some("https://api.anthropic.com".to_string()),
                model: Some("claude-sonnet-latest".to_string()),
                code: None,
            },
            runtime_policy: Default::default(),
            favorite_at: None,
            tool_config: old_tool_config,
            history: vec![],
            extra: Map::new(),
            is_enabled: Some(true),
            provider_key: None,
        }],
    };

    let new = super::migrate_providers_to_service_providers(old);
    assert_eq!(new.providers.len(), 1);
    let sp = &new.providers[0];
    assert_eq!(sp.id, "my-claude-id");
    assert_eq!(sp.name, "My Claude");
    assert_eq!(sp.tool, "claude");
    assert_eq!(sp.icon.as_deref(), Some("builtin:bailian"));
    assert_eq!(sp.claude_api_format, "anthropic_messages");
    assert_eq!(sp.claude_auth_env_key, "ANTHROPIC_API_KEY"); // non-empty api_key → ANTHROPIC_API_KEY
    assert_eq!(sp.claude_model_mappings.len(), 3);
    assert_eq!(sp.claude_model_mappings[0].family, "haiku");
    assert_eq!(
        sp.claude_model_mappings[0].upstream_model,
        "claude-haiku-latest"
    );
    assert_eq!(
        sp.claude_model_mappings[1].upstream_model,
        "claude-sonnet-latest"
    );
    assert_eq!(
        sp.claude_model_mappings[2].upstream_model,
        "claude-opus-latest"
    );
    assert_eq!(new.active.get("claude"), Some(&"my-claude-id".to_string()));
    assert!(new.active_opencode.is_empty());
    assert!(sp.tool_config.get("claude_model_mappings").is_some());
    assert!(sp.tool_config.get("claude_haiku_model").is_none());
}

#[test]
fn migrate_providers_non_claude_tool() {
    let old = ProvidersState {
        active: HashMap::new(),
        providers: vec![ProviderRecord {
            core: ProviderCore {
                id: "codex-1".to_string(),
                name: "My Codex".to_string(),
                tool: "codex".to_string(),
                api_key: "sk-codex".to_string(),
                base_url: None,
                model: Some("o3".to_string()),
                code: None,
            },
            runtime_policy: Default::default(),
            favorite_at: None,
            tool_config: Map::new(),
            history: vec![],
            extra: Map::new(),
            is_enabled: None,
            provider_key: None,
        }],
    };

    let new = super::migrate_providers_to_service_providers(old);
    assert_eq!(new.providers.len(), 1);
    let sp = &new.providers[0];
    assert_eq!(sp.tool, "codex");
    assert!(sp.claude_model_mappings.is_empty());
    assert_eq!(sp.claude_auth_env_key, "ANTHROPIC_API_KEY"); // default
}

#[test]
fn migrate_legacy_claude_router_provider_preserves_openai_responses_format() {
    let mut tool_config = Map::new();
    tool_config.insert(
        "claude_connection_mode".to_string(),
        Value::String("protocol_router".to_string()),
    );
    tool_config.insert(
        "wire_api".to_string(),
        Value::String("responses".to_string()),
    );

    let old = ProvidersState {
        active: HashMap::new(),
        providers: vec![ProviderRecord {
            core: ProviderCore {
                id: "router-claude".to_string(),
                name: "Router Claude".to_string(),
                tool: "claude".to_string(),
                api_key: "sk-test".to_string(),
                code: Some("opencode-go".to_string()),
                base_url: Some("https://example.com/v1".to_string()),
                model: Some("claude-sonnet-4".to_string()),
            },
            runtime_policy: ProviderRuntimePolicy::default(),
            favorite_at: None,
            tool_config,
            history: vec![],
            extra: Map::new(),
            is_enabled: Some(true),
            provider_key: None,
        }],
    };

    let migrated = migrate_providers_to_service_providers(old);
    let sp = &migrated.providers[0];
    assert_eq!(sp.claude_api_format, "open_ai_responses");
    assert_eq!(sp.claude_connection_mode, "protocol_router");
    assert_eq!(sp.protocol_router_wire_api, "open_ai_responses");
}

#[test]
fn service_provider_from_value_infers_openai_responses_from_router_fields() {
    let value = json!({
        "id": "router-claude",
        "name": "Router Claude",
        "tool": "claude",
        "api_key": "sk-test",
        "base_url": "https://example.com/v1",
        "claude_connection_mode": "protocol_router",
        "protocol_router_wire_api": "open_ai_responses",
        "tool_config": {
            "wire_api": "responses"
        }
    });

    let record = service_provider_from_value(value, None);
    assert_eq!(record.claude_api_format, "open_ai_responses");
    assert_eq!(record.claude_connection_mode, "protocol_router");
    assert_eq!(record.protocol_router_wire_api, "open_ai_responses");
}

#[test]
fn service_provider_from_value_prefers_top_level_claude_defaults_over_stale_tool_config() {
    let value = json!({
        "id": "work-alicode-plan",
        "name": "Work Alicode Plan",
        "tool": "claude",
        "api_key": "sk-test",
        "claude_default_model": "qwen3.7-plus",
        "claude_reasoning_effort": "xhigh",
        "tool_config": {
            "claude_default_model": "qwen3.6-plus",
            "claude_reasoning_effort": "high"
        }
    });

    let record = service_provider_from_value(value, None);
    assert_eq!(
        record
            .tool_config
            .get("claude_default_model")
            .and_then(|v| v.as_str()),
        Some("qwen3.7-plus")
    );
    assert_eq!(
        record
            .tool_config
            .get("claude_reasoning_effort")
            .and_then(|v| v.as_str()),
        Some("xhigh")
    );
    assert_eq!(record.model.as_deref(), Some("qwen3.7-plus"));
}

#[test]
fn service_provider_from_value_clears_claude_model_when_default_is_empty() {
    let value = json!({
        "id": "work-empty-model",
        "name": "Work Empty Model",
        "tool": "claude",
        "api_key": "sk-test",
        "model": "legacy-model",
        "claude_default_model": "   ",
        "tool_config": {
            "claude_default_model": "legacy-model"
        }
    });

    let record = service_provider_from_value(value, None);
    assert_eq!(record.model, None);
    assert!(record.tool_config.get("claude_default_model").is_none());
}
