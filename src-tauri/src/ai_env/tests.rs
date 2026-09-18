use super::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn make_temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("onespace-ai-env-{}-{}", name, uuid::Uuid::new_v4()))
}

fn write_test_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, content).expect("write file");
}

/// Retains the global `HOME` lock on purpose: applying an environment writes
/// the target tool's config under the real home (`dirs::home_dir()`), which the
/// thread-local `get_app_dir()` override does not cover. A process-wide `HOME`
/// change plus the shared mutex is required to keep these cases isolated and
/// serialized.
fn with_temp_home<T>(name: &str, f: impl FnOnce(&Path) -> T) -> T {
    let _guard = crate::lock_test_home_env();
    let temp_home = make_temp_dir(name);
    fs::create_dir_all(&temp_home).expect("create temp home");
    let original_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &temp_home);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&temp_home)));
    if let Some(home) = original_home {
        std::env::set_var("HOME", home);
    } else {
        std::env::remove_var("HOME");
    }
    let _ = fs::remove_dir_all(&temp_home);
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
fn openai_models_url_preserves_opencode_go_v1_base() {
    assert_eq!(
        openai_models_url("https://opencode.ai/zen/go/v1"),
        "https://opencode.ai/zen/go/v1/models"
    );
}

#[test]
fn openai_models_url_keeps_existing_models_endpoint() {
    assert_eq!(
        openai_models_url("https://opencode.ai/zen/go/v1/models"),
        "https://opencode.ai/zen/go/v1/models"
    );
}

#[test]
fn openai_models_url_adds_v1_for_plain_base_url() {
    assert_eq!(
        openai_models_url("https://api.example.com"),
        "https://api.example.com/v1/models"
    );
}

#[test]
fn openai_models_url_strips_chat_completions_suffix() {
    assert_eq!(
        openai_models_url("https://api.example.com/v1/chat/completions"),
        "https://api.example.com/v1/models"
    );
}

#[test]
fn get_ai_providers_imports_opencode_with_uuid_id_and_provider_key_match() {
    with_temp_home("opencode-import-provider-key", |home| {
        let opencode_path = home.join(".config").join("opencode").join("opencode.json");
        write_test_file(
            &opencode_path,
            r#"{
                "provider": {
                    "custom_provider": {
                        "name": "Custom OpenCode",
                        "options": {
                            "apiKey": "sk-open",
                            "baseURL": "https://opencode.example.com/v1"
                        },
                        "models": {
                            "open-model": {}
                        }
                    }
                }
            }"#,
        );

        let state = get_ai_providers().expect("load providers");
        let provider = state
            .providers
            .iter()
            .find(|provider| provider.tool == "opencode")
            .expect("opencode provider");
        assert!(Uuid::parse_str(&provider.id).is_ok());
        assert_ne!(provider.id, "default-opencode");
        assert_ne!(provider.id, "opencode-custom_provider");
        assert_eq!(provider.provider_key.as_deref(), Some("custom_provider"));
        assert_eq!(provider.api_key, "sk-open");
        assert_eq!(
            provider.base_url.as_deref(),
            Some("https://opencode.example.com/v1")
        );
        assert_eq!(provider.model.as_deref(), Some("open-model"));

        let second = get_ai_providers().expect("reload providers");
        let second_provider = second
            .providers
            .iter()
            .find(|provider| provider.tool == "opencode")
            .expect("opencode provider");
        assert!(Uuid::parse_str(&second_provider.id).is_ok());
        assert_ne!(second_provider.id, "default-opencode");
        assert_ne!(second_provider.id, "opencode-custom_provider");
        assert_eq!(
            second_provider.provider_key.as_deref(),
            Some("custom_provider")
        );
    });
}

#[test]
fn apply_opencode_requires_provider_key_and_writes_by_provider_key() {
    with_temp_home("opencode-apply-provider-key", |home| {
        let missing_key = AiProvider {
            id: Uuid::new_v4().to_string(),
            name: "OpenCode Missing Key".to_string(),
            tool: "opencode".to_string(),
            api_key: "sk-open".to_string(),
            is_enabled: Some(true),
            ..Default::default()
        };
        let err = tauri::async_runtime::block_on(apply_ai_environment(missing_key)).unwrap_err();
        assert!(err.contains("provider_key"));

        let mut extra_fields = std::collections::HashMap::new();
        extra_fields.insert(
            "models".to_string(),
            serde_json::json!({ "open-model": {} }),
        );
        let provider_id = Uuid::new_v4().to_string();
        let provider = AiProvider {
            id: provider_id.clone(),
            name: "OpenCode".to_string(),
            tool: "opencode".to_string(),
            api_key: "sk-open".to_string(),
            base_url: Some("https://opencode.example.com/v1".to_string()),
            model: Some("open-model".to_string()),
            is_enabled: Some(true),
            provider_key: Some("custom_provider".to_string()),
            extra_fields,
            ..Default::default()
        };
        tauri::async_runtime::block_on(apply_ai_environment(provider)).expect("apply");

        let opencode_path = home.join(".config").join("opencode").join("opencode.json");
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(opencode_path).unwrap()).unwrap();
        assert!(settings["provider"]["custom_provider"].is_object());
        assert!(settings["provider"][provider_id].is_null());
        assert_eq!(
            settings["provider"]["custom_provider"]["models"]["open-model"],
            serde_json::json!({})
        );
    });
}

#[test]
fn apply_antigravity_writes_antigravity_cli_settings_without_legacy_gemini_files() {
    with_temp_home("antigravity-apply", |home| {
        let provider = AiProvider {
            id: Uuid::new_v4().to_string(),
            name: "Antigravity".to_string(),
            tool: "antigravity".to_string(),
            api_key: "sk-antigravity".to_string(),
            base_url: Some("https://antigravity.example.com".to_string()),
            model: Some("gemini-3-pro".to_string()),
            provider_key: Some("google-vertex".to_string()),
            is_enabled: Some(true),
            ..Default::default()
        };
        tauri::async_runtime::block_on(apply_ai_environment(provider)).expect("apply");

        let settings_path = home
            .join(".gemini")
            .join("antigravity-cli")
            .join("settings.json");
        assert!(
            settings_path.exists(),
            "expected Antigravity settings at {}",
            settings_path.display()
        );
        let settings: Value = serde_json::from_str(
            &fs::read_to_string(&settings_path).expect("read antigravity settings"),
        )
        .expect("parse antigravity settings");
        assert_eq!(
            settings["modelProvider"],
            serde_json::json!("google-vertex"),
            "modelProvider must come from provider_key"
        );
        assert_eq!(settings["model"], serde_json::json!("gemini-3-pro"));

        assert!(
            !home.join(".gemini").join(".env").exists(),
            "applying must not create ~/.gemini/.env"
        );
        assert!(
            !home.join(".gemini").join("settings.json").exists(),
            "applying must not create legacy ~/.gemini/settings.json"
        );
    });
}

#[test]
fn remove_opencode_requires_provider_key_and_removes_by_provider_key() {
    with_temp_home("opencode-remove-provider-key", |home| {
        let opencode_path = home.join(".config").join("opencode").join("opencode.json");
        write_test_file(
            &opencode_path,
            r#"{
                "provider": {
                    "custom_provider": { "name": "Custom" },
                    "other_provider": { "name": "Other" }
                }
            }"#,
        );

        let missing_key = AiProvider {
            id: Uuid::new_v4().to_string(),
            name: "OpenCode Missing Key".to_string(),
            tool: "opencode".to_string(),
            ..Default::default()
        };
        let err = remove_ai_environment(missing_key).unwrap_err();
        assert!(err.contains("provider_key"));

        remove_ai_environment(AiProvider {
            id: Uuid::new_v4().to_string(),
            name: "OpenCode".to_string(),
            tool: "opencode".to_string(),
            provider_key: Some("custom_provider".to_string()),
            ..Default::default()
        })
        .expect("remove");

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(opencode_path).unwrap()).unwrap();
        assert!(settings["provider"]["custom_provider"].is_null());
        assert!(settings["provider"]["other_provider"].is_object());
    });
}
