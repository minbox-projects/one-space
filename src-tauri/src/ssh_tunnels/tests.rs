use super::*;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

fn local_forward() -> SshTunnelForwardConfig {
    SshTunnelForwardConfig {
        mode: SshTunnelForwardMode::Local,
        local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
        local_port: Some(5432),
        remote_bind_host: Some(REMOTE_BIND_HOST.to_string()),
        remote_port: None,
        target_host: Some("127.0.0.1".to_string()),
        target_port: Some(5432),
        dynamic_probe_host: None,
        dynamic_probe_port: None,
    }
}

fn sample_record(group_id: &str) -> SshTunnelRecord {
    SshTunnelRecord {
        id: "tunnel-1".to_string(),
        name: "Local tunnel".to_string(),
        group_id: group_id.to_string(),
        source_kind: SshTunnelSourceKind::SavedHost,
        saved_host_name: Some("dev".to_string()),
        custom: None,
        forward: local_forward(),
        auto_connect: false,
        auto_reconnect: true,
        created_at: 1,
        updated_at: 1,
        last_connected_at: None,
        last_error: None,
    }
}

#[test]
fn forward_summary_local() {
    assert_eq!(
        tunnel_summary(&local_forward()),
        "L 127.0.0.1:5432 -> 127.0.0.1:5432"
    );
}

#[test]
fn forward_summary_remote() {
    let forward = SshTunnelForwardConfig {
        mode: SshTunnelForwardMode::Remote,
        local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
        local_port: None,
        remote_bind_host: Some(REMOTE_BIND_HOST.to_string()),
        remote_port: Some(15432),
        target_host: Some("127.0.0.1".to_string()),
        target_port: Some(5432),
        dynamic_probe_host: None,
        dynamic_probe_port: None,
    };
    assert_eq!(
        tunnel_summary(&forward),
        "R 127.0.0.1:15432 <- 127.0.0.1:5432"
    );
}

#[test]
fn forward_summary_dynamic() {
    let forward = SshTunnelForwardConfig {
        mode: SshTunnelForwardMode::Dynamic,
        local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
        local_port: Some(1080),
        remote_bind_host: Some(REMOTE_BIND_HOST.to_string()),
        remote_port: None,
        target_host: None,
        target_port: None,
        dynamic_probe_host: Some("example.com".to_string()),
        dynamic_probe_port: Some(443),
    };
    assert_eq!(
        tunnel_summary(&forward),
        "D 127.0.0.1:1080 (SOCKS5) | Probe: example.com:443"
    );
}

#[test]
fn validate_dynamic_probe_pair() {
    let input = SshTunnelUpsertInput {
        id: None,
        name: "dynamic".to_string(),
        group_id: None,
        source_kind: SshTunnelSourceKind::SavedHost,
        saved_host_name: Some("dev".to_string()),
        custom: None,
        forward: SshTunnelForwardConfig {
            mode: SshTunnelForwardMode::Dynamic,
            local_bind_host: Some(LOCAL_BIND_HOST.to_string()),
            local_port: Some(1080),
            remote_bind_host: Some(REMOTE_BIND_HOST.to_string()),
            remote_port: None,
            target_host: None,
            target_port: None,
            dynamic_probe_host: Some("example.com".to_string()),
            dynamic_probe_port: None,
        },
        auto_connect: false,
        auto_reconnect: true,
    };
    assert!(validate_input(&input, None).is_err());
}

#[test]
fn validate_custom_password_requires_secret() {
    let input = SshTunnelUpsertInput {
        id: None,
        name: "local".to_string(),
        group_id: None,
        source_kind: SshTunnelSourceKind::Custom,
        saved_host_name: None,
        custom: Some(SshTunnelCustomInput {
            host: "1.2.3.4".to_string(),
            port: 22,
            user: "dev".to_string(),
            auth_kind: SshTunnelAuthKind::Password,
            key_path: None,
            password: None,
            preserve_password: Some(false),
        }),
        forward: local_forward(),
        auto_connect: false,
        auto_reconnect: true,
    };
    assert!(validate_input(&input, None).is_err());
}

#[test]
fn resolve_host_key_name_prefers_alias_then_host_key_alias() {
    assert_eq!(
        resolve_host_key_name("dev-box", "10.1.3.2", None),
        "dev-box"
    );
    assert_eq!(
        resolve_host_key_name("dev-box", "10.1.3.2", Some("cluster-entry")),
        "cluster-entry"
    );
}

#[test]
fn known_hosts_paths_option_supports_multiple_entries_and_none() {
    let paths = known_hosts_paths_from_option(Some("~/known_a ~/.ssh/known_b"))
        .expect("known_hosts paths should parse");
    assert_eq!(paths.len(), 2);
    assert!(paths[0].to_string_lossy().contains("known_a"));
    assert!(paths[1].to_string_lossy().contains(".ssh/known_b"));

    let none_paths =
        known_hosts_paths_from_option(Some("none")).expect("none should disable user paths");
    assert!(none_paths.is_empty());
}

#[test]
fn resolved_key_paths_preserve_order() {
    let paths = resolved_key_paths(&["~/first_key".to_string(), "/tmp/second_key".to_string()]);
    assert_eq!(paths.len(), 2);
    assert!(paths[0].to_string_lossy().contains("first_key"));
    assert!(paths[1].to_string_lossy().ends_with("/tmp/second_key"));
}

#[test]
fn normalize_state_injects_default_group() {
    let mut state = SshTunnelState {
        groups: vec![SshTunnelGroupRecord {
            id: "dev".to_string(),
            name: "Development".to_string(),
            created_at: 10,
            updated_at: 10,
            is_default: false,
        }],
        tunnels: vec![sample_record("")],
    };

    normalize_state(&mut state);

    assert!(state
        .groups
        .iter()
        .any(|group| group.id == DEFAULT_TUNNEL_GROUP_ID && group.is_default));
    assert_eq!(state.tunnels[0].group_id, DEFAULT_TUNNEL_GROUP_ID);
}

#[test]
fn normalize_state_falls_back_invalid_group_ids() {
    let mut state = SshTunnelState {
        groups: vec![
            default_group_record(),
            SshTunnelGroupRecord {
                id: "test".to_string(),
                name: "Testing".to_string(),
                created_at: 20,
                updated_at: 20,
                is_default: false,
            },
        ],
        tunnels: vec![sample_record("missing")],
    };

    normalize_state(&mut state);

    assert_eq!(state.tunnels[0].group_id, DEFAULT_TUNNEL_GROUP_ID);
}

#[test]
fn parse_state_payload_rejects_encrypted_wrapper() {
    let wrapped = serde_json::json!({
        "is_encrypted": true,
        "data": "ciphertext",
    });

    let parsed = parse_state_payload(&wrapped.to_string());

    assert!(parsed.is_err());
}

#[test]
fn parse_state_payload_accepts_structured_state_object() {
    let payload = serde_json::json!({
        "groups": [
            {
                "id": DEFAULT_TUNNEL_GROUP_ID,
                "name": DEFAULT_TUNNEL_GROUP_NAME,
                "created_at": 1,
                "updated_at": 1,
                "is_default": true
            },
            {
                "id": "dev",
                "name": "Development",
                "created_at": 2,
                "updated_at": 2,
                "is_default": false
            }
        ],
        "tunnels": [
            {
                "id": "tunnel-1",
                "name": "Local tunnel",
                "group_id": "dev",
                "source_kind": "saved_host",
                "saved_host_name": "dev",
                "forward": {
                    "mode": "local",
                    "local_bind_host": "127.0.0.1",
                    "local_port": 5432,
                    "remote_bind_host": "127.0.0.1",
                    "target_host": "127.0.0.1",
                    "target_port": 5432
                },
                "auto_connect": false,
                "created_at": 1,
                "updated_at": 1
            }
        ]
    });

    let parsed = parse_state_payload(&payload.to_string()).expect("state should parse");

    assert_eq!(parsed.groups.len(), 2);
    assert_eq!(parsed.tunnels.len(), 1);
    assert_eq!(parsed.tunnels[0].group_id, "dev");
    assert!(parsed.tunnels[0].auto_reconnect);
}

#[test]
fn retryable_io_errors_cover_would_block_and_timeout() {
    assert!(is_retryable_io_error(&io::Error::from(
        io::ErrorKind::WouldBlock
    )));
    assert!(is_retryable_io_error(&io::Error::from(
        io::ErrorKind::TimedOut
    )));
    assert!(!is_retryable_io_error(&io::Error::from(
        io::ErrorKind::ConnectionReset
    )));
}

#[test]
fn wait_before_io_retry_obeys_stop_signal() {
    let stop = Arc::new(AtomicBool::new(false));
    let started_at = Instant::now();
    assert!(wait_before_io_retry(&stop));
    assert!(started_at.elapsed() >= SSH_IO_RETRY_BACKOFF);

    stop.store(true, Ordering::Relaxed);
    let stopped_at = Instant::now();
    assert!(!wait_before_io_retry(&stop));
    assert!(stopped_at.elapsed() < SSH_IO_RETRY_BACKOFF);
}
