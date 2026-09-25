use super::*;
use serde_json::json;

#[test]
fn auto_import_system_provider_merges_without_reducing_existing_service_providers() {
    let existing_antigravity_id = "11111111-1111-4111-8111-111111111111".to_string();
    let existing_claude_id = "22222222-2222-4222-8222-222222222222".to_string();
    let mut state = ServiceProvidersState {
        active: HashMap::from([("antigravity".to_string(), existing_antigravity_id.clone())]),
        active_opencode: vec![],
        providers: vec![
            ServiceProviderRecord {
                id: existing_antigravity_id.clone(),
                name: "Existing Antigravity".to_string(),
                tool: "antigravity".to_string(),
                api_key: "antigravity-key".to_string(),
                code: Some("work-antigravity".to_string()),
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
    let system_provider = ServiceProviderRecord {
        id: "default-antigravity".to_string(),
        name: "Imported Antigravity Config".to_string(),
        tool: "antigravity".to_string(),
        code: Some("default-antigravity".to_string()),
        api_key: "SAFE_FIXTURE_system-antigravity-key".to_string(),
        base_url: Some("https://antigravity.example.com".to_string()),
        ..ServiceProviderRecord::default()
    };

    let outcome =
        auto_import_system_provider_into_service_state(&mut state, "antigravity", system_provider)
            .expect("auto import");

    assert!(outcome.imported);
    assert_eq!(state.providers.len(), 3);
    assert_eq!(
        state.active.get("antigravity").map(String::as_str),
        Some(existing_antigravity_id.as_str())
    );
    assert!(state
        .providers
        .iter()
        .all(|provider| is_uuid_v4(&provider.id)));
    assert_eq!(
        state
            .providers
            .iter()
            .filter(|provider| provider.tool == "antigravity")
            .count(),
        2
    );
    assert!(state.providers.iter().any(|provider| {
        provider.tool == "antigravity"
            && provider.code.as_deref() == Some("default-antigravity")
            && provider.env_managed == Some(true)
    }));
}

#[test]
fn auto_import_system_provider_skips_existing_default_code_without_active_requirement() {
    let existing_id = "11111111-1111-4111-8111-111111111111".to_string();
    let mut state = ServiceProvidersState {
        active: HashMap::new(),
        active_opencode: vec![],
        providers: vec![ServiceProviderRecord {
            id: existing_id.clone(),
            name: "Default Antigravity".to_string(),
            tool: "antigravity".to_string(),
            api_key: "antigravity-key".to_string(),
            code: Some("default-antigravity".to_string()),
            ..ServiceProviderRecord::default()
        }],
    };
    let system_provider = ServiceProviderRecord {
        id: "default-antigravity".to_string(),
        name: "Imported Antigravity Config".to_string(),
        tool: "antigravity".to_string(),
        code: Some("default-antigravity".to_string()),
        ..ServiceProviderRecord::default()
    };

    let outcome =
        auto_import_system_provider_into_service_state(&mut state, "antigravity", system_provider)
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
            active_opencode: vec![],
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
                    name: "Antigravity".to_string(),
                    tool: "antigravity".to_string(),
                    api_key: "antigravity-key".to_string(),
                    code: Some("work-antigravity".to_string()),
                    ..ServiceProviderRecord::default()
                },
            ],
        };
        save_service_providers_internal(&service_state).expect("save service providers");

        let legacy_ai_providers = json!({
            "active_antigravity": "default-antigravity",
            "providers": [{
                "id": "default-antigravity",
                "name": "Imported Antigravity Config",
                "tool": "antigravity",
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

        let canonical_path = StorageEngine::providers_path().unwrap();
        let canonical_content = fs::read_to_string(&canonical_path).expect("read providers state");
        let canonical_blob: EncryptedBlob =
            serde_json::from_str(&canonical_content).expect("encrypted providers state");
        let canonical_value =
            CryptoService::decrypt_json(&canonical_blob).expect("decrypt providers state");
        let canonical_state: ServiceProvidersState =
            serde_json::from_value(canonical_value).expect("service providers state");
        assert_eq!(canonical_state.providers.len(), 2);
        assert!(canonical_state
            .providers
            .iter()
            .all(|provider| is_uuid_v4(&provider.id)));
        assert_eq!(
            canonical_state.active.get("claude").map(String::as_str),
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
            active_opencode: vec![],
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
                    name: "Imported Antigravity Config".to_string(),
                    tool: "antigravity".to_string(),
                    code: Some("default-antigravity".to_string()),
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
        fs::remove_file(StorageEngine::providers_path().unwrap()).expect("remove provider state");

        let err = load_service_providers_state().expect_err("missing service state should fail");
        assert!(err.contains("service_providers state missing after migration"));
    });
}

#[test]
fn normalize_service_provider_ids_rewrites_legacy_ids_and_references() {
    let mut state = ServiceProvidersState {
        active: HashMap::from([
            ("claude".to_string(), "custom-claude".to_string()),
            ("codex".to_string(), "default-codex".to_string()),
        ]),
        active_opencode: vec![],
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
    let provider = ServiceProviderRecord {
        id: "11111111-1111-4111-8111-111111111111".to_string(),
        name: "OpenCode".to_string(),
        tool: "opencode".to_string(),
        api_key: "sk-test".to_string(),
        base_url: Some("https://example.com/v1".to_string()),
        model: Some("model".to_string()),
        ..ServiceProviderRecord::default()
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

#[test]
fn service_provider_state_migrates_when_providers_path_contains_old_schema() {
    with_temp_dir("service-provider-old-schema-at-providers-path", |_| {
        let new_path = StorageEngine::providers_path().expect("new path");
        let legacy_state = ProvidersState {
            active: HashMap::from([(
                "antigravity".to_string(),
                "11111111-1111-4111-8111-111111111111".to_string(),
            )]),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: "11111111-1111-4111-8111-111111111111".to_string(),
                    name: "Legacy Antigravity".to_string(),
                    tool: "antigravity".to_string(),
                    api_key: "legacy-key".to_string(),
                    ..ProviderCore::default()
                },
                ..ProviderRecord::default()
            }],
        };
        let legacy_blob =
            CryptoService::encrypt_json(&serde_json::to_value(&legacy_state).unwrap()).unwrap();
        StorageEngine::write_json(&new_path, &legacy_blob).expect("write old providers schema");

        let loaded = load_service_providers_state().expect("load migrated state");

        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].name, "Legacy Antigravity");
        assert_eq!(loaded.providers[0].api_key, "legacy-key");
        assert_eq!(
            loaded.active.get("antigravity").map(String::as_str),
            Some("11111111-1111-4111-8111-111111111111")
        );

        let canonical_content = fs::read_to_string(&new_path).expect("read migrated state");
        let canonical_blob: EncryptedBlob = serde_json::from_str(&canonical_content).unwrap();
        let canonical_state: ServiceProvidersState =
            serde_json::from_value(CryptoService::decrypt_json(&canonical_blob).unwrap()).unwrap();
        assert_eq!(canonical_state.providers[0].name, "Legacy Antigravity");
    });
}

fn read_json_file(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json file"))
        .expect("parse json file")
}

#[test]
fn migration_rewrites_legacy_provider_identity_and_preserves_brand_values() {
    with_temp_dir("migration-antigravity-legacy-providers", |home| {
        let legacy_path = home
            .join(".config")
            .join("onespace")
            .join("local_data")
            .join("ai_providers.json");
        let legacy = json!({
            "active_claude": "claude-legacy",
            "active_gemini": "gemini-legacy",
            "is_encrypted": false,
            "providers": [
                {
                    "id": "gemini-legacy",
                    "name": "Antigravity Legacy",
                    "tool": "gemini",
                    "api_key": "antigravity-key",
                    "base_url": "https://gemini.example.com",
                    "model": "gemini-2.5-pro",
                    "gemini_auth_type": "gemini-api-key",
                    "protocol": "google-gemini",
                    "capability": "gemini-2.5-flash"
                },
                {
                    "id": "claude-legacy",
                    "name": "Claude Legacy",
                    "tool": "claude",
                    "api_key": "claude-key"
                }
            ]
        });
        write_test_file(&legacy_path, &legacy.to_string());

        run_migration_impl().expect("migration");

        let state = load_service_providers_state().expect("load providers");
        assert!(state.active.contains_key("antigravity"));
        assert!(!state.active.contains_key("gemini"));

        let antigravity = state
            .providers
            .iter()
            .find(|provider| provider.tool == "antigravity")
            .expect("antigravity provider");
        assert!(antigravity
            .tool_config
            .contains_key("antigravity_auth_type"));
        assert!(!antigravity.tool_config.contains_key("gemini_auth_type"));
        assert_eq!(
            antigravity
                .tool_config
                .get("protocol")
                .and_then(Value::as_str),
            Some("google-gemini")
        );
        assert_eq!(
            antigravity
                .tool_config
                .get("capability")
                .and_then(Value::as_str),
            Some("gemini-2.5-flash")
        );
        assert_eq!(antigravity.model.as_deref(), Some("gemini-2.5-pro"));

        let claude = state
            .providers
            .iter()
            .find(|provider| provider.tool == "claude")
            .expect("claude provider");
        assert_eq!(claude.api_key, "claude-key");

        let rewritten = read_json_file(&legacy_path);
        assert!(rewritten.get("active_gemini").is_none());
        assert!(rewritten.get("active_antigravity").is_some());
        assert_eq!(rewritten["providers"][0]["tool"], "antigravity");
        assert_eq!(
            rewritten["providers"][0]["antigravity_auth_type"],
            "gemini-api-key"
        );
        assert_eq!(rewritten["providers"][0]["model"], "gemini-2.5-pro");
        assert_eq!(rewritten["providers"][0]["protocol"], "google-gemini");
    });
}

#[test]
fn migration_rewrites_canonical_service_providers_and_leaves_siblings_unchanged() {
    with_temp_dir("migration-antigravity-canonical-providers", |_| {
        let claude_id = "11111111-1111-4111-8111-111111111111";
        let antigravity_id = "22222222-2222-4222-8222-222222222222";
        let mut tool_config = Map::new();
        tool_config.insert(
            "gemini_auth_type".to_string(),
            Value::String("gemini-api-key".to_string()),
        );
        tool_config.insert(
            "protocol".to_string(),
            Value::String("google-gemini".to_string()),
        );
        let state = ServiceProvidersState {
            active: HashMap::from([
                ("claude".to_string(), claude_id.to_string()),
                ("gemini".to_string(), antigravity_id.to_string()),
            ]),
            active_opencode: vec![],
            providers: vec![
                ServiceProviderRecord {
                    id: claude_id.to_string(),
                    name: "Claude".to_string(),
                    tool: "claude".to_string(),
                    api_key: "claude-key".to_string(),
                    ..ServiceProviderRecord::default()
                },
                ServiceProviderRecord {
                    id: antigravity_id.to_string(),
                    name: "Antigravity".to_string(),
                    tool: "gemini".to_string(),
                    api_key: "antigravity-key".to_string(),
                    model: Some("gemini-2.5-pro".to_string()),
                    tool_config,
                    ..ServiceProviderRecord::default()
                },
            ],
        };
        save_service_providers_internal(&state).expect("save providers");

        let before = load_service_providers_state().expect("load before");
        let claude_before =
            serde_json::to_value(before.providers.iter().find(|p| p.tool == "claude").unwrap())
                .unwrap();
        let active_claude_before = before.active.get("claude").cloned();

        run_migration_impl().expect("migration");

        let after = load_service_providers_state().expect("load after");
        assert!(after.active.contains_key("antigravity"));
        assert!(!after.active.contains_key("gemini"));
        assert_eq!(after.active.get("claude").cloned(), active_claude_before);

        let antigravity = after
            .providers
            .iter()
            .find(|provider| provider.tool == "antigravity")
            .expect("antigravity provider");
        assert!(antigravity
            .tool_config
            .contains_key("antigravity_auth_type"));
        assert!(!antigravity.tool_config.contains_key("gemini_auth_type"));
        assert_eq!(
            antigravity
                .tool_config
                .get("protocol")
                .and_then(Value::as_str),
            Some("google-gemini")
        );
        assert_eq!(antigravity.model.as_deref(), Some("gemini-2.5-pro"));

        let claude_after =
            serde_json::to_value(after.providers.iter().find(|p| p.tool == "claude").unwrap())
                .unwrap();
        assert_eq!(claude_after, claude_before);
    });
}

#[test]
fn migration_removes_gemini_session_records_from_legacy_store() {
    with_temp_dir("migration-antigravity-legacy-sessions", |home| {
        let data_dir = home.join(".config").join("onespace").join("local_data");
        let legacy_sessions = json!([
            {
                "id": "s-gemini",
                "name": "Gemini Session",
                "working_dir": "/tmp/gemini-project",
                "model_type": "gemini",
                "tool_session_id": "gemini-session-1",
                "created_at": 1
            },
            {
                "id": "s-claude",
                "name": "Claude Session",
                "working_dir": "/tmp/claude-project",
                "model_type": "claude",
                "tool_session_id": "claude-session-1",
                "created_at": 2
            }
        ]);
        write_test_file(&data_dir.join("ai_sessions.json"), &legacy_sessions.to_string());

        run_migration_impl().expect("migration");

        let sessions = load_sessions_state().expect("load sessions");
        assert!(sessions.sessions.iter().all(|s| s.tool != "gemini"));
        assert!(sessions.sessions.iter().any(|s| s.tool == "claude"));

        let rewritten = read_json_file(&data_dir.join("ai_sessions.json"));
        assert!(rewritten
            .as_array()
            .expect("legacy sessions array")
            .iter()
            .all(|session| session["model_type"] != "gemini"));
    });
}

#[test]
fn migrate_step_removes_gemini_usage_state_and_tombstones() {
    with_temp_dir("migration-antigravity-usage-state", |_| {
        let mut state = SessionsState::default();
        state
            .sessions
            .push(session_record("s-gemini", "gemini", "/tmp/g", 1, "active"));
        state
            .sessions
            .push(session_record("s-claude", "claude", "/tmp/c", 2, "active"));
        state
            .history_sync
            .tools
            .insert("gemini".to_string(), SessionsHistoryToolState::default());
        state
            .history_sync
            .tools
            .insert("claude".to_string(), SessionsHistoryToolState::default());
        state.tombstones.insert("gemini::g1".to_string());
        state.tombstones.insert("claude::c1".to_string());
        save_sessions_state(&state).expect("save sessions");

        migrate_gemini_identifiers_to_antigravity().expect("migration step");

        let after = load_sessions_state().expect("load sessions");
        assert!(after.sessions.iter().all(|session| session.tool != "gemini"));
        assert!(after.sessions.iter().any(|session| session.tool == "claude"));
        assert!(!after.history_sync.tools.contains_key("gemini"));
        assert!(after.history_sync.tools.contains_key("claude"));
        assert!(!after.tombstones.contains("gemini::g1"));
        assert!(after.tombstones.contains("claude::c1"));
    });
}

#[test]
fn migrate_step_rewrites_workflows_workspaces_and_provider_presets() {
    with_temp_dir("migration-antigravity-related-stores", |_| {
        let workflow_presets = json!([
            {
                "id": "p1",
                "tool": "gemini",
                "launch_prompt": "please run gemini now"
            },
            {
                "id": "p2",
                "tool": "claude",
                "launch_prompt": "claude"
            }
        ]);
        write_test_file(
            &local_workflow_presets_path().unwrap(),
            &workflow_presets.to_string(),
        );
        let workflow_runs = json!([
            {
                "id": "r1",
                "preset_id": "p1",
                "tool": "gemini",
                "summary": "gemini run"
            },
            {
                "id": "r2",
                "preset_id": "p2",
                "tool": "claude",
                "summary": "claude run"
            }
        ]);
        write_test_file(
            &local_workflow_runs_path().unwrap(),
            &workflow_runs.to_string(),
        );

        let workspaces_path = crate::get_data_dir()
            .unwrap()
            .join("data")
            .join("workspaces")
            .join("state.json");
        write_test_file(
            &workspaces_path,
            &json!({
                "workspaces": [{ "id": "w1", "default_models": ["gemini", "claude"] }],
                "mcp_bindings": [{
                    "workspace_id": "w1",
                    "server_id": "m1",
                    "enabled_models": ["gemini", "codex"]
                }],
                "deleted_roots": [],
                "revision": 1
            })
            .to_string(),
        );

        let presets_path = StorageEngine::provider_presets_path().unwrap();
        StorageEngine::write_json(
            &presets_path,
            &json!({
                "builtin_seed_version": 2,
                "presets": [{
                    "id": "vendor",
                    "name": "Vendor",
                    "created_at": 1,
                    "updated_at": 1,
                    "endpoints": {
                        "openai_base_url": "https://x",
                        "gemini_base_url": "https://g.example.com"
                    },
                    "template": {
                        "tool": "gemini",
                        "gemini_base_url": "https://template"
                    }
                }]
            }),
        )
        .expect("write presets");

        migrate_gemini_identifiers_to_antigravity().expect("migration step");

        let presets = read_json_file(&local_workflow_presets_path().unwrap());
        assert_eq!(presets[0]["tool"], "antigravity");
        assert_eq!(presets[0]["launch_prompt"], "please run gemini now");
        assert_eq!(presets[1]["tool"], "claude");

        let runs = read_json_file(&local_workflow_runs_path().unwrap());
        assert_eq!(runs[0]["tool"], "antigravity");
        assert_eq!(runs[0]["summary"], "gemini run");
        assert_eq!(runs[1]["tool"], "claude");

        let workspaces = read_json_file(&workspaces_path);
        assert_eq!(workspaces["workspaces"][0]["default_models"][0], "antigravity");
        assert_eq!(
            workspaces["mcp_bindings"][0]["enabled_models"][0],
            "antigravity"
        );
        assert_eq!(workspaces["mcp_bindings"][0]["enabled_models"][1], "codex");

        let stored_presets = read_json_file(&presets_path);
        assert_eq!(
            stored_presets["presets"][0]["endpoints"]["antigravity_base_url"],
            "https://g.example.com"
        );
        assert!(stored_presets["presets"][0]["endpoints"]
            .get("gemini_base_url")
            .is_none());
        assert_eq!(
            stored_presets["presets"][0]["template"]["tool"],
            "antigravity"
        );
    });
}

#[test]
fn migrate_step_renames_config_maps_without_rewriting_user_command_text() {
    with_temp_dir("migration-antigravity-config", |_| {
        let config_path = config::get_app_dir().unwrap().join("config.json");
        write_test_file(
            &config_path,
            &json!({
                "default_ai_model": "gemini",
                "ai_model_launch_commands": {
                    "gemini": "gemini --dangerously-skip-permissions # keep gemini literal",
                    "claude": "claude --session-id {session_id}"
                },
                "ai_model_permission_modes": {
                    "gemini": "full_access",
                    "claude": "default"
                },
                "skills_sources": [{ "id": "s1", "default_models": ["gemini", "claude"] }],
                "subagents_sources": [{ "id": "s2", "default_models": ["gemini"] }]
            })
            .to_string(),
        );

        migrate_gemini_identifiers_to_antigravity().expect("migration step");

        let cfg = read_json_file(&config_path);
        assert_eq!(cfg["default_ai_model"], "antigravity");
        assert!(cfg["ai_model_launch_commands"].get("gemini").is_none());
        assert_eq!(
            cfg["ai_model_launch_commands"]["antigravity"],
            "gemini --dangerously-skip-permissions # keep gemini literal"
        );
        assert_eq!(
            cfg["ai_model_permission_modes"]["antigravity"],
            "full_access"
        );
        assert_eq!(cfg["skills_sources"][0]["default_models"][0], "antigravity");
        assert_eq!(cfg["skills_sources"][0]["default_models"][1], "claude");
        assert_eq!(cfg["subagents_sources"][0]["default_models"][0], "antigravity");
    });
}

#[test]
fn migration_second_run_is_a_no_op() {
    with_temp_dir("migration-antigravity-idempotent", |home| {
        let data_dir = home.join(".config").join("onespace").join("local_data");
        write_test_file(
            &data_dir.join("ai_providers.json"),
            &json!({
                "active_gemini": "gemini-legacy",
                "is_encrypted": false,
                "providers": [{
                    "id": "gemini-legacy",
                    "name": "Antigravity Legacy",
                    "tool": "gemini",
                    "api_key": "key"
                }]
            })
            .to_string(),
        );
        write_test_file(
            &config::get_app_dir().unwrap().join("config.json"),
            &json!({
                "default_ai_model": "gemini",
                "ai_model_launch_commands": { "gemini": "gemini" }
            })
            .to_string(),
        );

        run_migration_impl().expect("first migration");
        let providers_path = StorageEngine::providers_path().unwrap();
        let config_path = config::get_app_dir().unwrap().join("config.json");
        let sessions_path = StorageEngine::sessions_path().unwrap();
        let first_providers = fs::read(&providers_path).unwrap();
        let first_config = fs::read(&config_path).unwrap();
        let first_sessions = fs::read(&sessions_path).unwrap();

        run_migration_impl().expect("second migration");

        assert_eq!(fs::read(&providers_path).unwrap(), first_providers);
        assert_eq!(fs::read(&config_path).unwrap(), first_config);
        assert_eq!(fs::read(&sessions_path).unwrap(), first_sessions);
    });
}

#[test]
fn migration_backup_restore_rollback_brings_back_gemini_values() {
    with_temp_dir("migration-antigravity-rollback", |home| {
        let data_dir = home.join(".config").join("onespace").join("local_data");
        let providers_path = data_dir.join("ai_providers.json");
        write_test_file(
            &providers_path,
            &json!({
                "active_gemini": "gemini-legacy",
                "is_encrypted": false,
                "providers": [{
                    "id": "gemini-legacy",
                    "name": "Antigravity Legacy",
                    "tool": "gemini",
                    "api_key": "key"
                }]
            })
            .to_string(),
        );

        let state = run_migration_impl().expect("migration");
        let backup_id = state.last_backup_id.clone().expect("backup id");

        let migrated = load_service_providers_state().expect("load providers");
        assert!(migrated.providers.iter().any(|p| p.tool == "antigravity"));

        rollback_from_backup(&backup_id).expect("rollback");

        let restored = read_json_file(&providers_path);
        assert_eq!(restored["active_gemini"], "gemini-legacy");
        assert_eq!(restored["providers"][0]["tool"], "gemini");
    });
}
