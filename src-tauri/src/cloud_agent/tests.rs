use super::commands::resolve_host;
use super::gateway_sync::{build_cloud_gateway_section, build_remote_config_write};
use super::terminal::{
    build_ssh_connect_command, clean_terminal_app_name, escape_applescript_string,
    normalize_terminal_app_key,
};
use super::types::CloudAgentSettings;
use crate::ai_gateway::{
    GatewayConfig, GatewayUpstreamProvider, ModelMapping, UpstreamKey,
};

#[test]
fn test_cloud_agent_settings_defaults() {
    let s = CloudAgentSettings::default();
    assert_eq!(s.default_host, "");
    assert_eq!(s.remote_workspace_root, "/root/cloud-agent-workspaces");
    assert_eq!(s.remote_agent_dir, "/root/onespace-cloud-agent");
    assert!(s.opencode_yolo_mode);
    assert_eq!(s.opencode_extra_args, "");
    assert_eq!(s.auto_refresh_interval_seconds, 10);
    assert_eq!(s.disk_alert_threshold_percent, 80.0);
    assert_eq!(s.terminal_app, "auto");
}

#[test]
fn test_resolve_host() {
    let empty_settings = CloudAgentSettings::default();
    assert!(resolve_host(None, &empty_settings).is_err());
    assert!(resolve_host(Some("  ".to_string()), &empty_settings).is_err());

    let mut configured_settings = CloudAgentSettings::default();
    configured_settings.default_host = "server-a".to_string();
    assert_eq!(
        resolve_host(None, &configured_settings).unwrap(),
        "server-a"
    );
    assert_eq!(
        resolve_host(Some("server-b".to_string()), &configured_settings).unwrap(),
        "server-b"
    );
}

#[test]
fn test_terminal_name_and_applescript_escaping() {
    assert_eq!(clean_terminal_app_name("iTerm.app"), "iTerm");
    assert_eq!(clean_terminal_app_name("Ghostty"), "Ghostty");
    assert_eq!(normalize_terminal_app_key("  iTerm2.app "), "iterm2");
    assert_eq!(
        escape_applescript_string("hello \"world\" \\ test"),
        "hello \\\"world\\\" \\\\ test"
    );
}

#[test]
fn test_build_ssh_connect_command() {
    // 默认 yolo
    let cmd1 = build_ssh_connect_command("dmit-server", "dev_repo_feat", None, true, "");
    assert_eq!(cmd1, "ssh -t dmit-server \"cloud-session dev_repo_feat\"");

    // 禁用 yolo
    let cmd2 = build_ssh_connect_command("dmit-server", "one-space", Some("feat-auth"), false, "");
    assert_eq!(
        cmd2,
        "ssh -t dmit-server \"cloud-session one-space feat-auth --no-yolo\""
    );

    // 附带 extra_args
    let cmd3 = build_ssh_connect_command(
        "my-host",
        "dev_app",
        None,
        true,
        "--model deepseek-chat",
    );
    assert_eq!(
        cmd3,
        "ssh -t my-host \"cloud-session dev_app --extra-args '--model deepseek-chat'\""
    );
}

#[test]
fn test_build_cloud_gateway_section() {
    let mut config = GatewayConfig::default();
    let provider = GatewayUpstreamProvider {
        id: "deepseek".to_string(),
        name: "DeepSeek".to_string(),
        base_url: "https://api.deepseek.com/v1".to_string(),
        keys: vec![
            UpstreamKey {
                id: "k1".to_string(),
                name: "default".to_string(),
                value: "sk-real-secret".to_string(),
                enabled: true,
                daily_reset: false,
                auto_marked: false,
                failure_kind: None,
                marked_at: None,
                reason: None,
            },
            UpstreamKey {
                id: "k2".to_string(),
                name: "disabled".to_string(),
                value: "sk-disabled".to_string(),
                enabled: false,
                daily_reset: false,
                auto_marked: false,
                failure_kind: None,
                marked_at: None,
                reason: None,
            },
        ],
        enabled: true,
        mappings: vec![
            ModelMapping {
                local_model: "deepseek-chat".to_string(),
                upstream_model: "deepseek-chat".to_string(),
                display_name: Some("DeepSeek V3".to_string()),
                context_window: Some(64000),
                reasoning_efforts: vec!["low".to_string(), "medium".to_string(), "high".to_string()],
                enabled: true,
                auto_disabled: false,
                ..Default::default()
            },
            ModelMapping {
                local_model: "disabled-model".to_string(),
                upstream_model: "disabled-model".to_string(),
                enabled: false,
                auto_disabled: false,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    config.providers.push(provider);

    let (val, p_count, pool_count) = build_cloud_gateway_section(&config);
    assert_eq!(p_count, 1);
    assert_eq!(pool_count, 0);

    let providers = val.get("providers").unwrap().as_object().unwrap();
    assert!(providers.contains_key("deepseek"));
    let ds = providers.get("deepseek").unwrap();
    assert_eq!(ds.get("name").unwrap().as_str().unwrap(), "DeepSeek");
    let keys = ds.get("api_keys").unwrap().as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].as_str().unwrap(), "sk-real-secret");

    let models = ds.get("models").unwrap().as_object().unwrap();
    assert!(models.contains_key("deepseek-chat"));
    assert!(!models.contains_key("disabled-model"));

    let chat_model = models.get("deepseek-chat").unwrap();
    assert_eq!(chat_model.get("upstream_model").unwrap().as_str().unwrap(), "deepseek-chat");
    assert_eq!(chat_model.get("display_name").unwrap().as_str().unwrap(), "DeepSeek V3");
    assert_eq!(chat_model.get("context_window").unwrap().as_u64().unwrap(), 64000);
    let efforts = chat_model.get("reasoning_efforts").unwrap().as_array().unwrap();
    assert_eq!(efforts.len(), 3);
    assert_eq!(efforts[0].as_str().unwrap(), "low");
    assert_eq!(efforts[1].as_str().unwrap(), "medium");
    assert_eq!(efforts[2].as_str().unwrap(), "high");
}

#[test]
fn test_remote_config_write_keeps_large_payload_out_of_ssh_arguments() {
    let config_path = "/root/onespace-cloud-agent/config.json";
    let huge_key = format!("sk-{}", "x".repeat(600_000));
    let config = serde_json::json!({
        "ai_gateway": {
            "providers": {
                "large-provider": {
                    "api_keys": [huge_key]
                }
            }
        }
    });

    let (cmd, payload) = build_remote_config_write(config_path, &config).unwrap();

    assert_eq!(
        cmd,
        "cat > /root/onespace-cloud-agent/config.json.tmp && mv /root/onespace-cloud-agent/config.json.tmp /root/onespace-cloud-agent/config.json"
    );
    assert!(payload.len() > 600_000);
    assert!(!cmd.contains(&"x".repeat(64)));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&payload).unwrap(),
        config
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_run_with_stdin_streams_large_payload_to_child() {
    let dir = std::env::temp_dir().join(format!(
        "onespace-cloud-agent-stdin-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("payload.json");
    let payload = format!("{{\"key\":\"{}\"}}", "x".repeat(600_000));

    let mut cmd = tokio::process::Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(format!("cat > '{}'", target.display()));
    let output = super::ssh::run_with_stdin(cmd, payload.as_bytes())
        .await
        .unwrap();

    assert!(output.status.success());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), payload);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_cloud_agent_task_deserialization_backward_compatible() {
    use super::types::CloudAgentTask;

    // 1. 测试旧格式 JSON（缺少新字段）正常反序列化为 None
    let legacy_json = r#"{
        "id": "task_1",
        "repo": "minbox/onespace",
        "issue_number": 42,
        "title": "Fix something",
        "status": "completed",
        "started_at": 1700000000,
        "finished_at": 1700000060,
        "duration_seconds": 60,
        "branch": "bot/issue-42",
        "pr_url": "https://github.com/minbox/onespace/pull/42",
        "error_message": null
    }"#;

    let task: CloudAgentTask = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(task.id, "task_1");
    assert_eq!(task.author, None);
    assert_eq!(task.trigger_type, None);
    assert_eq!(task.attempts, None);
    assert_eq!(task.max_attempts, None);
    assert_eq!(task.diff_stat, None);
    assert_eq!(task.step, None);

    // 2. 测试带全量扩展字段的 JSON 正常反序列化
    let rich_json = r#"{
        "id": "task_2",
        "repo": "minbox/onespace",
        "issue_number": 43,
        "title": "Add feature",
        "status": "running",
        "started_at": 1700000100,
        "branch": "bot/issue-43",
        "author": "octocat",
        "trigger_type": "follow_up",
        "attempts": 2,
        "max_attempts": 3,
        "diff_stat": "3 files changed, +45 -12",
        "step": "opencode"
    }"#;

    let task2: CloudAgentTask = serde_json::from_str(rich_json).unwrap();
    assert_eq!(task2.author.as_deref(), Some("octocat"));
    assert_eq!(task2.trigger_type.as_deref(), Some("follow_up"));
    assert_eq!(task2.attempts, Some(2));
    assert_eq!(task2.max_attempts, Some(3));
    assert_eq!(task2.diff_stat.as_deref(), Some("3 files changed, +45 -12"));
    assert_eq!(task2.step.as_deref(), Some("opencode"));
}

#[path = "deployment_tests.rs"]
mod deployment_tests;
