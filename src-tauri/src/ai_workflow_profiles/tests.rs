use super::*;
use std::fs;
use std::path::{Path, PathBuf};

fn with_temp_test_home<T>(label: &str, f: impl FnOnce(&Path) -> T) -> T {
    let _guard = crate::lock_test_home_env();
    let temp_home_raw = std::env::temp_dir().join(format!(
        "onespace-ai-workflow-test-{}-{}-{}",
        label,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_home_raw).expect("create temp home");
    let temp_home = fs::canonicalize(&temp_home_raw).expect("canonical temp home");
    let previous_home = std::env::var_os("HOME");
    let previous_cli_path = std::env::var_os("AI_WORKFLOW_CLI_PATH");
    std::env::set_var("HOME", &temp_home);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&temp_home)));

    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_cli_path {
        Some(value) => std::env::set_var("AI_WORKFLOW_CLI_PATH", value),
        None => std::env::remove_var("AI_WORKFLOW_CLI_PATH"),
    }
    let _ = fs::remove_dir_all(&temp_home_raw);

    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[cfg(unix)]
fn create_mock_cli(dir: &Path, name: &str, script_body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let cli_path = dir.join(name);
    fs::write(&cli_path, script_body).expect("write mock cli");
    let mut perms = fs::metadata(&cli_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&cli_path, perms).expect("set permissions");
    cli_path
}

// ============================================================================
// 1. list_profiles tests
// ============================================================================

#[test]
fn test_ai_workflow_list_profiles_loads_fixtures_and_marks_active() {
    with_temp_test_home("list-active", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        let config_dir = home.join(".config/ai-workflow");
        fs::write(
            config_dir.join("config.yaml"),
            "active_profile: onesapce-ai-gateway\n",
        )
        .expect("write config.yaml");

        fs::write(
            profiles_dir.join("baibai-40.yaml"),
            "version: 1.0.0\nagents:\n  backend:\n    codex: { model: gpt-6-astra, reasoning_effort: medium }\n",
        )
        .expect("write baibai-40.yaml");

        fs::write(
            profiles_dir.join("onesapce-ai-gateway.yaml"),
            "version: 1.0.0\nagents:\n  frontend:\n    opencode: { model: apigateway/deepseek, reasoning_effort: high }\n",
        )
        .expect("write onesapce-ai-gateway.yaml");

        let list = list_profiles(Some(home)).expect("list_profiles should succeed");
        assert_eq!(list.len(), 2, "should list exactly 2 profiles");

        let active_entry = list
            .iter()
            .find(|p| p.name == "onesapce-ai-gateway")
            .expect("onesapce-ai-gateway should be present");
        assert!(active_entry.active, "onesapce-ai-gateway must be active");
        assert!(active_entry.error.is_none(), "should have no error");

        let inactive_entry = list
            .iter()
            .find(|p| p.name == "baibai-40")
            .expect("baibai-40 should be present");
        assert!(!inactive_entry.active, "baibai-40 must not be active");
        assert!(inactive_entry.error.is_none(), "should have no error");
    });
}

#[test]
fn test_ai_workflow_list_profiles_handles_corrupt_yaml_without_crashing() {
    with_temp_test_home("list-corrupt", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        fs::write(
            profiles_dir.join("valid.yaml"),
            "version: 1.0.0\nagents:\n  test:\n    codex: { model: gpt-5.6, reasoning_effort: high }\n",
        )
        .expect("write valid.yaml");

        fs::write(
            profiles_dir.join("broken.yaml"),
            ":::: invalid yaml content\n agents: [unmatched",
        )
        .expect("write broken.yaml");

        let list = list_profiles(Some(home)).expect("list_profiles should not crash on broken yaml");
        assert_eq!(list.len(), 2);

        let broken_entry = list
            .iter()
            .find(|p| p.name == "broken")
            .expect("broken entry should be listed");
        assert!(broken_entry.error.is_some(), "broken yaml must have an actionable error message");
        let error_msg = broken_entry.error.as_ref().unwrap();
        assert!(!error_msg.trim().is_empty(), "error message should not be empty");

        let valid_entry = list
            .iter()
            .find(|p| p.name == "valid")
            .expect("valid entry should be listed");
        assert!(valid_entry.error.is_none());
    });
}

#[test]
fn test_ai_workflow_list_profiles_empty_or_missing_directory_returns_empty_safely() {
    with_temp_test_home("list-empty", |home| {
        // directory does not exist yet
        let list = list_profiles(Some(home)).expect("should succeed safely on missing directory");
        assert!(list.is_empty(), "empty directory must yield empty vec");
    });
}

// ============================================================================
// 2. get_profile_matrix tests
// ============================================================================

#[test]
fn test_ai_workflow_get_profile_matrix_loads_9_roles_by_3_tools() {
    with_temp_test_home("matrix-load", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        let content = r#"version: 1.0.0
agents:
  backend:
    codex: { model: gpt-6-astra, reasoning_effort: medium }
    claude: { model: qwen3.6-plus, reasoning_effort: medium }
    opencode: { model: apigateway/deepseek-v4, reasoning_effort: high }
  test:
    codex: { model: gpt-5.6-terra, reasoning_effort: high }
"#;
        fs::write(profiles_dir.join("team.yaml"), content).expect("write team.yaml");

        let matrix = get_profile_matrix("team", Some(home)).expect("get_profile_matrix should succeed");
        assert_eq!(matrix.name, "team");
        assert_eq!(matrix.rows.len(), 9, "matrix must have exactly 9 rows for the 9 schema roles");

        let role_names: Vec<&str> = matrix.rows.iter().map(|r| r.role.as_str()).collect();
        assert_eq!(role_names, SUPPORTED_ROLES, "roles must follow standard supported role order");

        // backend row
        let backend_row = matrix.rows.iter().find(|r| r.role == "backend").unwrap();
        assert_eq!(
            backend_row.codex,
            Some(ModelEffort {
                model: "gpt-6-astra".to_string(),
                reasoning_effort: "medium".to_string()
            })
        );
        assert_eq!(
            backend_row.claude,
            Some(ModelEffort {
                model: "qwen3.6-plus".to_string(),
                reasoning_effort: "medium".to_string()
            })
        );
        assert_eq!(
            backend_row.opencode,
            Some(ModelEffort {
                model: "apigateway/deepseek-v4".to_string(),
                reasoning_effort: "high".to_string()
            })
        );

        // test row (codex set, claude & opencode missing)
        let test_row = matrix.rows.iter().find(|r| r.role == "test").unwrap();
        assert_eq!(
            test_row.codex,
            Some(ModelEffort {
                model: "gpt-5.6-terra".to_string(),
                reasoning_effort: "high".to_string()
            })
        );
        assert_eq!(test_row.claude, None, "unconfigured tool must be None");
        assert_eq!(test_row.opencode, None, "unconfigured tool must be None");

        // unconfigured role (e.g. frontend, researcher)
        let frontend_row = matrix.rows.iter().find(|r| r.role == "frontend").unwrap();
        assert_eq!(frontend_row.codex, None);
        assert_eq!(frontend_row.claude, None);
        assert_eq!(frontend_row.opencode, None);
    });
}

#[test]
fn test_ai_workflow_get_profile_matrix_invalid_profile_name_rejected() {
    with_temp_test_home("matrix-invalid-name", |home| {
        let invalid_names = ["../escaping", "-invalid-start", "has space", "bad@char", ""];
        for name in invalid_names {
            let res = get_profile_matrix(name, Some(home));
            assert!(
                res.is_err(),
                "profile name '{}' must be rejected as invalid",
                name
            );
            let err = res.unwrap_err();
            assert!(
                err.contains("name") || err.contains("invalid") || err.contains("Invalid"),
                "error for '{}' should explain name validation: {}",
                name,
                err
            );
        }
    });
}

#[test]
fn test_ai_workflow_get_profile_matrix_nonexistent_or_corrupt_file_fails_safely() {
    with_temp_test_home("matrix-nonexistent", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        let err = get_profile_matrix("nonexistent", Some(home)).unwrap_err();
        assert!(err.contains("not found") || err.contains("nonexistent") || err.contains("No such file"));

        fs::write(profiles_dir.join("corrupt.yaml"), "{ broken json/yaml").expect("write corrupt.yaml");
        let err_corrupt = get_profile_matrix("corrupt", Some(home)).unwrap_err();
        assert!(err_corrupt.contains("corrupt") || err_corrupt.contains("parse") || err_corrupt.contains("YAML"));
    });
}

// ============================================================================
// 3. get_model_sources tests
// ============================================================================

#[test]
fn test_ai_workflow_get_model_sources_opencode_extracts_provider_models() {
    with_temp_test_home("sources-opencode", |home| {
        let opencode_dir = home.join(".config/opencode");
        fs::create_dir_all(&opencode_dir).expect("create opencode dir");

        let opencode_json = r#"{
  "provider": {
    "apigateway": {
      "models": {
        "GLM-5": { "name": "GLM-5" },
        "deepseek-v4": { "name": "DeepSeek V4" }
      }
    },
    "command": {
      "models": {
        "deepseek/r1": { "name": "DeepSeek R1" }
      }
    }
  }
}"#;
        fs::write(opencode_dir.join("opencode.json"), opencode_json).expect("write opencode.json");

        let res = get_model_sources(Some(home)).expect("get_model_sources should succeed");
        assert!(res.opencode.error.is_none());

        assert!(
            res.opencode.models.contains(&"apigateway/GLM-5".to_string()),
            "must contain apigateway/GLM-5"
        );
        assert!(
            res.opencode.models.contains(&"apigateway/deepseek-v4".to_string()),
            "must contain apigateway/deepseek-v4"
        );
        assert!(
            res.opencode.models.contains(&"command/deepseek/r1".to_string()),
            "must contain command/deepseek/r1 (three-part preserved)"
        );
    });
}

#[test]
fn test_ai_workflow_get_model_sources_codex_unions_top_level_profiles_and_existing_profiles() {
    with_temp_test_home("sources-codex", |home| {
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");

        let config_toml = r#"
model = "gpt-5.6-sol"

[profiles.coding]
model = "gpt-5.6-terra"

[profiles.review]
model = "gpt-6-astra"

[model_providers.onespace_custom]
name = "Custom Provider Label"
"#;
        fs::write(codex_dir.join("config.toml"), config_toml).expect("write config.toml");

        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");
        fs::write(
            profiles_dir.join("existing.yaml"),
            "version: 1.0.0\nagents:\n  backend:\n    codex: { model: gpt-5.6-luna, reasoning_effort: medium }\n",
        )
        .expect("write existing.yaml");

        let res = get_model_sources(Some(home)).expect("get_model_sources should succeed");
        assert!(res.codex.error.is_none());

        assert!(res.codex.models.contains(&"gpt-5.6-sol".to_string()));
        assert!(res.codex.models.contains(&"gpt-5.6-terra".to_string()));
        assert!(res.codex.models.contains(&"gpt-6-astra".to_string()));
        assert!(res.codex.models.contains(&"gpt-5.6-luna".to_string()));

        // Ensure model_providers name is never added as a model
        assert!(
            !res.codex.models.contains(&"onespace_custom".to_string()),
            "model_providers id must NOT be included as a model"
        );
        assert!(
            !res.codex.models.contains(&"Custom Provider Label".to_string()),
            "model_providers name must NOT be included as a model"
        );
    });
}

#[test]
fn test_ai_workflow_get_model_sources_claude_extracts_named_envs_and_existing_profiles() {
    with_temp_test_home("sources-claude", |home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).expect("create claude dir");

        let settings_json = r#"{
  "env": {
    "ANTHROPIC_MODEL": "claude-3-5-sonnet",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-3-opus",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-3-sonnet",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "claude-3-haiku",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME": "Haiku",
    "CLAUDE_CODE_EFFORT_LEVEL": "xhigh"
  }
}"#;
        fs::write(claude_dir.join("settings.json"), settings_json).expect("write settings.json");

        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");
        fs::write(
            profiles_dir.join("existing.yaml"),
            "version: 1.0.0\nagents:\n  test:\n    claude: { model: qwen3.6-plus, reasoning_effort: high }\n",
        )
        .expect("write existing.yaml");

        let res = get_model_sources(Some(home)).expect("get_model_sources should succeed");
        assert!(res.claude.error.is_none());

        assert!(res.claude.models.contains(&"claude-3-5-sonnet".to_string()));
        assert!(res.claude.models.contains(&"claude-3-opus".to_string()));
        assert!(res.claude.models.contains(&"claude-3-sonnet".to_string()));
        assert!(res.claude.models.contains(&"claude-3-haiku".to_string()));
        assert!(res.claude.models.contains(&"qwen3.6-plus".to_string()));

        // Exclusions: *_MODEL_NAME and CLAUDE_CODE_EFFORT_LEVEL
        assert!(
            !res.claude.models.contains(&"Haiku".to_string()),
            "ANTHROPIC_*_MODEL_NAME must be excluded"
        );
        assert!(
            !res.claude.models.contains(&"xhigh".to_string()),
            "CLAUDE_CODE_EFFORT_LEVEL must be excluded"
        );
    });
}

#[test]
fn test_ai_workflow_get_model_sources_codex_degrades_without_blocking_other_columns() {
    with_temp_test_home("sources-codex-degrade", |home| {
        // Broken codex toml
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");
        fs::write(codex_dir.join("config.toml"), "[invalid toml syntax :::").expect("write bad toml");

        // Valid opencode
        let opencode_dir = home.join(".config/opencode");
        fs::create_dir_all(&opencode_dir).expect("create opencode dir");
        fs::write(
            opencode_dir.join("opencode.json"),
            r#"{"provider":{"apigateway":{"models":{"deepseek-chat":{}}}}}"#,
        )
        .expect("write opencode.json");

        // Valid claude
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).expect("create claude dir");
        fs::write(
            claude_dir.join("settings.json"),
            r#"{"env":{"ANTHROPIC_MODEL":"claude-3-7-sonnet"}}"#,
        )
        .expect("write settings.json");

        let res = get_model_sources(Some(home)).expect("broken single column must not block overall result");
        assert!(res.codex.error.is_some(), "codex must have an actionable error");
        assert!(res.opencode.error.is_none(), "opencode must remain healthy");
        assert!(res.opencode.models.contains(&"apigateway/deepseek-chat".to_string()));
        assert!(res.claude.error.is_none(), "claude must remain healthy");
        assert!(res.claude.models.contains(&"claude-3-7-sonnet".to_string()));
    });
}

#[test]
fn test_ai_workflow_get_model_sources_opencode_degrades_without_blocking_other_columns() {
    with_temp_test_home("sources-opencode-degrade", |home| {
        // Broken opencode json
        let opencode_dir = home.join(".config/opencode");
        fs::create_dir_all(&opencode_dir).expect("create opencode dir");
        fs::write(opencode_dir.join("opencode.json"), "invalid json { :::").expect("write bad json");

        // Valid codex
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).expect("create codex dir");
        fs::write(codex_dir.join("config.toml"), "model = \"gpt-5.6-terra\"").expect("write toml");

        // Valid claude
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).expect("create claude dir");
        fs::write(
            claude_dir.join("settings.json"),
            r#"{"env":{"ANTHROPIC_MODEL":"claude-3-5-sonnet"}}"#,
        )
        .expect("write settings.json");

        let res = get_model_sources(Some(home)).expect("broken single column must not block overall result");
        assert!(res.opencode.error.is_some(), "opencode must have an actionable error");
        assert!(res.codex.error.is_none(), "codex must remain healthy");
        assert!(res.codex.models.contains(&"gpt-5.6-terra".to_string()));
        assert!(res.claude.error.is_none(), "claude must remain healthy");
        assert!(res.claude.models.contains(&"claude-3-5-sonnet".to_string()));
    });
}

#[test]
fn test_ai_workflow_save_profile_persists_yaml_without_cli_activation() {
    with_temp_test_home("save-only", |home| {
        let config_dir = home.join(".config/ai-workflow");
        fs::create_dir_all(&config_dir).expect("create config dir");
        let config_path = config_dir.join("config.yaml");
        let original_config = "active_profile: existing-profile\n";
        fs::write(&config_path, original_config).expect("write initial config");

        // A save-only operation must not require or invoke the CLI.
        std::env::set_var(
            "AI_WORKFLOW_CLI_PATH",
            home.join("missing-ai-workflow-cli"),
        );

        let rows = vec![AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "gpt-6-astra".to_string(),
                reasoning_effort: "medium".to_string(),
            }),
            claude: None,
            opencode: None,
        }];

        save_profile("draft-profile", &rows, Some(home))
            .expect("saving a valid profile must succeed without the CLI");

        let profile_path = home
            .join(".config/ai-workflow/profiles/draft-profile.yaml");
        let content = fs::read_to_string(&profile_path).expect("read persisted profile YAML");
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(&content).expect("persisted profile must be valid YAML");
        assert_eq!(yaml["version"].as_str(), Some("1.0.0"));
        assert_eq!(yaml["agents"]["backend"]["codex"]["model"].as_str(), Some("gpt-6-astra"));
        assert_eq!(
            yaml["agents"]["backend"]["codex"]["reasoning_effort"].as_str(),
            Some("medium")
        );

        assert_eq!(
            fs::read_to_string(&config_path).expect("read config after save"),
            original_config,
            "saving a profile must not change the active profile"
        );
    });
}

// ============================================================================
// 5. activate_profile tests
// ============================================================================

#[test]
fn test_ai_workflow_activate_profile_does_not_rewrite_yaml() {
    with_temp_test_home("activate-no-rewrite", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        let target_file = profiles_dir.join("static-profile.yaml");
        let original_yaml = "version: 1.0.0\nagents:\n  backend:\n    codex: { model: gpt-5.6, reasoning_effort: medium }\n";
        fs::write(&target_file, original_yaml).expect("write static yaml");
        let original_bytes = fs::read(&target_file).expect("read original bytes");

        let bin_dir = home.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let mock_cli = create_mock_cli(
            &bin_dir,
            "ai-workflow",
            r#"#!/bin/sh
cat << 'EOF'
{
  "active_profile": "static-profile",
  "hosts": ["codex"],
  "installations": []
}
EOF
exit 0
"#,
        );
        std::env::set_var("AI_WORKFLOW_CLI_PATH", &mock_cli);

        let report = activate_profile("static-profile", Some(home)).expect("direct activate should succeed");
        assert_eq!(report.active_profile, "static-profile");

        let after_bytes = fs::read(&target_file).expect("read after bytes");
        assert_eq!(
            after_bytes, original_bytes,
            "direct activation must NOT rewrite or modify the profile YAML"
        );
    });
}

#[test]
fn test_ai_workflow_activate_profile_missing_cli_binary_returns_actionable_error() {
    with_temp_test_home("activate-missing-cli", |home| {
        // Point CLI path to nonexistent file
        std::env::set_var("AI_WORKFLOW_CLI_PATH", home.join("nonexistent-ai-workflow-bin"));

        let config_dir = home.join(".config/ai-workflow");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(config_dir.join("config.yaml"), "active_profile: initial-profile\n").expect("write config");

        let res = activate_profile("target-profile", Some(home));
        assert!(res.is_err(), "missing binary must fail");
        let err = res.unwrap_err();
        assert!(
            err.contains("binary") || err.contains("not found") || err.contains("PATH"),
            "must return actionable binary missing error: {}",
            err
        );

        // State preserved
        let config_content = fs::read_to_string(config_dir.join("config.yaml")).expect("read config");
        assert!(
            config_content.contains("initial-profile"),
            "active_profile must remain unchanged"
        );
    });
}

#[test]
fn test_ai_workflow_activate_profile_managed_file_conflict_echoes_verbatim_cli_error() {
    with_temp_test_home("activate-conflict", |home| {
        let bin_dir = home.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let mock_cli = create_mock_cli(
            &bin_dir,
            "ai-workflow",
            r#"#!/bin/sh
echo "Managed agent file .claude/agents/backend.md was modified externally. Refusing activation." >&2
exit 1
"#,
        );
        std::env::set_var("AI_WORKFLOW_CLI_PATH", &mock_cli);

        let config_dir = home.join(".config/ai-workflow");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(config_dir.join("config.yaml"), "active_profile: current-active\n").expect("write config");

        let res = activate_profile("new-profile", Some(home));
        assert!(res.is_err(), "conflict must cause activation to fail");
        let err = res.unwrap_err();
        assert!(
            err.contains("Managed agent file .claude/agents/backend.md was modified externally"),
            "verbatim CLI error must be returned: {}",
            err
        );

        // active_profile preserved
        let config_content = fs::read_to_string(config_dir.join("config.yaml")).expect("read config");
        assert!(
            config_content.contains("current-active"),
            "active_profile must remain preserved on conflict error"
        );
    });
}

// ============================================================================
// 6. create_profile & delete_profile tests
// ============================================================================

#[test]
fn test_ai_workflow_create_profile_blank_and_duplicate_handling() {
    with_temp_test_home("create-profile", |home| {
        // 创建空方案
        let res = create_profile("my-new-profile", None, Some(home));
        assert!(res.is_ok(), "create blank profile should succeed: {:?}", res);

        let target_file = home.join(".config/ai-workflow/profiles/my-new-profile.yaml");
        assert!(target_file.exists(), "target yaml file must exist");
        let content = fs::read_to_string(&target_file).expect("read created yaml");
        assert!(content.contains("version: 1.0.0"));

        // 重复创建相同名称应当报错
        let dup_res = create_profile("my-new-profile", None, Some(home));
        assert!(dup_res.is_err(), "duplicate profile creation must fail");
        assert!(dup_res.unwrap_err().contains("already exists"));

        // 非法名称应当报错
        let invalid_res = create_profile("invalid name with spaces", None, Some(home));
        assert!(invalid_res.is_err(), "invalid profile name must fail");
    });
}

#[test]
fn test_ai_workflow_create_profile_clones_existing() {
    with_temp_test_home("create-profile-clone", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        let source_content = "version: 1.0.0\nagents:\n  backend:\n    codex:\n      model: test-model\n      reasoning_effort: high\n";
        fs::write(profiles_dir.join("source-profile.yaml"), source_content).expect("write source");

        let res = create_profile("cloned-profile", Some("source-profile"), Some(home));
        assert!(res.is_ok(), "clone profile should succeed: {:?}", res);

        let cloned_file = profiles_dir.join("cloned-profile.yaml");
        assert!(cloned_file.exists());
        let cloned_content = fs::read_to_string(&cloned_file).expect("read cloned");
        assert_eq!(cloned_content, source_content);

        // 尝试从不存在的源复制
        let non_exist_res = create_profile("other-profile", Some("non-existent"), Some(home));
        assert!(non_exist_res.is_err());
        assert!(non_exist_res.unwrap_err().contains("does not exist"));
    });
}

#[test]
fn test_ai_workflow_delete_profile_removes_file_and_protects_active() {
    with_temp_test_home("delete-profile", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        fs::write(profiles_dir.join("active-profile.yaml"), "version: 1.0.0\n").expect("write active");
        fs::write(profiles_dir.join("inactive-profile.yaml"), "version: 1.0.0\n").expect("write inactive");

        let config_dir = home.join(".config/ai-workflow");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(config_dir.join("config.yaml"), "active_profile: active-profile\n").expect("write config");

        // 删除激活中的方案应当被阻止
        let del_active = delete_profile("active-profile", Some(home));
        assert!(del_active.is_err(), "deleting active profile must be rejected");
        assert!(del_active.unwrap_err().contains("Cannot delete active profile"));
        assert!(profiles_dir.join("active-profile.yaml").exists());

        // 删除非激活方案应当成功
        let del_inactive = delete_profile("inactive-profile", Some(home));
        assert!(del_inactive.is_ok(), "deleting inactive profile must succeed: {:?}", del_inactive);
        assert!(!profiles_dir.join("inactive-profile.yaml").exists());
    });
}

#[test]
fn test_ai_workflow_rename_profile_success_and_updates_active() {
    with_temp_test_home("rename-profile", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        let active_content = "version: 1.0.0\nagents:\n  backend:\n    codex: { model: gpt-4, reasoning_effort: high }\n";
        fs::write(profiles_dir.join("active-profile.yaml"), active_content).expect("write active");
        fs::write(profiles_dir.join("other-profile.yaml"), "version: 1.0.0\n").expect("write other");

        let config_dir = home.join(".config/ai-workflow");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(config_dir.join("config.yaml"), "active_profile: active-profile\n").expect("write config");

        // 1. 重命名非激活方案
        let res_other = rename_profile("other-profile", "renamed-other", Some(home));
        assert!(res_other.is_ok(), "rename other profile should succeed: {:?}", res_other);
        assert!(!profiles_dir.join("other-profile.yaml").exists());
        assert!(profiles_dir.join("renamed-other.yaml").exists());

        // 2. 重命名激活方案，config.yaml 中的 active_profile 同步更新
        let res_active = rename_profile("active-profile", "renamed-active", Some(home));
        assert!(res_active.is_ok(), "rename active profile should succeed: {:?}", res_active);
        assert!(!profiles_dir.join("active-profile.yaml").exists());
        assert!(profiles_dir.join("renamed-active.yaml").exists());

        let new_content = fs::read_to_string(profiles_dir.join("renamed-active.yaml")).expect("read renamed");
        assert_eq!(new_content, active_content);

        // 校验 config.yaml 的 active_profile
        let config_str = fs::read_to_string(config_dir.join("config.yaml")).expect("read config");
        assert!(config_str.contains("active_profile: renamed-active"), "config.yaml must be updated to new name: {}", config_str);

        // 校验 list_profiles 依然将 renamed-active 标识为 active
        let list = list_profiles(Some(home)).expect("list profiles");
        let active_entry = list.iter().find(|p| p.name == "renamed-active").expect("renamed-active should exist");
        assert!(active_entry.active, "renamed-active must be active");
    });
}

#[test]
fn test_ai_workflow_rename_profile_validations() {
    with_temp_test_home("rename-validations", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        fs::write(profiles_dir.join("profile-a.yaml"), "version: 1.0.0\n").expect("write a");
        fs::write(profiles_dir.join("profile-b.yaml"), "version: 1.0.0\n").expect("write b");

        // 1. 新旧名称相同，返回 Ok(())
        let res_same = rename_profile("profile-a", "profile-a", Some(home));
        assert!(res_same.is_ok());

        // 2. 原方案不存在，报错
        let res_nonexistent = rename_profile("nonexistent", "new-name", Some(home));
        assert!(res_nonexistent.is_err());
        assert!(res_nonexistent.unwrap_err().contains("does not exist"));

        // 3. 目标名称已存在，报错
        let res_conflict = rename_profile("profile-a", "profile-b", Some(home));
        assert!(res_conflict.is_err());
        assert!(res_conflict.unwrap_err().contains("already exists"));

        // 4. 名称格式非法，报错
        let res_invalid = rename_profile("profile-a", "invalid name!", Some(home));
        assert!(res_invalid.is_err());
        assert!(res_invalid.unwrap_err().contains("Invalid profile name"));
    });
}

// ============================================================================
// 7. get_active_models tests
//
// `get_active_models` reads the CURRENTLY INSTALLED per-host agent files for
// the active profile only. It must never read saved profile YAML or invoke the
// CLI, and any corrupt existing file/config must surface an error rather than
// stale data.
// ============================================================================

fn write_active_profile_config(home: &Path, name: &str) {
    let dir = home.join(".config/ai-workflow");
    fs::create_dir_all(&dir).expect("create ai-workflow config dir");
    fs::write(dir.join("config.yaml"), format!("active_profile: {}\n", name))
        .expect("write config.yaml");
}

fn write_installed_codex_agent(home: &Path, role: &str, body: &str) {
    let dir = home.join(".codex/agents");
    fs::create_dir_all(&dir).expect("create codex agents dir");
    fs::write(dir.join(format!("{}.toml", role)), body).expect("write codex agent toml");
}

fn write_installed_claude_agent(home: &Path, role: &str, frontmatter: &str) {
    let dir = home.join(".claude/agents");
    fs::create_dir_all(&dir).expect("create claude agents dir");
    let content = format!("---\n{}---\n\nBody for {}.\n", frontmatter, role);
    fs::write(dir.join(format!("{}.md", role)), content).expect("write claude agent md");
}

fn write_installed_opencode_agent(home: &Path, role: &str, frontmatter: &str) {
    let dir = home.join(".config/opencode/agents");
    fs::create_dir_all(&dir).expect("create opencode agents dir");
    let content = format!("---\n{}---\n\nBody for {}.\n", frontmatter, role);
    fs::write(dir.join(format!("{}.md", role)), content).expect("write opencode agent md");
}

#[test]
fn test_ai_workflow_get_active_models_uses_installed_hosts_not_saved_profile() {
    with_temp_test_home("active-models-installed", |home| {
        write_active_profile_config(home, "team-alpha");

        // A saved profile YAML with different sentinel values. It must never be
        // used as the source of truth, and it must not even be required to exist.
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");
        fs::write(
            profiles_dir.join("team-alpha.yaml"),
            "version: 1.0.0\nagents:\n  backend:\n    codex: { model: saved-sentinel, reasoning_effort: low }\n  frontend:\n    claude: { model: saved-sentinel, reasoning_effort: low }\n  test:\n    opencode: { model: saved-sentinel, reasoning_effort: low }\n",
        )
        .expect("write saved profile yaml");

        write_installed_codex_agent(
            home,
            "backend",
            "model = \"codex-installed-model\"\nmodel_reasoning_effort = \"high\"\n",
        );
        write_installed_claude_agent(
            home,
            "frontend",
            "model: claude-installed-model\neffort: medium\n",
        );
        write_installed_opencode_agent(
            home,
            "test",
            "model: opencode-installed-model\nreasoningEffort: low\n",
        );

        let tracked_paths = [
            home.join(".codex/agents/backend.toml"),
            home.join(".claude/agents/frontend.md"),
            home.join(".config/opencode/agents/test.md"),
            home.join(".config/ai-workflow/config.yaml"),
            profiles_dir.join("team-alpha.yaml"),
        ];
        let before: Vec<Vec<u8>> = tracked_paths
            .iter()
            .map(|path| fs::read(path).expect("read tracked file before"))
            .collect();

        let matrix = get_active_models(Some(home))
            .expect("get_active_models must succeed on valid installed fixtures")
            .expect("a config with an active_profile must yield Some(ProfileMatrix)");

        assert_eq!(
            matrix.name, "team-alpha",
            "the name must come from config.yaml active_profile"
        );
        assert_eq!(matrix.rows.len(), 9, "all supported roles must be present");
        let role_names: Vec<&str> = matrix.rows.iter().map(|row| row.role.as_str()).collect();
        assert_eq!(role_names, SUPPORTED_ROLES);

        let backend = matrix.rows.iter().find(|row| row.role == "backend").unwrap();
        assert_eq!(
            backend.codex,
            Some(ModelEffort {
                model: "codex-installed-model".to_string(),
                reasoning_effort: "high".to_string(),
            }),
            "codex model/effort must come from the installed TOML"
        );
        assert_eq!(backend.claude, None, "a missing claude file must be None");
        assert_eq!(backend.opencode, None, "a missing opencode file must be None");

        let frontend = matrix.rows.iter().find(|row| row.role == "frontend").unwrap();
        assert_eq!(
            frontend.claude,
            Some(ModelEffort {
                model: "claude-installed-model".to_string(),
                reasoning_effort: "medium".to_string(),
            }),
            "claude model/effort must come from the installed Markdown frontmatter"
        );
        assert_eq!(frontend.codex, None);

        let test_row = matrix.rows.iter().find(|row| row.role == "test").unwrap();
        assert_eq!(
            test_row.opencode,
            Some(ModelEffort {
                model: "opencode-installed-model".to_string(),
                reasoning_effort: "low".to_string(),
            }),
            "opencode model/effort must come from the installed Markdown frontmatter"
        );

        // The saved profile sentinels must never surface anywhere.
        for row in &matrix.rows {
            for host in [&row.codex, &row.claude, &row.opencode] {
                if let Some(entry) = host {
                    assert_ne!(
                        entry.model, "saved-sentinel",
                        "saved profile YAML values must never be returned"
                    );
                }
            }
        }

        // Reading must not mutate any tracked file.
        let after: Vec<Vec<u8>> = tracked_paths
            .iter()
            .map(|path| fs::read(path).expect("read tracked file after"))
            .collect();
        assert_eq!(after, before, "reading active models must never mutate files");
    });
}

#[test]
fn test_ai_workflow_get_active_models_reads_fresh_values_after_edit() {
    with_temp_test_home("active-models-fresh", |home| {
        write_active_profile_config(home, "team-alpha");
        write_installed_codex_agent(
            home,
            "backend",
            "model = \"codex-first\"\nmodel_reasoning_effort = \"low\"\n",
        );

        let first = get_active_models(Some(home)).unwrap().unwrap();
        let first_backend = first.rows.iter().find(|row| row.role == "backend").unwrap();
        assert_eq!(first_backend.codex.as_ref().unwrap().model, "codex-first");

        write_installed_codex_agent(
            home,
            "backend",
            "model = \"codex-second\"\nmodel_reasoning_effort = \"xhigh\"\n",
        );
        let second = get_active_models(Some(home)).unwrap().unwrap();
        let second_backend = second.rows.iter().find(|row| row.role == "backend").unwrap();
        assert_eq!(
            second_backend.codex,
            Some(ModelEffort {
                model: "codex-second".to_string(),
                reasoning_effort: "xhigh".to_string(),
            }),
            "a subsequent read must observe the edited installed value"
        );

        write_installed_claude_agent(home, "frontend", "model: claude-first\neffort: low\n");
        let third = get_active_models(Some(home)).unwrap().unwrap();
        let third_frontend = third.rows.iter().find(|row| row.role == "frontend").unwrap();
        assert_eq!(
            third_frontend.claude.as_ref().unwrap().model,
            "claude-first"
        );

        write_installed_claude_agent(home, "frontend", "model: claude-second\neffort: high\n");
        let fourth = get_active_models(Some(home)).unwrap().unwrap();
        let fourth_frontend = fourth.rows.iter().find(|row| row.role == "frontend").unwrap();
        assert_eq!(
            fourth_frontend.claude.as_ref().unwrap().model,
            "claude-second"
        );
    });
}

#[test]
fn test_ai_workflow_get_active_models_blank_fields_yield_empty_strings() {
    with_temp_test_home("active-models-blank", |home| {
        write_active_profile_config(home, "team-alpha");

        // Present codex file with a model but no effort.
        write_installed_codex_agent(home, "backend", "model = \"codex-only-model\"\n");
        // Present codex file with no model and no effort.
        write_installed_codex_agent(home, "researcher", "# configuration only\n");
        // Present claude file with an effort but no model.
        write_installed_claude_agent(home, "frontend", "effort: high\n");

        let matrix = get_active_models(Some(home)).unwrap().unwrap();

        let backend = matrix.rows.iter().find(|row| row.role == "backend").unwrap();
        assert_eq!(
            backend.codex,
            Some(ModelEffort {
                model: "codex-only-model".to_string(),
                reasoning_effort: String::new(),
            }),
            "a present host with a missing effort must yield an empty effort string"
        );

        let researcher = matrix.rows.iter().find(|row| row.role == "researcher").unwrap();
        assert_eq!(
            researcher.codex,
            Some(ModelEffort {
                model: String::new(),
                reasoning_effort: String::new(),
            }),
            "a present host with no fields must yield empty strings, not None"
        );

        let frontend = matrix.rows.iter().find(|row| row.role == "frontend").unwrap();
        assert_eq!(
            frontend.claude,
            Some(ModelEffort {
                model: String::new(),
                reasoning_effort: "high".to_string(),
            }),
            "a present host with a missing model must yield an empty model string"
        );
    });
}

#[test]
fn test_ai_workflow_get_active_models_missing_config_or_active_is_none() {
    with_temp_test_home("active-models-none", |home| {
        let missing = get_active_models(Some(home)).expect("a missing config must not error");
        assert!(missing.is_none(), "a missing config.yaml must yield None");

        let config_dir = home.join(".config/ai-workflow");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(config_dir.join("config.yaml"), "other_setting: true\n")
            .expect("write config without active profile");
        let no_active = get_active_models(Some(home))
            .expect("a config without active_profile must not error");
        assert!(
            no_active.is_none(),
            "a config without active_profile must yield None"
        );

        fs::write(config_dir.join("config.yaml"), "active_profile: \"\"\n")
            .expect("write blank active profile");
        let blank_active = get_active_models(Some(home))
            .expect("a blank active_profile must not error");
        assert!(
            blank_active.is_none(),
            "a blank active_profile must yield None"
        );

        // Installed files alone must never fabricate an active profile.
        write_installed_codex_agent(
            home,
            "backend",
            "model = \"codex-installed\"\nmodel_reasoning_effort = \"high\"\n",
        );
        fs::write(config_dir.join("config.yaml"), "other_setting: true\n")
            .expect("write config without active profile again");
        let installed_only = get_active_models(Some(home))
            .expect("installed files without an active profile must not error");
        assert!(
            installed_only.is_none(),
            "installed files must not fabricate an active profile"
        );
    });
}

#[test]
fn test_ai_workflow_get_active_models_corrupt_config_is_error() {
    with_temp_test_home("active-models-corrupt-config", |home| {
        let config_dir = home.join(".config/ai-workflow");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(
            config_dir.join("config.yaml"),
            ":::: invalid yaml content\n active: [unmatched",
        )
        .expect("write corrupt config.yaml");

        let res = get_active_models(Some(home));
        assert!(
            res.is_err(),
            "a corrupt config.yaml must be an error, never stale data"
        );
    });
}

#[test]
fn test_ai_workflow_get_active_models_corrupt_installed_host_is_error() {
    with_temp_test_home("active-models-corrupt-hosts", |home| {
        write_active_profile_config(home, "team-alpha");

        // Corrupt codex TOML.
        write_installed_codex_agent(home, "backend", "[invalid toml syntax :::");
        assert!(
            get_active_models(Some(home)).is_err(),
            "a corrupt codex agent TOML must be an error"
        );

        // Corrupt claude frontmatter, with the codex file restored to valid.
        write_installed_codex_agent(
            home,
            "backend",
            "model = \"ok\"\nmodel_reasoning_effort = \"low\"\n",
        );
        write_installed_claude_agent(home, "frontend", "model: [unclosed\n");
        assert!(
            get_active_models(Some(home)).is_err(),
            "corrupt claude frontmatter must be an error"
        );

        // Corrupt opencode frontmatter, with the claude file restored to valid.
        write_installed_claude_agent(home, "frontend", "model: ok\neffort: low\n");
        write_installed_opencode_agent(home, "test", "model: { broken\n");
        assert!(
            get_active_models(Some(home)).is_err(),
            "corrupt opencode frontmatter must be an error"
        );
    });
}

// ============================================================================
// 8. Local model capability discovery and atomic Save refusal
//
// The capability DTO is additive, so these tests assert it through the
// serialized public `get_model_sources` boundary: a missing field is a
// behavioral RED, never a compile error. No private parser shape is mirrored.
// ============================================================================

fn model_sources_json(home: &Path) -> serde_json::Value {
    let result = get_model_sources(Some(home)).expect("get_model_sources must succeed");
    serde_json::to_value(&result).expect("model sources must serialize to JSON")
}

fn capability_entry<'a>(
    value: &'a serde_json::Value,
    host: &str,
    model_id: &str,
) -> Option<&'a serde_json::Value> {
    value.get(host)?.get("model_capabilities")?.get(model_id)
}

fn effort_strings(entry: &serde_json::Value) -> Vec<String> {
    entry
        .get("efforts")
        .and_then(|efforts| efforts.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn sorted_efforts(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

/// True when a column surfaces any non-empty diagnostic string besides `models`.
/// The exact field name is deliberately not hard-coded; the contract only
/// requires that a capability-read failure produces a useful warning.
fn has_extra_diagnostic(column: &serde_json::Value) -> bool {
    column
        .as_object()
        .map(|map| {
            map.iter().any(|(key, value)| {
                key != "models"
                    && value
                        .as_str()
                        .map(|text| !text.trim().is_empty())
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn write_env_file(home: &Path, relative: &str, contents: &str) {
    let path = home.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture parent dir");
    }
    fs::write(&path, contents).expect("write fixture file");
}

#[test]
fn test_ai_workflow_model_sources_codex_reports_exact_slug_capabilities() {
    with_temp_test_home("cap-codex", |home| {
        // The exact slug stays a name candidate as well.
        write_env_file(home, ".codex/config.toml", "model = \"gpt-5.6-astra\"\n");

        let cache = r#"{
  "fetched_at": "2026-10-15T00:00:00Z",
  "api_key": "sk-fixture-codex-secret-000",
  "instructions": "SECRET-RAW-CONFIG-MARKER",
  "models": [
    {
      "slug": "gpt-5.6-astra",
      "display_name": "Astra",
      "default_reasoning_level": "high",
      "supported_reasoning_levels": [
        { "effort": "none", "description": "no reasoning" },
        { "effort": "low", "description": "light" },
        { "effort": "high", "description": "deep" }
      ]
    },
    {
      "slug": "gpt-empty-fallback",
      "default_reasoning_level": "high",
      "supported_reasoning_levels": []
    },
    {
      "slug": "gpt-default-only",
      "default_reasoning_level": "medium"
    },
    {
      "slug": "gpt-malformed",
      "default_reasoning_level": "high",
      "supported_reasoning_levels": "not-an-array"
    },
    {
      "slug": "gpt-blank-efforts",
      "default_reasoning_level": "high",
      "supported_reasoning_levels": [ { "effort": "" }, { "effort": "   " } ]
    }
  ]
}"#;
        write_env_file(home, ".codex/models_cache.json", cache);

        let value = model_sources_json(home);

        let codex_models: Vec<String> = value["codex"]["models"]
            .as_array()
            .expect("codex.models must be an array")
            .iter()
            .filter_map(|model| model.as_str().map(str::to_string))
            .collect();
        assert!(
            codex_models.contains(&"gpt-5.6-astra".to_string()),
            "codex name candidates must survive capability discovery: {:?}",
            codex_models
        );

        let entry = capability_entry(&value, "codex", "gpt-5.6-astra")
            .expect("an exact declared slug must expose a capability entry");
        assert_eq!(
            effort_strings(entry),
            vec!["none".to_string(), "low".to_string(), "high".to_string()],
            "raw declared efforts must follow the source declaration order"
        );
        assert_eq!(
            entry.get("default_effort").and_then(|value| value.as_str()),
            Some("high"),
            "the declared default_reasoning_level must surface as default_effort"
        );

        for unknown in [
            "gpt-empty-fallback",
            "gpt-default-only",
            "gpt-malformed",
            "gpt-blank-efforts",
            "gpt-absent-from-cache",
        ] {
            assert!(
                capability_entry(&value, "codex", unknown).is_none(),
                "{} must remain unknown (no capability entry)",
                unknown
            );
        }

        let serialized = value.to_string();
        assert!(
            !serialized.contains("sk-fixture-codex-secret-000"),
            "serialized capability output must never include fixture credentials"
        );
        assert!(
            !serialized.contains("SECRET-RAW-CONFIG-MARKER"),
            "serialized capability output must never include raw configuration"
        );
        assert!(
            !serialized.contains("api_key"),
            "serialized capability output must not carry raw configuration keys"
        );
    });
}

#[test]
fn test_ai_workflow_model_sources_opencode_reads_variant_effort_values() {
    with_temp_test_home("cap-opencode-variants", |home| {
        let config = r#"{
  "provider": {
    "apigateway": {
      "models": {
        "deepseek-v4": {
          "variants": {
            "fast": { "reasoningEffort": "low" },
            "strong": { "reasoningEffort": "high" },
            "maxed": { "reasoningEffort": "xhigh", "disabled": true }
          }
        },
        "deepseek/r1": {
          "variants": { "only": { "reasoningEffort": "medium" } }
        },
        "scalar-default": { "reasoningEffort": "high" },
        "bad-variant": { "variants": { "weird": { "reasoningEffort": 42 } } }
      }
    },
    "other": {
      "models": {
        "deepseek/r1": {
          "variants": { "only": { "reasoningEffort": "ultra" } }
        }
      }
    }
  }
}"#;
        write_env_file(home, ".config/opencode/opencode.json", config);

        let value = model_sources_json(home);

        let v4 = capability_entry(&value, "opencode", "apigateway/deepseek-v4")
            .expect("declared variants must expose an effort list");
        assert_eq!(
            sorted_efforts(effort_strings(v4)),
            vec!["high".to_string(), "low".to_string()],
            "effort values (not variant keys) must be read and disabled variants excluded"
        );

        let triple = capability_entry(&value, "opencode", "apigateway/deepseek/r1")
            .expect("a slash-containing model id must stay intact");
        assert_eq!(
            sorted_efforts(effort_strings(triple)),
            vec!["medium".to_string()]
        );

        let other = capability_entry(&value, "opencode", "other/deepseek/r1")
            .expect("a distinct provider/model identity must keep its own capabilities");
        assert_eq!(
            sorted_efforts(effort_strings(other)),
            vec!["ultra".to_string()],
            "capabilities must not be merged across providers or similar ids"
        );

        for unknown in ["apigateway/scalar-default", "apigateway/bad-variant"] {
            assert!(
                capability_entry(&value, "opencode", unknown).is_none(),
                "{} must remain unknown without an explicit effort declaration",
                unknown
            );
        }
    });
}

#[test]
fn test_ai_workflow_model_sources_opencode_declarations_override_catalog() {
    with_temp_test_home("cap-opencode-precedence", |home| {
        let config = r#"{
  "provider": {
    "apigateway": {
      "models": {
        "declared-wins": { "variants": { "fast": { "reasoningEffort": "low" } } },
        "all-disabled": {
          "variants": {
            "low-tier": { "reasoningEffort": "low", "disabled": true },
            "high-tier": { "reasoningEffort": "high", "disabled": true }
          }
        }
      }
    }
  }
}"#;
        write_env_file(home, ".config/opencode/opencode.json", config);

        // The optional local catalog may be keyed by provider id directly or
        // carry the same rows under a `providers` wrapper; both mirrors hold
        // identical rows so the assertion targets behavior, not an envelope.
        let catalog = r#"{
  "apigateway": {
    "id": "apigateway",
    "models": {
      "declared-wins": { "reasoning_options": [ { "type": "effort", "values": ["low", "high", "ultra"] } ] },
      "all-disabled": { "reasoning_options": [ { "type": "effort", "values": ["low", "high"] } ] }
    }
  },
  "providers": {
    "apigateway": {
      "id": "apigateway",
      "models": {
        "declared-wins": { "reasoning_options": [ { "type": "effort", "values": ["low", "high", "ultra"] } ] },
        "all-disabled": { "reasoning_options": [ { "type": "effort", "values": ["low", "high"] } ] }
      }
    }
  }
}"#;
        write_env_file(home, ".cache/opencode/models.json", catalog);

        let value = model_sources_json(home);

        let declared = capability_entry(&value, "opencode", "apigateway/declared-wins")
            .expect("declared variants must win over the catalog");
        assert_eq!(
            sorted_efforts(effort_strings(declared)),
            vec!["low".to_string()],
            "explicit configuration declarations must take precedence over catalog fallback"
        );

        let disabled = capability_entry(&value, "opencode", "apigateway/all-disabled")
            .expect("an all-disabled declaration is a known entry with no writable values");
        assert!(
            effort_strings(disabled).is_empty(),
            "declared-but-all-disabled variants must not regain catalog efforts: {:?}",
            effort_strings(disabled)
        );
    });
}

#[test]
fn test_ai_workflow_model_sources_opencode_catalog_effort_fallback() {
    with_temp_test_home("cap-opencode-catalog", |home| {
        let config = r#"{
  "provider": {
    "apigateway": {
      "models": {
        "deepseek-v4": {},
        "deepseek-v4-mini": {},
        "glm-5": {}
      }
    },
    "other": {
      "models": { "deepseek-v4": {} }
    }
  }
}"#;
        write_env_file(home, ".config/opencode/opencode.json", config);

        let catalog = r#"{
  "apigateway": {
    "id": "apigateway",
    "models": {
      "deepseek-v4": {
        "reasoning_options": [
          { "type": "budget_tokens", "values": ["1000"] },
          { "type": "effort", "values": ["low", "high"] }
        ]
      },
      "glm-5": {
        "reasoning_options": [ { "type": "effort", "values": "low" } ]
      }
    }
  },
  "other": {
    "id": "other",
    "models": {
      "deepseek-v4": { "reasoning_options": [ { "type": "effort", "values": ["ultra"] } ] }
    }
  },
  "providers": {
    "apigateway": {
      "id": "apigateway",
      "models": {
        "deepseek-v4": {
          "reasoning_options": [
            { "type": "budget_tokens", "values": ["1000"] },
            { "type": "effort", "values": ["low", "high"] }
          ]
        },
        "glm-5": {
          "reasoning_options": [ { "type": "effort", "values": "low" } ]
        }
      }
    },
    "other": {
      "id": "other",
      "models": {
        "deepseek-v4": { "reasoning_options": [ { "type": "effort", "values": ["ultra"] } ] }
      }
    }
  }
}"#;
        write_env_file(home, ".cache/opencode/models.json", catalog);

        let value = model_sources_json(home);

        let matched = capability_entry(&value, "opencode", "apigateway/deepseek-v4")
            .expect("an exact provider/model match may supply otherwise undeclared efforts");
        assert_eq!(
            sorted_efforts(effort_strings(matched)),
            vec!["high".to_string(), "low".to_string()],
            "only the type=effort option values may supply capabilities"
        );

        let other = capability_entry(&value, "opencode", "other/deepseek-v4")
            .expect("the other provider keeps its own catalog row");
        assert_eq!(
            sorted_efforts(effort_strings(other)),
            vec!["ultra".to_string()]
        );

        for unknown in ["apigateway/deepseek-v4-mini", "apigateway/glm-5"] {
            assert!(
                capability_entry(&value, "opencode", unknown).is_none(),
                "{} must not borrow a catalog row (similar name or malformed values)",
                unknown
            );
        }
    });
}

// ============================================================================
// 9. Dynamic free-model discovery for the self-operated OpenCode providers
//
// The two self-operated providers (`opencode`, `opencode-go`) must contribute
// their current free models from `~/.cache/opencode/models.json` to the
// OpenCode candidate list without hardcoding any model id and without relying
// on user declarations in `opencode.json`.
// ============================================================================

#[test]
fn test_ai_workflow_model_sources_opencode_merges_free_catalog_provider_models() {
    with_temp_test_home("cap-opencode-free", |home| {
        // No opencode.json at all: the dynamic merge must stand on its own.
        // `opencode` sits under the `providers` wrapper while `opencode-go`
        // sits directly at the root, proving both resolutions in one pass.
        let catalog = r#"{
  "providers": {
    "opencode": {
      "id": "opencode",
      "models": {
        "cat-jam-free": { "cost": { "input": 0, "output": 0 } },
        "big-pickle": { "cost": { "input": 0.0, "output": 0 } },
        "pricey-input": { "cost": { "input": 0.5, "output": 0 } },
        "pricey-output": { "cost": { "input": 0, "output": 0.25 } },
        "retired-free": {
          "cost": { "input": 0, "output": 0 },
          "status": "deprecated"
        }
      }
    },
    "unrelated": {
      "models": { "should-not-appear": { "cost": { "input": 0, "output": 0 } } }
    }
  },
  "opencode-go": {
    "id": "opencode-go",
    "models": {
      "step-5-preview-free": { "cost": { "input": 0, "output": 0 } }
    }
  }
}"#;
        write_env_file(home, ".cache/opencode/models.json", catalog);

        let value = model_sources_json(home);
        let models: Vec<String> = value["opencode"]["models"]
            .as_array()
            .expect("opencode.models must be an array")
            .iter()
            .filter_map(|model| model.as_str().map(str::to_string))
            .collect();

        assert!(
            models.contains(&"opencode/cat-jam-free".to_string()),
            "the wrapped free provider model must be discovered: {:?}",
            models
        );
        assert!(
            models.contains(&"opencode-go/step-5-preview-free".to_string()),
            "the root-level free provider model must be discovered: {:?}",
            models
        );
        assert!(
            models.contains(&"opencode/big-pickle".to_string()),
            "a free model id without a '-free' suffix must still be discovered: {:?}",
            models
        );
        assert!(
            !models.contains(&"opencode/pricey-input".to_string()),
            "a non-zero input cost must be excluded: {:?}",
            models
        );
        assert!(
            !models.contains(&"opencode/pricey-output".to_string()),
            "a non-zero output cost must be excluded: {:?}",
            models
        );
        assert!(
            !models.contains(&"opencode/retired-free".to_string()),
            "a deprecated free model must be excluded: {:?}",
            models
        );
        assert!(
            !models.contains(&"unrelated/should-not-appear".to_string()),
            "only the two self-operated providers may contribute dynamic models: {:?}",
            models
        );
        assert_eq!(value["opencode"]["error"], serde_json::Value::Null);
    });
}

#[test]
fn test_ai_workflow_model_sources_opencode_requires_exact_zero_cost() {
    with_temp_test_home("cap-opencode-cost", |home| {
        let catalog = r#"{
  "providers": {
    "opencode": {
      "id": "opencode",
      "models": {
        "missing-cost": {},
        "cost-not-object": { "cost": "free" },
        "cost-array": { "cost": [0, 0] },
        "empty-cost": { "cost": {} },
        "input-string": { "cost": { "input": "0", "output": 0 } },
        "output-string": { "cost": { "input": 0, "output": "0" } },
        "input-missing": { "cost": { "output": 0 } },
        "genuinely-free": { "cost": { "input": 0, "output": 0 } }
      }
    }
  }
}"#;
        write_env_file(home, ".cache/opencode/models.json", catalog);

        let value = model_sources_json(home);
        let models: Vec<String> = value["opencode"]["models"]
            .as_array()
            .expect("opencode.models must be an array")
            .iter()
            .filter_map(|model| model.as_str().map(str::to_string))
            .collect();

        for excluded in [
            "opencode/missing-cost",
            "opencode/cost-not-object",
            "opencode/cost-array",
            "opencode/empty-cost",
            "opencode/input-string",
            "opencode/output-string",
            "opencode/input-missing",
        ] {
            assert!(
                !models.contains(&excluded.to_string()),
                "{} must not be treated as free: {:?}",
                excluded,
                models
            );
        }
        assert!(
            models.contains(&"opencode/genuinely-free".to_string()),
            "an exactly-zero input and output cost must be free: {:?}",
            models
        );
    });
}

#[test]
fn test_ai_workflow_model_sources_opencode_free_catalog_supplies_capabilities() {
    with_temp_test_home("cap-opencode-free-effort", |home| {
        let catalog = r#"{
  "providers": {
    "opencode": {
      "id": "opencode",
      "models": {
        "cat-jam-free": {
          "cost": { "input": 0, "output": 0 },
          "reasoning_options": [
            { "type": "budget_tokens", "values": ["1000"] },
            { "type": "effort", "values": ["low", "high"] }
          ]
        },
        "no-effort-free": { "cost": { "input": 0, "output": 0 } }
      }
    },
    "opencode-go": {
      "id": "opencode-go",
      "models": {
        "step-5-preview-free": {
          "cost": { "input": 0, "output": 0 },
          "reasoning_options": [ { "type": "effort", "values": ["medium"] } ]
        }
      }
    }
  }
}"#;
        write_env_file(home, ".cache/opencode/models.json", catalog);

        let value = model_sources_json(home);

        let jam = capability_entry(&value, "opencode", "opencode/cat-jam-free")
            .expect("a free dynamic model must expose its catalog efforts");
        assert_eq!(
            sorted_efforts(effort_strings(jam)),
            vec!["high".to_string(), "low".to_string()],
            "only type=effort option values may supply dynamic capabilities"
        );

        let step = capability_entry(&value, "opencode", "opencode-go/step-5-preview-free")
            .expect("the second provider keeps its own catalog capability");
        assert_eq!(
            sorted_efforts(effort_strings(step)),
            vec!["medium".to_string()]
        );

        assert!(
            capability_entry(&value, "opencode", "opencode/no-effort-free").is_none(),
            "a free model without reasoning options stays unknown"
        );
    });
}

#[test]
fn test_ai_workflow_model_sources_opencode_declared_model_dedups_and_keeps_variant_priority() {
    with_temp_test_home("cap-opencode-free-dedup", |home| {
        let config = r#"{
  "provider": {
    "opencode": {
      "models": {
        "cat-jam-free": { "variants": { "fast": { "reasoningEffort": "low" } } }
      }
    }
  }
}"#;
        write_env_file(home, ".config/opencode/opencode.json", config);

        let catalog = r#"{
  "providers": {
    "opencode": {
      "id": "opencode",
      "models": {
        "cat-jam-free": {
          "cost": { "input": 0, "output": 0 },
          "reasoning_options": [ { "type": "effort", "values": ["low", "high", "ultra"] } ]
        }
      }
    }
  }
}"#;
        write_env_file(home, ".cache/opencode/models.json", catalog);

        let value = model_sources_json(home);
        let models: Vec<String> = value["opencode"]["models"]
            .as_array()
            .expect("opencode.models must be an array")
            .iter()
            .filter_map(|model| model.as_str().map(str::to_string))
            .collect();

        assert_eq!(
            models
                .iter()
                .filter(|model| model.as_str() == "opencode/cat-jam-free")
                .count(),
            1,
            "a declared model that is also free in the catalog must appear exactly once: {:?}",
            models
        );

        let entry = capability_entry(&value, "opencode", "opencode/cat-jam-free")
            .expect("the declared model keeps a capability entry");
        assert_eq!(
            sorted_efforts(effort_strings(entry)),
            vec!["low".to_string()],
            "an explicit declared variant must win over the catalog fallback"
        );
    });
}

#[test]
fn test_ai_workflow_model_sources_opencode_catalog_missing_or_corrupt_behavior() {
    with_temp_test_home("cap-opencode-catalog-state", |home| {
        write_env_file(
            home,
            ".config/opencode/opencode.json",
            r#"{"provider":{"apigateway":{"models":{"deepseek-chat":{}}}}}"#,
        );

        // A missing catalog is normal: declared candidates only, no warning.
        let missing = model_sources_json(home);
        let missing_models: Vec<String> = missing["opencode"]["models"]
            .as_array()
            .expect("opencode.models must be an array")
            .iter()
            .filter_map(|model| model.as_str().map(str::to_string))
            .collect();
        assert_eq!(
            missing_models,
            vec!["apigateway/deepseek-chat".to_string()],
            "without a catalog the candidates are exactly the declared models"
        );
        assert_eq!(missing["opencode"]["error"], serde_json::Value::Null);
        assert!(
            !has_extra_diagnostic(&missing["opencode"]),
            "a missing catalog must not fabricate a warning: {}",
            missing["opencode"]
        );

        // A corrupt catalog must warn, keep declared candidates and add nothing.
        write_env_file(home, ".cache/opencode/models.json", "{ not json :::");

        let corrupt = model_sources_json(home);
        let corrupt_models: Vec<String> = corrupt["opencode"]["models"]
            .as_array()
            .expect("opencode.models must be an array")
            .iter()
            .filter_map(|model| model.as_str().map(str::to_string))
            .collect();
        assert_eq!(
            corrupt_models,
            vec!["apigateway/deepseek-chat".to_string()],
            "a corrupt catalog must not drop or fabricate candidates"
        );
        assert!(
            has_extra_diagnostic(&corrupt["opencode"]),
            "a corrupt catalog must surface a capability warning: {}",
            corrupt["opencode"]
        );
        assert_eq!(
            corrupt["opencode"]["error"],
            serde_json::Value::Null,
            "a corrupt catalog is a capability warning, not a column error"
        );
    });
}

#[test]
fn test_ai_workflow_model_sources_capability_failures_keep_candidates_and_columns() {
    with_temp_test_home("cap-warning", |home| {
        write_env_file(home, ".codex/config.toml", "model = \"gpt-5.6-sol\"\n");
        write_env_file(
            home,
            ".config/opencode/opencode.json",
            r#"{"provider":{"apigateway":{"models":{"deepseek-chat":{}}}}}"#,
        );
        write_env_file(
            home,
            ".claude/settings.json",
            r#"{"env":{"ANTHROPIC_MODEL":"claude-3-7-sonnet"}}"#,
        );

        // A missing capability cache is a normal case: candidates stay and no
        // warning is fabricated.
        let absent = model_sources_json(home);
        assert!(
            absent["codex"]["models"]
                .as_array()
                .unwrap()
                .iter()
                .any(|model| model.as_str() == Some("gpt-5.6-sol")),
            "a missing capability cache must not drop codex name candidates"
        );
        assert!(
            !has_extra_diagnostic(&absent["codex"]),
            "a missing capability cache is normal and must not warn: {}",
            absent["codex"]
        );

        // A corrupt capability cache must warn while preserving candidates and
        // the healthy columns.
        write_env_file(home, ".codex/models_cache.json", "{ this is not json :::");

        let value = model_sources_json(home);
        assert!(
            value["codex"]["models"]
                .as_array()
                .unwrap()
                .iter()
                .any(|model| model.as_str() == Some("gpt-5.6-sol")),
            "a corrupt capability cache must not drop codex name candidates"
        );
        assert!(
            has_extra_diagnostic(&value["codex"]),
            "a corrupt capability cache must surface a useful warning: {}",
            value["codex"]
        );
        assert!(
            capability_entry(&value, "codex", "gpt-5.6-sol").is_none(),
            "a corrupt capability cache yields unknown, never fabricated efforts"
        );
        assert_eq!(
            value["opencode"]["error"],
            serde_json::Value::Null,
            "the opencode column must stay healthy when the codex cache is corrupt"
        );
        assert_eq!(
            value["claude"]["error"],
            serde_json::Value::Null,
            "the claude column must stay healthy when the codex cache is corrupt"
        );
        assert!(
            value["opencode"]["models"]
                .as_array()
                .unwrap()
                .iter()
                .any(|model| model.as_str() == Some("apigateway/deepseek-chat")),
            "a healthy column must keep its candidates"
        );
    });
}

#[test]
fn test_ai_workflow_model_sources_claude_effort_is_not_a_capability_list() {
    with_temp_test_home("cap-claude-unknown", |home| {
        write_env_file(
            home,
            ".claude/settings.json",
            r#"{"env":{"ANTHROPIC_MODEL":"claude-3-7-sonnet","CLAUDE_CODE_EFFORT_LEVEL":"xhigh"}}"#,
        );

        let value = model_sources_json(home);
        let claude_unknown = value
            .get("claude")
            .and_then(|column| column.get("model_capabilities"))
            .map(|map| map.as_object().map(|entries| entries.is_empty()).unwrap_or(true))
            .unwrap_or(true);
        assert!(
            claude_unknown,
            "a scalar Claude effort must never become a capability list: {}",
            value["claude"]
        );
    });
}

#[test]
fn test_ai_workflow_save_profile_rejects_known_unsupported_effort_atomically() {
    with_temp_test_home("save-unsupported", |home| {
        write_env_file(
            home,
            ".codex/models_cache.json",
            r#"{"models":[{"slug":"gpt-5.6-astra","default_reasoning_level":"high","supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}]}]}"#,
        );

        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");
        let profile_path = profiles_dir.join("draft.yaml");
        fs::write(
            &profile_path,
            "version: 1.0.0\nagents:\n  backend:\n    codex: { model: gpt-5.6-astra, reasoning_effort: low }\n",
        )
        .expect("write existing profile");
        let original_profile_bytes = fs::read(&profile_path).expect("read profile bytes");

        let config_path = home.join(".config/ai-workflow/config.yaml");
        fs::write(&config_path, "active_profile: draft\n").expect("write config");
        let original_config_bytes = fs::read(&config_path).expect("read config bytes");

        let rows = vec![AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "gpt-5.6-astra".to_string(),
                reasoning_effort: "max".to_string(),
            }),
            claude: None,
            opencode: None,
        }];

        let err = save_profile("draft", &rows, Some(home))
            .expect_err("a schema-valid but declared-unsupported effort must be rejected");
        let lower = err.to_lowercase();
        assert!(lower.contains("backend"), "error must name the role: {}", err);
        assert!(lower.contains("codex"), "error must name the host: {}", err);
        assert!(
            lower.contains("gpt-5.6-astra"),
            "error must name the model: {}",
            err
        );
        assert!(
            lower.contains("max"),
            "error must mention the rejected effort: {}",
            err
        );

        assert_eq!(
            fs::read(&profile_path).expect("read profile after refusal"),
            original_profile_bytes,
            "an unsupported effort must leave the existing profile bytes unchanged"
        );
        assert_eq!(
            fs::read(&config_path).expect("read config after refusal"),
            original_config_bytes,
            "an unsupported effort must leave the active-profile configuration unchanged"
        );
    });
}

#[test]
fn test_ai_workflow_save_profile_reloads_changed_capabilities_before_write() {
    with_temp_test_home("save-reload", |home| {
        write_env_file(
            home,
            ".codex/models_cache.json",
            r#"{"models":[{"slug":"gpt-5.6-astra","default_reasoning_level":"high","supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}]}]}"#,
        );

        let config_path = home.join(".config/ai-workflow/config.yaml");
        fs::create_dir_all(home.join(".config/ai-workflow")).expect("create config dir");
        fs::write(&config_path, "active_profile: elsewhere\n").expect("write config");
        let original_config_bytes = fs::read(&config_path).expect("read config bytes");

        let rows = vec![AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "gpt-5.6-astra".to_string(),
                reasoning_effort: "high".to_string(),
            }),
            claude: None,
            opencode: None,
        }];

        save_profile("draft", &rows, Some(home))
            .expect("a currently declared supported effort must save");
        let profile_path = home.join(".config/ai-workflow/profiles/draft.yaml");
        let saved_bytes = fs::read(&profile_path).expect("read saved profile bytes");

        // The local capability snapshot changes: 'high' is no longer declared.
        write_env_file(
            home,
            ".codex/models_cache.json",
            r#"{"models":[{"slug":"gpt-5.6-astra","default_reasoning_level":"low","supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"}]}]}"#,
        );

        let err = save_profile("draft", &rows, Some(home))
            .expect_err("Save must reload capabilities and reject a now-unsupported effort");
        assert!(
            err.to_lowercase().contains("gpt-5.6-astra"),
            "the reload refusal must name the model: {}",
            err
        );

        assert_eq!(
            fs::read(&profile_path).expect("read profile after refusal"),
            saved_bytes,
            "a capability change before Save must preserve the existing profile bytes"
        );
        assert_eq!(
            fs::read(&config_path).expect("read config after refusal"),
            original_config_bytes,
            "a capability change before Save must preserve the active-profile configuration"
        );
    });
}

#[test]
fn test_ai_workflow_save_profile_allows_unknown_manual_and_rejects_invalid() {
    with_temp_test_home("save-unknown-manual", |home| {
        // No capability source declares this model, so it is unknown.
        let manual_rows = vec![AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "custom/mystery-model".to_string(),
                reasoning_effort: "xhigh".to_string(),
            }),
            claude: None,
            opencode: None,
        }];
        save_profile("manual-profile", &manual_rows, Some(home))
            .expect("an unknown model with a schema-valid manual effort must still save");

        let saved =
            fs::read_to_string(home.join(".config/ai-workflow/profiles/manual-profile.yaml"))
                .expect("read manual profile");
        let yaml: serde_yaml::Value = serde_yaml::from_str(&saved).expect("persisted YAML");
        assert_eq!(
            yaml["agents"]["backend"]["codex"]["reasoning_effort"].as_str(),
            Some("xhigh")
        );

        let invalid_effort = vec![AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "custom/mystery-model".to_string(),
                reasoning_effort: "bogus".to_string(),
            }),
            claude: None,
            opencode: None,
        }];
        assert!(
            save_profile("manual-profile", &invalid_effort, Some(home)).is_err(),
            "a schema-invalid manual effort must be rejected for unknown models too"
        );

        let blank_effort = vec![AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "custom/mystery-model".to_string(),
                reasoning_effort: "   ".to_string(),
            }),
            claude: None,
            opencode: None,
        }];
        assert!(
            save_profile("manual-profile", &blank_effort, Some(home)).is_err(),
            "a blank effort must be rejected"
        );

        let blank_model = vec![AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "   ".to_string(),
                reasoning_effort: "high".to_string(),
            }),
            claude: None,
            opencode: None,
        }];
        assert!(
            save_profile("manual-profile", &blank_model, Some(home)).is_err(),
            "a blank model must be rejected"
        );
    });
}

