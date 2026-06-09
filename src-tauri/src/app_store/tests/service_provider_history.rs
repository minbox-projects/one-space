    #[test]
    fn service_providers_upsert_changed_existing_appends_old_snapshot_history() {
        with_temp_dir("service-provider-history-upsert-changed", |_| {
            let provider_id = generate_provider_uuid();
            tauri::async_runtime::block_on(service_providers_upsert_inner(json!({
                "id": provider_id,
                "name": "Codex One",
                "tool": "codex",
                "api_key": "sk-old",
                "base_url": "https://old.example.com/v1",
                "model": "o3"
            })))
            .expect("create provider");

            tauri::async_runtime::block_on(service_providers_upsert_inner(json!({
                "id": provider_id,
                "name": "Codex One",
                "tool": "codex",
                "api_key": "sk-new",
                "base_url": "https://new.example.com/v1",
                "model": "o3"
            })))
            .expect("update provider");

            let state = load_service_providers_state().expect("load state");
            let provider = state
                .providers
                .iter()
                .find(|p| p.id == provider_id)
                .unwrap();
            assert_eq!(provider.history.len(), 1);
            assert_eq!(provider.history[0].action, "upsert");
            let snapshot = provider.history[0].snapshot.as_ref().expect("snapshot");
            assert_eq!(snapshot["api_key"], Value::String("sk-old".to_string()));
            assert_eq!(
                snapshot["base_url"],
                Value::String("https://old.example.com/v1".to_string())
            );
        });
    }

    #[test]
    fn service_providers_upsert_unchanged_existing_does_not_append_history() {
        with_temp_dir("service-provider-history-upsert-unchanged", |_| {
            let provider_id = generate_provider_uuid();
            let payload = json!({
                "id": provider_id,
                "name": "Gemini One",
                "tool": "gemini",
                "api_key": "sk-gemini",
                "base_url": "https://gemini.example.com/v1",
                "model": "gemini-pro"
            });
            tauri::async_runtime::block_on(service_providers_upsert_inner(payload.clone()))
                .expect("create provider");
            tauri::async_runtime::block_on(service_providers_upsert_inner(payload))
                .expect("unchanged update");

            let state = load_service_providers_state().expect("load state");
            let provider = state
                .providers
                .iter()
                .find(|p| p.id == provider_id)
                .unwrap();
            assert!(provider.history.is_empty());
        });
    }

    #[test]
    fn provider_history_keeps_five_entries_in_descending_order() {
        let mut state = ServiceProvidersState {
            active: HashMap::new(),
            providers: vec![ServiceProviderRecord {
                id: "p1".to_string(),
                name: "Provider 0".to_string(),
                tool: "codex".to_string(),
                api_key: "key-0".to_string(),
                ..ServiceProviderRecord::default()
            }],
        };

        for index in 1..=7 {
            let mut next = ServiceProviderRecord {
                id: "p1".to_string(),
                name: format!("Provider {index}"),
                tool: "codex".to_string(),
                api_key: format!("key-{index}"),
                ..ServiceProviderRecord::default()
            };
            let existing = state.providers[0].clone();
            append_provider_history_if_changed(Some(&existing), &mut next, "upsert");
            state.providers[0] = next;
        }

        let history = &state.providers[0].history;
        assert_eq!(history.len(), 5);
        assert!(history.windows(2).all(|items| items[0].ts >= items[1].ts));
        let retained_keys: HashSet<String> = history
            .iter()
            .filter_map(|entry| {
                entry
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.get("api_key"))
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
            })
            .collect();
        assert!(!retained_keys.contains("key-0"));
        assert!(!retained_keys.contains("key-1"));
        assert!(retained_keys.contains("key-6"));
    }

    #[test]
    fn service_provider_import_merge_appends_history_for_changed_existing() {
        let mut state = ServiceProvidersState {
            active: HashMap::new(),
            providers: vec![ServiceProviderRecord {
                id: "p1".to_string(),
                name: "Codex Old".to_string(),
                tool: "codex".to_string(),
                api_key: "old-key".to_string(),
                base_url: Some("https://old.example.com/v1".to_string()),
                ..ServiceProviderRecord::default()
            }],
        };

        merge_imported_service_provider(
            &mut state,
            ServiceProviderRecord {
                id: "p1".to_string(),
                name: "Codex New".to_string(),
                tool: "codex".to_string(),
                api_key: "new-key".to_string(),
                base_url: Some("https://new.example.com/v1".to_string()),
                ..ServiceProviderRecord::default()
            },
        );

        assert_eq!(state.providers[0].history.len(), 1);
        assert_eq!(state.providers[0].history[0].action, "import");
        assert_eq!(
            state.providers[0].history[0].snapshot.as_ref().unwrap()["name"],
            Value::String("Codex Old".to_string())
        );
    }

    #[test]
    fn favorite_and_env_managed_changes_do_not_append_provider_history() {
        let existing = ServiceProviderRecord {
            id: "p1".to_string(),
            name: "Codex".to_string(),
            tool: "codex".to_string(),
            api_key: "key".to_string(),
            favorite_at: None,
            env_managed: Some(true),
            ..ServiceProviderRecord::default()
        };
        let mut next = ServiceProviderRecord {
            favorite_at: Some(123),
            env_managed: Some(false),
            ..existing.clone()
        };

        let changed = append_provider_history_if_changed(Some(&existing), &mut next, "upsert");
        assert!(!changed);
        assert!(next.history.is_empty());
    }

    #[test]
    fn legacy_timestamp_content_history_entries_deserialize() {
        let history: Vec<ProviderHistoryEntry> = serde_json::from_value(json!([
            { "timestamp": 1_700_000_000_123u64, "content": "{\"name\":\"old\"}" }
        ]))
        .expect("deserialize legacy history");

        assert_eq!(history[0].ts, 1_700_000_000);
        assert_eq!(history[0].action, "update");
        assert_eq!(history[0].content.as_deref(), Some("{\"name\":\"old\"}"));
        assert!(history[0].snapshot.is_none());
    }

    #[test]
    fn service_provider_history_deserializes_when_ts_and_timestamp_both_exist() {
        let history: Vec<ProviderHistoryEntry> = serde_json::from_value(json!([
            { "ts": 1_700_000_000u64, "timestamp": 1_700_000_000_999u64, "action": "upsert" }
        ]))
        .expect("deserialize history with both timestamp fields");

        assert_eq!(history[0].ts, 1_700_000_000);
        assert_eq!(history[0].action, "upsert");
    }

    #[test]
    fn service_provider_history_snapshot_does_not_embed_history() {
        let provider = ServiceProviderRecord {
            id: "p1".to_string(),
            name: "Codex".to_string(),
            tool: "codex".to_string(),
            api_key: "key".to_string(),
            history: vec![ProviderHistoryEntry {
                ts: 1_700_000_000,
                action: "upsert".to_string(),
                snapshot: Some(json!({ "name": "Old" })),
                content: None,
                summary: None,
            }],
            ..ServiceProviderRecord::default()
        };

        let snapshot = service_provider_history_snapshot(&provider);
        assert!(snapshot.get("history").is_none());
    }
