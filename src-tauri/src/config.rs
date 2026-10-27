mod commands;
mod migration;
mod normalization;
mod storage;
mod types;

pub use commands::*;
#[allow(unused_imports)]
pub use types::{
    AiNewsRssSource, DeviceConfig, ProxyConfig, SharedProfile, SkillSourceConfig, StorageConfig,
    SubagentSourceConfig, SyncPolicy,
};
#[allow(unused_imports)]
pub(crate) use storage::{
    app_dir_for, default_storage_type_for, get_app_dir, get_config, get_local_data_dir,
    get_shared_data_dir_for, is_dev_build, resolve_selected_storage_root_at,
    resolve_shared_storage_root, resolve_shared_storage_root_at, seed_dev_gateway_files_on_start,
    seed_gateway_files, shared_profile_local_path,
};
#[allow(unused_imports)]
pub(crate) use storage::{load_shared_profile_local, save_shared_profile_local};
#[allow(unused_imports)]
use normalization::{normalize_ai_model_launch_commands, normalize_ai_news_keywords};
#[allow(unused_imports)]
use storage::{ensure_local_data_mirror_initialized_at, seed_gateway_files_with};
#[allow(unused_imports)]
use types::apply_shared_profile;

#[cfg(test)]
pub(crate) mod test_home {
    use std::cell::RefCell;
    use std::marker::PhantomData;
    use std::path::PathBuf;

    thread_local! {
        static TEST_HOME_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    /// Returns the current thread's HOME override, if a [`TestHomeGuard`] is
    /// active. Production path resolution is expected to fall back to
    /// `dirs::home_dir()` when this is `None`.
    pub(crate) fn test_home_override() -> Option<PathBuf> {
        TEST_HOME_OVERRIDE.with(|slot| slot.borrow().clone())
    }

    /// Scopes a thread-local HOME override to the creating thread. Dropping the
    /// guard restores the previous value, so nested guards unwind in order.
    ///
    /// The guard is intentionally `!Send`: the override lives in thread-local
    /// storage and must be created and dropped on the same thread.
    pub(crate) struct TestHomeGuard {
        previous: Option<PathBuf>,
        _not_send: PhantomData<*const ()>,
    }

    impl TestHomeGuard {
        pub(crate) fn set(home: impl Into<PathBuf>) -> Self {
            let previous = TEST_HOME_OVERRIDE.with(|slot| slot.replace(Some(home.into())));
            Self {
                previous,
                _not_send: PhantomData,
            }
        }
    }

    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            let previous = self.previous.take();
            TEST_HOME_OVERRIDE.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_home::{test_home_override, TestHomeGuard};
    use super::{
        apply_shared_profile, get_app_dir, get_local_data_dir, normalize_ai_model_launch_commands,
        normalize_ai_news_keywords, AiNewsRssSource, SharedProfile, StorageConfig, SyncPolicy,
    };
    use super::{
        app_dir_for, default_storage_type_for, ensure_local_data_mirror_initialized_at, is_dev_build,
        resolve_selected_storage_root_at, resolve_shared_storage_root_at, DeviceConfig,
    };
    use super::{
        seed_dev_gateway_files_on_start, seed_gateway_files, seed_gateway_files_with,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Barrier};

    struct TempTestHome(PathBuf);

    impl Drop for TempTestHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sync_policy_default_disables_skills_repository() {
        let policy = SyncPolicy::default();
        assert!(!policy.skills_repository);
        assert!(!policy.subagents_repository);
        assert!(!policy.ai_news);
    }

    #[test]
    fn sync_policy_deserialize_without_skills_repository_defaults_to_false() {
        let json = r#"{
            "providers": true,
            "mcp": true,
            "content": true,
            "workflow_presets": true,
            "skills_sources": true
        }"#;

        let policy: SyncPolicy =
            serde_json::from_str(json).expect("sync policy should deserialize");
        assert!(!policy.skills_repository);
        assert!(!policy.subagents_repository);
        assert!(!policy.ai_news);
    }

    #[test]
    fn legacy_sync_policy_with_removed_workflow_scope_loads_without_serializing_it() {
        let json = r#"{
            "providers": false,
            "mcp": true,
            "content": false,
            "workflow_presets": true,
            "skills_sources": true,
            "skills_repository": true,
            "subagents_sources": false,
            "subagents_repository": true,
            "ai_news": true
        }"#;

        let policy: SyncPolicy =
            serde_json::from_str(json).expect("legacy sync policy should deserialize");

        assert!(!policy.providers);
        assert!(policy.mcp);
        assert!(!policy.content);
        assert!(policy.skills_sources);
        assert!(policy.skills_repository);
        assert!(!policy.subagents_sources);
        assert!(policy.subagents_repository);
        assert!(policy.ai_news);

        let serialized = serde_json::to_string(&policy).expect("sync policy should serialize");
        assert!(
            !serialized.contains("workflow_presets"),
            "removed workflow scope must not be serialized: {serialized}"
        );
        let serialized_value: serde_json::Value =
            serde_json::from_str(&serialized).expect("serialized sync policy should be json");
        assert!(
            serialized_value.get("workflow_presets").is_none(),
            "removed workflow scope must not be serialized: {serialized}"
        );
    }

    #[test]
    fn ai_news_rss_sources_missing_field_and_explicit_empty_stay_empty() {
        let missing: SharedProfile = serde_json::from_str("{}").expect("profile");
        let mut missing_cfg = StorageConfig::default();
        apply_shared_profile(&mut missing_cfg, &missing);
        assert!(missing_cfg.ai_news_rss_sources.is_empty());

        let storage_missing: StorageConfig =
            serde_json::from_str(r#"{"storage_type":"local"}"#).expect("storage config");
        assert!(storage_missing.ai_news_rss_sources.is_empty());

        let empty: SharedProfile =
            serde_json::from_str(r#"{"ai_news_rss_sources":[]}"#).expect("profile");
        let mut empty_cfg = StorageConfig::default();
        apply_shared_profile(&mut empty_cfg, &empty);
        assert!(empty_cfg.ai_news_rss_sources.is_empty());
    }

    #[test]
    fn ai_news_rss_sources_existing_custom_and_builtin_sources_are_kept() {
        let profile = SharedProfile {
            ai_news_rss_sources: Some(vec![
                AiNewsRssSource {
                    id: "custom".to_string(),
                    name: "Custom".to_string(),
                    url: "https://example.com/feed.xml".to_string(),
                    enabled: true,
                },
                AiNewsRssSource {
                    id: "36kr".to_string(),
                    name: "Custom 36Kr Name".to_string(),
                    url: "https://example.com/36kr.xml".to_string(),
                    enabled: false,
                },
            ]),
            ..SharedProfile::default()
        };
        let mut cfg = StorageConfig::default();

        apply_shared_profile(&mut cfg, &profile);

        assert_eq!(cfg.ai_news_rss_sources.len(), 2);
        assert!(cfg.ai_news_rss_sources.iter().any(|source| {
            source.id == "custom" && source.url == "https://example.com/feed.xml"
        }));
        assert!(cfg.ai_news_rss_sources.iter().any(|source| {
            source.id == "36kr"
                && source.name == "Custom 36Kr Name"
                && source.url == "https://example.com/36kr.xml"
                && !source.enabled
        }));
    }

    #[test]
    fn empty_ai_news_keywords_stay_empty() {
        assert_eq!(normalize_ai_news_keywords(Some("   ".to_string())), None);
    }

    #[test]
    fn apply_shared_profile_keeps_explicit_ai_news_keywords() {
        let profile = SharedProfile {
            ai_news_keywords: Some("OpenAI, Claude".to_string()),
            ..SharedProfile::default()
        };
        let mut cfg = StorageConfig::default();

        apply_shared_profile(&mut cfg, &profile);

        assert_eq!(cfg.ai_news_keywords.as_deref(), Some("OpenAI, Claude"));
    }

    #[test]
    fn apply_shared_profile_clears_blank_ai_news_keywords() {
        let profile = SharedProfile {
            ai_news_keywords: Some("   ".to_string()),
            ..SharedProfile::default()
        };
        let mut cfg = StorageConfig::default();

        apply_shared_profile(&mut cfg, &profile);

        assert_eq!(cfg.ai_news_keywords, None);
    }

    #[test]
    fn apply_shared_profile_clamps_ai_news_sync_interval_to_new_minimum() {
        let profile = SharedProfile {
            ai_news_sync_interval_minutes: Some(1),
            ..SharedProfile::default()
        };
        let mut cfg = StorageConfig::default();

        apply_shared_profile(&mut cfg, &profile);
        cfg.ai_news_sync_interval_minutes = Some(
            cfg.ai_news_sync_interval_minutes
                .unwrap_or(60)
                .clamp(10, 1440),
        );

        assert_eq!(cfg.ai_news_sync_interval_minutes, Some(10));
    }

    #[test]
    fn normalize_launch_commands_replaces_legacy_gemini_for_antigravity() {
        let input = HashMap::from([
            ("antigravity".to_string(), "gemini -y".to_string()),
            (
                "claude".to_string(),
                "claude --dangerously-skip-permissions".to_string(),
            ),
        ]);

        let out = normalize_ai_model_launch_commands(Some(input));

        assert_eq!(out.get("antigravity").map(String::as_str), Some("agy"));
        assert_eq!(
            out.get("claude").map(String::as_str),
            Some("claude --dangerously-skip-permissions")
        );
        assert_eq!(out.get("codex").map(String::as_str), Some("codex"));
        assert_eq!(out.get("opencode").map(String::as_str), Some("opencode"));
    }

    #[test]
    fn normalize_launch_commands_keeps_custom_antigravity_command() {
        let input = HashMap::from([(
            "antigravity".to_string(),
            "agy --dangerously-skip-permissions".to_string(),
        )]);

        let out = normalize_ai_model_launch_commands(Some(input));

        assert_eq!(
            out.get("antigravity").map(String::as_str),
            Some("agy --dangerously-skip-permissions")
        );
    }

    #[test]
    fn normalize_launch_commands_treats_absolute_gemini_path_as_legacy() {
        let input = HashMap::from([(
            "antigravity".to_string(),
            "/usr/local/bin/gemini --approval-mode=yolo".to_string(),
        )]);

        let out = normalize_ai_model_launch_commands(Some(input));

        assert_eq!(out.get("antigravity").map(String::as_str), Some("agy"));
    }

    #[test]
    fn normalize_launch_commands_replaces_blank_entry_with_default() {
        let input = HashMap::from([("opencode".to_string(), "   ".to_string())]);

        let out = normalize_ai_model_launch_commands(Some(input));

        assert_eq!(out.get("opencode").map(String::as_str), Some("opencode"));
    }

    #[test]
    fn test_home_fixture_isolates_app_dir_and_local_data_across_threads() {
        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();

        for label in ["home-a", "home-b"] {
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                let temp_home = std::env::temp_dir().join(format!(
                    "onespace-config-test-home-{}-{}",
                    label,
                    uuid::Uuid::new_v4()
                ));
                let _cleanup = TempTestHome(temp_home.clone());
                fs::create_dir_all(&temp_home).expect("create temp home");

                // Seed local storage so an isolated local-data resolution never
                // falls back to the real iCloud/legacy location.
                let isolated_app_dir = temp_home.join(".config").join("onespace");
                fs::create_dir_all(&isolated_app_dir).expect("create temp app dir");
                let seeded_config = format!(
                    r#"{{"storage_type":"local","local_storage_path":{}}}"#,
                    serde_json::to_string(&temp_home.join("data")).expect("encode temp data path")
                );
                fs::write(isolated_app_dir.join("config.json"), seeded_config)
                    .expect("seed temp config");

                let _guard = TestHomeGuard::set(&temp_home);

                // A nested guard must restore the previous thread-local HOME.
                {
                    let nested_home = temp_home.join("nested-home");
                    let _nested_guard = TestHomeGuard::set(&nested_home);
                    assert_eq!(test_home_override(), Some(nested_home));
                }
                assert_eq!(test_home_override(), Some(temp_home.clone()));

                // Reaching the barrier proves both threads hold distinct
                // thread-local HOME scopes at the same time.
                barrier.wait();

                let resolved_app_dir = get_app_dir().expect("resolve app dir");
                assert_eq!(
                    resolved_app_dir, isolated_app_dir,
                    "get_app_dir must resolve under the thread-local HOME override"
                );

                let resolved_local_dir =
                    get_local_data_dir().expect("resolve local data dir");
                assert_eq!(
                    resolved_local_dir,
                    isolated_app_dir.join("local_data"),
                    "get_local_data_dir must resolve under the thread-local HOME override"
                );
            }));
        }

        for handle in handles {
            handle.join().expect("test home thread must not panic");
        }
    }

    // -----------------------------------------------------------------------
    // Dev/release environment isolation (REQ-001 / AC-001)
    //
    // Observable boundary: the profile app directory, the profile-bound storage
    // roots and the local-data mirror must all be driven by an explicit profile
    // flag / explicit roots instead of the ambient process environment, so a
    // dev build never reads or writes the release profile's data.
    //
    // These tests pin the delivered isolation contract: `app_dir_for` selects the
    // profile app directory from an explicit home plus `is_dev` flag,
    // `is_dev_build` / `default_storage_type_for` expose the build profile, and
    // the storage-root and local-data-mirror seams take explicit app dirs or
    // roots so they never fall back to ambient process state.
    // -----------------------------------------------------------------------

    #[test]
    fn app_dir_for_returns_dev_directory_for_dev_profile() {
        let home = PathBuf::from("/tmp/onespace-profile-home");
        assert_eq!(
            app_dir_for(&home, true),
            home.join(".config").join("onespace-dev")
        );
    }

    #[test]
    fn app_dir_for_returns_release_directory_for_release_profile() {
        let home = PathBuf::from("/tmp/onespace-profile-home");
        assert_eq!(
            app_dir_for(&home, false),
            home.join(".config").join("onespace")
        );
    }

    #[test]
    fn is_dev_build_is_false_in_test_compilation() {
        let temp_home = std::env::temp_dir().join(format!(
            "onespace-dev-build-profile-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _cleanup = TempTestHome(temp_home.clone());
        fs::create_dir_all(&temp_home).expect("create temp home");
        let _guard = TestHomeGuard::set(&temp_home);

        assert!(
            !is_dev_build(),
            "test compilation must not be treated as a dev build"
        );
        let app_dir = get_app_dir().expect("resolve app dir under test home");
        assert_eq!(app_dir, temp_home.join(".config").join("onespace"));
        assert!(
            !app_dir.ends_with("onespace-dev"),
            "get_app_dir under test must never resolve the dev directory"
        );
    }

    #[test]
    fn get_app_dir_resolves_release_profile_under_test_home_override() {
        let temp_home = std::env::temp_dir().join(format!(
            "onespace-release-profile-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _cleanup = TempTestHome(temp_home.clone());
        fs::create_dir_all(&temp_home).expect("create temp home");
        let _guard = TestHomeGuard::set(&temp_home);

        let app_dir = get_app_dir().expect("resolve app dir under test home");
        assert_eq!(app_dir, temp_home.join(".config").join("onespace"));
        assert!(
            !temp_home.join(".config").join("onespace-dev").exists(),
            "the dev profile directory must not exist for the release profile"
        );
    }

    #[test]
    fn default_storage_type_is_local_for_dev_profile() {
        assert_eq!(default_storage_type_for(true), "local");
    }

    #[test]
    fn default_storage_type_keeps_release_default_for_test_compilation() {
        let expected = if cfg!(target_os = "macos") {
            "icloud"
        } else {
            "local"
        };
        assert_eq!(default_storage_type_for(false), expected);
        assert_eq!(
            DeviceConfig::default().storage_type,
            default_storage_type_for(false)
        );
        #[cfg(target_os = "macos")]
        assert_ne!(
            DeviceConfig::default().storage_type,
            "local",
            "the macOS release default must stay iCloud"
        );
    }

    #[test]
    fn selected_local_storage_root_resolves_under_profile_app_dir() {
        let home = PathBuf::from("/tmp/onespace-selected-profile");
        let cfg = DeviceConfig {
            storage_type: "local".to_string(),
            local_storage_path: None,
            ..DeviceConfig::default()
        };
        let release_app_dir = app_dir_for(&home, false);
        let dev_app_dir = app_dir_for(&home, true);

        assert_eq!(
            resolve_selected_storage_root_at(&cfg, &release_app_dir).expect("release root"),
            release_app_dir.join("data")
        );
        assert_eq!(
            resolve_selected_storage_root_at(&cfg, &dev_app_dir).expect("dev root"),
            dev_app_dir.join("data")
        );
        assert_eq!(
            dev_app_dir,
            home.join(".config").join("onespace-dev"),
            "the dev storage root must resolve under the dev profile directory"
        );
    }

    #[test]
    fn shared_local_storage_root_resolves_under_profile_app_dir() {
        let home = PathBuf::from("/tmp/onespace-shared-profile");
        let cfg = StorageConfig {
            storage_type: "local".to_string(),
            local_storage_path: None,
            ..StorageConfig::default()
        };
        let release_app_dir = app_dir_for(&home, false);
        let dev_app_dir = app_dir_for(&home, true);

        assert_eq!(
            resolve_shared_storage_root_at(&cfg, &release_app_dir).expect("release root"),
            release_app_dir.join("data")
        );
        assert_eq!(
            resolve_shared_storage_root_at(&cfg, &dev_app_dir).expect("dev root"),
            dev_app_dir.join("data")
        );
        assert_eq!(dev_app_dir, home.join(".config").join("onespace-dev"));
    }

    #[test]
    fn local_data_mirror_initialization_stays_inside_profile() {
        let temp_home = std::env::temp_dir().join(format!(
            "onespace-mirror-isolation-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _cleanup = TempTestHome(temp_home.clone());
        fs::create_dir_all(&temp_home).expect("create temp home");

        let dev_local = temp_home
            .join(".config")
            .join("onespace-dev")
            .join("local_data");
        let dev_legacy = temp_home.join(".config").join("onespace-dev").join("data");
        let release_legacy = temp_home.join(".config").join("onespace").join("data");
        fs::create_dir_all(&release_legacy).expect("create release legacy tree");
        let release_notes = release_legacy.join("notes.json");
        fs::write(&release_notes, "release-only-content").expect("write release notes");

        ensure_local_data_mirror_initialized_at(&dev_local, &dev_legacy)
            .expect("initialize dev local mirror");

        assert!(
            !dev_local.join("notes.json").exists(),
            "the dev mirror must not import the release profile tree"
        );
        assert_eq!(
            fs::read_to_string(&release_notes).expect("release notes remain"),
            "release-only-content",
            "the release profile file must be untouched"
        );
    }

    #[test]
    fn local_data_mirror_initialization_keeps_release_copy_behavior() {
        let temp_home = std::env::temp_dir().join(format!(
            "onespace-mirror-release-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _cleanup = TempTestHome(temp_home.clone());
        fs::create_dir_all(&temp_home).expect("create temp home");

        let release_local = temp_home
            .join(".config")
            .join("onespace")
            .join("local_data");
        let release_legacy = temp_home.join(".config").join("onespace").join("data");
        fs::create_dir_all(&release_legacy).expect("create release legacy tree");
        fs::write(release_legacy.join("notes.json"), b"mirror-me").expect("write legacy notes");

        ensure_local_data_mirror_initialized_at(&release_local, &release_legacy)
            .expect("initialize release local mirror");

        assert_eq!(
            fs::read(release_local.join("notes.json")).expect("copied notes"),
            fs::read(release_legacy.join("notes.json")).expect("legacy notes"),
            "the release copy behavior must copy the legacy file byte for byte"
        );
    }

    // -----------------------------------------------------------------------
    // Dev/release environment isolation (REQ-002 / AC-004..AC-008)
    //
    // Observable boundary: `seed_gateway_files` over explicit release/dev
    // directories (copy rules, byte equality, retry and no-overwrite), and the
    // on-start entry `seed_dev_gateway_files_on_start` plus `run()`'s call
    // order. These tests are written before the implementation exists; they are
    // the RED evidence for Step 2.
    // -----------------------------------------------------------------------

    fn seed_temp_root(label: &str) -> (PathBuf, TempTestHome) {
        let root = std::env::temp_dir().join(format!(
            "onespace-seed-{}-{}-{}",
            label,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let cleanup = TempTestHome(root.clone());
        fs::create_dir_all(&root).expect("create temp root");
        (root, cleanup)
    }

    #[test]
    fn debug_start_seeds_both_gateway_files_with_intact_content() {
        let (root, _cleanup) = seed_temp_root("seed-both");
        let release = root.join("release");
        let dev = root.join("dev");
        fs::create_dir_all(&release).expect("create release dir");

        let password = "correct horse battery staple";
        let expected_json = r#"{"schema_version":2,"providers":[]}"#;
        fs::write(release.join(".local_key"), password).expect("write release local key");
        let encrypted =
            crate::crypto::encrypt(expected_json, password).expect("encrypt release gateway");
        fs::write(release.join("ai_gateway.json"), &encrypted).expect("write release gateway");

        seed_gateway_files(&release, &dev).expect("seed gateway files");

        assert!(dev.join(".local_key").is_file(), "the dev key must be seeded");
        assert!(
            dev.join("ai_gateway.json").is_file(),
            "the dev gateway must be seeded"
        );
        assert_eq!(
            fs::read(dev.join(".local_key")).expect("read dev key"),
            fs::read(release.join(".local_key")).expect("read release key"),
            "the copied master password must be byte-identical"
        );
        assert_eq!(
            fs::read(dev.join("ai_gateway.json")).expect("read dev gateway"),
            fs::read(release.join("ai_gateway.json")).expect("read release gateway"),
            "the copied gateway config must be byte-identical"
        );

        let decrypted = crate::crypto::decrypt(
            &fs::read_to_string(dev.join("ai_gateway.json")).expect("read dev gateway as string"),
            password,
        )
        .expect("decrypt copied gateway");
        assert_eq!(
            decrypted, expected_json,
            "the copied ciphertext must decrypt to the exact source JSON"
        );
    }

    #[test]
    fn seeding_never_overwrites_existing_dev_files() {
        let (root, _cleanup) = seed_temp_root("seed-no-overwrite");
        let release = root.join("release");
        let dev = root.join("dev");
        fs::create_dir_all(&release).expect("create release dir");
        fs::create_dir_all(&dev).expect("create dev dir");

        fs::write(release.join(".local_key"), "release-key").expect("write release key");
        fs::write(
            release.join("ai_gateway.json"),
            crate::crypto::encrypt("release-gateway", "release-key")
                .expect("encrypt release gateway"),
        )
        .expect("write release gateway");

        fs::write(dev.join(".local_key"), "dev-key").expect("write dev key");
        fs::write(dev.join("ai_gateway.json"), b"dev-gateway-bytes").expect("write dev gateway");
        let dev_key_before = fs::read(dev.join(".local_key")).expect("read dev key");
        let dev_gateway_before = fs::read(dev.join("ai_gateway.json")).expect("read dev gateway");

        seed_gateway_files(&release, &dev).expect("seed gateway files");

        assert_eq!(
            fs::read(dev.join(".local_key")).expect("read dev key after"),
            dev_key_before,
            "an existing dev master password must never be overwritten"
        );
        assert_eq!(
            fs::read(dev.join("ai_gateway.json")).expect("read dev gateway after"),
            dev_gateway_before,
            "an existing dev gateway config must never be overwritten"
        );
        assert_eq!(
            fs::read_to_string(dev.join(".local_key")).expect("read dev key string"),
            "dev-key"
        );
        assert_eq!(
            fs::read(dev.join("ai_gateway.json")).expect("read dev gateway bytes"),
            b"dev-gateway-bytes"
        );
    }

    #[test]
    fn seeding_restores_deleted_dev_files_and_directory() {
        let (root, _cleanup) = seed_temp_root("seed-restore");
        let release = root.join("release");
        let dev = root.join("dev");
        fs::create_dir_all(&release).expect("create release dir");

        let password = "restore-key";
        fs::write(release.join(".local_key"), password).expect("write release key");

        seed_gateway_files(&release, &dev).expect("seed key only");
        assert!(
            dev.join(".local_key").is_file(),
            "the missing dev master password must be seeded"
        );
        assert!(
            !dev.join("ai_gateway.json").exists(),
            "no gateway source must mean no gateway copy"
        );

        let encrypted = crate::crypto::encrypt(r#"{"providers":[{"id":"p"}]}"#, password)
            .expect("encrypt release gateway");
        fs::write(release.join("ai_gateway.json"), &encrypted).expect("write release gateway");

        seed_gateway_files(&release, &dev).expect("seed gateway later");
        assert_eq!(
            fs::read(dev.join("ai_gateway.json")).expect("read dev gateway"),
            fs::read(release.join("ai_gateway.json")).expect("read release gateway"),
            "a gateway added to the release dir must be picked up on the next start"
        );

        fs::remove_file(dev.join("ai_gateway.json")).expect("delete dev gateway");
        seed_gateway_files(&release, &dev).expect("restore gateway");
        assert_eq!(
            fs::read(dev.join("ai_gateway.json")).expect("read restored gateway"),
            fs::read(release.join("ai_gateway.json")).expect("read release gateway again"),
            "a deleted dev gateway file must be restored byte-identically"
        );

        fs::remove_dir_all(&dev).expect("delete whole dev dir");
        seed_gateway_files(&release, &dev).expect("restore dev dir");
        assert!(dev.join(".local_key").is_file(), "the key must be restored");
        assert_eq!(
            fs::read(dev.join("ai_gateway.json")).expect("read gateway after dir restore"),
            fs::read(release.join("ai_gateway.json")).expect("read release gateway after dir restore"),
            "both files must be restored after the whole dev directory is deleted"
        );
    }

    #[test]
    fn missing_release_source_is_a_noop_and_retried_later() {
        let (root, _cleanup) = seed_temp_root("seed-missing-source");
        let release = root.join("release");
        let dev = root.join("dev");

        seed_gateway_files(&release, &dev).expect("a missing release source must be a no-op");
        assert!(
            !dev.exists(),
            "a missing release source must not create the dev directory"
        );
        assert!(!release.exists(), "the source directory must not be created");

        fs::create_dir_all(&release).expect("create release dir later");
        let password = "late-key";
        fs::write(release.join(".local_key"), password).expect("write release key");
        fs::write(
            release.join("ai_gateway.json"),
            crate::crypto::encrypt(r#"{"schema_version":2,"providers":[]}"#, password)
                .expect("encrypt release gateway"),
        )
        .expect("write release gateway");

        seed_gateway_files(&release, &dev).expect("a later start must seed the late source");
        assert!(dev.join(".local_key").is_file(), "the late key must be seeded");
        assert!(
            dev.join("ai_gateway.json").is_file(),
            "the late gateway must be seeded"
        );
    }

    #[test]
    fn seed_copy_failure_is_returned_without_partial_overwrite() {
        let (root, _cleanup) = seed_temp_root("seed-failure");
        let release = root.join("release");
        let dev = root.join("dev");
        fs::create_dir_all(&release).expect("create release dir");

        let password = "failure-key";
        fs::write(release.join(".local_key"), password).expect("write release key");
        fs::write(
            release.join("ai_gateway.json"),
            crate::crypto::encrypt(r#"{"schema_version":2,"providers":[]}"#, password)
                .expect("encrypt release gateway"),
        )
        .expect("write release gateway");

        fs::write(&dev, b"not-a-directory").expect("write blocker file");

        let result = seed_gateway_files(&release, &dev);
        assert!(result.is_err(), "a failed copy must be reported as Err");
        assert_eq!(
            fs::read(&dev).expect("read blocker file"),
            b"not-a-directory",
            "the file blocking the copy must be unchanged"
        );

        fs::remove_file(&dev).expect("remove blocker file");
        seed_gateway_files(&release, &dev).expect("the next start must retry successfully");
        assert_eq!(
            fs::read(dev.join(".local_key")).expect("read dev key"),
            fs::read(release.join(".local_key")).expect("read release key")
        );
        assert_eq!(
            fs::read(dev.join("ai_gateway.json")).expect("read dev gateway"),
            fs::read(release.join("ai_gateway.json")).expect("read release gateway")
        );
    }

    #[test]
    fn failed_seed_copy_leaves_no_partial_file_and_next_start_retries() {
        let (root, _cleanup) = seed_temp_root("seed-partial-failure");
        let release = root.join("release");
        let dev = root.join("dev");
        fs::create_dir_all(&release).expect("create release dir");
        fs::create_dir_all(&dev).expect("create dev dir");

        let password = "SAFE_FIXTURE_partial-failure-key";
        fs::write(release.join(".local_key"), password).expect("write release key");
        fs::write(release.join("ai_gateway.json"), b"partial-failure-gateway-bytes")
            .expect("write release gateway");

        let result = seed_gateway_files_with(&release, &dev, |_src, dst| {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).expect("create destination parent");
            }
            fs::write(dst, b"partial-bytes").expect("write partial destination");
            Err("simulated mid-copy failure".to_string())
        });

        assert!(
            result.is_err(),
            "a failed copy seam must report the seeding error"
        );
        assert!(
            !dev.join(".local_key").exists(),
            "a failed copy must not leave a partial destination that blocks the retry"
        );

        seed_gateway_files(&release, &dev).expect("the next start must complete the retry");
        assert!(
            dev.join(".local_key").is_file(),
            "the retried start must seed the dev master password"
        );
        assert!(
            dev.join("ai_gateway.json").is_file(),
            "the retried start must seed the dev gateway config"
        );
        assert_eq!(
            fs::read(dev.join(".local_key")).expect("read dev key"),
            fs::read(release.join(".local_key")).expect("read release key"),
            "the retried dev master password must be byte-identical to the release copy"
        );
        assert_eq!(
            fs::read(dev.join("ai_gateway.json")).expect("read dev gateway"),
            fs::read(release.join("ai_gateway.json")).expect("read release gateway"),
            "the retried dev gateway config must be byte-identical to the release copy"
        );
    }

    #[test]
    fn missing_release_master_password_prevents_gateway_copy() {
        let (root, _cleanup) = seed_temp_root("seed-missing-password");
        let release = root.join("release");
        let dev = root.join("dev");
        fs::create_dir_all(&release).expect("create release dir");

        fs::write(
            release.join("ai_gateway.json"),
            crate::crypto::encrypt(r#"{"schema_version":2,"providers":[]}"#, "orphan-key")
                .expect("encrypt orphan gateway"),
        )
        .expect("write release gateway");

        seed_gateway_files(&release, &dev).expect("a missing master password is not fatal");

        assert!(
            !dev.join(".local_key").exists(),
            "no source master password must mean no dev master password"
        );
        assert!(
            !dev.join("ai_gateway.json").exists(),
            "a gateway config must never be copied without a matching master password"
        );
        assert!(
            !dev.exists(),
            "no dev directory may be created without a source master password"
        );
    }

    #[test]
    fn differing_dev_master_password_prevents_gateway_copy() {
        let (root, _cleanup) = seed_temp_root("seed-differing-password");
        let release = root.join("release");
        let dev = root.join("dev");
        fs::create_dir_all(&release).expect("create release dir");
        fs::create_dir_all(&dev).expect("create dev dir");

        fs::write(release.join(".local_key"), "release-key-a").expect("write release key");
        fs::write(
            release.join("ai_gateway.json"),
            crate::crypto::encrypt(r#"{"schema_version":2,"providers":[]}"#, "release-key-a")
                .expect("encrypt release gateway"),
        )
        .expect("write release gateway");
        fs::write(dev.join(".local_key"), "dev-key-b").expect("write dev key");

        seed_gateway_files(&release, &dev).expect("a differing master password is not fatal");

        assert_eq!(
            fs::read_to_string(dev.join(".local_key")).expect("read dev key"),
            "dev-key-b",
            "the dev master password must be preserved"
        );
        assert!(
            !dev.join("ai_gateway.json").exists(),
            "the gateway config must not be copied under a different master password"
        );
    }

    #[test]
    fn unit_test_compilation_never_creates_dev_directory_or_copies() {
        let (root, _cleanup) = seed_temp_root("seed-test-profile");
        let _guard = crate::config::test_home::TestHomeGuard::set(&root);

        let release_dir = root.join(".config").join("onespace");
        fs::create_dir_all(&release_dir).expect("create release dir");
        let key_bytes = b"test-compilation-key";
        let gateway_bytes = b"test-compilation-gateway";
        fs::write(release_dir.join(".local_key"), key_bytes).expect("write release key");
        fs::write(release_dir.join("ai_gateway.json"), gateway_bytes)
            .expect("write release gateway");

        assert!(
            !crate::config::is_dev_build(),
            "the unit-test compilation must not be treated as a dev build"
        );

        seed_dev_gateway_files_on_start();

        assert!(
            !root.join(".config").join("onespace-dev").exists(),
            "a unit-test compilation must never create the dev directory"
        );
        assert_eq!(
            fs::read(release_dir.join(".local_key")).expect("read release key"),
            key_bytes,
            "the release master password must be unchanged"
        );
        assert_eq!(
            fs::read(release_dir.join("ai_gateway.json")).expect("read release gateway"),
            gateway_bytes,
            "the release gateway config must be unchanged"
        );
    }
}
