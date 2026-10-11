use super::*;

/// A gateway-owned Claude service provider exactly as AI Gateway's Step 3
/// terminal sync builds it: native Anthropic Messages, the bare gateway origin
/// as `base_url`, the local key as the credential, the default local mapping id
/// as `model`, the three family local mapping ids in `claude_model_mappings`,
/// and the profile's `ai_gateway_gateway` marker.
#[allow(clippy::too_many_arguments)]
fn gateway_claude_record(
    id: &str,
    api_key: &str,
    base_url: &str,
    default_model: &str,
    haiku: &str,
    sonnet: &str,
    opus: &str,
    code: &str,
) -> ServiceProviderRecord {
    let mut tool_config = Map::new();
    tool_config.insert("ai_gateway_gateway".to_string(), Value::Bool(true));
    let mapping = |family: &str, display_name: &str, upstream_model: &str| ClaudeModelMapping {
        family: family.to_string(),
        display_name: display_name.to_string(),
        upstream_model: upstream_model.to_string(),
        supports_1m: Some(false),
        supported_capabilities: None,
    };
    ServiceProviderRecord {
        id: id.to_string(),
        name: "AI Gateway".to_string(),
        tool: "claude".to_string(),
        api_key: api_key.to_string(),
        base_url: Some(base_url.to_string()),
        model: Some(default_model.to_string()),
        claude_api_format: "anthropic_messages".to_string(),
        claude_connection_mode: "native_anthropic".to_string(),
        claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
        claude_model_mappings: vec![
            mapping("haiku", "Haiku", haiku),
            mapping("sonnet", "Sonnet", sonnet),
            mapping("opus", "Opus", opus),
        ],
        code: Some(code.to_string()),
        is_enabled: Some(true),
        env_managed: Some(true),
        tool_config,
        history: vec![],
        extra: Map::new(),
        ..ServiceProviderRecord::default()
    }
}

/// AC-014: the real Claude projector emits the gateway origin as
/// `ANTHROPIC_BASE_URL` (never a duplicated `/v1`), the local key as
/// `ANTHROPIC_API_KEY`, the default local id as `ANTHROPIC_MODEL` and each of
/// the three family local ids as its `ANTHROPIC_DEFAULT_*_MODEL` variable.
#[test]
fn claude_gateway_record_renders_origin_key_and_family_models() {
    with_temp_dir("claude-gateway-render-origin-models", |home| {
        let record = gateway_claude_record(
            "gateway-claude",
            "local-key-123",
            "http://127.0.0.1:19000",
            "gpt-4o",
            "claude-haiku-local",
            "claude-sonnet-local",
            "claude-opus-local",
            "gateway-claude-code",
        );

        let outputs =
            render_claude_to_dir(&record, &home.join(".claude")).expect("render claude");
        let rendered: Value =
            serde_json::from_str(&rendered_content(&outputs, ".claude/settings.json"))
                .expect("parse rendered");
        let env = rendered["env"].as_object().expect("env");

        assert_eq!(rendered["model"], Value::String("gpt-4o".to_string()));
        assert_eq!(env["ANTHROPIC_MODEL"], Value::String("gpt-4o".to_string()));
        assert_eq!(
            env["ANTHROPIC_BASE_URL"],
            Value::String("http://127.0.0.1:19000".to_string())
        );
        assert!(
            !env["ANTHROPIC_BASE_URL"]
                .as_str()
                .expect("base url")
                .contains("/v1"),
            "the gateway origin must never be doubled with /v1: {rendered}"
        );
        assert_eq!(
            env["ANTHROPIC_API_KEY"],
            Value::String("local-key-123".to_string())
        );
        assert_eq!(
            env["ANTHROPIC_DEFAULT_HAIKU_MODEL"],
            Value::String("claude-haiku-local".to_string())
        );
        assert_eq!(
            env["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            Value::String("claude-sonnet-local".to_string())
        );
        assert_eq!(
            env["ANTHROPIC_DEFAULT_OPUS_MODEL"],
            Value::String("claude-opus-local".to_string())
        );
    });
}

/// AC-014: a projection that cannot write must surface an error rather than
/// report a successful application. Pointing `HOME` at a regular file makes the
/// `~/.claude` directory uncreatable, so `apply_projection` must return `Err`.
#[test]
fn claude_projection_failure_is_surfaced_not_swallowed() {
    let _guard = crate::lock_test_home_env();
    let broken_home = std::env::temp_dir().join(format!(
        "onespace-claude-projection-failure-{}",
        uuid::Uuid::new_v4()
    ));
    fs::write(&broken_home, "not a directory").expect("write placeholder file");
    let original_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &broken_home);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let record = gateway_claude_record(
            "gateway-claude",
            "local-key-123",
            "http://127.0.0.1:19000",
            "gpt-4o",
            "claude-haiku-local",
            "claude-sonnet-local",
            "claude-opus-local",
            "gateway-claude-code",
        );
        let result = apply_projection(&record);
        assert!(
            result.is_err(),
            "an unwritable projection target must surface an error: {result:?}"
        );
    }));
    if let Some(home) = original_home {
        std::env::set_var("HOME", home);
    } else {
        std::env::remove_var("HOME");
    }
    let _ = fs::remove_file(&broken_home);
    if let Err(payload) = outcome {
        std::panic::resume_unwind(payload);
    }
}

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
        assert_eq!(provider.api_key, "import-key");
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
        assert_eq!(provider.model.as_deref(), Some("env-model"));
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
            &ServiceProviderRecord {
                id: "claude-custom".to_string(),
                name: "Claude Custom".to_string(),
                tool: "claude".to_string(),
                api_key: "render-key".to_string(),
                code: Some("claude-custom".to_string()),
                base_url: Some("https://example.com".to_string()),
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
                ..ServiceProviderRecord::default()
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
            &ServiceProviderRecord {
                id: "claude-custom".to_string(),
                name: "Claude Custom".to_string(),
                tool: "claude".to_string(),
                api_key: "render-key".to_string(),
                code: Some("claude-custom".to_string()),
                base_url: Some("https://example.com".to_string()),
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
                ..ServiceProviderRecord::default()
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
            &ServiceProviderRecord {
                id: "claude-custom".to_string(),
                name: "Claude Custom".to_string(),
                tool: "claude".to_string(),
                api_key: "render-key".to_string(),
                code: Some("claude-custom".to_string()),
                base_url: Some("https://example.com".to_string()),
                tool_config: Map::new(),
                history: vec![],
                extra: Map::new(),
                is_enabled: Some(true),
                ..ServiceProviderRecord::default()
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
