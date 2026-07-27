use super::*;
use serde_json::json;

#[test]
fn provider_presets_seed_once_and_delete_persists() {
    with_temp_dir("provider-presets-seed-delete", |_| {
        let first = load_service_provider_presets_state().expect("load default presets");
        assert!(first
            .presets
            .iter()
            .any(|preset| preset.endpoints.openai_base_url.is_some()));
        assert!(first
            .presets
            .iter()
            .any(|preset| preset.endpoints.anthropic_base_url.is_some()));
        let bailian = first
            .presets
            .iter()
            .find(|preset| preset.id == "alibaba-bailian")
            .expect("bailian preset");
        assert_eq!(bailian.icon.as_deref(), Some("builtin:bailian"));
        assert_eq!(
            bailian.endpoints.openai_base_url.as_deref(),
            Some("https://dashscope.aliyuncs.com/compatible-mode/v1")
        );
        let volcengine = first
            .presets
            .iter()
            .find(|preset| preset.id == "volcengine-ark")
            .expect("volcengine preset");
        assert_eq!(
            volcengine.endpoints.openai_base_url.as_deref(),
            Some("https://ark.cn-beijing.volces.com/api/v3")
        );
        assert_eq!(
            volcengine.endpoints.anthropic_base_url.as_deref(),
            Some("https://ark.cn-beijing.volces.com/api/compatible")
        );
        let deepseek = first
            .presets
            .iter()
            .find(|preset| preset.id == "deepseek")
            .expect("deepseek preset");
        assert_eq!(
            deepseek.endpoints.anthropic_base_url.as_deref(),
            Some("https://api.deepseek.com/anthropic")
        );
        let opencode_go = first
            .presets
            .iter()
            .find(|preset| preset.id == "opencode-go")
            .expect("opencode go preset");
        assert_eq!(opencode_go.icon.as_deref(), Some("builtin:opencode"));
        assert_eq!(
            opencode_go.endpoints.openai_base_url.as_deref(),
            Some("https://opencode.ai/zen/go/v1")
        );
        assert_eq!(
            opencode_go.endpoints.anthropic_base_url.as_deref(),
            Some("https://opencode.ai/zen/go/v1")
        );

        let mut next = first.clone();
        next.presets.retain(|preset| preset.id != "openai");
        save_service_provider_presets_state(&next).expect("save edited presets");

        let reloaded = load_service_provider_presets_state().expect("reload presets");
        assert!(!reloaded.presets.iter().any(|preset| preset.id == "openai"));
    });
}

#[test]
fn provider_presets_backfills_new_builtin_presets_once() {
    with_temp_dir("provider-presets-backfill-builtins", |_| {
        let old_state = ServiceProviderPresetsState {
            builtin_seed_version: 1,
            presets: vec![ServiceProviderPresetRecord {
                id: "deepseek".to_string(),
                name: "DeepSeek".to_string(),
                icon: Some("builtin:deepseek".to_string()),
                endpoints: ServiceProviderPresetEndpoints {
                    openai_base_url: Some("https://api.deepseek.com".to_string()),
                    anthropic_base_url: None,
                    gemini_base_url: None,
                },
                created_at: 1,
                updated_at: 1,
                ..ServiceProviderPresetRecord::default()
            }],
        };
        StorageEngine::write_json(&StorageEngine::provider_presets_path().unwrap(), &old_state)
            .expect("write old presets");

        let upgraded = load_service_provider_presets_state().expect("load upgraded presets");
        assert_eq!(upgraded.builtin_seed_version, 2);
        assert!(upgraded
            .presets
            .iter()
            .any(|preset| preset.id == "alibaba-bailian"));
        assert!(upgraded
            .presets
            .iter()
            .any(|preset| preset.id == "volcengine-ark"));
        assert!(upgraded
            .presets
            .iter()
            .any(|preset| preset.id == "opencode-go"));
        assert_eq!(
            upgraded
                .presets
                .iter()
                .find(|preset| preset.id == "deepseek")
                .and_then(|preset| preset.endpoints.anthropic_base_url.as_deref()),
            Some("https://api.deepseek.com/anthropic")
        );

        let mut deleted = upgraded.clone();
        deleted.presets.retain(|preset| preset.id != "opencode-go");
        save_service_provider_presets_state(&deleted).expect("save deleted preset");
        let reloaded = load_service_provider_presets_state().expect("reload presets");
        assert!(!reloaded
            .presets
            .iter()
            .any(|preset| preset.id == "opencode-go"));
    });
}

#[test]
fn provider_presets_upsert_sanitizes_template_fields() {
    with_temp_dir("provider-presets-upsert-sanitize", |_| {
        let mut template = Map::new();
        template.insert("api_key".to_string(), Value::String("secret".to_string()));
        template.insert(
            "code".to_string(),
            Value::String("profile-code".to_string()),
        );
        template.insert(
            "provider_key".to_string(),
            Value::String("opencode-key".to_string()),
        );
        template.insert("favorite_at".to_string(), Value::Number(1.into()));
        template.insert("model".to_string(), Value::String("gpt-4.1".to_string()));

        let preset = sanitize_provider_preset(
            ServiceProviderPresetRecord {
                id: "vendor".to_string(),
                name: "Vendor".to_string(),
                endpoints: ServiceProviderPresetEndpoints {
                    openai_base_url: Some("https://vendor.example/v1".to_string()),
                    anthropic_base_url: Some("https://anthropic.vendor.example".to_string()),
                    gemini_base_url: None,
                },
                template,
                ..ServiceProviderPresetRecord::default()
            },
            None,
        )
        .expect("sanitize preset");

        assert!(!preset.template.contains_key("api_key"));
        assert!(!preset.template.contains_key("code"));
        assert!(!preset.template.contains_key("provider_key"));
        assert!(!preset.template.contains_key("favorite_at"));
        assert_eq!(
            preset.template.get("model").and_then(Value::as_str),
            Some("gpt-4.1")
        );
    });
}

#[test]
fn provider_presets_sanitizes_claude_template_fields() {
    with_temp_dir("provider-presets-claude-template-sanitize", |_| {
        let mut template = Map::new();
        template.insert(
            "claude_default_model".to_string(),
            Value::String(" claude-sonnet-4-5 ".to_string()),
        );
        template.insert(
            "claude_reasoning_effort".to_string(),
            Value::String(" high ".to_string()),
        );
        template.insert(
            "claude_model_mappings".to_string(),
            json!([
                {
                    "family": " haiku ",
                    "display_name": " Haiku ",
                    "upstream_model": " claude-haiku-4-5 ",
                    "supports_1m": false,
                    "supported_capabilities": [" image ", "", 7]
                },
                {
                    "family": "sonnet",
                    "display_name": "Sonnet",
                    "upstream_model": "   ",
                    "supports_1m": false,
                    "supported_capabilities": []
                },
                "invalid"
            ]),
        );
        template.insert("api_key".to_string(), Value::String("secret".to_string()));

        let preset = sanitize_provider_preset(
            ServiceProviderPresetRecord {
                id: "vendor".to_string(),
                name: "Vendor".to_string(),
                template,
                ..ServiceProviderPresetRecord::default()
            },
            None,
        )
        .expect("sanitize preset");

        assert!(!preset.template.contains_key("api_key"));
        assert_eq!(
            preset
                .template
                .get("claude_default_model")
                .and_then(Value::as_str),
            Some("claude-sonnet-4-5")
        );
        assert_eq!(
            preset
                .template
                .get("claude_reasoning_effort")
                .and_then(Value::as_str),
            Some("high")
        );
        let mappings = preset
            .template
            .get("claude_model_mappings")
            .and_then(Value::as_array)
            .expect("claude mappings");
        assert_eq!(mappings.len(), 1);
        assert_eq!(
            mappings[0].get("family").and_then(Value::as_str),
            Some("haiku")
        );
        assert_eq!(
            mappings[0].get("display_name").and_then(Value::as_str),
            Some("Haiku")
        );
        assert_eq!(
            mappings[0].get("upstream_model").and_then(Value::as_str),
            Some("claude-haiku-4-5")
        );
        assert_eq!(
            mappings[0]
                .get("supported_capabilities")
                .and_then(Value::as_array)
                .and_then(|values| values.first())
                .and_then(Value::as_str),
            Some("image")
        );
    });
}

#[test]
fn provider_presets_removes_invalid_or_empty_claude_mappings() {
    with_temp_dir("provider-presets-empty-claude-mappings", |_| {
        for mappings in [
            Value::String("invalid".to_string()),
            json!([{ "family": "haiku", "display_name": "Haiku", "upstream_model": " " }]),
        ] {
            let mut template = Map::new();
            template.insert("claude_model_mappings".to_string(), mappings);
            let preset = sanitize_provider_preset(
                ServiceProviderPresetRecord {
                    id: "vendor".to_string(),
                    name: "Vendor".to_string(),
                    template,
                    ..ServiceProviderPresetRecord::default()
                },
                None,
            )
            .expect("sanitize preset");

            assert!(!preset.template.contains_key("claude_model_mappings"));
        }
    });
}

#[test]
fn provider_sync_exports_provider_presets_when_enabled() {
    with_temp_dir("provider-presets-sync-export", |home| {
        let mut cfg = config::StorageConfig::default();
        cfg.storage_type = "local".to_string();
        cfg.local_storage_path = Some(home.join("shared-root").to_string_lossy().to_string());
        cfg.sync_policy = config::SyncPolicy {
            providers: true,
            mcp: false,
            content: false,
            workflow_presets: false,
            skills_sources: false,
            skills_repository: false,
            subagents_sources: false,
            subagents_repository: false,
            ai_news: false,
        };

        let mut state = ServiceProviderPresetsState::default();
        state.builtin_seed_version = 2;
        state.presets.push(ServiceProviderPresetRecord {
            id: "vendor".to_string(),
            name: "Vendor".to_string(),
            endpoints: ServiceProviderPresetEndpoints {
                openai_base_url: Some("https://vendor.example/v1".to_string()),
                anthropic_base_url: Some("https://anthropic.vendor.example".to_string()),
                gemini_base_url: None,
            },
            created_at: 1,
            updated_at: 1,
            ..ServiceProviderPresetRecord::default()
        });
        save_service_provider_presets_state(&state).expect("save presets");

        run_local_shared_sync(&cfg).expect("run sync");

        let shared_path = shared_profile_path(&cfg, "provider_presets.json")
            .expect("shared provider presets path");
        let shared: ServiceProviderPresetsState =
            StorageEngine::read_json(&shared_path).expect("read shared presets");
        assert_eq!(shared.presets.len(), 1);
        assert_eq!(shared.presets[0].id, "vendor");
        assert_eq!(
            shared.presets[0].endpoints.anthropic_base_url.as_deref(),
            Some("https://anthropic.vendor.example")
        );
    });
}
