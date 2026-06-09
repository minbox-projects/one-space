use super::*;
use serde_json::json;

#[test]
fn restore_service_provider_api_keys_from_legacy_matches_uuid_by_code_and_name() {
    with_temp_dir("service-provider-restore-key-from-legacy", |_| {
        let legacy = ProvidersState {
            active: HashMap::from([("claude".to_string(), "custom-claude".to_string())]),
            providers: vec![
                ProviderRecord {
                    core: ProviderCore {
                        id: "custom-claude".to_string(),
                        name: "Code Provider".to_string(),
                        tool: "claude".to_string(),
                        api_key: "code-key".to_string(),
                        code: Some("work-code".to_string()),
                        base_url: None,
                        model: None,
                    },
                    ..ProviderRecord::default()
                },
                ProviderRecord {
                    core: ProviderCore {
                        id: "custom-codex".to_string(),
                        name: "Named Provider".to_string(),
                        tool: "codex".to_string(),
                        api_key: "name-key".to_string(),
                        code: None,
                        base_url: None,
                        model: None,
                    },
                    ..ProviderRecord::default()
                },
            ],
        };
        let legacy_blob =
            CryptoService::encrypt_json(&serde_json::to_value(&legacy).expect("legacy value"))
                .expect("encrypt legacy");
        StorageEngine::write_json(&StorageEngine::providers_path().unwrap(), &legacy_blob)
            .expect("write legacy providers");

        let mut state = ServiceProvidersState {
            active: HashMap::new(),
            providers: vec![
                ServiceProviderRecord {
                    id: "11111111-1111-4111-8111-111111111111".to_string(),
                    name: "Code Provider Renamed".to_string(),
                    tool: "claude".to_string(),
                    api_key: String::new(),
                    code: Some("work-code".to_string()),
                    ..ServiceProviderRecord::default()
                },
                ServiceProviderRecord {
                    id: "22222222-2222-4222-8222-222222222222".to_string(),
                    name: "Named Provider".to_string(),
                    tool: "codex".to_string(),
                    api_key: String::new(),
                    code: None,
                    ..ServiceProviderRecord::default()
                },
            ],
        };

        let changed = restore_missing_service_provider_api_keys_from_legacy(&mut state).unwrap();

        assert!(changed);
        assert_eq!(state.providers[0].api_key, "code-key");
        assert_eq!(state.providers[1].api_key, "name-key");
        assert!(state
            .providers
            .iter()
            .all(|provider| is_uuid_v4(&provider.id)));
    });
}

#[test]
fn import_shared_providers_preserves_local_api_key_when_incoming_uuid_differs() {
    with_temp_dir("shared-provider-import-preserve-key", |home| {
        let local = ProvidersState {
            active: HashMap::from([(
                "claude".to_string(),
                "11111111-1111-4111-8111-111111111111".to_string(),
            )]),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: "11111111-1111-4111-8111-111111111111".to_string(),
                    name: "Work Claude".to_string(),
                    tool: "claude".to_string(),
                    api_key: "local-key".to_string(),
                    code: Some("work-claude".to_string()),
                    base_url: Some("https://local.example.com/v1".to_string()),
                    model: Some("local-model".to_string()),
                },
                ..ProviderRecord::default()
            }],
        };
        save_providers_state(&local).expect("save local providers");

        let shared = ProvidersState {
            active: HashMap::from([(
                "claude".to_string(),
                "22222222-2222-4222-8222-222222222222".to_string(),
            )]),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: "22222222-2222-4222-8222-222222222222".to_string(),
                    name: "Work Claude Remote".to_string(),
                    tool: "claude".to_string(),
                    api_key: String::new(),
                    code: Some("work-claude".to_string()),
                    base_url: Some("https://shared.example.com/v1".to_string()),
                    model: Some("shared-model".to_string()),
                },
                ..ProviderRecord::default()
            }],
        };
        let shared_path = home.join("shared-providers.json");
        StorageEngine::write_json(&shared_path, &shared).expect("write shared providers");

        import_shared_providers_to_local(&shared_path).expect("import shared");
        let updated = load_providers_state().expect("load updated providers");

        assert_eq!(updated.providers.len(), 1);
        assert_eq!(
            updated.providers[0].core.id,
            "11111111-1111-4111-8111-111111111111"
        );
        assert_eq!(updated.providers[0].core.api_key, "local-key");
        assert_eq!(
            updated.providers[0].core.base_url.as_deref(),
            Some("https://shared.example.com/v1")
        );
        assert_eq!(
            updated.active.get("claude").map(String::as_str),
            Some("11111111-1111-4111-8111-111111111111")
        );
    });
}

#[test]
fn import_shared_providers_does_not_delete_local_providers_missing_from_shared() {
    with_temp_dir("shared-provider-import-no-delete-missing-local", |home| {
        let local = ProvidersState {
            active: HashMap::from([
                (
                    "claude".to_string(),
                    "11111111-1111-4111-8111-111111111111".to_string(),
                ),
                (
                    "gemini".to_string(),
                    "22222222-2222-4222-8222-222222222222".to_string(),
                ),
            ]),
            providers: vec![
                ProviderRecord {
                    core: ProviderCore {
                        id: "11111111-1111-4111-8111-111111111111".to_string(),
                        name: "Work Claude".to_string(),
                        tool: "claude".to_string(),
                        api_key: "claude-key".to_string(),
                        code: Some("work-claude".to_string()),
                        base_url: None,
                        model: None,
                    },
                    ..ProviderRecord::default()
                },
                ProviderRecord {
                    core: ProviderCore {
                        id: "22222222-2222-4222-8222-222222222222".to_string(),
                        name: "Gemini".to_string(),
                        tool: "gemini".to_string(),
                        api_key: String::new(),
                        code: None,
                        base_url: None,
                        model: None,
                    },
                    ..ProviderRecord::default()
                },
            ],
        };
        save_providers_state(&local).expect("save local providers");

        let shared = ProvidersState {
            active: HashMap::from([(
                "gemini".to_string(),
                "33333333-3333-4333-8333-333333333333".to_string(),
            )]),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: "33333333-3333-4333-8333-333333333333".to_string(),
                    name: "Gemini".to_string(),
                    tool: "gemini".to_string(),
                    api_key: String::new(),
                    code: None,
                    base_url: Some("https://gemini.example.com".to_string()),
                    model: None,
                },
                ..ProviderRecord::default()
            }],
        };
        let shared_path = home.join("shared-providers-partial.json");
        StorageEngine::write_json(&shared_path, &shared).expect("write shared providers");

        import_shared_providers_to_local(&shared_path).expect("import shared");
        let updated = load_providers_state().expect("load updated providers");

        assert_eq!(updated.providers.len(), 2);
        assert!(updated.providers.iter().any(|provider| {
            provider.core.id == "11111111-1111-4111-8111-111111111111"
                && provider.core.api_key == "claude-key"
        }));
        assert_eq!(
            updated.active.get("claude").map(String::as_str),
            Some("11111111-1111-4111-8111-111111111111")
        );
        assert_eq!(
            updated.active.get("gemini").map(String::as_str),
            Some("22222222-2222-4222-8222-222222222222")
        );
    });
}

#[test]
fn shared_profile_sync_remaps_imported_mcp_and_workflow_provider_refs() {
    with_temp_dir("shared-profile-sync-remaps-provider-refs", |home| {
        let local_provider_id = "11111111-1111-4111-8111-111111111111";
        let remote_provider_id = "22222222-2222-4222-8222-222222222222";
        let mut cfg = config::StorageConfig::default();
        cfg.storage_type = "local".to_string();
        cfg.local_storage_path = Some(home.join("shared-root").to_string_lossy().to_string());
        cfg.sync_policy = config::SyncPolicy {
            providers: true,
            mcp: true,
            workflow_presets: true,
            content: false,
            skills_sources: false,
            skills_repository: false,
            subagents_sources: false,
            subagents_repository: false,
            ai_news: false,
        };

        let local = ProvidersState {
            active: HashMap::from([("claude".to_string(), local_provider_id.to_string())]),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: local_provider_id.to_string(),
                    name: "Work Claude".to_string(),
                    tool: "claude".to_string(),
                    api_key: "local-key".to_string(),
                    code: Some("work-claude".to_string()),
                    base_url: Some("https://local.example.com/v1".to_string()),
                    model: None,
                },
                ..ProviderRecord::default()
            }],
        };
        save_providers_state(&local).expect("save local providers");
        sleep(Duration::from_secs(2));

        let shared_providers = ProvidersState {
            active: HashMap::from([("claude".to_string(), remote_provider_id.to_string())]),
            providers: vec![ProviderRecord {
                core: ProviderCore {
                    id: remote_provider_id.to_string(),
                    name: "Work Claude Remote".to_string(),
                    tool: "claude".to_string(),
                    api_key: String::new(),
                    code: Some("work-claude".to_string()),
                    base_url: Some("https://shared.example.com/v1".to_string()),
                    model: None,
                },
                ..ProviderRecord::default()
            }],
        };
        StorageEngine::write_json(
            &shared_profile_path(&cfg, "providers.json").expect("shared providers path"),
            &shared_providers,
        )
        .expect("write shared providers");

        let shared_mcp = mcp_servers::MCPServersState {
            servers: vec![mcp_servers::MCPServer {
                id: "mcp-remote".to_string(),
                name: "Remote MCP".to_string(),
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
                linked_provider_ids: vec![remote_provider_id.to_string()],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }],
            is_encrypted: false,
        };
        StorageEngine::write_json(
            &shared_profile_path(&cfg, "mcp.json").expect("shared mcp path"),
            &shared_mcp,
        )
        .expect("write shared mcp");

        write_test_file(
            &shared_profile_path(&cfg, "workflow_presets.json")
                .expect("shared workflow presets path"),
            &json!([
                {
                    "id": "preset-remote",
                    "tool": "claude",
                    "provider_id": remote_provider_id,
                    "active_provider_id": remote_provider_id,
                    "linked_provider_ids": [remote_provider_id]
                }
            ])
            .to_string(),
        );

        run_local_shared_sync(&cfg).expect("shared sync");

        let updated = load_providers_state().expect("load providers");
        assert_eq!(
            updated.active.get("claude").map(String::as_str),
            Some(local_provider_id)
        );
        assert_eq!(updated.providers[0].core.id, local_provider_id);
        assert_eq!(updated.providers[0].core.api_key, "local-key");
        assert_eq!(
            updated.providers[0].core.base_url.as_deref(),
            Some("https://shared.example.com/v1")
        );

        let mcp_after: mcp_servers::MCPServersState =
            StorageEngine::read_json(&StorageEngine::mcp_path().unwrap()).unwrap();
        assert_eq!(
            mcp_after.servers[0].linked_provider_ids,
            vec![local_provider_id.to_string()]
        );

        let workflow_after: Value = serde_json::from_str(
            &fs::read_to_string(local_workflow_presets_path().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(workflow_after[0]["provider_id"], local_provider_id);
        assert_eq!(workflow_after[0]["active_provider_id"], local_provider_id);
        assert_eq!(
            workflow_after[0]["linked_provider_ids"][0],
            local_provider_id
        );
    });
}
