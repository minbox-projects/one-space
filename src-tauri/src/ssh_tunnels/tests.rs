use super::*;
use crate::config::test_home::TestHomeGuard;
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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

    run_supervision(
        &state,
        true,
        &stop,
        source,
        delay_collector(delays.clone()),
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

    run_supervision(&state, true, &stop, source, sleep, &mut observer);

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
