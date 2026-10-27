mod import_export;
mod provider_commands;
mod records;
mod sync_commands;

#[cfg(test)]
use super::{apply_opencode_remove_projection, ServiceProviderRecord};
pub use import_export::*;
pub use provider_commands::*;
pub(crate) use records::*;
#[cfg(test)]
use serde_json::{json, Value};
#[cfg(test)]
use std::fs::{self};
#[cfg(test)]
use std::path::Path;
pub use sync_commands::*;

#[cfg(test)]
mod opencode_config_read_tests {
    use super::*;

    fn with_opencode_config(name: &str, content: &str, test: impl FnOnce(&Path)) {
        let home = std::env::temp_dir().join(format!(
            "onespace-opencode-config-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        let path = home.join(".config").join("opencode").join("opencode.json");
        fs::create_dir_all(path.parent().expect("config parent")).expect("create config dir");
        fs::write(path, content).expect("write OpenCode config");
        test(&home);
        fs::remove_dir_all(home).expect("remove temp home");
    }

    #[test]
    fn reads_only_the_requested_opencode_provider_and_preserves_all_fields() {
        with_opencode_config(
            "read-provider",
            r#"{
                    "provider": {
                        "target": {
                            "name": "Latest",
                            "options": {"apiKey": "new-key", "nested": {"keep": true}},
                            "models": {"latest-model": {"limit": {"context": 200000}}},
                            "unknownTopLevel": [1, 2, 3]
                        },
                        "other": {"name": "Untouched"}
                    }
                }"#,
            |home| {
                let provider = read_opencode_provider_config_at_home(home, "target")
                    .expect("read target provider");

                assert_eq!(provider["name"], "Latest");
                assert_eq!(provider["options"]["nested"]["keep"], true);
                assert_eq!(
                    provider["models"]["latest-model"]["limit"]["context"],
                    200000
                );
                assert_eq!(provider["unknownTopLevel"], json!([1, 2, 3]));
                assert!(provider.get("other").is_none());
            },
        );
    }

    #[test]
    fn rejects_missing_provider_key_and_invalid_provider_structure() {
        with_opencode_config(
            "invalid-provider",
            r#"{"provider":{"valid":{"name":"Valid"},"invalid":"not-an-object"}}"#,
            |home| {
                let missing = read_opencode_provider_config_at_home(home, "missing")
                    .expect_err("missing key must fail");
                assert!(missing.contains("provider key not found: missing"));

                let invalid = read_opencode_provider_config_at_home(home, "invalid")
                    .expect_err("non-object provider must fail");
                assert!(invalid.contains("provider 'invalid' must be an object"));
            },
        );
    }

    #[test]
    fn provider_list_redaction_covers_top_level_nested_and_history_secrets() {
        let plaintext = "gateway-plaintext-fixture";
        let mut value = json!({
            "api_key": plaintext,
            "options": { "apiKey": plaintext },
            "history": [{ "snapshot": { "api_key": plaintext } }]
        });
        redact_provider_secrets(&mut value);
        assert_eq!(value["api_key"], "********");
        assert_eq!(value["options"]["apiKey"], "********");
        assert_eq!(value["history"][0]["snapshot"]["api_key"], "********");
        assert!(!serde_json::to_string(&value).unwrap().contains(plaintext));
    }

    fn opencode_record_with_key(provider_key: &str) -> ServiceProviderRecord {
        ServiceProviderRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name: "OpenCode Test".to_string(),
            tool: "opencode".to_string(),
            provider_key: Some(provider_key.to_string()),
            ..ServiceProviderRecord::default()
        }
    }

    #[test]
    fn delete_opencode_provider_removes_only_its_opencode_json_entry() {
        let _guard = crate::lock_test_home_env();
        let home = std::env::temp_dir().join(format!(
            "onespace-opencode-delete-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&home).expect("create temp home");
        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let path = home
                .join(".config")
                .join("opencode")
                .join("opencode.json");
            fs::create_dir_all(path.parent().expect("config parent"))
                .expect("create config dir");
            fs::write(
                &path,
                r#"{
                    "$schema": "https://opencode.ai/config.json",
                    "model": "keep-me",
                    "provider": {
                        "target": { "name": "Target" },
                        "other": { "name": "Other" }
                    }
                }"#,
            )
            .expect("write OpenCode config");

            apply_opencode_remove_projection(&opencode_record_with_key("target"))
                .expect("remove target provider");

            let content = fs::read_to_string(&path).expect("read OpenCode config");
            let root: Value = serde_json::from_str(&content).expect("parse OpenCode config");
            assert!(
                root["provider"]["target"].is_null(),
                "deleted provider must be gone: {root:?}"
            );
            assert!(
                root["provider"]["other"].is_object(),
                "other providers must be preserved: {root:?}"
            );
            assert_eq!(root["model"], "keep-me");

            // 未命中的 provider_key 不应改写文件，避免删除产生无意义写入。
            let before = fs::read_to_string(&path).expect("read before no-op");
            let renders =
                crate::app_store::render_opencode_remove(&opencode_record_with_key("missing"))
                    .expect("missing key renders");
            assert!(
                renders.is_empty(),
                "missing key must be a no-op without file write"
            );
            let after = fs::read_to_string(&path).expect("read after no-op");
            assert_eq!(before, after);
        }));
        if let Some(home_value) = original_home {
            std::env::set_var("HOME", home_value);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(&home);
        if let Err(payload) = outcome {
            std::panic::resume_unwind(payload);
        }
    }

    #[test]
    fn remove_opencode_render_is_noop_when_config_file_missing() {
        let _guard = crate::lock_test_home_env();
        let home = std::env::temp_dir().join(format!(
            "onespace-opencode-delete-missing-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&home).expect("create temp home");
        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let renders =
                crate::app_store::render_opencode_remove(&opencode_record_with_key("target"))
                    .expect("missing file renders");
            assert!(
                renders.is_empty(),
                "missing opencode.json must be a no-op"
            );
            assert!(
                !home.join(".config").join("opencode").join("opencode.json").exists(),
                "delete must not create opencode.json when it did not exist"
            );
        }));
        if let Some(home_value) = original_home {
            std::env::set_var("HOME", home_value);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(&home);
        if let Err(payload) = outcome {
            std::panic::resume_unwind(payload);
        }
    }
}
