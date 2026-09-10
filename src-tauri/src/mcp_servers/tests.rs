use super::*;
use chrono::Utc;
use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

fn with_temp_home<T>(name: &str, f: impl FnOnce(&Path) -> T) -> T {
    let _guard = crate::lock_test_home_env();
    let temp_home = std::env::temp_dir().join(format!(
        "onespace-mcp-servers-{}-{}",
        name,
        uuid::Uuid::new_v4()
    ));
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

fn sample_server(name: &str) -> MCPServer {
    MCPServer {
        id: format!("mcp-{}", name),
        name: name.to_string(),
        config_key: None,
        description: None,
        transport: MCPServerTransport::Stdio,
        command: Some("npx".to_string()),
        args: Some(vec!["-y".to_string(), "@upstash/context7-mcp".to_string()]),
        cwd: None,
        url: None,
        http_url: None,
        env: None,
        headers: None,
        timeout: Some(60000),
        trust: Some(false),
        linked_provider_ids: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn slugify_and_unique_config_key() {
    let mut state = MCPServersState {
        servers: vec![
            sample_server("Context 7"),
            sample_server("Context-7"),
            sample_server(""),
        ],
        is_encrypted: false,
    };

    assert!(ensure_server_config_keys(&mut state));
    let keys = state
        .servers
        .iter()
        .map(|s| s.config_key.clone().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(keys[0], "context-7");
    assert_ne!(keys[1], "context-7");
    assert!(keys[1].starts_with("context-7-"));
    assert_eq!(keys[2], "server");
}

#[test]
fn standard_entry_contains_transport_and_fields() {
    let server = MCPServer {
        transport: MCPServerTransport::Sse,
        url: Some("http://localhost:3000/sse".to_string()),
        ..sample_server("test")
    };
    let value = build_standard_entry(&server, true);
    let obj = value.as_object().expect("object");
    assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("sse"));
    assert_eq!(
        obj.get("url").and_then(|v| v.as_str()),
        Some("http://localhost:3000/sse")
    );
}

#[test]
fn codex_entry_serializes_stdio_command() {
    let server = sample_server("codex");
    let table = build_codex_entry(&server);
    assert_eq!(table.get("type").and_then(|v| v.as_str()), Some("stdio"));
    assert_eq!(table.get("command").and_then(|v| v.as_str()), Some("npx"));
    assert!(table.get("args").is_some());
}

#[test]
fn remove_json_entry_keeps_others() {
    let mut root = Map::new();
    root.insert(
        "mcpServers".to_string(),
        serde_json::json!({
            "keep": { "type": "stdio", "command": "npx" },
            "drop": { "type": "stdio", "command": "uvx" }
        }),
    );
    set_json_mcp_entry(&mut root, "mcpServers", "drop", None);
    let section = root
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .expect("section exists");
    assert!(section.contains_key("keep"));
    assert!(!section.contains_key("drop"));
}

#[test]
fn opencode_entry_maps_to_local_and_remote() {
    let local = build_opencode_entry(&sample_server("local"));
    assert_eq!(
        local
            .as_object()
            .and_then(|obj| obj.get("type"))
            .and_then(|v| v.as_str()),
        Some("local")
    );

    let remote_server = MCPServer {
        transport: MCPServerTransport::Http,
        http_url: Some("https://example.com/mcp".to_string()),
        ..sample_server("remote")
    };
    let remote = build_opencode_entry(&remote_server);
    let obj = remote.as_object().expect("remote object");
    assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("remote"));
    assert_eq!(
        obj.get("url").and_then(|v| v.as_str()),
        Some("https://example.com/mcp")
    );
}

#[test]
fn parse_codex_mcp_servers_supports_stdio_and_remote() {
    let content = r#"
[mcp_servers.context7]
type = "stdio"
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
timeout = 30000

[mcp_servers.remoteapi]
type = "http"
url = "https://example.com/mcp"
trust = true
"#;
    let parsed = parse_codex_mcp_servers(content).expect("parsed");
    let stdio = parsed.get("context7").expect("stdio entry");
    assert_eq!(stdio.transport, MCPServerTransport::Stdio);
    assert_eq!(stdio.command.as_deref(), Some("npx"));
    assert_eq!(stdio.timeout, Some(30000));

    let remote = parsed.get("remoteapi").expect("remote entry");
    assert_eq!(remote.transport, MCPServerTransport::Http);
    assert_eq!(remote.http_url.as_deref(), Some("https://example.com/mcp"));
    assert_eq!(remote.trust, Some(true));
}

#[test]
fn parse_opencode_json_section_supports_local_and_remote() {
    let mut root = Map::new();
    root.insert(
        "mcp".to_string(),
        serde_json::json!({
            "local_one": {
                "type": "local",
                "command": ["uvx", "mcp-local"],
                "cwd": "/tmp/local"
            },
            "remote_one": {
                "type": "remote",
                "url": "https://example.com/remote"
            }
        }),
    );
    let parsed = parse_opencode_json_section(&root);
    let local = parsed.get("local_one").expect("local entry");
    assert_eq!(local.transport, MCPServerTransport::Stdio);
    assert_eq!(local.command.as_deref(), Some("uvx"));
    assert_eq!(
        local
            .args
            .as_ref()
            .and_then(|v| v.first())
            .map(|s| s.as_str()),
        Some("mcp-local")
    );
    assert_eq!(local.cwd.as_deref(), Some("/tmp/local"));

    let remote = parsed.get("remote_one").expect("remote entry");
    assert_eq!(remote.transport, MCPServerTransport::Http);
    assert_eq!(
        remote.http_url.as_deref(),
        Some("https://example.com/remote")
    );
}

#[test]
fn merge_discovered_servers_keeps_existing_onespace_definition() {
    let mut existing = sample_server("existing");
    existing.config_key = Some("shared".to_string());
    existing.command = Some("onespace-command".to_string());

    let mut discovered = sample_server("discovered");
    discovered.config_key = Some("shared".to_string());
    discovered.command = Some("external-command".to_string());

    let mut state = MCPServersState {
        servers: vec![existing.clone()],
        is_encrypted: false,
    };
    let mut local = LocalModelConfigs::default();
    local.claude.insert("shared".to_string(), discovered);

    let changed = merge_discovered_servers(&mut state, &local);
    assert!(!changed);
    assert_eq!(state.servers.len(), 1);
    assert_eq!(
        state.servers[0].command.as_deref(),
        Some("onespace-command")
    );
}

#[test]
fn merge_discovered_servers_prefers_priority_when_conflict() {
    let mut claude = sample_server("claude-candidate");
    claude.config_key = Some("same-key".to_string());
    claude.command = Some("claude-command".to_string());

    let mut codex = sample_server("codex-candidate");
    codex.config_key = Some("same-key".to_string());
    codex.command = Some("codex-command".to_string());

    let mut local = LocalModelConfigs::default();
    local.claude.insert("same-key".to_string(), claude);
    local.codex.insert("same-key".to_string(), codex);

    let mut state = MCPServersState::default();
    let changed = merge_discovered_servers(&mut state, &local);
    assert!(changed);
    assert_eq!(state.servers.len(), 1);
    assert_eq!(state.servers[0].config_key.as_deref(), Some("same-key"));
    assert_eq!(state.servers[0].command.as_deref(), Some("claude-command"));
}

#[test]
fn build_model_switch_state_marks_multiple_models() {
    let mut keysets = ModelKeysets::default();
    keysets.claude.insert("alpha".to_string());
    keysets.codex.insert("alpha".to_string());
    keysets.antigravity.insert("beta".to_string());
    let state = build_model_switch_state("alpha", &keysets);
    assert!(state.claude);
    assert!(state.codex);
    assert!(!state.antigravity);
    assert!(!state.opencode);
}

#[test]
fn parse_npm_package_spec_supports_scoped_and_pinned() {
    let parsed = parse_npm_package_spec("@scope/pkg@1.2.3").expect("parsed");
    assert_eq!(parsed.0, "@scope/pkg");
    assert_eq!(parsed.1.as_deref(), Some("1.2.3"));

    let parsed = parse_npm_package_spec("@scope/pkg").expect("parsed");
    assert_eq!(parsed.0, "@scope/pkg");
    assert!(parsed.1.is_none());

    let parsed = parse_npm_package_spec("plain-pkg@2.0.0").expect("parsed");
    assert_eq!(parsed.0, "plain-pkg");
    assert_eq!(parsed.1.as_deref(), Some("2.0.0"));
}

#[test]
fn parse_server_npm_spec_detects_template_style() {
    let mut server = sample_server("pkg");
    server.args = Some(vec![
        "-y".to_string(),
        "@modelcontextprotocol/server-github@1.0.0".to_string(),
    ]);
    let parsed = parse_server_npm_spec(&server).expect("parsed");
    assert_eq!(parsed.package_name, "@modelcontextprotocol/server-github");
    assert_eq!(parsed.version.as_deref(), Some("1.0.0"));
    assert_eq!(parsed.token_index, 1);
}

#[test]
fn compare_semver_like_orders_versions() {
    assert_eq!(
        compare_semver_like("1.2.3", "1.2.4"),
        Some(std::cmp::Ordering::Less)
    );
    assert_eq!(
        compare_semver_like("v1.2.3", "1.2.3"),
        Some(std::cmp::Ordering::Equal)
    );
    assert_eq!(compare_semver_like("latest", "1.2.3"), None);
}

#[test]
fn antigravity_mcp_paths_resolve_to_gemini_config_and_agents_workspace() {
    with_temp_home("antigravity-paths", |home| {
        let global = get_antigravity_mcp_path().expect("global antigravity MCP path");
        assert_eq!(
            global,
            home.join(".gemini").join("config").join("mcp_config.json")
        );

        let project_root = home.join("project");
        let workspace =
            get_workspace_antigravity_mcp_path(project_root.to_string_lossy().as_ref());
        assert_eq!(
            workspace,
            project_root.join(".agents").join("mcp_config.json")
        );
    });
}

#[test]
fn antigravity_entry_serializes_remote_server_with_server_url() {
    let remote = MCPServer {
        transport: MCPServerTransport::Http,
        http_url: Some("https://example.com/mcp".to_string()),
        ..sample_server("remote")
    };
    let value = build_antigravity_entry(&remote, true);
    let obj = value.as_object().expect("antigravity entry object");
    assert_eq!(
        obj.get("serverUrl").and_then(|v| v.as_str()),
        Some("https://example.com/mcp")
    );
    assert!(!obj.contains_key("url"), "must not serialize `url`");
    assert!(!obj.contains_key("httpUrl"), "must not serialize `httpUrl`");
}

#[test]
fn antigravity_model_switch_uses_antigravity_key() {
    let parsed: MCPModel = "antigravity".parse().expect("parse antigravity model");
    assert_eq!(parsed, MCPModel::Antigravity);

    let state = MCPModelSwitchState {
        antigravity: true,
        ..Default::default()
    };
    let value = serde_json::to_value(&state).expect("serialize switch state");
    assert_eq!(
        value.get("antigravity").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert!(value.get("gemini").is_none(), "must not use legacy gemini key");
}

#[test]
fn antigravity_apply_writes_config_and_avoids_legacy_gemini_settings() {
    with_temp_home("antigravity-apply", |home| {
        let server = sample_server("context7");
        apply_model_switch(MCPModel::Antigravity, &server, "context7", true).expect("apply switch");

        let global_path = home.join(".gemini").join("config").join("mcp_config.json");
        assert!(
            global_path.exists(),
            "global Antigravity MCP config should be written at {}",
            global_path.display()
        );
        let root: Value = serde_json::from_str(
            &fs::read_to_string(&global_path).expect("read global antigravity MCP config"),
        )
        .expect("parse global antigravity MCP config");
        assert!(root["mcpServers"]["context7"].is_object());

        let legacy_path = home.join(".gemini").join("settings.json");
        if legacy_path.exists() {
            let legacy: Value = serde_json::from_str(
                &fs::read_to_string(&legacy_path).expect("read legacy gemini settings"),
            )
            .expect("parse legacy gemini settings");
            assert!(
                legacy.get("mcpServers").is_none(),
                "legacy ~/.gemini/settings.json must not contain mcpServers"
            );
        }

        let project_root = home.join("project");
        fs::create_dir_all(&project_root).expect("create project root");
        let canonical_project = fs::canonicalize(&project_root).expect("canonical project root");
        let remote = MCPServer {
            transport: MCPServerTransport::Http,
            http_url: Some("https://example.com/mcp".to_string()),
            ..sample_server("remote")
        };
        apply_project_workspace_servers(
            canonical_project.to_string_lossy().as_ref(),
            "antigravity",
            &[remote],
        )
        .expect("apply workspace servers");

        let workspace_path = canonical_project
            .join(".agents")
            .join("mcp_config.json");
        assert!(
            workspace_path.exists(),
            "workspace Antigravity MCP config should be written at {}",
            workspace_path.display()
        );
        let workspace: Value = serde_json::from_str(
            &fs::read_to_string(&workspace_path).expect("read workspace antigravity MCP config"),
        )
        .expect("parse workspace antigravity MCP config");
        let entry = &workspace["mcpServers"]["onespace-remote"];
        assert_eq!(
            entry.get("serverUrl").and_then(|v| v.as_str()),
            Some("https://example.com/mcp")
        );
    });
}
