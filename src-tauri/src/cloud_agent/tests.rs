use super::gateway_sync::build_cloud_gateway_section;
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
    assert_eq!(s.default_host, "dmit-server");
    assert_eq!(s.remote_workspace_root, "/root/cloud-agent-workspaces");
    assert_eq!(s.remote_agent_dir, "/root/onespace-cloud-agent");
    assert!(s.opencode_yolo_mode);
    assert_eq!(s.opencode_extra_args, "");
    assert_eq!(s.auto_refresh_interval_seconds, 10);
    assert_eq!(s.disk_alert_threshold_percent, 80.0);
    assert_eq!(s.terminal_app, "auto");
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
