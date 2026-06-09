use super::*;

#[test]
fn codex_projection_preserves_login_auth_and_uses_model_provider() {
    with_temp_dir("codex-projection-login-preserve", |home| {
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");
        write_test_file(
            &codex_dir.join("auth.json"),
            r#"{
  "OPENAI_API_KEY": "old-key",
  "tokens": {"id_token": "login-token"},
  "account_id": "acct_123"
}"#,
        );
        write_test_file(
            &codex_dir.join("config.toml"),
            r#"preferred_auth_method = "login"
model = "old-model"
model_provider = "ollama_lan"

[model_providers.ollama_lan]
name = "Ollama LAN"
base_url = "http://127.0.0.1:11434/v1"
wire_api = "responses"
"#,
        );

        let provider = codex_provider(
            "work-openai",
            "Work OpenAI",
            "new-key",
            "https://proxy.example.com/v1",
            "gpt-5.5",
        );
        let outputs = render_codex_at_home(&provider, home).expect("render codex");
        let auth: Value = serde_json::from_str(&rendered_content(&outputs, ".codex/auth.json"))
            .expect("parse auth");
        assert_eq!(
            auth.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
            Some("new-key")
        );
        assert_eq!(
            auth.pointer("/tokens/id_token").and_then(|v| v.as_str()),
            Some("login-token")
        );
        assert_eq!(
            auth.get("account_id").and_then(|v| v.as_str()),
            Some("acct_123")
        );

        let doc = rendered_content(&outputs, ".codex/config.toml")
            .parse::<toml_edit::DocumentMut>()
            .expect("parse toml");
        assert!(doc.get("preferred_auth_method").is_none());
        assert_eq!(
            doc.get("forced_login_method").and_then(|v| v.as_str()),
            Some("api")
        );
        assert_eq!(doc.get("model").and_then(|v| v.as_str()), Some("gpt-5.5"));
        assert_eq!(
            doc.get("model_provider").and_then(|v| v.as_str()),
            Some("onespace_work_openai")
        );
        assert!(doc
            .get("model_providers")
            .and_then(|v| v.as_table())
            .and_then(|table| table.get("ollama_lan"))
            .is_some());
        let onespace = doc
            .get("model_providers")
            .and_then(|v| v.as_table())
            .and_then(|table| table.get("onespace_work_openai"))
            .and_then(|v| v.as_table())
            .expect("onespace provider table");
        assert_eq!(
            onespace.get("base_url").and_then(|v| v.as_str()),
            Some("https://proxy.example.com/v1")
        );
        assert_eq!(
            onespace
                .get("requires_openai_auth")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    });
}

#[test]
fn codex_projection_switches_model_provider_without_deleting_old_provider() {
    with_temp_dir("codex-projection-switch", |home| {
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");
        write_test_file(
            &codex_dir.join("config.toml"),
            r#"[model_providers.user_provider]
name = "User Provider"
base_url = "https://user.example.com/v1"
"#,
        );
        write_test_file(
            &codex_dir.join("auth.json"),
            r#"{"tokens":{"id_token":"login-token"}}"#,
        );

        let first = codex_provider(
            "provider-a",
            "Provider A",
            "key-a",
            "https://a.example.com/v1",
            "gpt-5.4",
        );
        let first_outputs = render_codex_at_home(&first, home).expect("render first");
        write_test_file(
            &codex_dir.join("config.toml"),
            &rendered_content(&first_outputs, ".codex/config.toml"),
        );
        write_test_file(
            &codex_dir.join("auth.json"),
            &rendered_content(&first_outputs, ".codex/auth.json"),
        );

        let second = codex_provider(
            "provider-b",
            "Provider B",
            "key-b",
            "https://b.example.com/v1",
            "gpt-5.5",
        );
        let second_outputs = render_codex_at_home(&second, home).expect("render second");
        let auth: Value =
            serde_json::from_str(&rendered_content(&second_outputs, ".codex/auth.json"))
                .expect("parse auth");
        assert_eq!(
            auth.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
            Some("key-b")
        );
        assert_eq!(
            auth.pointer("/tokens/id_token").and_then(|v| v.as_str()),
            Some("login-token")
        );

        let doc = rendered_content(&second_outputs, ".codex/config.toml")
            .parse::<toml_edit::DocumentMut>()
            .expect("parse toml");
        let providers = doc
            .get("model_providers")
            .and_then(|v| v.as_table())
            .expect("model providers table");
        assert!(providers.get("user_provider").is_some());
        assert!(providers.get("onespace_provider_a").is_some());
        assert!(providers.get("onespace_provider_b").is_some());
        assert_eq!(
            doc.get("model_provider").and_then(|v| v.as_str()),
            Some("onespace_provider_b")
        );
        assert_eq!(doc.get("model").and_then(|v| v.as_str()), Some("gpt-5.5"));
    });
}

#[test]
fn codex_unmanaged_reset_only_removes_onespace_provider_and_api_key() {
    with_temp_dir("codex-reset-unmanaged", |home| {
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");
        write_test_file(
            &codex_dir.join("auth.json"),
            r#"{
  "OPENAI_API_KEY": "key",
  "tokens": {"id_token": "login-token"}
}"#,
        );
        write_test_file(
            &codex_dir.join("config.toml"),
            r#"forced_login_method = "api"
model = "gpt-5.5"
model_provider = "onespace_provider_a"

[model_providers.user_provider]
name = "User Provider"
base_url = "https://user.example.com/v1"

[model_providers.onespace_provider_a]
name = "Provider A"
base_url = "https://a.example.com/v1"
wire_api = "responses"
"#,
        );

        let outputs = render_codex_reset_to_unmanaged_at_home(home).expect("render reset");
        let auth: Value = serde_json::from_str(&rendered_content(&outputs, ".codex/auth.json"))
            .expect("parse auth");
        assert!(auth.get("OPENAI_API_KEY").is_none());
        assert_eq!(
            auth.pointer("/tokens/id_token").and_then(|v| v.as_str()),
            Some("login-token")
        );

        let doc = rendered_content(&outputs, ".codex/config.toml")
            .parse::<toml_edit::DocumentMut>()
            .expect("parse toml");
        assert!(doc.get("forced_login_method").is_none());
        assert!(doc.get("model").is_none());
        assert!(doc.get("model_provider").is_none());
        let providers = doc
            .get("model_providers")
            .and_then(|v| v.as_table())
            .expect("model providers table");
        assert!(providers.get("user_provider").is_some());
        assert!(providers.get("onespace_provider_a").is_none());
    });
}

#[test]
fn codex_system_import_reads_active_model_provider_table() {
    with_temp_dir("codex-system-import-model-provider", |home| {
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");
        write_test_file(
            &codex_dir.join("auth.json"),
            r#"{"OPENAI_API_KEY":"import-key"}"#,
        );
        write_test_file(
            &codex_dir.join("config.toml"),
            r#"forced_login_method = "api"
model = "gpt-5.5"
model_provider = "onespace_imported"

[model_providers.onespace_imported]
name = "Imported"
base_url = "https://import.example.com/v1"
wire_api = "responses"
"#,
        );

        let provider = read_system_provider_at_home("codex", home).expect("system provider");
        assert_eq!(provider.core.api_key, "import-key");
        assert_eq!(provider.core.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(
            provider.core.base_url.as_deref(),
            Some("https://import.example.com/v1")
        );
        assert_eq!(
            provider
                .tool_config
                .get("codex_auth_mode")
                .and_then(|v| v.as_str()),
            Some("api")
        );
        assert_eq!(
            provider
                .tool_config
                .get("wire_api")
                .and_then(|v| v.as_str()),
            Some("responses")
        );
    });
}
