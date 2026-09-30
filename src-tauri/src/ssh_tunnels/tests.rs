use super::*;
use crate::config::test_home::TestHomeGuard;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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
        common_ports: default_common_ports(),
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
        common_ports: default_common_ports(),
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

fn record_with_id(id: &str, name: &str) -> SshTunnelRecord {
    let mut record = sample_record(DEFAULT_TUNNEL_GROUP_ID);
    record.id = id.to_string();
    record.name = name.to_string();
    record
}

fn running_ids(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn connect_all_selection_skips_running_tunnels_and_preserves_saved_order() {
    let tunnels = vec![
        record_with_id("tunnel-a", "A"),
        record_with_id("tunnel-b", "B"),
        record_with_id("tunnel-c", "C"),
    ];
    let running = running_ids(&["tunnel-b"]);

    let selected =
        select_all_tunnels_batch_ids(&tunnels, &running, AllTunnelsBatchOperation::Connect);

    assert_eq!(
        selected,
        vec!["tunnel-a".to_string(), "tunnel-c".to_string()]
    );
}

#[test]
fn disconnect_all_selection_returns_only_running_tunnels_in_saved_order() {
    let tunnels = vec![
        record_with_id("tunnel-a", "A"),
        record_with_id("tunnel-b", "B"),
        record_with_id("tunnel-c", "C"),
    ];
    let running = running_ids(&["tunnel-c", "tunnel-a"]);

    let selected =
        select_all_tunnels_batch_ids(&tunnels, &running, AllTunnelsBatchOperation::Disconnect);

    assert_eq!(
        selected,
        vec!["tunnel-a".to_string(), "tunnel-c".to_string()]
    );
}

#[test]
fn all_tunnels_selection_with_no_saved_tunnels_returns_empty_for_both_operations() {
    let tunnels: Vec<SshTunnelRecord> = Vec::new();
    let running = running_ids(&["tunnel-a"]);

    let connect_selection =
        select_all_tunnels_batch_ids(&tunnels, &running, AllTunnelsBatchOperation::Connect);
    let disconnect_selection =
        select_all_tunnels_batch_ids(&tunnels, &running, AllTunnelsBatchOperation::Disconnect);

    assert!(connect_selection.is_empty());
    assert!(disconnect_selection.is_empty());
}

#[test]
fn all_tunnels_selection_ignores_running_ids_without_a_saved_tunnel() {
    let tunnels = vec![record_with_id("tunnel-a", "A")];
    let running = running_ids(&["ghost-tunnel"]);

    let connect_selection =
        select_all_tunnels_batch_ids(&tunnels, &running, AllTunnelsBatchOperation::Connect);
    let disconnect_selection =
        select_all_tunnels_batch_ids(&tunnels, &running, AllTunnelsBatchOperation::Disconnect);

    assert_eq!(connect_selection, vec!["tunnel-a".to_string()]);
    assert!(disconnect_selection.is_empty());
}

#[test]
fn aggregation_derives_failed_count_from_failures_and_keeps_named_details() {
    let failures = vec![
        SshTunnelBatchFailureDetail {
            tunnel_id: "tunnel-b".to_string(),
            tunnel_name: "B".to_string(),
            error: "connection refused".to_string(),
        },
        SshTunnelBatchFailureDetail {
            tunnel_id: "tunnel-d".to_string(),
            tunnel_name: "D".to_string(),
            error: "authentication failed".to_string(),
        },
    ];

    let result = aggregate_all_tunnels_batch_result("connect", 5, 1, 2, failures);

    assert_eq!(result.operation, "connect");
    assert_eq!(result.group_id, "all");
    assert_eq!(result.group_name, "All Tunnels");
    assert_eq!(result.total_count, 5);
    assert_eq!(result.skipped_count, 1);
    assert_eq!(result.success_count, 2);
    assert_eq!(result.failed_count, 2);
    assert_eq!(result.failures.len(), 2);
    assert_eq!(result.failures[0].tunnel_id, "tunnel-b");
    assert_eq!(result.failures[0].tunnel_name, "B");
    assert_eq!(result.failures[0].error, "connection refused");
    assert_eq!(result.failures[1].tunnel_id, "tunnel-d");
    assert_eq!(result.failures[1].tunnel_name, "D");
    assert_eq!(result.failures[1].error, "authentication failed");
}

#[test]
fn aggregation_without_failures_reports_zero_failed_and_keeps_all_tunnels_identity() {
    let result = aggregate_all_tunnels_batch_result("disconnect", 3, 3, 0, Vec::new());

    assert_eq!(result.operation, "disconnect");
    assert_eq!(result.group_id, "all");
    assert_eq!(result.group_name, "All Tunnels");
    assert_eq!(result.total_count, 3);
    assert_eq!(result.skipped_count, 3);
    assert_eq!(result.success_count, 0);
    assert_eq!(result.failed_count, 0);
    assert!(result.failures.is_empty());
}

const RUN_APP_SOURCE: &str = include_str!("../app_runtime/run_app.rs");

fn invoke_handler_registration_block() -> &'static str {
    RUN_APP_SOURCE
        .split_once(".invoke_handler(tauri::generate_handler![")
        .expect("invoke handler registration block start")
        .1
        .split_once(".build(tauri::generate_context!())")
        .expect("invoke handler registration block end")
        .0
}

#[test]
fn invoke_handler_registers_both_all_tunnels_batch_commands_exactly_once() {
    let block = invoke_handler_registration_block();

    assert_eq!(
        block
            .matches("ssh_tunnels::ssh_tunnels_connect_all")
            .count(),
        1,
        "ssh_tunnels_connect_all must be registered exactly once"
    );
    assert_eq!(
        block
            .matches("ssh_tunnels::ssh_tunnels_disconnect_all")
            .count(),
        1,
        "ssh_tunnels_disconnect_all must be registered exactly once"
    );
}

#[derive(Default)]
struct RecordingObserver {
    failures: Vec<(String, FailureKind, String)>,
    updates: usize,
}

impl SupervisorObserver for RecordingObserver {
    fn record_failure(&mut self, category: &str, kind: FailureKind, message: &str) {
        self.failures
            .push((category.to_string(), kind, message.to_string()));
    }

    fn state_changed(&mut self) {
        self.updates += 1;
    }
}

fn test_runtime_state() -> RuntimeState {
    RuntimeState {
        status: SshTunnelStatus::Connecting,
        mode: SshTunnelForwardMode::Local,
        summary: "test".to_string(),
        resolved_server_host: None,
        listening_addr: None,
        last_error: None,
    }
}

fn outcome_script(
    outcomes: Vec<RuntimeOutcome>,
    attempts: Arc<AtomicUsize>,
) -> impl FnMut() -> RuntimeOutcome {
    let mut remaining = outcomes.into_iter();
    move || {
        attempts.fetch_add(1, Ordering::Relaxed);
        remaining.next().unwrap_or(RuntimeOutcome::Stopped)
    }
}

fn delay_collector(delays: Arc<Mutex<Vec<Duration>>>) -> impl FnMut(Duration) -> bool {
    move |delay| {
        delays.lock().unwrap().push(delay);
        true
    }
}

fn status_of(state: &Arc<Mutex<RuntimeState>>) -> SshTunnelStatus {
    state.lock().unwrap().status.clone()
}

#[test]
fn retry_delay_starts_at_two_seconds_and_caps_at_forty_eight_without_jitter() {
    assert_eq!(next_retry_delay(0, 0.0), Duration::from_secs(2));
    assert_eq!(next_retry_delay(1, 0.0), Duration::from_secs(4));
    assert_eq!(next_retry_delay(4, 0.0), Duration::from_secs(32));
    assert_eq!(next_retry_delay(5, 0.0), Duration::from_secs(48));
    assert_eq!(next_retry_delay(20, 0.0), Duration::from_secs(48));
}

#[test]
fn retry_delay_jitter_never_exceeds_sixty_seconds_and_differs_per_fraction() {
    assert_eq!(next_retry_delay(0, 0.25), Duration::from_millis(2500));
    assert_eq!(next_retry_delay(5, 0.25), Duration::from_secs(60));

    // The same attempt with different jitter fractions must yield different delays.
    assert_ne!(next_retry_delay(3, 0.1), next_retry_delay(3, 0.2));

    // base(2) == 8s, so jittered results stay inside [8s, 10s].
    let low = next_retry_delay(2, 0.1);
    let high = next_retry_delay(2, 0.2);
    assert!(low >= Duration::from_secs(8) && low <= Duration::from_secs(10));
    assert!(high >= Duration::from_secs(8) && high <= Duration::from_secs(10));
}

#[test]
fn failure_kind_classification_matches_retry_policy() {
    assert!(is_retryable(FailureKind::Transport));
    assert!(is_retryable(FailureKind::Port));
    assert!(is_retryable(FailureKind::Target));
    assert!(!is_retryable(FailureKind::Auth));
    assert!(!is_retryable(FailureKind::HostKey));
    assert!(!is_retryable(FailureKind::Config));
}

#[test]
fn supervisor_keeps_retrying_a_failed_first_attempt_until_user_stop() {
    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let delays = Arc::new(Mutex::new(Vec::new()));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(
        vec![
            RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Transport,
                message: "boom-1".to_string(),
            },
            RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Transport,
                message: "boom-2".to_string(),
            },
            RuntimeOutcome::Stopped,
        ],
        attempts.clone(),
    );

    run_supervision(
        "test-supervisor-retry-first-attempt",
        &state,
        true,
        &stop,
        source,
        delay_collector(delays.clone()),
        &mut observer,
    );

    assert_eq!(attempts.load(Ordering::Relaxed), 3);
    assert_eq!(observer.failures.len(), 2);
    assert_eq!(status_of(&state), SshTunnelStatus::Disconnected);

    let delays = delays.lock().unwrap();
    assert_eq!(delays.len(), 2);
    assert!(delays[0] >= Duration::from_secs(2) && delays[0] <= Duration::from_millis(2500));
    assert!(delays[1] >= Duration::from_secs(4) && delays[1] <= Duration::from_secs(5));
}

#[test]
fn supervisor_three_consecutive_startup_failures_still_retry_with_growing_delays() {
    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let delays = Arc::new(Mutex::new(Vec::new()));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(
        vec![
            RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Transport,
                message: "net down".to_string(),
            },
            RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Transport,
                message: "net down".to_string(),
            },
            RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Transport,
                message: "net down".to_string(),
            },
            RuntimeOutcome::Stopped,
        ],
        attempts.clone(),
    );

    // The supervisor must keep the status at `Connecting` until it has an
    // outcome, and must switch to `Reconnecting` before every backoff sleep.
    assert_eq!(status_of(&state), SshTunnelStatus::Connecting);

    let state_for_sleep = state.clone();
    let delays_for_sleep = delays.clone();
    let mut sleep = move |delay: Duration| {
        delays_for_sleep.lock().unwrap().push(delay);
        assert_eq!(
            status_of(&state_for_sleep),
            SshTunnelStatus::Reconnecting,
            "the tunnel must read as Reconnecting while backing off"
        );
        true
    };

    run_supervision(
        "test-supervisor-three-failures",
        &state,
        true,
        &stop,
        source,
        &mut sleep,
        &mut observer,
    );

    assert_eq!(attempts.load(Ordering::Relaxed), 4);
    assert_eq!(observer.failures.len(), 3);

    let delays = delays.lock().unwrap();
    assert_eq!(delays.len(), 3);
    assert!(delays[0] >= Duration::from_secs(2) && delays[0] <= Duration::from_millis(2500));
    assert!(delays[1] >= Duration::from_secs(4) && delays[1] <= Duration::from_secs(5));
    assert!(delays[2] >= Duration::from_secs(8) && delays[2] <= Duration::from_secs(10));
    assert!(delays[0] < delays[1]);
    assert!(delays[1] < delays[2]);

    for (category, _, message) in &observer.failures {
        assert_eq!(category, "auto-connect");
        assert_eq!(message, "net down");
    }
    assert!(observer.updates > 0);
}

#[test]
fn supervisor_resets_backoff_after_a_dropped_connected_runtime() {
    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let delays = Arc::new(Mutex::new(Vec::new()));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(
        vec![
            RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Transport,
                message: "a".to_string(),
            },
            RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Transport,
                message: "b".to_string(),
            },
            RuntimeOutcome::DroppedAfterConnected {
                kind: FailureKind::Transport,
                message: "c".to_string(),
            },
            RuntimeOutcome::Stopped,
        ],
        attempts.clone(),
    );

    run_supervision(
        "test-supervisor-backoff-reset",
        &state,
        true,
        &stop,
        source,
        delay_collector(delays.clone()),
        &mut observer,
    );

    assert_eq!(attempts.load(Ordering::Relaxed), 4);

    let delays = delays.lock().unwrap();
    assert_eq!(delays.len(), 3);
    assert!(delays[0] >= Duration::from_secs(2) && delays[0] <= Duration::from_millis(2500));
    assert!(delays[1] >= Duration::from_secs(4) && delays[1] <= Duration::from_secs(5));
    assert!(delays[2] >= Duration::from_secs(2) && delays[2] <= Duration::from_millis(2500));
    assert!(delays[2] < delays[1]);

    assert_eq!(observer.failures.len(), 3);
    assert_eq!(observer.failures[2].0, "auto-reconnect");
}

#[test]
fn supervisor_terminal_failure_stops_in_error_with_one_message_and_one_update() {
    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let delays = Arc::new(Mutex::new(Vec::new()));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(
        vec![RuntimeOutcome::FailedAtStartup {
            kind: FailureKind::Auth,
            message: "bad password".to_string(),
        }],
        attempts.clone(),
    );

    run_supervision(
        "test-supervisor-terminal-failure",
        &state,
        true,
        &stop,
        source,
        delay_collector(delays.clone()),
        &mut observer,
    );

    assert_eq!(attempts.load(Ordering::Relaxed), 1);
    assert_eq!(status_of(&state), SshTunnelStatus::Error);
    assert_eq!(
        state.lock().unwrap().last_error,
        Some("bad password".to_string())
    );
    assert_eq!(observer.failures.len(), 1);
    assert_eq!(observer.failures[0].0, "auto-connect");
    assert_eq!(observer.updates, 1);
    assert!(delays.lock().unwrap().is_empty());
}

#[test]
fn supervisor_disabled_auto_reconnect_stops_in_error_after_one_failure() {
    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let delays = Arc::new(Mutex::new(Vec::new()));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(
        vec![RuntimeOutcome::FailedAtStartup {
            kind: FailureKind::Transport,
            message: "x".to_string(),
        }],
        attempts.clone(),
    );

    run_supervision(
        "test-supervisor-disabled-auto-reconnect",
        &state,
        false,
        &stop,
        source,
        delay_collector(delays.clone()),
        &mut observer,
    );

    assert_eq!(attempts.load(Ordering::Relaxed), 1);
    assert_eq!(status_of(&state), SshTunnelStatus::Error);
    assert_eq!(observer.failures.len(), 1);
    assert_eq!(observer.updates, 1);
    assert!(delays.lock().unwrap().is_empty());
}

#[test]
fn supervisor_user_stop_during_backoff_ends_in_disconnected() {
    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let delays = Arc::new(Mutex::new(Vec::new()));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(
        vec![RuntimeOutcome::FailedAtStartup {
            kind: FailureKind::Transport,
            message: "x".to_string(),
        }],
        attempts.clone(),
    );

    let stop_for_sleep = stop.clone();
    let delays_for_sleep = delays.clone();
    let sleep = move |delay: Duration| {
        delays_for_sleep.lock().unwrap().push(delay);
        stop_for_sleep.store(true, Ordering::Relaxed);
        false
    };

    run_supervision(
        "test-supervisor-user-stop-backoff",
        &state,
        true,
        &stop,
        source,
        sleep,
        &mut observer,
    );

    assert_eq!(attempts.load(Ordering::Relaxed), 1);
    assert_eq!(status_of(&state), SshTunnelStatus::Disconnected);
    assert_eq!(delays.lock().unwrap().len(), 1);

    // A stop that is already set before the first outcome must end immediately.
    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(true));
    let attempts = Arc::new(AtomicUsize::new(0));
    let delays = Arc::new(Mutex::new(Vec::new()));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(Vec::new(), attempts.clone());

    run_supervision(
        "test-supervisor-prestop",
        &state,
        true,
        &stop,
        source,
        delay_collector(delays.clone()),
        &mut observer,
    );

    assert_eq!(attempts.load(Ordering::Relaxed), 0);
    assert_eq!(status_of(&state), SshTunnelStatus::Disconnected);
}

struct TempHome {
    path: PathBuf,
    _guard: TestHomeGuard,
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn isolated_temp_home(name: &str) -> TempHome {
    let path = std::env::temp_dir().join(format!(
        "onespace-ssh-tunnels-{}-{}",
        name,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&path).expect("create temp home");
    let guard = TestHomeGuard::set(&path);
    TempHome {
        path,
        _guard: guard,
    }
}

fn custom_key_record_without_key_path(id: &str) -> SshTunnelRecord {
    let mut record = sample_record(DEFAULT_TUNNEL_GROUP_ID);
    record.id = id.to_string();
    record.name = "Pre-spawn tunnel".to_string();
    record.source_kind = SshTunnelSourceKind::Custom;
    record.saved_host_name = None;
    record.custom = Some(SshTunnelCustomConfig {
        host: "127.0.0.1".to_string(),
        port: 22,
        user: "u".to_string(),
        auth_kind: SshTunnelAuthKind::Key,
        key_path: None,
    });
    record
}

#[test]
fn pre_spawn_resolution_failure_persists_error_without_spawning() {
    let _home = isolated_temp_home("pre-spawn");
    let id = format!("tunnel-pre-spawn-{}", uuid::Uuid::new_v4());
    let record = custom_key_record_without_key_path(&id);

    mutate_records(|records| {
        records.push(record.clone());
        Ok(())
    })
    .expect("seed tunnel record");

    let record = load_record_by_id(&id).expect("load seeded record").unwrap();
    let failure = resolve_connect_target(&record).expect_err("resolution must fail without a key path");

    assert_eq!(failure.kind, FailureKind::Config);
    assert!(!failure.message.is_empty());

    let reloaded = load_record_by_id(&id).expect("reload seeded record").unwrap();
    assert_eq!(reloaded.last_error, Some(failure.message.clone()));
    assert!(runtime_manager().lock().unwrap().get(&id).is_none());
}

#[test]
fn supervised_failure_messages_deduplicate_for_one_tunnel() {
    let _home = isolated_temp_home("dedupe");
    let app = tauri::test::mock_app();
    let id = format!("tunnel-pre-spawn-{}", uuid::Uuid::new_v4());
    let record = custom_key_record_without_key_path(&id);

    let first = tunnel_failure_message_input(&record, "auto-connect", "boom");
    crate::messages::create_message_with_app(app.handle(), first)
        .expect("first failure message must persist");
    let second = tunnel_failure_message_input(&record, "auto-connect", "boom");
    crate::messages::create_message_with_app(app.handle(), second)
        .expect("second failure message must persist");

    let messages = crate::messages::list_messages_with_app(app.handle()).expect("list messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].dedupe_key,
        Some(format!("ssh-tunnels:auto-connect:{}", id))
    );
    assert_eq!(messages[0].occurrences, 2);
}

#[test]
fn two_step_probe_classifies_transport_failure_and_skips_target() {
    let target_ran = Arc::new(AtomicBool::new(false));
    let target_ran_for_probe = target_ran.clone();

    let outcome = run_two_step_probe(
        || -> Result<(), String> { Err("blackhole".to_string()) },
        move || -> Result<(), String> {
            target_ran_for_probe.store(true, Ordering::Relaxed);
            Ok(())
        },
    );

    assert_eq!(
        outcome,
        Err((FailureKind::Transport, "blackhole".to_string()))
    );
    assert!(!target_ran.load(Ordering::Relaxed));
}

#[test]
fn two_step_probe_classifies_target_failure_after_transport_success() {
    let outcome = run_two_step_probe(
        || -> Result<(), String> { Ok(()) },
        || -> Result<(), String> { Err("target refused".to_string()) },
    );

    assert_eq!(
        outcome,
        Err((FailureKind::Target, "target refused".to_string()))
    );
}

#[test]
fn two_step_probe_succeeds_when_both_steps_succeed() {
    let outcome = run_two_step_probe(
        || -> Result<(), String> { Ok(()) },
        || -> Result<(), String> { Ok(()) },
    );

    assert_eq!(outcome, Ok(()));
}

#[test]
fn probe_reconnects_only_on_the_second_consecutive_transport_failure() {
    let state = Arc::new(Mutex::new(test_runtime_state()));
    let mut counter: u32 = 0;

    let first = apply_probe_outcome(
        &state,
        &mut counter,
        Err((FailureKind::Transport, "reset".to_string())),
    );
    assert!(!first);
    assert_eq!(counter, 1);
    assert_eq!(state.lock().unwrap().last_error, Some("reset".to_string()));

    let second = apply_probe_outcome(
        &state,
        &mut counter,
        Err((FailureKind::Transport, "reset".to_string())),
    );
    assert!(second);
    assert_eq!(counter, 2);
}

#[test]
fn probe_success_resets_the_transport_counter_and_clears_last_error() {
    let state = Arc::new(Mutex::new(test_runtime_state()));
    state.lock().unwrap().last_error = Some("stale".to_string());
    let mut counter: u32 = 2;

    let should_reconnect = apply_probe_outcome(&state, &mut counter, Ok(()));

    assert!(!should_reconnect);
    assert_eq!(counter, 0);
    assert_eq!(state.lock().unwrap().last_error, None);
}

#[test]
fn probe_target_failure_records_error_without_touching_the_transport_counter() {
    let state = Arc::new(Mutex::new(test_runtime_state()));
    let mut counter: u32 = 1;

    let target_result = apply_probe_outcome(
        &state,
        &mut counter,
        Err((FailureKind::Target, "refused".to_string())),
    );

    assert!(!target_result);
    assert_eq!(counter, 1);
    assert_eq!(state.lock().unwrap().last_error, Some("refused".to_string()));

    // The retained earlier transport failure makes this the second consecutive
    // transport failure.
    let transport_result = apply_probe_outcome(
        &state,
        &mut counter,
        Err((FailureKind::Transport, "dead".to_string())),
    );
    assert!(transport_result);

    // A single target failure on a fresh counter never reconnects.
    let fresh_state = Arc::new(Mutex::new(test_runtime_state()));
    let mut fresh_counter: u32 = 0;
    let alone = apply_probe_outcome(
        &fresh_state,
        &mut fresh_counter,
        Err((FailureKind::Target, "refused".to_string())),
    );
    assert!(!alone);
    assert_eq!(fresh_counter, 0);
}

#[test]
fn accept_timeout_kinds_are_periodic_ticks_regardless_of_message_text() {
    assert!(accept_error_is_periodic_tick(&io::Error::new(
        io::ErrorKind::TimedOut,
        "totally unrelated"
    )));
    assert!(accept_error_is_periodic_tick(&io::Error::new(
        io::ErrorKind::WouldBlock,
        "some other text"
    )));
    assert!(!accept_error_is_periodic_tick(&io::Error::new(
        io::ErrorKind::ConnectionReset,
        "Operation timed out"
    )));
    assert!(!accept_error_is_periodic_tick(&io::Error::new(
        io::ErrorKind::Other,
        "timed out"
    )));
}

fn dummy_running_tunnel_with_stop(stop: Arc<AtomicBool>) -> RunningTunnel {
    RunningTunnel {
        stop,
        active_clients: Arc::new(AtomicUsize::new(0)),
        state: Arc::new(Mutex::new(test_runtime_state())),
        join: None,
    }
}

fn dummy_running_tunnel_with_status(status: SshTunnelStatus) -> RunningTunnel {
    let mut state = test_runtime_state();
    state.status = status;
    RunningTunnel {
        stop: Arc::new(AtomicBool::new(false)),
        active_clients: Arc::new(AtomicUsize::new(0)),
        state: Arc::new(Mutex::new(state)),
        join: None,
    }
}

fn running_tunnel_with_join(join: Option<JoinHandle<()>>) -> RunningTunnel {
    RunningTunnel {
        stop: Arc::new(AtomicBool::new(false)),
        active_clients: Arc::new(AtomicUsize::new(0)),
        state: Arc::new(Mutex::new(test_runtime_state())),
        join,
    }
}

fn dummy_running_tunnel() -> RunningTunnel {
    dummy_running_tunnel_with_stop(Arc::new(AtomicBool::new(false)))
}

/// Blocks the calling thread until `handle` has finished, bounded so a broken
/// spawn can never hang the test run.
fn wait_until_finished(handle: &JoinHandle<()>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !handle.is_finished() {
        assert!(
            Instant::now() < deadline,
            "spawned thread must finish within the test bound"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn unique_tunnel_id(prefix: &str) -> String {
    format!("test-{}-{}", prefix, uuid::Uuid::new_v4())
}

/// Restores the process-memory desired set, start claims and runtime manager to
/// their pre-test state for `id`.
fn cleanup_tunnel_state(id: &str) {
    clear_tunnel_desired(id);
    release_tunnel_start_claim(id);
    abandon_tunnel_start(id);
    runtime_manager().lock().unwrap().remove(id);
}

#[test]
fn terminal_error_view_survives_refresh_status() {
    let _home = isolated_temp_home("terminal-error-view");
    let error_id = unique_tunnel_id("terminal-error");
    let disconnected_id = unique_tunnel_id("terminal-disconnected");

    mutate_records(|records| {
        records.push(record_with_id(&error_id, "Terminal error"));
        records.push(record_with_id(&disconnected_id, "Finished disconnected"));
        Ok(())
    })
    .expect("seed tunnel records");

    let error_join = std::thread::spawn(|| {});
    wait_until_finished(&error_join);
    let mut error_state = test_runtime_state();
    error_state.status = SshTunnelStatus::Error;
    error_state.last_error = Some("boom".to_string());
    runtime_manager().lock().unwrap().insert(
        error_id.clone(),
        RunningTunnel {
            stop: Arc::new(AtomicBool::new(false)),
            active_clients: Arc::new(AtomicUsize::new(0)),
            state: Arc::new(Mutex::new(error_state)),
            join: Some(error_join),
        },
    );

    let disconnected_join = std::thread::spawn(|| {});
    wait_until_finished(&disconnected_join);
    let mut disconnected_state = test_runtime_state();
    disconnected_state.status = SshTunnelStatus::Disconnected;
    runtime_manager().lock().unwrap().insert(
        disconnected_id.clone(),
        RunningTunnel {
            stop: Arc::new(AtomicBool::new(false)),
            active_clients: Arc::new(AtomicUsize::new(0)),
            state: Arc::new(Mutex::new(disconnected_state)),
            join: Some(disconnected_join),
        },
    );

    let views = ssh_tunnels_refresh_status().expect("refresh status must succeed");

    let error_view = views
        .iter()
        .find(|view| view.id == error_id)
        .expect("error tunnel view must be present");
    assert_eq!(
        error_view.status,
        SshTunnelStatus::Error,
        "a terminal Error view must survive refresh_status reaping"
    );

    let disconnected_view = views
        .iter()
        .find(|view| view.id == disconnected_id)
        .expect("disconnected tunnel view must be present");
    assert_eq!(disconnected_view.status, SshTunnelStatus::Disconnected);
    assert!(
        !runtime_manager().lock().unwrap().contains_key(&disconnected_id),
        "a finished Disconnected instance must still be reaped"
    );

    cleanup_tunnel_state(&error_id);
    cleanup_tunnel_state(&disconnected_id);
}

#[test]
fn watchdog_candidates_are_desired_minus_live_and_claimed() {
    let desired: HashSet<String> = ["a", "b", "c"].iter().map(|id| id.to_string()).collect();
    let busy: HashSet<String> = ["b", "c", "d"].iter().map(|id| id.to_string()).collect();
    assert_eq!(
        watchdog_restart_candidates(&desired, &busy),
        vec!["a".to_string()]
    );

    let desired: HashSet<String> = ["a", "b"].iter().map(|id| id.to_string()).collect();
    let busy: HashSet<String> = ["b"].iter().map(|id| id.to_string()).collect();
    assert_eq!(
        watchdog_restart_candidates(&desired, &busy),
        vec!["a".to_string()]
    );
}

#[test]
fn watchdog_candidates_are_empty_without_desired_tunnels() {
    let desired = HashSet::<String>::new();
    let busy: HashSet<String> = ["a", "b"].iter().map(|id| id.to_string()).collect();
    assert_eq!(watchdog_restart_candidates(&desired, &busy), Vec::<String>::new());
}

#[test]
fn manual_disconnect_removes_a_tunnel_from_watchdog_candidates() {
    let id = unique_tunnel_id("watchdog-manual-disconnect");
    mark_tunnel_desired(&id);

    let busy = HashSet::<String>::new();
    let before = desired_tunnel_ids();
    assert!(watchdog_restart_candidates(&before, &busy).contains(&id));

    assert!(disconnect_tunnel(&id).is_ok());
    assert!(!desired_tunnel_ids().contains(&id));

    let after = desired_tunnel_ids();
    assert!(!watchdog_restart_candidates(&after, &busy).contains(&id));

    cleanup_tunnel_state(&id);
}

#[test]
fn start_claim_is_exclusive() {
    let id = unique_tunnel_id("start-claim-exclusive");

    mark_tunnel_desired(&id);
    assert!(matches!(begin_tunnel_start(&id, false), Ok(None)));
    assert!(begin_tunnel_start(&id, false).is_err());
    assert!(tunnel_start_claim_held(&id));

    release_tunnel_start_claim(&id);
    assert!(matches!(begin_tunnel_start(&id, false), Ok(None)));

    cleanup_tunnel_state(&id);
}

#[test]
fn instance_in_manager_blocks_a_new_claim() {
    let id = unique_tunnel_id("instance-blocks-claim");
    mark_tunnel_desired(&id);
    runtime_manager()
        .lock()
        .unwrap()
        .insert(id.clone(), dummy_running_tunnel());

    assert!(begin_tunnel_start(&id, false).is_err());
    assert!(!tunnel_start_claim_held(&id));

    cleanup_tunnel_state(&id);
}

#[test]
fn watchdog_start_requires_the_desired_flag() {
    let id = unique_tunnel_id("watchdog-requires-desired");

    assert!(begin_tunnel_start(&id, false).is_err());
    assert!(!desired_tunnel_ids().contains(&id));

    mark_tunnel_desired(&id);
    assert!(matches!(begin_tunnel_start(&id, false), Ok(None)));
    finish_tunnel_start(&id);

    cleanup_tunnel_state(&id);
}

#[test]
fn reconcile_started_tunnel_stops_an_instance_whose_desired_flag_was_cleared() {
    let id = unique_tunnel_id("reconcile-stops-cleared");
    let stop = Arc::new(AtomicBool::new(false));
    runtime_manager()
        .lock()
        .unwrap()
        .insert(id.clone(), dummy_running_tunnel_with_stop(stop.clone()));
    mark_tunnel_desired(&id);

    assert!(reconcile_started_tunnel_with_desired(&id));
    assert!(runtime_manager().lock().unwrap().contains_key(&id));

    clear_tunnel_desired(&id);
    assert!(!reconcile_started_tunnel_with_desired(&id));
    assert!(!runtime_manager().lock().unwrap().contains_key(&id));
    assert!(stop.load(Ordering::Relaxed));

    cleanup_tunnel_state(&id);
}

#[test]
fn watchdog_busy_ids_ignores_finished_instances() {
    let live_id = unique_tunnel_id("watchdog-busy-live");
    let finished_id = unique_tunnel_id("watchdog-busy-finished");
    let no_join_id = unique_tunnel_id("watchdog-busy-no-join");
    let claimed_id = unique_tunnel_id("watchdog-busy-claimed");

    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let live_join = std::thread::spawn(move || {
        let _ = release_rx.recv();
    });

    let finished_join = std::thread::spawn(|| {});
    wait_until_finished(&finished_join);

    let mut instances: HashMap<String, RunningTunnel> = HashMap::new();
    instances.insert(live_id.clone(), running_tunnel_with_join(Some(live_join)));
    instances.insert(
        finished_id.clone(),
        running_tunnel_with_join(Some(finished_join)),
    );
    instances.insert(no_join_id.clone(), dummy_running_tunnel());

    let mut claims: HashSet<String> = HashSet::new();
    claims.insert(claimed_id.clone());

    let busy = watchdog_busy_ids(&instances, &claims);

    let expected: HashSet<String> = [live_id.clone(), no_join_id.clone(), claimed_id.clone()]
        .into_iter()
        .collect();
    assert_eq!(busy, expected);
    assert!(!busy.contains(&finished_id));

    let _ = release_tx.send(());
    if let Some(instance) = instances.remove(&live_id) {
        if let Some(join) = instance.join {
            join.join().expect("live watchdog thread must join");
        }
    }
}

#[test]
fn group_connect_blocking_ids_exclude_terminal_errors() {
    let error_id = unique_tunnel_id("blocking-error");
    let connecting_id = unique_tunnel_id("blocking-connecting");
    let connected_id = unique_tunnel_id("blocking-connected");
    let reconnecting_id = unique_tunnel_id("blocking-reconnecting");

    let mut instances: HashMap<String, RunningTunnel> = HashMap::new();
    instances.insert(
        error_id.clone(),
        dummy_running_tunnel_with_status(SshTunnelStatus::Error),
    );
    instances.insert(
        connecting_id.clone(),
        dummy_running_tunnel_with_status(SshTunnelStatus::Connecting),
    );
    instances.insert(
        connected_id.clone(),
        dummy_running_tunnel_with_status(SshTunnelStatus::Connected),
    );
    instances.insert(
        reconnecting_id.clone(),
        dummy_running_tunnel_with_status(SshTunnelStatus::Reconnecting),
    );

    let blocking = connect_blocking_running_ids(&instances);

    let expected: HashSet<String> = [connecting_id, connected_id, reconnecting_id]
        .into_iter()
        .collect();
    assert_eq!(blocking, expected);
    assert!(!blocking.contains(&error_id));
}

#[test]
fn concurrent_start_claims_have_exactly_one_winner() {
    let id = unique_tunnel_id("start-claim-race");
    mark_tunnel_desired(&id);
    let barrier = Arc::new(std::sync::Barrier::new(2));

    let mut handles = Vec::new();
    for _ in 0..2 {
        let id = id.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || -> bool {
            barrier.wait();
            begin_tunnel_start(&id, false).is_ok()
        }));
    }

    let results: Vec<bool> = handles
        .into_iter()
        .map(|handle| handle.join().expect("claim thread must join"))
        .collect();

    assert_eq!(results.iter().filter(|claimed| **claimed).count(), 1);

    cleanup_tunnel_state(&id);
}

#[test]
fn connecting_marks_the_tunnel_desired() {
    let id = unique_tunnel_id("connecting-marks-desired");

    let started = begin_tunnel_start(&id, true);
    assert!(matches!(started, Ok(None)));
    assert!(desired_tunnel_ids().contains(&id));
    assert!(tunnel_start_claim_held(&id));

    finish_tunnel_start(&id);
    assert!(!tunnel_start_claim_held(&id));

    cleanup_tunnel_state(&id);
}

#[test]
fn begin_replace_existing_returns_the_running_instance_once() {
    let id = unique_tunnel_id("begin-replace-existing");
    runtime_manager()
        .lock()
        .unwrap()
        .insert(id.clone(), dummy_running_tunnel());

    let replaced = begin_tunnel_start(&id, true);
    assert!(matches!(replaced, Ok(Some(_))));
    assert!(!runtime_manager().lock().unwrap().contains_key(&id));

    let blocked = begin_tunnel_start(&id, false);
    assert!(blocked.is_err());

    cleanup_tunnel_state(&id);
}

#[test]
fn abandon_start_releases_claim_and_clears_desired() {
    let id = unique_tunnel_id("abandon-start");

    mark_tunnel_desired(&id);
    assert!(begin_tunnel_start(&id, false).is_ok());
    abandon_tunnel_start(&id);

    assert!(!desired_tunnel_ids().contains(&id));
    assert!(!tunnel_start_claim_held(&id));

    // The claim is free again, so the tunnel can be started once it is desired.
    mark_tunnel_desired(&id);
    assert!(begin_tunnel_start(&id, false).is_ok());

    cleanup_tunnel_state(&id);
}

#[test]
fn disconnect_tunnel_clears_desired_and_removes_the_instance() {
    let id = unique_tunnel_id("disconnect-clears");
    mark_tunnel_desired(&id);
    runtime_manager()
        .lock()
        .unwrap()
        .insert(id.clone(), dummy_running_tunnel());

    assert!(disconnect_tunnel(&id).is_ok());
    assert!(!runtime_manager().lock().unwrap().contains_key(&id));
    assert!(!desired_tunnel_ids().contains(&id));

    cleanup_tunnel_state(&id);
}

#[test]
fn supervisor_terminal_exit_clears_desired() {
    let id = unique_tunnel_id("supervisor-terminal-desired");
    mark_tunnel_desired(&id);

    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(
        vec![RuntimeOutcome::FailedAtStartup {
            kind: FailureKind::Auth,
            message: "bad".to_string(),
        }],
        attempts,
    );

    run_supervision(
        &id,
        &state,
        true,
        &stop,
        source,
        delay_collector(Arc::new(Mutex::new(Vec::new()))),
        &mut observer,
    );

    assert_eq!(status_of(&state), SshTunnelStatus::Error);
    assert!(!desired_tunnel_ids().contains(&id));

    cleanup_tunnel_state(&id);
}

#[test]
fn supervisor_disabled_auto_reconnect_exit_clears_desired() {
    let id = unique_tunnel_id("supervisor-disabled-desired");
    mark_tunnel_desired(&id);

    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(
        vec![RuntimeOutcome::FailedAtStartup {
            kind: FailureKind::Transport,
            message: "x".to_string(),
        }],
        attempts,
    );

    run_supervision(
        &id,
        &state,
        false,
        &stop,
        source,
        delay_collector(Arc::new(Mutex::new(Vec::new()))),
        &mut observer,
    );

    assert_eq!(status_of(&state), SshTunnelStatus::Error);
    assert!(!desired_tunnel_ids().contains(&id));

    cleanup_tunnel_state(&id);
}

#[test]
fn supervisor_keeps_desired_while_retrying_and_clears_it_on_stop() {
    let id = unique_tunnel_id("supervisor-retrying-desired");
    mark_tunnel_desired(&id);

    let state = Arc::new(Mutex::new(test_runtime_state()));
    let stop = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let mut observer = RecordingObserver::default();

    let source = outcome_script(
        vec![
            RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Transport,
                message: "a".to_string(),
            },
            RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Transport,
                message: "b".to_string(),
            },
            RuntimeOutcome::Stopped,
        ],
        attempts,
    );

    let id_for_sleep = id.clone();
    let sleep = move |_delay: Duration| {
        assert!(desired_tunnel_ids().contains(&id_for_sleep));
        true
    };

    run_supervision(&id, &state, true, &stop, source, sleep, &mut observer);

    assert!(!desired_tunnel_ids().contains(&id));
    assert_eq!(status_of(&state), SshTunnelStatus::Disconnected);

    cleanup_tunnel_state(&id);
}

#[test]
fn retry_sleep_returns_early_when_poked() {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = stop.clone();
    let (sender, receiver) = std::sync::mpsc::channel();

    let handle = std::thread::spawn(move || {
        let started = Instant::now();
        let result = sleep_respecting_stop_and_poke(&stop_for_thread, Duration::from_secs(30));
        let _ = sender.send((started.elapsed(), result));
    });

    std::thread::sleep(Duration::from_millis(100));
    bump_retry_poke();

    let received = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("a retry poke must wake the sleep early");
    handle.join().expect("sleep thread must join");

    let (elapsed, result) = received;
    assert!(result, "an early poke must report a successful wake");
    assert!(
        elapsed < Duration::from_secs(10),
        "an early poke must end well before the 30s duration: {:?}",
        elapsed
    );
}

#[test]
fn retry_sleep_returns_false_when_stopped() {
    let stop = Arc::new(AtomicBool::new(true));
    let started = Instant::now();

    assert!(!sleep_respecting_stop_and_poke(
        &stop,
        Duration::from_secs(30)
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn probe_tick_due_on_interval_or_poke() {
    assert!(!probe_tick_due(Duration::from_secs(1), false));
    assert!(probe_tick_due(Duration::from_secs(1), true));
    assert!(probe_tick_due(Duration::from_secs(11), false));
    assert!(probe_tick_due(Duration::ZERO, true));
}

#[test]
fn invoke_handler_registers_common_port_commands_exactly_once() {
    let block = invoke_handler_registration_block();

    assert_eq!(
        block
            .matches("ssh_tunnels::ssh_common_ports_list")
            .count(),
        1,
        "ssh_common_ports_list must be registered exactly once"
    );
    assert_eq!(
        block
            .matches("ssh_tunnels::ssh_common_port_upsert")
            .count(),
        1,
        "ssh_common_port_upsert must be registered exactly once"
    );
    assert_eq!(
        block
            .matches("ssh_tunnels::ssh_common_port_delete")
            .count(),
        1,
        "ssh_common_port_delete must be registered exactly once"
    );
}

#[test]
fn normalize_state_cleans_and_sorts_common_ports() {
    let mut state = SshTunnelState {
        groups: vec![default_group_record()],
        tunnels: vec![],
        common_ports: vec![
            SshCommonPortRecord {
                id: "dup".to_string(),
                name: "MySQL".to_string(),
                local_port: 3306,
                remote_port: 3306,
                port: None,
                description: None,
                created_at: 1,
                updated_at: 1,
            },
            SshCommonPortRecord {
                id: "dup".to_string(),
                name: "Duplicate".to_string(),
                local_port: 3307,
                remote_port: 3307,
                port: None,
                description: None,
                created_at: 2,
                updated_at: 2,
            },
            SshCommonPortRecord {
                id: "redis".to_string(),
                name: "Redis".to_string(),
                local_port: 6379,
                remote_port: 6379,
                port: None,
                description: Some("Redis cache".to_string()),
                created_at: 3,
                updated_at: 3,
            },
            SshCommonPortRecord {
                id: "ssh".to_string(),
                name: "SSH".to_string(),
                local_port: 22,
                remote_port: 22,
                port: None,
                description: None,
                created_at: 4,
                updated_at: 4,
            },
            SshCommonPortRecord {
                id: "zero".to_string(),
                name: "Zero Port".to_string(),
                local_port: 0,
                remote_port: 0,
                port: None,
                description: None,
                created_at: 5,
                updated_at: 5,
            },
            SshCommonPortRecord {
                id: "empty-name".to_string(),
                name: "   ".to_string(),
                local_port: 8080,
                remote_port: 8080,
                port: None,
                description: None,
                created_at: 6,
                updated_at: 6,
            },
        ],
    };

    normalize_state(&mut state);

    assert_eq!(state.common_ports.len(), 3);
    assert_eq!(state.common_ports[0].local_port, 22);
    assert_eq!(state.common_ports[0].remote_port, 22);
    assert_eq!(state.common_ports[0].name, "SSH");
    assert_eq!(state.common_ports[1].local_port, 3306);
    assert_eq!(state.common_ports[1].remote_port, 3306);
    assert_eq!(state.common_ports[1].name, "MySQL");
    assert_eq!(state.common_ports[2].local_port, 6379);
    assert_eq!(state.common_ports[2].remote_port, 6379);
    assert_eq!(state.common_ports[2].name, "Redis");
}

#[test]
fn default_common_ports_contain_standard_services() {
    let ports = default_common_ports();
    assert!(ports.iter().any(|p| p.local_port == 22 && p.remote_port == 22 && p.name == "SSH"));
    assert!(ports.iter().any(|p| p.local_port == 80 && p.remote_port == 80 && p.name == "HTTP"));
    assert!(ports.iter().any(|p| p.local_port == 443 && p.remote_port == 443 && p.name == "HTTPS"));
    assert!(ports.iter().any(|p| p.local_port == 3306 && p.remote_port == 3306 && p.name == "MySQL"));
    assert!(ports.iter().any(|p| p.local_port == 5432 && p.remote_port == 5432 && p.name == "PostgreSQL"));
    assert!(ports.iter().any(|p| p.local_port == 6379 && p.remote_port == 6379 && p.name == "Redis"));
}
