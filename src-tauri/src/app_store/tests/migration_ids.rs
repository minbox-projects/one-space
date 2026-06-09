use super::*;
use serde_json::json;

#[test]
fn auto_import_system_provider_merges_without_reducing_existing_service_providers() {
    let existing_gemini_id = "11111111-1111-4111-8111-111111111111".to_string();
    let existing_claude_id = "22222222-2222-4222-8222-222222222222".to_string();
    let mut state = ServiceProvidersState {
        active: HashMap::from([("gemini".to_string(), existing_gemini_id.clone())]),
        providers: vec![
            ServiceProviderRecord {
                id: existing_gemini_id.clone(),
                name: "Existing Gemini".to_string(),
                tool: "gemini".to_string(),
                api_key: "gemini-key".to_string(),
                code: Some("work-gemini".to_string()),
                ..ServiceProviderRecord::default()
            },
            ServiceProviderRecord {
                id: existing_claude_id,
                name: "Existing Claude".to_string(),
                tool: "claude".to_string(),
                api_key: "claude-key".to_string(),
                code: Some("work-claude".to_string()),
                ..ServiceProviderRecord::default()
            },
        ],
    };
    let mut system_provider = ProviderRecord::default();
    system_provider.core.id = "default-gemini".to_string();
    system_provider.core.name = "Imported Gemini Config".to_string();
    system_provider.core.tool = "gemini".to_string();
    system_provider.core.code = Some("default-gemini".to_string());
    system_provider.core.api_key = "system-gemini-key".to_string();
    system_provider.core.base_url = Some("https://gemini.example.com".to_string());

    let outcome =
        auto_import_system_provider_into_service_state(&mut state, "gemini", system_provider)
            .expect("auto import");

    assert!(outcome.imported);
    assert_eq!(state.providers.len(), 3);
    assert_eq!(
        state.active.get("gemini").map(String::as_str),
        Some(existing_gemini_id.as_str())
    );
    assert!(state
        .providers
        .iter()
        .all(|provider| is_uuid_v4(&provider.id)));
    assert_eq!(
        state
            .providers
            .iter()
            .filter(|provider| provider.tool == "gemini")
            .count(),
        2
    );
    assert!(state.providers.iter().any(|provider| {
        provider.tool == "gemini"
            && provider.code.as_deref() == Some("default-gemini")
            && provider.env_managed == Some(true)
    }));
}

#[test]
fn auto_import_system_provider_skips_existing_default_code_without_active_requirement() {
    let existing_id = "11111111-1111-4111-8111-111111111111".to_string();
    let mut state = ServiceProvidersState {
        active: HashMap::new(),
        providers: vec![ServiceProviderRecord {
            id: existing_id.clone(),
            name: "Default Gemini".to_string(),
            tool: "gemini".to_string(),
            api_key: "gemini-key".to_string(),
            code: Some("default-gemini".to_string()),
            ..ServiceProviderRecord::default()
        }],
    };
    let mut system_provider = ProviderRecord::default();
    system_provider.core.id = "default-gemini".to_string();
    system_provider.core.name = "Imported Gemini Config".to_string();
    system_provider.core.tool = "gemini".to_string();
    system_provider.core.code = Some("default-gemini".to_string());

    let outcome =
        auto_import_system_provider_into_service_state(&mut state, "gemini", system_provider)
            .expect("auto import");

    assert!(!outcome.imported);
    assert_eq!(outcome.reason, Some("provider_exists"));
    assert_eq!(state.providers.len(), 1);
    assert_eq!(state.providers[0].id, existing_id);
    assert!(state.active.is_empty());
}

#[test]
fn run_migration_impl_does_not_rebuild_providers_when_service_state_exists() {
    with_temp_dir("migration-keeps-existing-service-providers", |home| {
        let service_state = ServiceProvidersState {
            active: HashMap::from([("claude".to_string(), "legacy-claude".to_string())]),
            providers: vec![
                ServiceProviderRecord {
                    id: "legacy-claude".to_string(),
                    name: "Claude".to_string(),
                    tool: "claude".to_string(),
                    api_key: "claude-key".to_string(),
                    code: Some("work-claude".to_string()),
                    ..ServiceProviderRecord::default()
                },
                ServiceProviderRecord {
                    id: "22222222-2222-4222-8222-222222222222".to_string(),
                    name: "Gemini".to_string(),
                    tool: "gemini".to_string(),
                    api_key: "gemini-key".to_string(),
                    code: Some("work-gemini".to_string()),
                    ..ServiceProviderRecord::default()
                },
            ],
        };
        save_service_providers_internal(&service_state).expect("save service providers");

        let legacy_ai_providers = json!({
            "active_gemini": "default-gemini",
            "providers": [{
                "id": "default-gemini",
                "name": "Imported Gemini Config",
                "tool": "gemini",
                "api_key": "",
                "base_url": "https://system.example.com"
            }],
            "is_encrypted": false
        });
        write_test_file(
            &home
                .join(".config")
                .join("onespace")
                .join("local_data")
                .join("ai_providers.json"),
            &serde_json::to_string(&legacy_ai_providers).unwrap(),
        );
        let legacy_mcp = mcp_servers::MCPServersState {
            servers: vec![mcp_servers::MCPServer {
                id: "mcp-1".to_string(),
                name: "MCP".to_string(),
                config_key: None,
                description: None,
                transport: mcp_servers::MCPServerTransport::Stdio,
                command: Some("echo".to_string()),
                args: None,
                cwd: None,
                url: None,
                http_url: None,
                env: None,
                headers: None,
                timeout: None,
                trust: None,
                linked_provider_ids: vec!["legacy-claude".to_string()],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }],
            is_encrypted: false,
        };
        write_test_file(
            &home
                .join(".config")
                .join("onespace")
                .join("local_data")
                .join("mcp_servers.json"),
            &serde_json::to_string(&legacy_mcp).unwrap(),
        );

        run_migration_impl().expect("migration");
        let loaded = load_service_providers_state().expect("load service providers");

        assert_eq!(loaded.providers.len(), 2);
        assert!(loaded
            .providers
            .iter()
            .any(|provider| provider.name == "Claude" && provider.api_key == "claude-key"));
        let claude_id = loaded
            .providers
            .iter()
            .find(|provider| provider.name == "Claude")
            .map(|provider| provider.id.clone())
            .expect("claude provider");
        assert!(is_uuid_v4(&claude_id));
        assert_ne!(claude_id, "legacy-claude");
        assert_eq!(
            loaded.active.get("claude").map(String::as_str),
            Some(claude_id.as_str())
        );

        let legacy_snapshot = load_legacy_providers_state_raw().expect("load legacy snapshot");
        assert_eq!(legacy_snapshot.providers.len(), 2);
        assert!(legacy_snapshot
            .providers
            .iter()
            .all(|provider| is_uuid_v4(&provider.core.id)));
        assert_eq!(
            legacy_snapshot.active.get("claude").map(String::as_str),
            Some(claude_id.as_str())
        );

        let mcp_after: mcp_servers::MCPServersState =
            StorageEngine::read_json(&StorageEngine::mcp_path().unwrap()).unwrap();
        assert_eq!(
            mcp_after.servers[0].linked_provider_ids,
            vec![claude_id.clone()]
        );
    });
}

#[test]
fn migrated_service_providers_missing_does_not_rebuild_from_legacy_snapshot() {
    with_temp_dir("missing-service-state-does-not-use-legacy-snapshot", |_| {
        let service_state = ServiceProvidersState {
            active: HashMap::from([(
                "claude".to_string(),
                "11111111-1111-4111-8111-111111111111".to_string(),
            )]),
            providers: vec![
                ServiceProviderRecord {
                    id: "11111111-1111-4111-8111-111111111111".to_string(),
                    name: "Work Claude".to_string(),
                    tool: "claude".to_string(),
                    code: Some("work-claude".to_string()),
                    ..ServiceProviderRecord::default()
                },
                ServiceProviderRecord {
                    id: "22222222-2222-4222-8222-222222222222".to_string(),
                    name: "Work Codex".to_string(),
                    tool: "codex".to_string(),
                    ..ServiceProviderRecord::default()
                },
            ],
        };
        save_service_providers_internal(&service_state).expect("save service providers");

        let legacy_snapshot = ProvidersState {
            active: HashMap::new(),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: "33333333-3333-4333-8333-333333333333".to_string(),
                    name: "Imported Gemini Config".to_string(),
                    tool: "gemini".to_string(),
                    code: Some("default-gemini".to_string()),
                    ..ProviderCore::default()
                },
                ..ProviderRecord::default()
            }],
        };
        let legacy_blob =
            CryptoService::encrypt_json(&serde_json::to_value(&legacy_snapshot).unwrap()).unwrap();
        StorageEngine::write_json(&StorageEngine::providers_path().unwrap(), &legacy_blob)
            .expect("write sparse legacy snapshot");

        let migrated = MigrationState {
            migrated: true,
            schema_version: SCHEMA_VERSION,
            ..MigrationState::default()
        };
        save_migration_state(&migrated).expect("save migration state");
        fs::remove_file(StorageEngine::service_providers_path().unwrap())
            .expect("remove service provider state");

        let err = load_service_providers_state().expect_err("missing service state should fail");
        assert!(err.contains("service_providers state missing after migration"));

        let legacy_after = load_legacy_providers_state_raw().expect("load legacy snapshot");
        assert_eq!(legacy_after.providers.len(), 1);
        assert_eq!(legacy_after.providers[0].core.tool, "gemini");
        assert!(!StorageEngine::service_providers_path().unwrap().exists());
    });
}

#[test]
fn normalize_service_provider_ids_rewrites_legacy_ids_and_references() {
    let mut state = ServiceProvidersState {
        active: HashMap::from([
            ("claude".to_string(), "custom-claude".to_string()),
            ("codex".to_string(), "default-codex".to_string()),
        ]),
        providers: vec![
            ServiceProviderRecord {
                id: "custom-claude".to_string(),
                name: "Claude".to_string(),
                tool: "claude".to_string(),
                icon: None,
                api_key: "sk-test".to_string(),
                base_url: Some("https://example.com/v1".to_string()),
                model: Some("qwen".to_string()),
                claude_api_format: "open_ai_chat".to_string(),
                claude_connection_mode: "protocol_router".to_string(),
                protocol_router_upstream_provider_id: Some("default-codex".to_string()),
                protocol_router_wire_api: "open_ai_chat".to_string(),
                claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
                claude_model_mappings: vec![],
                claude_enable_tool_search: None,
                claude_auto_memory_enabled: None,
                claude_always_thinking_enabled: None,
                claude_away_summary_enabled: None,
                claude_include_git_instructions: None,
                claude_enable_attribution: None,
                code: Some("ali-code-plan-openai".to_string()),
                is_enabled: Some(true),
                provider_key: None,
                env_managed: Some(true),
                favorite_at: None,
                tool_config: Map::new(),
                history: vec![],
                extra: Map::new(),
                fetched_models: None,
            },
            ServiceProviderRecord {
                id: "default-codex".to_string(),
                name: "Codex".to_string(),
                tool: "codex".to_string(),
                api_key: "sk-codex".to_string(),
                ..ServiceProviderRecord::default()
            },
        ],
    };

    let (id_map, changed) = normalize_service_provider_ids(&mut state);

    assert!(changed);
    assert_eq!(id_map.len(), 2);
    assert!(state
        .providers
        .iter()
        .all(|provider| is_uuid_v4(&provider.id)));
    assert_eq!(state.active.get("claude"), id_map.get("custom-claude"));
    assert_eq!(
        state.providers[0]
            .protocol_router_upstream_provider_id
            .as_ref(),
        id_map.get("default-codex")
    );

    let before = serde_json::to_value(&state).unwrap();
    let (second_map, second_changed) = normalize_service_provider_ids(&mut state);
    assert!(!second_changed);
    assert!(second_map.is_empty());
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn apply_provider_id_map_rewrites_dependent_state_files_and_profile_dirs() {
    with_temp_dir("provider-id-remap-dependent-state", |_| {
        let app_dir = config::get_app_dir().expect("app dir");
        let data_dir = crate::get_data_dir().expect("data dir");
        let old_id = "legacy-claude";
        let new_id = "11111111-1111-4111-8111-111111111111";
        let id_map = HashMap::from([(old_id.to_string(), new_id.to_string())]);

        let sessions_path = StorageEngine::sessions_path().expect("sessions path");
        let sessions = SessionsState {
            sessions: vec![SessionRecord {
                id: "session-1".to_string(),
                name: "Session".to_string(),
                working_dir: "/tmp/project".to_string(),
                tool: "claude".to_string(),
                tool_session_id: "claude-session".to_string(),
                model_name: None,
                name_source: "manual".to_string(),
                runtime_mode: "shared".to_string(),
                runtime_profile_id: None,
                preset_id: None,
                created_at: 1,
                last_used_at: 2,
                status: "active".to_string(),
                favorited_at: None,
                provider_id: Some(old_id.to_string()),
            }],
            ..SessionsState::default()
        };
        let sessions_blob =
            CryptoService::encrypt_json(&serde_json::to_value(&sessions).unwrap()).unwrap();
        StorageEngine::write_json(&sessions_path, &sessions_blob).unwrap();

        write_test_file(
            &data_dir.join("workflow_presets.json"),
            &json!([
                { "id": "preset-1", "provider_id": old_id, "active_provider_id": old_id }
            ])
            .to_string(),
        );
        write_test_file(
            &data_dir.join("workflow_runs.json"),
            &json!([
                { "id": "run-1", "preset_id": "preset-1", "provider_id": old_id }
            ])
            .to_string(),
        );

        let mcp = mcp_servers::MCPServersState {
            servers: vec![mcp_servers::MCPServer {
                id: "mcp-1".to_string(),
                name: "MCP".to_string(),
                config_key: None,
                description: None,
                transport: mcp_servers::MCPServerTransport::Stdio,
                command: Some("echo".to_string()),
                args: None,
                cwd: None,
                url: None,
                http_url: None,
                env: None,
                headers: None,
                timeout: None,
                trust: None,
                linked_provider_ids: vec![old_id.to_string()],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }],
            is_encrypted: true,
        };
        StorageEngine::write_json(&StorageEngine::mcp_path().unwrap(), &mcp).unwrap();

        write_test_file(
            &app_dir.join("protocol_router_calls.json"),
            &json!({
                "calls": [{
                    "ts": 1,
                    "route_id": crate::protocol_router::route_id_for_claude_provider(old_id),
                    "provider": "Claude",
                    "model": "sonnet",
                    "endpoint": "/v1/messages",
                    "wire_api": "open_ai_chat",
                    "status": 200,
                    "latency_ms": 1,
                    "input_tokens": 1,
                    "output_tokens": 1,
                    "total_tokens": 2
                }]
            })
            .to_string(),
        );

        let old_profile_dir = app_dir.join("claude_profiles").join(old_id);
        write_test_file(&old_profile_dir.join("settings.json"), "{\"env\":{}}");

        apply_provider_id_map_to_dependent_state(&id_map).expect("apply id map");

        let loaded_sessions = load_sessions_state().expect("sessions");
        assert_eq!(
            loaded_sessions.sessions[0].provider_id.as_deref(),
            Some(new_id)
        );

        let presets: Value = serde_json::from_str(
            &fs::read_to_string(data_dir.join("workflow_presets.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(presets[0]["provider_id"], new_id);
        assert_eq!(presets[0]["active_provider_id"], new_id);

        let runs: Value =
            serde_json::from_str(&fs::read_to_string(data_dir.join("workflow_runs.json")).unwrap())
                .unwrap();
        assert_eq!(runs[0]["provider_id"], new_id);

        let mcp_after: mcp_servers::MCPServersState =
            StorageEngine::read_json(&StorageEngine::mcp_path().unwrap()).unwrap();
        assert_eq!(
            mcp_after.servers[0].linked_provider_ids,
            vec![new_id.to_string()]
        );

        let stats: Value = serde_json::from_str(
            &fs::read_to_string(app_dir.join("protocol_router_calls.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            stats["calls"][0]["route_id"],
            crate::protocol_router::route_id_for_claude_provider(new_id)
        );
        assert!(app_dir
            .join("claude_profiles")
            .join(new_id)
            .join("settings.json")
            .exists());
    });
}

#[test]
fn render_opencode_requires_provider_key() {
    let provider = ProviderRecord {
        core: ProviderCore {
            id: "11111111-1111-4111-8111-111111111111".to_string(),
            name: "OpenCode".to_string(),
            tool: "opencode".to_string(),
            api_key: "sk-test".to_string(),
            code: None,
            base_url: Some("https://example.com/v1".to_string()),
            model: Some("model".to_string()),
        },
        ..ProviderRecord::default()
    };

    let err = render_opencode(&provider).expect_err("missing provider key fails");
    assert!(err.contains("provider_key"));
}

#[test]
fn normalize_service_provider_record_preserves_opencode_go_openai_responses() {
    let mut record = ServiceProviderRecord {
        id: "opencode-go".to_string(),
        name: "OpenCode Go".to_string(),
        tool: "claude".to_string(),
        icon: None,
        api_key: "sk-test".to_string(),
        base_url: Some("https://opencode.ai/zen/go/v1".to_string()),
        model: Some("claude-sonnet-4".to_string()),
        claude_api_format: "open_ai_responses".to_string(),
        claude_connection_mode: "protocol_router".to_string(),
        protocol_router_upstream_provider_id: None,
        protocol_router_wire_api: "open_ai_responses".to_string(),
        claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
        claude_model_mappings: vec![],
        claude_enable_tool_search: None,
        claude_auto_memory_enabled: None,
        claude_always_thinking_enabled: None,
        claude_away_summary_enabled: None,
        claude_include_git_instructions: None,
        claude_enable_attribution: None,
        code: Some("opencode-go".to_string()),
        is_enabled: Some(true),
        provider_key: None,
        env_managed: Some(true),
        favorite_at: None,
        tool_config: Map::new(),
        history: vec![],
        extra: Map::new(),
        fetched_models: None,
    };

    normalize_service_provider_record(&mut record);

    assert_eq!(record.claude_api_format, "open_ai_responses");
    assert_eq!(record.protocol_router_wire_api, "open_ai_responses");
}
