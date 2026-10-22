//! Real native-rendering behavior for synchronized model context windows.
//!
//! Coverage traceability: REQ-004/REQ-005, AC-005 through AC-008. These tests
//! use the real projectors and isolated temporary homes; they never read or
//! modify the operator's actual terminal configuration.

use super::*;
use crate::claude_profiles::materialize_claude_settings_sp;
use serde_json::json;

fn opencode_record(models: Value) -> ServiceProviderRecord {
    ServiceProviderRecord {
        id: "gateway-open".to_string(),
        name: "AI Gateway".to_string(),
        tool: "opencode".to_string(),
        api_key: "local-key-123".to_string(),
        base_url: Some("http://127.0.0.1:19000/v1".to_string()),
        provider_key: Some("gateway".to_string()),
        tool_config: json!({
            "ai_gateway_gateway": true,
            "npm": "@ai-sdk/openai-compatible",
            "options": {
                "baseURL": "http://127.0.0.1:19000/v1",
                "apiKey": "local-key-123"
            },
            "models": models
        })
        .as_object()
        .cloned()
        .expect("object fixture"),
        ..ServiceProviderRecord::default()
    }
}

fn generated_models() -> Value {
    json!({
        "gpt-4o": {
            "name": "GPT-4o",
            "limit": { "context": 128000, "output": 0 },
            "variants": {
                "low": { "reasoningEffort": "low" },
                "high": { "reasoningEffort": "high" }
            }
        },
        "unknown-model": { "name": "Unknown" }
    })
}

fn codex_record(window: Value) -> ServiceProviderRecord {
    ServiceProviderRecord {
        id: "gateway-codex".to_string(),
        name: "AI Gateway".to_string(),
        tool: "codex".to_string(),
        api_key: "local-key-123".to_string(),
        base_url: Some("http://127.0.0.1:19000/v1".to_string()),
        model: Some("gpt-4o".to_string()),
        tool_config: json!({
            "ai_gateway_gateway": true,
            "wire_api": "chat",
            "model_context_window": window
        })
        .as_object()
        .cloned()
        .expect("object fixture"),
        ..ServiceProviderRecord::default()
    }
}

fn claude_record(threshold: Value) -> ServiceProviderRecord {
    let mapping = |family: &str, model: &str| ClaudeModelMapping {
        family: family.to_string(),
        display_name: family.to_string(),
        upstream_model: model.to_string(),
        supports_1m: Some(false),
        supported_capabilities: None,
    };
    ServiceProviderRecord {
        id: "gateway-claude".to_string(),
        name: "AI Gateway".to_string(),
        tool: "claude".to_string(),
        api_key: "local-key-123".to_string(),
        base_url: Some("http://127.0.0.1:19000".to_string()),
        model: Some("gpt-4o".to_string()),
        code: Some("gateway-claude-code".to_string()),
        claude_api_format: "anthropic_messages".to_string(),
        claude_connection_mode: "native_anthropic".to_string(),
        claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
        claude_model_mappings: vec![
            mapping("haiku", "claude-haiku-local"),
            mapping("sonnet", "claude-sonnet-local"),
            mapping("opus", "claude-opus-local"),
        ],
        tool_config: json!({
            "ai_gateway_gateway": true,
            "claude_auto_compact_window": threshold
        })
        .as_object()
        .cloned()
        .expect("object fixture"),
        ..ServiceProviderRecord::default()
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read rendered file"))
        .expect("parse rendered JSON")
}

fn read_toml(path: &Path) -> toml_edit::DocumentMut {
    fs::read_to_string(path)
        .expect("read rendered TOML")
        .parse::<toml_edit::DocumentMut>()
        .expect("parse rendered TOML")
}

fn upsert_provider(value: Value) -> String {
    let response = tauri::async_runtime::block_on(service_providers_upsert_inner(value))
        .expect("canonical provider upsert");
    response
        .data
        .get("id")
        .and_then(Value::as_str)
        .expect("provider id")
        .to_string()
}

fn stored_provider(id: &str) -> ServiceProviderRecord {
    load_service_providers_state()
        .expect("load service provider state")
        .providers
        .into_iter()
        .find(|provider| provider.id == id)
        .expect("stored provider")
}

// ---------------------------------------------------------------------------
// OpenCode
// ---------------------------------------------------------------------------

#[test]
fn opencode_real_projection_writes_limits_and_preserves_unrelated_providers() {
    with_temp_dir("ctx-opencode-rendered", |home| {
        let path = home.join(".config").join("opencode").join("opencode.json");
        write_test_file(
            &path,
            r#"{
  "provider": {
    "unrelated": { "name": "Unrelated", "models": { "x": { "name": "X" } } },
    "gateway": { "name": "Old Gateway", "models": { "gpt-4o": { "name": "Old" } } }
  }
}"#,
        );

        let record = opencode_record(generated_models());
        apply_projection(&record).expect("apply OpenCode projection");
        let rendered = read_json(&path);
        let gateway = &rendered["provider"]["gateway"];

        assert_eq!(
            gateway["models"]["gpt-4o"]["limit"]["context"], 128000,
            "the real renderer must write the generated numeric context: {rendered}"
        );
        assert_eq!(
            gateway["models"]["gpt-4o"]["limit"]["output"], 0,
            "the real renderer must write output zero: {rendered}"
        );
        assert!(
            gateway["models"]["unknown-model"].get("limit").is_none(),
            "an unknown model must omit the limit: {rendered}"
        );
        assert_eq!(
            rendered["provider"]["unrelated"]["name"], "Unrelated",
            "unrelated providers must survive: {rendered}"
        );
        assert_eq!(
            gateway["models"]["gpt-4o"]["variants"]["high"]["reasoningEffort"], "high",
            "reasoning variants must survive: {rendered}"
        );
        assert_eq!(
            gateway["options"]["apiKey"], "local-key-123",
            "the owned credential stays in place: {rendered}"
        );
    });
}

#[test]
fn opencode_real_projection_clears_a_removed_limit_and_keeps_profiles_separate() {
    with_temp_dir("ctx-opencode-clear-profiles", |home| {
        let path = home.join(".config").join("opencode").join("opencode.json");
        write_test_file(
            &path,
            r#"{
  "provider": {
    "gateway": {
      "name": "Old Gateway",
      "models": { "gpt-4o": { "name": "GPT-4o", "limit": { "context": 1, "output": 0 } } }
    }
  }
}"#,
        );

        let cleared = opencode_record(json!({ "gpt-4o": { "name": "GPT-4o" } }));
        apply_projection(&cleared).expect("apply cleared OpenCode projection");
        let rendered = read_json(&path);
        assert!(
            rendered["provider"]["gateway"]["models"]["gpt-4o"]
                .get("limit")
                .is_none(),
            "a cleared value must remove the previously rendered limit: {rendered}"
        );

        apply_projection(&opencode_record(generated_models()))
            .expect("apply release OpenCode projection");

        let mut dev = opencode_record(json!({
            "gpt-4o": {
                "name": "GPT-4o",
                "limit": { "context": 64000, "output": 0 }
            }
        }));
        dev.name = "AI Gateway (Dev)".to_string();
        dev.provider_key = Some("gateway-dev".to_string());
        dev.tool_config
            .insert("ai_gateway_gateway".to_string(), Value::String("dev".to_string()));
        apply_projection(&dev).expect("apply dev OpenCode projection");

        let rendered = read_json(&path);
        assert_eq!(
            rendered["provider"]["gateway"]["models"]["gpt-4o"]["limit"]["context"], 128000,
            "the release record must remain separate: {rendered}"
        );
        assert_eq!(
            rendered["provider"]["gateway-dev"]["models"]["gpt-4o"]["limit"]["context"], 64000,
            "the dev record must carry its own limit: {rendered}"
        );
        assert_eq!(rendered["provider"]["gateway-dev"]["name"], "AI Gateway (Dev)");
    });
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

#[test]
fn codex_canonical_upsert_registers_without_activating_or_writing_native_config() {
    with_temp_dir("ctx-codex-register-only", |home| {
        let id = upsert_provider(json!({
            "id": "codex-context",
            "name": "AI Gateway",
            "tool": "codex",
            "api_key": "local-key-123",
            "base_url": "http://127.0.0.1:19000/v1",
            "model": "gpt-4o",
            "tool_config": {
                "ai_gateway_gateway": true,
                "wire_api": "chat",
                "model_context_window": 256000
            }
        }));
        let record = stored_provider(&id);
        assert_eq!(
            record.tool_config.get("model_context_window"),
            Some(&json!(256000)),
            "the selected-model scalar must be registered in canonical storage"
        );
        assert!(
            !home.join(".codex").join("config.toml").exists(),
            "registration alone must not write or activate native Codex configuration"
        );
    });
}

#[test]
fn codex_projection_writes_top_level_integer_and_preserves_unrelated_toml() {
    with_temp_dir("ctx-codex-projection", |home| {
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");
        write_test_file(
            &codex_dir.join("config.toml"),
            r#"model = "old-model"
model_provider = "user_provider"
model_reasoning_effort = "high"

[model_providers.user_provider]
name = "User Provider"
base_url = "https://user.example.com/v1"
"#,
        );

        let record = codex_record(json!(128000));
        apply_projection(&record).expect("apply Codex projection");
        let doc = read_toml(&codex_dir.join("config.toml"));

        assert_eq!(
            doc.get("model_context_window").and_then(|v| v.as_integer()),
            Some(128000),
            "Codex receives a top-level integer on activation/projection: {doc}"
        );
        assert_eq!(
            doc.get("model_reasoning_effort").and_then(|v| v.as_str()),
            Some("high"),
            "unrelated model settings must survive: {doc}"
        );
        assert!(
            doc.get("model_providers")
                .and_then(|v| v.as_table())
                .and_then(|table| table.get("user_provider"))
                .is_some(),
            "unrelated provider tables must survive: {doc}"
        );
    });
}

#[test]
fn codex_null_survives_the_real_canonical_merge_and_clears_the_native_scalar() {
    with_temp_dir("ctx-codex-null-merge", |home| {
        let id = upsert_provider(json!({
            "id": "codex-context",
            "name": "AI Gateway",
            "tool": "codex",
            "api_key": "local-key-123",
            "base_url": "http://127.0.0.1:19000/v1",
            "model": "gpt-4o",
            "tool_config": {
                "ai_gateway_gateway": true,
                "wire_api": "chat",
                "model_context_window": 128000
            }
        }));

        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");
        write_test_file(
            &codex_dir.join("config.toml"),
            "model_context_window = 128000\nmodel = \"gpt-4o\"\n",
        );

        let _ = upsert_provider(json!({
            "id": id,
            "name": "AI Gateway",
            "tool": "codex",
            "api_key": "local-key-123",
            "base_url": "http://127.0.0.1:19000/v1",
            "model": "gpt-4o",
            "tool_config": {
                "model_context_window": null
            }
        }));
        let record = stored_provider(&id);
        assert_eq!(
            record.tool_config.get("model_context_window"),
            Some(&Value::Null),
            "the explicit null must survive the merging canonical storage: {:?}",
            record.tool_config
        );

        apply_projection(&record).expect("apply cleared Codex projection");
        let doc = read_toml(&codex_dir.join("config.toml"));
        assert!(
            doc.get("model_context_window").is_none(),
            "the stale native scalar must be removed after an explicit clear: {doc}"
        );
    });
}

#[test]
fn codex_new_selection_and_unmanaged_reset_remove_the_managed_scalar() {
    with_temp_dir("ctx-codex-selection-reset", |home| {
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");
        let config_path = codex_dir.join("config.toml");
        write_test_file(
            &config_path,
            r#"model_context_window = 9999
model = "gpt-4o"
model_provider = "onespace_gateway_codex"
model_reasoning_effort = "high"

[model_providers.onespace_gateway_codex]
name = "AI Gateway"
base_url = "http://127.0.0.1:19000/v1"
wire_api = "chat"
"#,
        );

        let mut new_selection = codex_record(Value::Null);
        new_selection.model = Some("gpt-4o-new".to_string());
        apply_projection(&new_selection).expect("apply new Codex selection");
        let doc = read_toml(&config_path);
        assert!(
            doc.get("model_context_window").is_none(),
            "a new selection without a known window must remove the previous scalar: {doc}"
        );

        write_test_file(
            &config_path,
            r#"model_context_window = 9999
model = "gpt-4o"
model_provider = "onespace_gateway_codex"
model_reasoning_effort = "high"

[model_providers.onespace_gateway_codex]
name = "AI Gateway"
base_url = "http://127.0.0.1:19000/v1"
wire_api = "chat"
"#,
        );
        render_codex_reset_to_unmanaged_at_home(home).expect("reset Codex to unmanaged");
        let doc = read_toml(&config_path);
        assert!(
            doc.get("model_context_window").is_none(),
            "unmanaged reset must remove the managed context scalar: {doc}"
        );
        assert_eq!(
            doc.get("model_reasoning_effort").and_then(|v| v.as_str()),
            Some("high"),
            "unmanaged reset must keep unrelated settings: {doc}"
        );
    });
}

// ---------------------------------------------------------------------------
// Claude global and isolated settings
// ---------------------------------------------------------------------------

#[test]
fn claude_global_and_isolated_settings_write_the_same_decimal_string() {
    with_temp_dir("ctx-claude-both-paths", |home| {
        let global_dir = home.join(".claude");
        fs::create_dir_all(&global_dir).expect("create global claude dir");
        write_test_file(
            &global_dir.join("settings.json"),
            r#"{
  "theme": "dark",
  "env": {
    "CLAUDE_CODE_AUTO_COMPACT_WINDOW": "64000",
    "UNRELATED": "keep"
  }
}"#,
        );

        let record = claude_record(json!(128000));
        apply_projection(&record).expect("apply global Claude projection");
        let global = read_json(&global_dir.join("settings.json"));
        assert_eq!(
            global["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "128000",
            "the global settings must carry a decimal string: {global}"
        );
        assert_eq!(global["env"]["UNRELATED"], "keep");
        assert_eq!(global["theme"], "dark");
        assert!(global["env"].get("DISABLE_COMPACT").is_none());
        assert!(global["env"].get("CLAUDE_CODE_MAX_CONTEXT_TOKENS").is_none());
        assert_eq!(global["model"], "gpt-4o", "the model must not be renamed");

        let profile_dir = home
            .join(".config")
            .join("onespace")
            .join("claude_profiles")
            .join("gateway-claude-code");
        fs::create_dir_all(&profile_dir).expect("create isolated profile");
        write_test_file(
            &profile_dir.join("settings.json"),
            r#"{
  "theme": "light",
  "env": {
    "CLAUDE_CODE_AUTO_COMPACT_WINDOW": "64000",
    "UNRELATED": "keep"
  }
}"#,
        );
        materialize_claude_settings_sp(&record, &profile_dir).expect("materialize isolated profile");
        let isolated = read_json(&profile_dir.join("settings.json"));
        assert_eq!(
            isolated["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "128000",
            "the isolated profile must agree with the global settings: {isolated}"
        );
        assert_eq!(isolated["env"]["UNRELATED"], "keep");
        assert_eq!(isolated["theme"], "light");
        assert!(isolated["env"].get("DISABLE_COMPACT").is_none());
        assert!(isolated["env"].get("CLAUDE_CODE_MAX_CONTEXT_TOKENS").is_none());
        assert_eq!(isolated["model"], "gpt-4o", "the model must not be renamed");
        assert!(
            isolated["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"]
                .as_str()
                .unwrap_or_default()
                .ends_with("claude-sonnet-local"),
            "the family model must not gain a [1m] suffix: {isolated}"
        );
    });
}

#[test]
fn claude_explicit_clear_removes_the_managed_threshold_in_both_paths() {
    with_temp_dir("ctx-claude-clear", |home| {
        let global_dir = home.join(".claude");
        fs::create_dir_all(&global_dir).expect("create global claude dir");
        write_test_file(
            &global_dir.join("settings.json"),
            r#"{"env":{"CLAUDE_CODE_AUTO_COMPACT_WINDOW":"128000","OTHER":"keep"}}"#,
        );
        let record = claude_record(Value::Null);
        apply_projection(&record).expect("apply cleared global Claude projection");
        let global = read_json(&global_dir.join("settings.json"));
        assert!(
            global["env"].get("CLAUDE_CODE_AUTO_COMPACT_WINDOW").is_none(),
            "an explicit clear must remove the global managed threshold: {global}"
        );
        assert_eq!(global["env"]["OTHER"], "keep");

        let profile_dir = home
            .join(".config")
            .join("onespace")
            .join("claude_profiles")
            .join("gateway-claude-code");
        fs::create_dir_all(&profile_dir).expect("create isolated profile");
        write_test_file(
            &profile_dir.join("settings.json"),
            r#"{"env":{"CLAUDE_CODE_AUTO_COMPACT_WINDOW":"128000","OTHER":"keep"}}"#,
        );
        materialize_claude_settings_sp(&record, &profile_dir).expect("materialize cleared profile");
        let isolated = read_json(&profile_dir.join("settings.json"));
        assert!(
            isolated["env"].get("CLAUDE_CODE_AUTO_COMPACT_WINDOW").is_none(),
            "an explicit clear must remove the isolated managed threshold: {isolated}"
        );
        assert_eq!(isolated["env"]["OTHER"], "keep");
    });
}

#[test]
fn claude_rematerialization_path_receives_the_same_threshold() {
    with_temp_dir("ctx-claude-rematerialize", |home| {
        let id = upsert_provider(json!({
            "id": "claude-context",
            "name": "AI Gateway",
            "tool": "claude",
            "code": "gateway-context",
            "api_key": "local-key-123",
            "base_url": "http://127.0.0.1:19000",
            "model": "gpt-4o",
            "claude_api_format": "anthropic_messages",
            "claude_connection_mode": "native_anthropic",
            "claude_auth_env_key": "ANTHROPIC_API_KEY",
            "claude_model_mappings": [
                { "family": "haiku", "display_name": "Haiku", "upstream_model": "claude-haiku-local" },
                { "family": "sonnet", "display_name": "Sonnet", "upstream_model": "claude-sonnet-local" },
                { "family": "opus", "display_name": "Opus", "upstream_model": "claude-opus-local" }
            ],
            "tool_config": {
                "ai_gateway_gateway": true,
                "claude_auto_compact_window": 1000000
            }
        }));

        let dir = tauri::async_runtime::block_on(resolve_claude_config_dir_for_provider_id_async(&id))
            .expect("resolve and rematerialize the isolated Claude profile");
        let settings = read_json(&dir.join("settings.json"));
        assert_eq!(
            settings["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "1000000",
            "the rematerialized (resume) path must receive the same threshold: {settings}"
        );
        assert!(settings["env"].get("DISABLE_COMPACT").is_none());
        assert!(settings["env"].get("CLAUDE_CODE_MAX_CONTEXT_TOKENS").is_none());
        let _ = home;
    });
}
