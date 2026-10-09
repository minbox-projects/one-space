use super::*;
use serde_json::json;

#[test]
fn import_shared_providers_preserves_local_api_key_when_incoming_uuid_differs() {
    with_temp_dir("shared-provider-import-preserve-key", |home| {
        let local = ServiceProvidersState {
            active: HashMap::from([(
                "claude".to_string(),
                "11111111-1111-4111-8111-111111111111".to_string(),
            )]),
            active_opencode: vec![],
            providers: vec![ServiceProviderRecord {
                id: "11111111-1111-4111-8111-111111111111".to_string(),
                name: "Work Claude".to_string(),
                tool: "claude".to_string(),
                api_key: "local-key".to_string(),
                code: Some("work-claude".to_string()),
                base_url: Some("https://local.example.com/v1".to_string()),
                model: Some("local-model".to_string()),
                ..ServiceProviderRecord::default()
            }],
        };
        save_service_providers_internal(&local).expect("save local providers");

        let shared = ServiceProvidersState {
            active: HashMap::from([(
                "claude".to_string(),
                "22222222-2222-4222-8222-222222222222".to_string(),
            )]),
            active_opencode: vec![],
            providers: vec![ServiceProviderRecord {
                id: "22222222-2222-4222-8222-222222222222".to_string(),
                name: "Work Claude Remote".to_string(),
                tool: "claude".to_string(),
                api_key: String::new(),
                code: Some("work-claude".to_string()),
                base_url: Some("https://shared.example.com/v1".to_string()),
                model: Some("shared-model".to_string()),
                ..ServiceProviderRecord::default()
            }],
        };
        let shared_path = home.join("shared-providers.json");
        StorageEngine::write_json(&shared_path, &shared).expect("write shared providers");

        import_shared_service_providers_to_local(&shared_path).expect("import shared");
        let updated = load_service_providers_state().expect("load updated providers");

        assert_eq!(updated.providers.len(), 1);
        assert_eq!(
            updated.providers[0].id,
            "11111111-1111-4111-8111-111111111111"
        );
        assert_eq!(updated.providers[0].api_key, "local-key");
        assert_eq!(
            updated.providers[0].base_url.as_deref(),
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
        let local = ServiceProvidersState {
            active: HashMap::from([
                (
                    "claude".to_string(),
                    "11111111-1111-4111-8111-111111111111".to_string(),
                ),
                (
                    "antigravity".to_string(),
                    "22222222-2222-4222-8222-222222222222".to_string(),
                ),
            ]),
            active_opencode: vec![],
            providers: vec![
                ServiceProviderRecord {
                    id: "11111111-1111-4111-8111-111111111111".to_string(),
                    name: "Work Claude".to_string(),
                    tool: "claude".to_string(),
                    api_key: "claude-key".to_string(),
                    code: Some("work-claude".to_string()),
                    ..ServiceProviderRecord::default()
                },
                ServiceProviderRecord {
                    id: "22222222-2222-4222-8222-222222222222".to_string(),
                    name: "Antigravity".to_string(),
                    tool: "antigravity".to_string(),
                    api_key: String::new(),
                    ..ServiceProviderRecord::default()
                },
            ],
        };
        save_service_providers_internal(&local).expect("save local providers");

        let shared = ServiceProvidersState {
            active: HashMap::from([(
                "antigravity".to_string(),
                "33333333-3333-4333-8333-333333333333".to_string(),
            )]),
            active_opencode: vec![],
            providers: vec![ServiceProviderRecord {
                id: "33333333-3333-4333-8333-333333333333".to_string(),
                name: "Antigravity".to_string(),
                tool: "antigravity".to_string(),
                api_key: String::new(),
                base_url: Some("https://antigravity.example.com".to_string()),
                ..ServiceProviderRecord::default()
            }],
        };
        let shared_path = home.join("shared-providers-partial.json");
        StorageEngine::write_json(&shared_path, &shared).expect("write shared providers");

        import_shared_service_providers_to_local(&shared_path).expect("import shared");
        let updated = load_service_providers_state().expect("load updated providers");

        assert_eq!(updated.providers.len(), 2);
        assert!(updated.providers.iter().any(|provider| {
            provider.id == "11111111-1111-4111-8111-111111111111"
                && provider.api_key == "claude-key"
        }));
        assert_eq!(
            updated.active.get("claude").map(String::as_str),
            Some("11111111-1111-4111-8111-111111111111")
        );
        assert_eq!(
            updated.active.get("antigravity").map(String::as_str),
            Some("22222222-2222-4222-8222-222222222222")
        );
    });
}

#[test]
fn shared_profile_sync_import_merges_service_state_without_legacy_overwrite() {
    with_temp_dir(
        "shared-provider-import-service-state-no-overwrite",
        |home| {
            let local = ServiceProvidersState {
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
                        api_key: "local-key".to_string(),
                        code: Some("work-claude".to_string()),
                        base_url: Some("https://local.example.com/v1".to_string()),
                        ..ServiceProviderRecord::default()
                    },
                    ServiceProviderRecord {
                        id: "22222222-2222-4222-8222-222222222222".to_string(),
                        name: "Work Codex".to_string(),
                        tool: "codex".to_string(),
                        api_key: "codex-key".to_string(),
                        ..ServiceProviderRecord::default()
                    },
                ],
            };
            save_service_providers_internal(&local).expect("save local service providers");

            let shared = ServiceProvidersState {
                active: HashMap::new(),
                active_opencode: vec![],
                providers: vec![ServiceProviderRecord {
                    id: "33333333-3333-4333-8333-333333333333".to_string(),
                    name: "Imported Antigravity Config".to_string(),
                    tool: "antigravity".to_string(),
                    code: Some("default-antigravity".to_string()),
                    ..ServiceProviderRecord::default()
                }],
            };
            let shared_path = home.join("shared-sparse-providers.json");
            StorageEngine::write_json(&shared_path, &shared)
                .expect("write sparse shared providers");

            import_shared_service_providers_to_local(&shared_path).expect("import shared");
            let updated = load_service_providers_state().expect("load service providers");

            assert_eq!(updated.providers.len(), 3);
            assert!(updated.providers.iter().any(|provider| {
                provider.id == "11111111-1111-4111-8111-111111111111"
                    && provider.tool == "claude"
                    && provider.api_key == "local-key"
            }));
            assert!(updated.providers.iter().any(|provider| {
                provider.id == "22222222-2222-4222-8222-222222222222"
                    && provider.tool == "codex"
                    && provider.api_key == "codex-key"
            }));
            assert!(updated.providers.iter().any(|provider| {
                provider.tool == "antigravity"
                    && provider.code.as_deref() == Some("default-antigravity")
            }));
        },
    );
}

#[test]
fn shared_profile_sync_remaps_mcp_refs_and_leaves_workflow_files_untouched() {
    with_temp_dir("shared-profile-sync-remaps-provider-refs", |home| {
        let local_provider_id = "11111111-1111-4111-8111-111111111111";
        let remote_provider_id = "22222222-2222-4222-8222-222222222222";
        let mut cfg = config::StorageConfig::default();
        cfg.storage_type = "local".to_string();
        cfg.local_storage_path = Some(home.join("shared-root").to_string_lossy().to_string());
        cfg.sync_policy = config::SyncPolicy {
            providers: true,
            mcp: true,
            content: false,
            skills_sources: false,
            skills_repository: false,
            subagents_sources: false,
            subagents_repository: false,
            ai_news: false,
            ..config::SyncPolicy::default()
        };

        let local = ServiceProvidersState {
            active: HashMap::from([("claude".to_string(), local_provider_id.to_string())]),
            active_opencode: vec![],
            providers: vec![ServiceProviderRecord {
                id: local_provider_id.to_string(),
                name: "Work Claude".to_string(),
                tool: "claude".to_string(),
                api_key: "local-key".to_string(),
                code: Some("work-claude".to_string()),
                base_url: Some("https://local.example.com/v1".to_string()),
                ..ServiceProviderRecord::default()
            }],
        };
        save_service_providers_internal(&local).expect("save local providers");
        sleep(Duration::from_secs(2));

        let shared_providers = ServiceProvidersState {
            active: HashMap::from([("claude".to_string(), remote_provider_id.to_string())]),
            active_opencode: vec![],
            providers: vec![ServiceProviderRecord {
                id: remote_provider_id.to_string(),
                name: "Work Claude Remote".to_string(),
                tool: "claude".to_string(),
                api_key: String::new(),
                code: Some("work-claude".to_string()),
                base_url: Some("https://shared.example.com/v1".to_string()),
                ..ServiceProviderRecord::default()
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

        // Local workflow files must survive the sync untouched: no provider-id
        // remap of the local copies and no export into the shared profile.
        let local_presets_path = crate::get_data_dir()
            .expect("data dir")
            .join("workflow_presets.json");
        let local_runs_path = crate::get_data_dir()
            .expect("data dir")
            .join("workflow_runs.json");
        write_test_file(
            &local_presets_path,
            &json!([
                {
                    "id": "preset-local",
                    "tool": "claude",
                    "provider_id": remote_provider_id,
                    "active_provider_id": remote_provider_id,
                    "linked_provider_ids": [remote_provider_id]
                }
            ])
            .to_string(),
        );
        write_test_file(
            &local_runs_path,
            &json!([
                {
                    "id": "run-local",
                    "preset_id": "preset-local",
                    "provider_id": remote_provider_id
                }
            ])
            .to_string(),
        );
        let presets_before = fs::read(&local_presets_path).expect("read local workflow presets");
        let runs_before = fs::read(&local_runs_path).expect("read local workflow runs");

        run_local_shared_sync(&cfg).expect("shared sync");

        let updated = load_service_providers_state().expect("load providers");
        assert_eq!(
            updated.active.get("claude").map(String::as_str),
            Some(local_provider_id)
        );
        assert_eq!(updated.providers[0].id, local_provider_id);
        assert_eq!(updated.providers[0].api_key, "local-key");
        assert_eq!(
            updated.providers[0].base_url.as_deref(),
            Some("https://shared.example.com/v1")
        );

        let mcp_after: mcp_servers::MCPServersState =
            StorageEngine::read_json(&StorageEngine::mcp_path().unwrap()).unwrap();
        assert_eq!(
            mcp_after.servers[0].linked_provider_ids,
            vec![local_provider_id.to_string()]
        );

        assert_eq!(
            fs::read(&local_presets_path).expect("read local workflow presets"),
            presets_before,
            "workflow_presets.json must stay byte-identical through local/shared sync"
        );
        assert_eq!(
            fs::read(&local_runs_path).expect("read local workflow runs"),
            runs_before,
            "workflow_runs.json must stay byte-identical through local/shared sync"
        );
        let shared_presets_path = shared_profile_path(&cfg, "workflow_presets.json")
            .expect("shared workflow presets path");
        assert!(
            !shared_presets_path.exists(),
            "local/shared sync must not create a workflow_presets.json copy in the shared profile"
        );
        assert!(
            !shared_profile_path(&cfg, "workflow_runs.json")
                .expect("shared workflow runs path")
                .exists(),
            "local/shared sync must not create a workflow_runs.json copy in the shared profile"
        );
    });
}

#[test]
fn shared_profile_sync_tolerates_shared_workflow_presets_without_local_copy() {
    with_temp_dir("shared-profile-sync-shared-only-workflow-presets", |home| {
        let mut cfg = config::StorageConfig::default();
        cfg.storage_type = "local".to_string();
        cfg.local_storage_path = Some(home.join("shared-root").to_string_lossy().to_string());
        cfg.sync_policy = config::SyncPolicy {
            providers: true,
            mcp: true,
            content: false,
            skills_sources: false,
            skills_repository: false,
            subagents_sources: false,
            subagents_repository: false,
            ai_news: false,
            ..config::SyncPolicy::default()
        };

        let shared_presets_path = shared_profile_path(&cfg, "workflow_presets.json")
            .expect("shared workflow presets path");
        write_test_file(
            &shared_presets_path,
            &json!([
                {
                    "id": "preset-shared-only",
                    "tool": "claude",
                    "provider_id": "22222222-2222-4222-8222-222222222222"
                }
            ])
            .to_string(),
        );
        let shared_presets_before =
            fs::read(&shared_presets_path).expect("read shared workflow presets");

        let local_presets_path = crate::get_data_dir()
            .expect("data dir")
            .join("workflow_presets.json");
        assert!(
            !local_presets_path.exists(),
            "local workflow_presets.json must not exist before the sync"
        );

        run_local_shared_sync(&cfg).expect("shared sync must not error");

        assert_eq!(
            fs::read(&shared_presets_path).expect("read shared workflow presets"),
            shared_presets_before,
            "shared workflow_presets.json must stay byte-identical through local/shared sync"
        );
        assert!(
            !local_presets_path.exists(),
            "local/shared sync must not create a local workflow_presets.json from the shared copy"
        );
    });
}

#[test]
fn synced_device_provider_scan_reads_canonical_service_provider_state() {
    with_temp_dir("synced-provider-scan-canonical-state", |home| {
        let shared_root = home.join("shared-root");
        let cfg = config::StorageConfig {
            storage_type: "local".to_string(),
            local_storage_path: Some(shared_root.to_string_lossy().to_string()),
            ..config::StorageConfig::default()
        };
        write_test_file(
            &home.join(".config").join("onespace").join("config.json"),
            &serde_json::to_string(&cfg).unwrap(),
        );

        let device_path = home
            .join("shared-root")
            .join("shared")
            .join("remote-device")
            .join("data")
            .join("providers")
            .join("state.json");
        let state = ServiceProvidersState {
            active: HashMap::from([(
                "claude".to_string(),
                "11111111-1111-4111-8111-111111111111".to_string(),
            )]),
            active_opencode: vec![],
            providers: vec![ServiceProviderRecord {
                id: "11111111-1111-4111-8111-111111111111".to_string(),
                name: "Remote Claude".to_string(),
                tool: "claude".to_string(),
                api_key: String::new(),
                base_url: Some("https://remote.example.com/v1".to_string()),
                model: Some("remote-model".to_string()),
                ..ServiceProviderRecord::default()
            }],
        };
        let blob = CryptoService::encrypt_json(&serde_json::to_value(&state).unwrap()).unwrap();
        StorageEngine::write_json(&device_path, &blob).expect("write remote state");

        let devices = list_synced_device_providers().expect("list synced providers");

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, "remote-device");
        assert_eq!(devices[0].providers.len(), 1);
        assert_eq!(devices[0].providers[0].name, "Remote Claude");
        assert_eq!(
            devices[0].active.get("claude").map(String::as_str),
            Some("11111111-1111-4111-8111-111111111111")
        );
    });
}

#[test]
fn service_provider_import_rejects_old_core_provider_payload() {
    with_temp_dir("service-provider-import-rejects-old-core", |home| {
        let import_path = home.join("old-core-providers.json");
        write_test_file(
            &import_path,
            &json!({
                "providers": [{
                    "core": {
                        "id": "legacy-claude",
                        "name": "Legacy Claude",
                        "tool": "claude",
                        "api_key": "legacy-key"
                    }
                }]
            })
            .to_string(),
        );

        let err = service_providers_import_preview(import_path.to_string_lossy().to_string())
            .expect_err("old core payload should be rejected");

        assert_eq!(err.code, "invalid_payload");
    });
}
