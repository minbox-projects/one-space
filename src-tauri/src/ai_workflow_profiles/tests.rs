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

// ============================================================================
// 4. save_and_activate_profile tests
// ============================================================================

#[test]
fn test_ai_workflow_save_and_activate_validates_profile_name() {
    with_temp_test_home("save-invalid-name", |home| {
        let invalid_names = ["../bad", "-invalid", "bad name", "a*b", ""];
        for name in invalid_names {
            let res = save_and_activate_profile(name, &[], Some(home));
            assert!(res.is_err(), "name '{}' must fail validation before writing", name);
            let err = res.unwrap_err();
            assert!(err.contains("name") || err.contains("invalid") || err.contains("Invalid"));

            // Ensure no dirty yaml file was created
            let profile_file = home.join(".config/ai-workflow/profiles").join(format!("{}.yaml", name));
            assert!(!profile_file.exists(), "must not write yaml on invalid name");
        }
    });
}

#[test]
fn test_ai_workflow_save_and_activate_validates_schema_and_effort() {
    with_temp_test_home("save-invalid-schema", |home| {
        // Invalid effort level (not one of low, medium, high, xhigh, max, ultra)
        let row_bad_effort = AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "gpt-6".to_string(),
                reasoning_effort: "extreme".to_string(),
            }),
            claude: None,
            opencode: None,
        };

        let res1 = save_and_activate_profile("test-profile", &[row_bad_effort], Some(home));
        assert!(res1.is_err(), "effort 'extreme' must be rejected");
        let err1 = res1.unwrap_err();
        assert!(err1.contains("effort") || err1.contains("reasoning_effort"));

        // Blank model string
        let row_blank_model = AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "   ".to_string(),
                reasoning_effort: "high".to_string(),
            }),
            claude: None,
            opencode: None,
        };

        let res2 = save_and_activate_profile("test-profile", &[row_blank_model], Some(home));
        assert!(res2.is_err(), "blank model string must be rejected");
    });
}

#[test]
fn test_ai_workflow_save_and_activate_persists_yaml_atomically_and_omits_unconfigured_roles() {
    with_temp_test_home("save-atomic", |home| {
        // Setup mock CLI that returns success report JSON
        let bin_dir = home.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let mock_cli = create_mock_cli(
            &bin_dir,
            "ai-workflow",
            r#"#!/bin/sh
cat << 'EOF'
{
  "active_profile": "custom-save",
  "hosts": ["codex"],
  "installations": [
    {
      "host": "codex",
      "agents_directory": "/tmp/.codex/agents",
      "agents": [
        {
          "name": "backend",
          "path": "/tmp/.codex/agents/backend.toml",
          "model": "gpt-6-astra",
          "reasoning_effort": "medium"
        }
      ]
    }
  ]
}
EOF
exit 0
"#,
        );
        std::env::set_var("AI_WORKFLOW_CLI_PATH", &mock_cli);

        let rows = vec![
            AgentMatrixRow {
                role: "backend".to_string(),
                codex: Some(ModelEffort {
                    model: "gpt-6-astra".to_string(),
                    reasoning_effort: "medium".to_string(),
                }),
                claude: None,
                opencode: None,
            },
            AgentMatrixRow {
                role: "test".to_string(),
                codex: None,
                claude: None,
                opencode: Some(ModelEffort {
                    model: "apigateway/deepseek".to_string(),
                    reasoning_effort: "high".to_string(),
                }),
            },
            // other roles unconfigured
            AgentMatrixRow {
                role: "frontend".to_string(),
                codex: None,
                claude: None,
                opencode: None,
            },
        ];

        let report = save_and_activate_profile("custom-save", &rows, Some(home))
            .expect("save_and_activate_profile should succeed");
        assert_eq!(report.active_profile, "custom-save");
        assert_eq!(report.installations.len(), 1);

        // Verify written YAML file
        let profile_path = home.join(".config/ai-workflow/profiles/custom-save.yaml");
        assert!(profile_path.exists(), "YAML file must be created");
        let content = fs::read_to_string(&profile_path).expect("read written yaml");

        assert!(content.contains("version: 1.0.0"), "must include version: 1.0.0");
        assert!(content.contains("backend:"), "must include configured backend agent");
        assert!(content.contains("test:"), "must include configured test agent");
        assert!(!content.contains("frontend:"), "unconfigured roles must be omitted from yaml");
    });
}

#[test]
fn test_ai_workflow_save_and_activate_restores_snapshot_on_activation_failure() {
    with_temp_test_home("save-restore-snapshot", |home| {
        let profiles_dir = home.join(".config/ai-workflow/profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");

        let target_file = profiles_dir.join("existing.yaml");
        let original_yaml = "version: 1.0.0\nagents:\n  backend:\n    codex: { model: original-model, reasoning_effort: low }\n";
        fs::write(&target_file, original_yaml).expect("write original yaml");
        let original_bytes = fs::read(&target_file).expect("read original bytes");

        // Mock CLI that exits with failure and emits error
        let bin_dir = home.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let mock_cli = create_mock_cli(
            &bin_dir,
            "ai-workflow",
            r#"#!/bin/sh
echo "error: installation conflict in .codex/agents/backend.toml: modified by user" >&2
exit 1
"#,
        );
        std::env::set_var("AI_WORKFLOW_CLI_PATH", &mock_cli);

        let rows = vec![AgentMatrixRow {
            role: "backend".to_string(),
            codex: Some(ModelEffort {
                model: "mutated-model".to_string(),
                reasoning_effort: "high".to_string(),
            }),
            claude: None,
            opencode: None,
        }];

        let res = save_and_activate_profile("existing", &rows, Some(home));
        assert!(res.is_err(), "should fail when CLI activation fails");
        let err = res.unwrap_err();
        assert!(
            err.contains("error: installation conflict in .codex/agents/backend.toml"),
            "must echo verbatim CLI error: {}",
            err
        );

        // Snapshot restoration assertion
        let restored_bytes = fs::read(&target_file).expect("read restored file");
        assert_eq!(
            restored_bytes, original_bytes,
            "original YAML bytes must be restored after activation failure, no dirty YAML left"
        );
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

