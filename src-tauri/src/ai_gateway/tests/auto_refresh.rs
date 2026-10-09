//! Behavior tests for the process-owned provider-template auto-refresh
//! scheduler and the shared same-template operation guard
//! (20261009-core-workflows-cleanup-and-optimization, REQ-002 / AC-002).
//!
//! These tests drive the production seams (`eligible_template_ids`,
//! `run_batch_with`, `start_template_op` and the failure snapshot helpers) and
//! observe only behavior: which templates are eligible, that a batch is
//! sequential, that ticks during a batch and automatic contenders against a busy
//! template are skipped, that a manual follower reuses the owner's result, and
//! that the failure snapshot/events follow record-and-clear transitions.
//!
//! Every test that touches process-global scheduler state takes the shared
//! `temp_home` lock so the batch-in-flight guard, the operation guard, the
//! failure store and the installed-scheduler singleton are never exercised
//! concurrently.

use super::temp_home;
use crate::ai_gateway::auto_refresh::{
    clear_template_failure, eligible_template_ids, failure_snapshot,
    install_scheduler_for_tests_with, request_rearm, reset_auto_refresh_for_tests, run_batch_with,
    scheduler_started, set_template_failure, start_template_op, stop_scheduler,
    sync_template_automatic, sync_template_manual, AutomaticOutcome, StartOutcome,
    TemplateAutoRefreshFailure, TEMPLATE_AUTO_REFRESH_EVENTS,
};
use crate::ai_gateway::templates::ProviderTemplateView;
use crate::ai_gateway::types_config::{
    GatewayConfig, ProviderTemplate, ProviderTemplateState, UpstreamProtocol,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn sample_template(id: &str, models_url: Option<&str>) -> ProviderTemplate {
    ProviderTemplate {
        id: id.to_string(),
        name: format!("Template {id}"),
        description: String::new(),
        base_url: "https://sample.test/v1".to_string(),
        protocol: UpstreamProtocol::ChatCompletions,
        source: "https://sample.test/models".to_string(),
        models_url: models_url.map(str::to_string),
        models: Vec::new(),
        icon: None,
    }
}

fn template_state(id: &str, models_url: Option<&str>) -> ProviderTemplateState {
    ProviderTemplateState {
        template_id: id.to_string(),
        template: Some(sample_template(id, models_url)),
        synced_at: None,
        source: None,
    }
}

fn sample_view(id: &str) -> ProviderTemplateView {
    ProviderTemplateView {
        template: sample_template(id, Some("https://sample.test/models")),
        synced_at: Some(1),
        source: "test".to_string(),
        from_snapshot: false,
    }
}

fn recorded_events() -> usize {
    TEMPLATE_AUTO_REFRESH_EVENTS.with(|events| events.borrow().len())
}

/// AC-002: only templates that declare a non-empty `models_url` are eligible;
/// blank, whitespace-only and absent URLs are skipped.
#[test]
fn eligible_templates_skip_blank_and_whitespace_model_urls() {
    let mut config = GatewayConfig::default();
    config.provider_templates = vec![
        template_state("blank", Some("")),
        template_state("space", Some("   ")),
        template_state("absent", None),
        template_state("valid", Some("https://valid.test/models")),
    ];

    let ids = eligible_template_ids(&config);

    assert!(
        ids.contains(&"valid".to_string()),
        "a template with a usable models URL must be eligible: {ids:?}"
    );
    for skipped in ["blank", "space", "absent"] {
        assert!(
            !ids.contains(&skipped.to_string()),
            "a template with a blank/absent models URL must be skipped: {skipped}"
        );
    }
}

/// AC-002: one batch runs eligible templates strictly in order and returns one
/// outcome per template.
#[tokio::test]
async fn batch_runs_templates_sequentially_in_order() {
    let _home = temp_home("auto-refresh-batch-order");
    reset_auto_refresh_for_tests();

    let order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = order.clone();

    let results = run_batch_with(
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        move |template_id| {
            let sink = sink.clone();
            let id = template_id.to_string();
            async move {
                sink.lock().expect("order lock").push(id);
                AutomaticOutcome::Skipped
            }
        },
    )
    .await
    .expect("an uncontended batch must run");

    assert_eq!(results.len(), 3, "one outcome per eligible template");
    assert_eq!(
        *order.lock().expect("order lock"),
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        "templates must run sequentially in order"
    );
}

/// AC-002: a tick that arrives while a batch is in flight is skipped instead of
/// starting a second concurrent batch.
#[tokio::test(flavor = "current_thread")]
async fn second_concurrent_batch_is_skipped_while_one_is_in_flight() {
    let _home = temp_home("auto-refresh-batch-skip");
    reset_auto_refresh_for_tests();

    let release = Arc::new(tokio::sync::Notify::new());
    let release_for_owner = release.clone();
    let owner = run_batch_with(vec!["t1".to_string()], move |_id| {
        let release = release_for_owner.clone();
        async move {
            release.notified().await;
            AutomaticOutcome::Skipped
        }
    });
    let contender = run_batch_with(vec!["t2".to_string()], |_id| async {
        AutomaticOutcome::Skipped
    });
    let notify = async { release.notify_one() };

    let (owner_result, contender_result, ()) = tokio::join!(owner, contender, notify);

    assert!(owner_result.is_some(), "the in-flight batch must complete");
    assert!(
        contender_result.is_none(),
        "a tick during a batch must be skipped, not run concurrently"
    );
}

/// AC-002: while a manual owner holds a template, an automatic contender skips
/// that template and a manual follower reuses the owner's result without
/// fetching again.
#[tokio::test]
async fn busy_template_skips_automatic_and_manual_follower_reuses_owner_result() {
    let _home = temp_home("auto-refresh-guard");
    reset_auto_refresh_for_tests();

    let owner = match start_template_op("t1") {
        StartOutcome::Owner(owner) => owner,
        StartOutcome::Follower(_) => panic!("no operation should own t1 yet"),
    };

    let automatic = sync_template_automatic(None, "t1").await;
    assert!(
        matches!(automatic, AutomaticOutcome::Skipped),
        "an automatic contender must skip a template another operation is refreshing"
    );

    let manual = sync_template_manual(None, "t1");
    let finish = async move { owner.finish(Ok(sample_view("t1"))) };
    let (manual_result, ()) = tokio::join!(manual, finish);
    let reused = manual_result.expect("the manual follower must reuse the owner result");
    assert_eq!(
        reused.template.id, "t1",
        "the follower must observe the owner's result, not fetch again"
    );
}

/// AC-002: a failure is recorded with its reason and emits one update event; an
/// unchanged reason re-emits nothing and clearing removes it with one event.
#[test]
fn failure_snapshot_records_and_clears_with_change_events() {
    let _home = temp_home("auto-refresh-failures");
    reset_auto_refresh_for_tests();

    assert!(
        failure_snapshot().failures.is_empty(),
        "the reset seam must clear the process-memory failure store"
    );

    set_template_failure("t1", "boom", None);
    assert_eq!(
        failure_snapshot().failures,
        vec![TemplateAutoRefreshFailure {
            template_id: "t1".to_string(),
            reason: "boom".to_string(),
        }],
        "the failing template and its reason must be snapshotted"
    );
    assert_eq!(recorded_events(), 1, "recording a failure must emit one event");

    set_template_failure("t1", "boom", None);
    assert_eq!(
        recorded_events(),
        1,
        "an unchanged reason must not re-emit the update event"
    );

    clear_template_failure("t1", None);
    assert!(
        failure_snapshot().failures.is_empty(),
        "a successful sync clears the recorded failure"
    );
    assert_eq!(recorded_events(), 2, "clearing a failure must emit one event");

    clear_template_failure("t1", None);
    assert_eq!(
        recorded_events(),
        2,
        "clearing an absent failure must not emit an event"
    );
}

/// AC-002: the process scheduler is installed only through the `AppHandle`
/// setup path, which this harness cannot construct for the Wry runtime, so the
/// reachable scheduler seams are covered here: no scheduler runs, a re-arm
/// request is a safe no-op before installation, and the reset seam clears every
/// process-memory artifact including the shutdown flag.
#[tokio::test]
async fn reset_seam_clears_process_memory_and_rearm_is_safe_before_start() {
    let _home = temp_home("auto-refresh-scheduler-reset");
    reset_auto_refresh_for_tests();

    assert!(
        !scheduler_started(),
        "no process scheduler runs until the app setup installs it"
    );
    request_rearm();
    assert!(
        !scheduler_started(),
        "a re-arm request must not install a scheduler by itself"
    );

    set_template_failure("t1", "boom", None);
    assert_eq!(failure_snapshot().failures.len(), 1);
    assert_eq!(recorded_events(), 1);

    // `stop_scheduler` sets the shutdown flag even with no scheduler running;
    // the reset seam must clear it so a later batch still runs.
    stop_scheduler();
    reset_auto_refresh_for_tests();
    assert!(
        failure_snapshot().failures.is_empty(),
        "the reset seam must clear the failure store"
    );
    assert_eq!(
        recorded_events(),
        0,
        "the reset seam must clear the recorded update events"
    );

    let results = run_batch_with(vec!["x".to_string()], |_id| async {
        AutomaticOutcome::Skipped
    })
    .await;
    assert!(
        results.is_some(),
        "the reset seam must clear the shutdown flag so batches run again"
    );
}

/// AC-002: the process scheduler installs at startup exactly once and runs the
/// controlled startup batch exactly once. A second install is a no-op that keeps
/// the existing schedule, `request_rearm` never starts an immediate batch, an
/// interval of `0` installs nothing, and the reset seam removes the singleton so
/// the serialized sibling tests stay isolated.
///
/// The injected executor runs synchronously through
/// `install_scheduler_for_tests_with`, so the counters observe exactly how many
/// startup batches ran without touching the network or real time.
#[test]
fn scheduler_installs_at_startup_exactly_once() {
    let _home = temp_home("auto-refresh-start-once");
    reset_auto_refresh_for_tests();
    assert!(
        !scheduler_started(),
        "the reset seam must start from no installed scheduler"
    );

    // 1. A positive interval installs the schedule and runs the startup batch
    //    exactly once; a later re-arm request does not run another batch.
    let first_runs = Arc::new(AtomicUsize::new(0));
    let first_sink = first_runs.clone();
    let installed = install_scheduler_for_tests_with(60, move || {
        first_sink.fetch_add(1, Ordering::SeqCst);
    });
    assert!(installed, "the first positive-interval install must succeed");
    assert!(scheduler_started(), "the scheduler must be installed");
    assert_eq!(
        first_runs.load(Ordering::SeqCst),
        1,
        "the startup batch must run exactly once"
    );

    request_rearm();
    assert_eq!(
        first_runs.load(Ordering::SeqCst),
        1,
        "a re-arm request must not run an immediate second batch"
    );

    // 2. A second install while started is a no-op: the existing schedule is
    //    kept and the replacement executor is never called.
    let second_runs = Arc::new(AtomicUsize::new(0));
    let second_sink = second_runs.clone();
    let reinstalled = install_scheduler_for_tests_with(60, move || {
        second_sink.fetch_add(1, Ordering::SeqCst);
    });
    assert!(!reinstalled, "a second install while started must return false");
    assert!(scheduler_started(), "the existing scheduler must be kept");
    assert_eq!(
        first_runs.load(Ordering::SeqCst),
        1,
        "the kept schedule's startup batch must not run again"
    );
    assert_eq!(
        second_runs.load(Ordering::SeqCst),
        0,
        "the replacement executor must never run"
    );

    // 3. Interval 0 installs nothing and never runs the executor.
    reset_auto_refresh_for_tests();
    assert!(
        !scheduler_started(),
        "the reset seam must remove the installed scheduler"
    );
    let zero_runs = Arc::new(AtomicUsize::new(0));
    let zero_sink = zero_runs.clone();
    let installed_zero = install_scheduler_for_tests_with(0, move || {
        zero_sink.fetch_add(1, Ordering::SeqCst);
    });
    assert!(!installed_zero, "interval 0 must install nothing");
    assert!(
        !scheduler_started(),
        "interval 0 must leave no scheduler installed"
    );
    assert_eq!(
        zero_runs.load(Ordering::SeqCst),
        0,
        "interval 0 must never run the executor"
    );

    // 4. Leave process memory clean for the other serialized tests.
    reset_auto_refresh_for_tests();
    assert!(
        !scheduler_started(),
        "the trailing reset must leave no scheduler installed"
    );
}
