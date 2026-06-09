use super::*;
use serde_json::json;

#[test]
fn launch_claude_config_dir_with_provider_id() {
    with_temp_dir("launch-claude-config-dir-with-provider", |_| {
        let provider_id = generate_provider_uuid();
        let mut state = load_service_providers_state().unwrap();
        state.providers.push(ServiceProviderRecord {
            id: provider_id.clone(),
            name: "Work Claude".to_string(),
            tool: "claude".to_string(),
            icon: None,
            api_key: "sk-test".to_string(),
            base_url: None,
            model: None,
            claude_api_format: "anthropic_messages".to_string(),
            claude_connection_mode: "native_anthropic".to_string(),
            protocol_router_upstream_provider_id: None,
            protocol_router_wire_api: "open_ai_chat".to_string(),
            claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
            claude_model_mappings: vec![],
            claude_enable_tool_search: None,
            claude_auto_memory_enabled: None,
            claude_always_thinking_enabled: None,
            claude_away_summary_enabled: None,
            claude_include_git_instructions: None,
            claude_enable_attribution: None,
            code: None,
            is_enabled: Some(true),
            provider_key: None,
            env_managed: Some(true),
            favorite_at: None,
            tool_config: Map::new(),
            history: vec![],
            extra: Map::new(),
            fetched_models: None,
        });
        save_service_providers_internal(&state).unwrap();

        let mut record = session_record("s1", "claude", "/tmp", 100, "active");
        record.provider_id = Some(provider_id.clone());
        let options =
            tauri::async_runtime::block_on(super::launch_options_for_session_async(&record))
                .unwrap();
        let env = options
            .env
            .expect("Claude with provider_id should have env");
        let dir = env
            .get("CLAUDE_CONFIG_DIR")
            .expect("Should have CLAUDE_CONFIG_DIR");
        assert!(dir.contains("claude_profiles"));
        assert!(dir.contains(&provider_id));
        assert!(Path::new(dir).join("settings.json").exists());
    });
}

#[test]
fn service_providers_upsert_materializes_claude_isolated_profile_without_touching_global() {
    with_temp_dir("service-providers-upsert-materializes-claude", |home| {
        let global_dir = home.join(".claude");
        fs::create_dir_all(&global_dir).expect("create global claude dir");
        write_test_file(
            &global_dir.join("settings.json"),
            r#"{"theme":"global-dark"}"#,
        );

        let app_dir = home.join(".config").join("onespace");
        fs::create_dir_all(&app_dir).expect("create onespace app dir");
        write_test_file(
            &app_dir.join("protocol_router.json"),
            r#"{"enabled":true,"port":18080,"token":"osp_test","retention_days":30}"#,
        );
        write_test_file(
            &app_dir
                .join("claude_profiles")
                .join("work")
                .join("settings.json"),
            r#"{"theme":"dark"}"#,
        );

        let response = tauri::async_runtime::block_on(service_providers_upsert_inner(json!({
            "id": "claude-router",
            "name": "Claude Router",
            "tool": "claude",
            "code": "work",
            "api_key": "sk-upstream",
            "base_url": "https://upstream.example.com/v1",
            "claude_api_format": "open_ai_responses",
            "claude_connection_mode": "protocol_router"
        })))
        .expect("upsert claude provider");

        let provider_id = response
            .data
            .get("id")
            .and_then(|v| v.as_str())
            .expect("provider id");
        assert!(is_uuid_v4(provider_id));

        let profile_settings_path = home
            .join(".config")
            .join("onespace")
            .join("claude_profiles")
            .join("work")
            .join("settings.json");
        assert!(profile_settings_path.exists());
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&profile_settings_path).unwrap())
                .expect("parse profile settings");
        assert_eq!(settings["theme"], Value::String("dark".to_string()));
        let env = settings["env"].as_object().expect("profile env");
        assert!(env.get("ANTHROPIC_API_KEY").is_some());
        let expected_router_url =
            format!("http://127.0.0.1:18080/anthropic/service-provider-{provider_id}/v1");
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
            Some(expected_router_url.as_str())
        );

        let global_settings =
            fs::read_to_string(global_dir.join("settings.json")).expect("read global settings");
        assert_eq!(global_settings, r#"{"theme":"global-dark"}"#);
    });
}

#[test]
fn service_providers_upsert_materializes_native_claude_profile() {
    with_temp_dir(
        "service-providers-upsert-materializes-native-claude",
        |home| {
            let response = tauri::async_runtime::block_on(service_providers_upsert_inner(json!({
                "id": "claude-native",
                "name": "Claude Native",
                "tool": "claude",
                "api_key": "auth-token",
                "base_url": "https://anthropic.example.com",
                "claude_auth_env_key": "ANTHROPIC_AUTH_TOKEN"
            })))
            .expect("upsert native claude provider");

            let provider_id = response
                .data
                .get("id")
                .and_then(|v| v.as_str())
                .expect("provider id");
            assert!(is_uuid_v4(provider_id));

            let profile_settings_path = home
                .join(".config")
                .join("onespace")
                .join("claude_profiles")
                .join(provider_id)
                .join("settings.json");
            assert!(profile_settings_path.exists());
            let settings: Value =
                serde_json::from_str(&fs::read_to_string(&profile_settings_path).unwrap())
                    .expect("parse profile settings");
            let env = settings["env"].as_object().expect("profile env");
            assert_eq!(
                env.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()),
                Some("auth-token")
            );
            assert!(env.get("ANTHROPIC_API_KEY").is_none());
            assert_eq!(
                env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
                Some("https://anthropic.example.com")
            );
        },
    );
}

#[test]
fn resolve_claude_config_dir_for_provider_id_self_heals_dirty_profile() {
    with_temp_dir("resolve-claude-config-dir-self-heals", |home| {
        let mut state = load_service_providers_state().unwrap();
        state.providers.push(ServiceProviderRecord {
            id: "11111111-1111-4111-8111-111111111111".to_string(),
            name: "Dirty Claude".to_string(),
            tool: "claude".to_string(),
            icon: None,
            api_key: "sk-test".to_string(),
            base_url: Some("https://anthropic.example.com".to_string()),
            model: None,
            claude_api_format: "anthropic_messages".to_string(),
            claude_connection_mode: "native_anthropic".to_string(),
            protocol_router_upstream_provider_id: None,
            protocol_router_wire_api: "open_ai_chat".to_string(),
            claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
            claude_model_mappings: vec![],
            claude_enable_tool_search: None,
            claude_auto_memory_enabled: None,
            claude_always_thinking_enabled: None,
            claude_away_summary_enabled: None,
            claude_include_git_instructions: None,
            claude_enable_attribution: None,
            code: Some("dirty".to_string()),
            is_enabled: Some(true),
            provider_key: None,
            env_managed: Some(true),
            favorite_at: None,
            tool_config: Map::new(),
            history: vec![],
            extra: Map::new(),
            fetched_models: None,
        });
        save_service_providers_internal(&state).unwrap();

        let dirty_path = home
            .join(".config")
            .join("onespace")
            .join("claude_profiles")
            .join("dirty")
            .join("settings.json");
        write_test_file(&dirty_path, r#"{"theme":"dark"}"#);

        let dir = resolve_claude_config_dir_for_provider_id("11111111-1111-4111-8111-111111111111")
            .expect("resolve config dir");
        assert_eq!(dir, dirty_path.parent().unwrap());

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&dirty_path).unwrap()).unwrap();
        assert_eq!(settings["theme"], Value::String("dark".to_string()));
        let env = settings["env"].as_object().expect("env");
        assert_eq!(
            env.get("ANTHROPIC_API_KEY").and_then(|v| v.as_str()),
            Some("sk-test")
        );
    });
}

#[test]
fn service_providers_upsert_non_claude_does_not_create_claude_profile() {
    with_temp_dir("service-providers-upsert-non-claude", |home| {
        tauri::async_runtime::block_on(service_providers_upsert_inner(json!({
            "id": "codex-1",
            "name": "Codex One",
            "tool": "codex",
            "api_key": "sk-codex"
        })))
        .expect("upsert non-claude provider");

        let profile_root = home
            .join(".config")
            .join("onespace")
            .join("claude_profiles");
        assert!(!profile_root.exists());
    });
}
