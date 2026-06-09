    #[test]
    fn claude_system_import_reads_supported_capabilities_and_effort() {
        with_temp_dir("claude-system-import-capabilities", |home| {
            let claude_dir = home.join(".claude");
            fs::create_dir_all(&claude_dir).expect("create claude dir");
            write_test_file(
                &claude_dir.join("settings.json"),
                r#"{
  "env": {
    "ANTHROPIC_API_KEY": "import-key",
    "ANTHROPIC_BASE_URL": "https://example.com",
    "ANTHROPIC_MODEL": "claude-sonnet-4-5[1m]",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-4-5[1m]",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "Sonnet",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES": "image,pdfs",
    "CLAUDE_CODE_EFFORT_LEVEL": "max"
  }
}"#,
            );

            let provider = read_system_provider_at_home("claude", home).expect("system provider");
            assert_eq!(provider.core.api_key, "import-key");
            assert_eq!(
                provider
                    .tool_config
                    .get("claude_reasoning_effort")
                    .and_then(|v| v.as_str()),
                Some("max")
            );

            let mappings: Vec<ClaudeModelMapping> = serde_json::from_value(
                provider
                    .tool_config
                    .get("claude_model_mappings")
                    .cloned()
                    .expect("claude model mappings"),
            )
            .expect("parse mappings");
            let sonnet = mappings
                .iter()
                .find(|mapping| mapping.family == "sonnet")
                .expect("sonnet mapping");
            assert_eq!(sonnet.upstream_model, "claude-sonnet-4-5");
            assert_eq!(sonnet.supports_1m, Some(true));
            assert_eq!(sonnet.display_name, "Sonnet");
            assert_eq!(
                sonnet.supported_capabilities.as_ref(),
                Some(&vec!["image".to_string(), "pdfs".to_string()])
            );
        });
    }

    #[test]
    fn claude_system_import_prefers_env_default_model_over_top_level_model() {
        with_temp_dir("claude-system-import-default-model-priority", |home| {
            let claude_dir = home.join(".claude");
            fs::create_dir_all(&claude_dir).expect("create claude dir");
            write_test_file(
                &claude_dir.join("settings.json"),
                r#"{
  "model": "top-level-model",
  "env": {
    "ANTHROPIC_API_KEY": "import-key",
    "ANTHROPIC_MODEL": "env-model"
  }
}"#,
            );

            let provider = read_system_provider_at_home("claude", home).expect("system provider");
            assert_eq!(provider.core.model.as_deref(), Some("env-model"));
            assert_eq!(
                provider
                    .tool_config
                    .get("claude_default_model")
                    .and_then(|v| v.as_str()),
                Some("env-model")
            );
        });
    }

    #[test]
    fn render_claude_to_dir_writes_supported_capabilities_and_selected_effort() {
        with_temp_dir("claude-render-capabilities", |home| {
            let outputs = render_claude_to_dir(
                &ProviderRecord {
                    core: ProviderCore {
                        id: "claude-custom".to_string(),
                        name: "Claude Custom".to_string(),
                        tool: "claude".to_string(),
                        api_key: "render-key".to_string(),
                        code: Some("claude-custom".to_string()),
                        base_url: Some("https://example.com".to_string()),
                        model: None,
                    },
                    runtime_policy: ProviderRuntimePolicy::default(),
                    favorite_at: None,
                    tool_config: serde_json::from_str(
                        r#"{
                            "claude_default_model": "claude-sonnet-4-5[1m]",
                            "claude_reasoning_effort": "auto",
                            "claude_model_mappings": [
                                {
                                    "family": "haiku",
                                    "display_name": "Haiku",
                                    "upstream_model": "claude-haiku-4-5",
                                    "supported_capabilities": ["prompt-cache"]
                                },
                                {
                                    "family": "sonnet",
                                    "display_name": "Sonnet",
                                    "upstream_model": "claude-sonnet-4-5",
                                    "supports_1m": true,
                                    "supported_capabilities": ["image", "pdfs"]
                                }
                            ]
                        }"#,
                    )
                    .unwrap(),
                    history: vec![],
                    extra: Map::new(),
                    is_enabled: Some(true),
                    provider_key: None,
                },
                &home.join(".claude"),
            )
            .expect("render claude");

            let rendered: Value =
                serde_json::from_str(&rendered_content(&outputs, ".claude/settings.json"))
                    .expect("parse rendered");
            let env = rendered["env"].as_object().expect("env");
            assert_eq!(
                rendered["model"],
                Value::String("claude-sonnet-4-5[1m]".to_string())
            );
            assert_eq!(
                env["CLAUDE_CODE_EFFORT_LEVEL"],
                Value::String("auto".to_string())
            );
            assert_eq!(
                env["ANTHROPIC_DEFAULT_HAIKU_MODEL_SUPPORTED_CAPABILITIES"],
                Value::String("prompt-cache".to_string())
            );
            assert_eq!(
                env["ANTHROPIC_DEFAULT_SONNET_MODEL"],
                Value::String("claude-sonnet-4-5[1m]".to_string())
            );
            assert_eq!(
                env["ANTHROPIC_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES"],
                Value::String("image,pdfs".to_string())
            );
        });
    }

    #[test]
    fn render_claude_to_dir_ignores_legacy_mapping_reasoning_effort() {
        with_temp_dir("claude-render-ignores-legacy-mapping-effort", |home| {
            let outputs = render_claude_to_dir(
                &ProviderRecord {
                    core: ProviderCore {
                        id: "claude-custom".to_string(),
                        name: "Claude Custom".to_string(),
                        tool: "claude".to_string(),
                        api_key: "render-key".to_string(),
                        code: Some("claude-custom".to_string()),
                        base_url: Some("https://example.com".to_string()),
                        model: None,
                    },
                    runtime_policy: ProviderRuntimePolicy::default(),
                    favorite_at: None,
                    tool_config: serde_json::from_str(
                        r#"{
                            "claude_default_model": "claude-sonnet-4-5[1m]",
                            "claude_reasoning_effort": "auto",
                            "claude_model_mappings": [
                                {
                                    "family": "sonnet",
                                    "display_name": "Sonnet",
                                    "upstream_model": "claude-sonnet-4-5",
                                    "supports_1m": true,
                                    "reasoning_effort": "xhigh"
                                }
                            ]
                        }"#,
                    )
                    .unwrap(),
                    history: vec![],
                    extra: Map::new(),
                    is_enabled: Some(true),
                    provider_key: None,
                },
                &home.join(".claude"),
            )
            .expect("render claude");

            let rendered: Value =
                serde_json::from_str(&rendered_content(&outputs, ".claude/settings.json"))
                    .expect("parse rendered");
            let env = rendered["env"].as_object().expect("env");
            assert_eq!(
                env["CLAUDE_CODE_EFFORT_LEVEL"],
                Value::String("auto".to_string())
            );
        });
    }

    #[test]
    fn render_claude_to_dir_removes_top_level_and_env_model_when_default_is_empty() {
        with_temp_dir("claude-render-removes-empty-default-model", |home| {
            let claude_dir = home.join(".claude");
            fs::create_dir_all(&claude_dir).expect("create claude dir");
            write_test_file(
                &claude_dir.join("settings.json"),
                r#"{
  "model": "old-model",
  "env": {
    "ANTHROPIC_API_KEY": "render-key",
    "ANTHROPIC_MODEL": "old-model"
  }
}"#,
            );

            let outputs = render_claude_to_dir(
                &ProviderRecord {
                    core: ProviderCore {
                        id: "claude-custom".to_string(),
                        name: "Claude Custom".to_string(),
                        tool: "claude".to_string(),
                        api_key: "render-key".to_string(),
                        code: Some("claude-custom".to_string()),
                        base_url: Some("https://example.com".to_string()),
                        model: None,
                    },
                    runtime_policy: ProviderRuntimePolicy::default(),
                    favorite_at: None,
                    tool_config: Map::new(),
                    history: vec![],
                    extra: Map::new(),
                    is_enabled: Some(true),
                    provider_key: None,
                },
                &claude_dir,
            )
            .expect("render claude");

            let rendered: Value =
                serde_json::from_str(&rendered_content(&outputs, ".claude/settings.json"))
                    .expect("parse rendered");
            assert!(rendered.get("model").is_none());
            assert!(rendered["env"]
                .as_object()
                .expect("env")
                .get("ANTHROPIC_MODEL")
                .is_none());
        });
    }
