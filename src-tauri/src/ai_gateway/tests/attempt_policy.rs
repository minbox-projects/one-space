//! Behavior tests for the shared per-request decision core (REQ-007 / AC-007).
//!
//! Every test drives the delivered `pub(in crate::ai_gateway)` surface of
//! [`crate::ai_gateway::attempt_policy`] directly and derives its expected
//! values independently, so the decision logic stays correct for both the
//! buffered and the streaming transport. Fixtures are built in memory; no
//! internal collaborator is mocked.

use crate::ai_gateway::attempt_policy::{
    all_unavailable_message, clear_ttl_expired_key_runtime_state, excluded_key_summary,
    key_failure_kind, record_provider_failure, record_provider_failure_if_absent,
    select_attempt_key, settle_key_failure, settle_key_probe_failure, settle_key_probe_success,
    RequestHealth, RetryCandidate,
};
use crate::ai_gateway::selection::{FailureClass, MappingTarget};
use crate::ai_gateway::types_config::{
    GatewayConfig, GatewayUpstreamProvider, KeyFailureKind, ModelMapping, UpstreamKey,
    KEY_PROBE_COOLDOWN_SECS, KEY_QUOTA_MARK_TTL_SECS,
};
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::Instant;

fn upstream_key(id: &str, enabled: bool) -> UpstreamKey {
    UpstreamKey {
        id: id.to_string(),
        name: id.to_string(),
        value: format!("value-{id}"),
        enabled,
        ..UpstreamKey::default()
    }
}

fn marked_key(id: &str, kind: KeyFailureKind, marked_at: u64) -> UpstreamKey {
    UpstreamKey {
        auto_marked: true,
        failure_kind: Some(kind),
        marked_at: Some(marked_at),
        reason: Some(format!("marked-{id}")),
        ..upstream_key(id, true)
    }
}

fn provider_with_keys(id: &str, keys: Vec<UpstreamKey>) -> GatewayUpstreamProvider {
    GatewayUpstreamProvider {
        id: id.to_string(),
        name: format!("Provider {id}"),
        base_url: "https://api.example.com/v1".to_string(),
        keys,
        ..GatewayUpstreamProvider::default()
    }
}

fn provider_with_mapping(
    id: &str,
    local_model: &str,
    upstream_model: &str,
) -> GatewayUpstreamProvider {
    GatewayUpstreamProvider {
        id: id.to_string(),
        name: format!("Provider {id}"),
        base_url: "https://api.example.com/v1".to_string(),
        mappings: vec![ModelMapping {
            local_model: local_model.to_string(),
            upstream_model: upstream_model.to_string(),
            ..ModelMapping::default()
        }],
        ..GatewayUpstreamProvider::default()
    }
}

/// Persist one provider and read it back through the encrypted write/read path,
/// so every settlement test observes both the in-memory and the persisted state.
fn seed_provider(provider: GatewayUpstreamProvider) -> GatewayUpstreamProvider {
    let mut config = GatewayConfig::default();
    config.providers.push(provider);
    crate::ai_gateway::storage::write_config(&config).expect("seed the provider config");
    crate::ai_gateway::storage::read_config()
        .expect("read the seeded config")
        .providers
        .remove(0)
}

fn load_mapping(provider_id: &str, upstream_model: &str) -> ModelMapping {
    crate::ai_gateway::storage::read_config()
        .expect("read config")
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .expect("the provider must persist")
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == upstream_model)
        .cloned()
        .expect("the mapping must persist")
}

fn retry_candidate(ready_at: Option<Instant>) -> RetryCandidate {
    RetryCandidate {
        provider: GatewayUpstreamProvider {
            id: "p-retry".to_string(),
            ..GatewayUpstreamProvider::default()
        },
        model: "m".to_string(),
        attempts: 1,
        ready_at,
        attempted_keys: HashSet::new(),
    }
}

// ---------------------------------------------------------------------------
// Retry scheduling and the 120-second wait budget
// ---------------------------------------------------------------------------

#[test]
fn retry_candidate_earliest_orders_by_deadline_then_initial_index() {
    let now = Instant::now();
    let candidates = vec![
        retry_candidate(Some(now + Duration::from_secs(5))),
        // A header delay beyond the clock range is represented as `None` and
        // must be treated as the latest, not the earliest, deadline.
        retry_candidate(None),
        retry_candidate(Some(now + Duration::from_secs(1))),
        retry_candidate(Some(now + Duration::from_secs(1))),
    ];
    assert_eq!(
        RetryCandidate::earliest(&candidates),
        Some(2),
        "earliest deadline wins, ties keep the initial candidate order"
    );
    assert_eq!(
        RetryCandidate::earliest(&[]),
        None,
        "an empty retry queue has no earliest candidate"
    );

    let only_none = vec![retry_candidate(None)];
    assert_eq!(RetryCandidate::earliest(&only_none), Some(0));
}

#[tokio::test]
async fn retry_candidate_next_ready_debits_the_wait_budget_by_the_actual_wait() {
    // A candidate already past its deadline is selected without sleeping and
    // spends no budget.
    let ready = vec![retry_candidate(Some(
        Instant::now() - Duration::from_millis(1),
    ))];
    let mut budget = Duration::from_secs(120);
    assert_eq!(
        RetryCandidate::next_ready(&ready, &mut budget).await,
        Some(0)
    );
    assert_eq!(
        budget,
        Duration::from_secs(120),
        "a due candidate must not spend the wait budget"
    );

    // A deadline inside the budget sleeps until it is ready and debits only the
    // elapsed wait, not the whole budget.
    let soon = vec![retry_candidate(Some(
        Instant::now() + Duration::from_millis(20),
    ))];
    let mut budget = Duration::from_secs(5);
    assert_eq!(
        RetryCandidate::next_ready(&soon, &mut budget).await,
        Some(0)
    );
    assert!(budget < Duration::from_secs(5), "the sleep must debit the budget");
    assert!(
        budget >= Duration::from_secs(4),
        "only the elapsed wait is debited: {budget:?}"
    );

    // A deadline beyond the remaining budget is refused and leaves the budget
    // untouched so the caller can stop the retry loop.
    let later = vec![retry_candidate(Some(
        Instant::now() + Duration::from_secs(60),
    ))];
    let mut budget = Duration::from_secs(1);
    assert_eq!(
        RetryCandidate::next_ready(&later, &mut budget).await,
        None
    );
    assert_eq!(
        budget,
        Duration::from_secs(1),
        "a refused candidate must not spend the budget"
    );
}

// ---------------------------------------------------------------------------
// Ordered key selection, runtime marks, TTL recovery and probes
// ---------------------------------------------------------------------------

#[test]
fn select_attempt_key_prefers_first_usable_and_skips_tried_marked_or_disabled() {
    let now = 10_000u64;
    let provider = provider_with_keys(
        "p",
        vec![
            upstream_key("k1", true),
            marked_key("k2-auth", KeyFailureKind::Authentication, now - 1),
            marked_key("k3-quota-fresh", KeyFailureKind::Quota, now - 10),
            upstream_key("k4-disabled", false),
            upstream_key("k5", true),
        ],
    );

    let selected = select_attempt_key(&provider, now, false, &HashSet::new())
        .expect("the first enabled unmarked key must be selected");
    assert_eq!(selected.id, "k1");
    assert!(!selected.probe);
    assert!(!selected.ttl_cleared);

    let attempted: HashSet<String> = ["k1".to_string()].into_iter().collect();
    let selected = select_attempt_key(&provider, now, false, &attempted)
        .expect("the next usable key after the attempted one");
    assert_eq!(
        selected.id, "k5",
        "a tried key, an auth mark, a fresh quota mark and a disabled key are all skipped"
    );

    assert!(
        select_attempt_key(&provider_with_keys("empty", vec![]), now, false, &HashSet::new())
            .is_none(),
        "an empty pool has no usable key and no probe candidate"
    );
}

#[test]
fn select_attempt_key_recovers_and_flags_a_ttl_expired_quota_mark() {
    let now = 50_000u64;

    let expired = provider_with_keys(
        "p",
        vec![marked_key(
            "k-expired",
            KeyFailureKind::Quota,
            now - KEY_QUOTA_MARK_TTL_SECS,
        )],
    );
    let selected = select_attempt_key(&expired, now, false, &HashSet::new())
        .expect("a quota mark at the TTL boundary is usable again");
    assert_eq!(selected.id, "k-expired");
    assert!(!selected.probe);
    assert!(
        selected.ttl_cleared,
        "recovering a TTL-expired mark must request a persisted clear"
    );

    // One second below the TTL the key is not normally selectable; because the
    // probe cooldown is shorter than the TTL it is offered only as a probe.
    let fresh = provider_with_keys(
        "p",
        vec![marked_key(
            "k-fresh",
            KeyFailureKind::Quota,
            now - (KEY_QUOTA_MARK_TTL_SECS - 1),
        )],
    );
    let selected = select_attempt_key(&fresh, now, false, &HashSet::new())
        .expect("a still-fresh quota mark is only probe-eligible");
    assert!(
        selected.probe,
        "below the TTL the mark must not be treated as normally recovered"
    );
    assert!(!selected.ttl_cleared);
}

#[test]
fn select_attempt_key_allows_one_probe_only_when_no_usable_key_remains() {
    let now = 50_000u64;
    let provider = provider_with_keys(
        "p",
        vec![marked_key(
            "k-quota",
            KeyFailureKind::Quota,
            now - KEY_PROBE_COOLDOWN_SECS,
        )],
    );

    let selected = select_attempt_key(&provider, now, false, &HashSet::new())
        .expect("the exhausted pool offers one half-open probe");
    assert!(selected.probe);
    assert_eq!(selected.id, "k-quota");

    assert!(
        select_attempt_key(&provider, now, true, &HashSet::new()).is_none(),
        "the request's single probe budget is already spent"
    );

    // Auth-marked keys are manual-only: they are never a probe candidate.
    let auth_only = provider_with_keys(
        "p-auth",
        vec![marked_key(
            "k-auth",
            KeyFailureKind::Authentication,
            now - 1_000,
        )],
    );
    assert!(select_attempt_key(&auth_only, now, false, &HashSet::new()).is_none());
}

#[test]
fn clear_ttl_expired_key_runtime_state_clears_memory_and_persisted_mark() {
    super::with_temp_home("attempt-policy-ttl-clear", |_home| {
        let now = 70_000u64;
        let mut provider = seed_provider(provider_with_keys(
            "p",
            vec![marked_key(
                "k-expired",
                KeyFailureKind::Quota,
                now - KEY_QUOTA_MARK_TTL_SECS,
            )],
        ));
        let selected = select_attempt_key(&provider, now, false, &HashSet::new())
            .expect("the TTL-expired quota mark is normally selectable");
        assert!(selected.ttl_cleared);
        clear_ttl_expired_key_runtime_state(&mut provider, &selected);

        let in_memory = &provider.keys[0];
        assert!(!in_memory.auto_marked);
        assert_eq!(in_memory.failure_kind, None);
        assert_eq!(in_memory.marked_at, None);
        assert_eq!(in_memory.reason, None);

        let persisted = crate::ai_gateway::storage::read_config()
            .expect("read config")
            .providers
            .remove(0);
        assert!(
            !persisted.keys[0].auto_marked,
            "the persisted quota mark must be cleared"
        );
        assert_eq!(persisted.keys[0].failure_kind, None);
    });
}

// ---------------------------------------------------------------------------
// Key-domain classification and settlement
// ---------------------------------------------------------------------------

#[test]
fn key_failure_kind_classifies_only_credential_and_quota_scoped_statuses() {
    assert_eq!(
        key_failure_kind(401, None),
        Some(KeyFailureKind::Authentication)
    );
    assert_eq!(
        key_failure_kind(403, Some("Invalid API key provided")),
        Some(KeyFailureKind::Authentication)
    );
    assert_eq!(
        key_failure_kind(403, None),
        None,
        "a bare 403 has no credential text and is not key-scoped"
    );
    assert_eq!(
        key_failure_kind(403, Some("Forbidden by region")),
        None,
        "a non-credential 403 is not key-scoped"
    );
    assert_eq!(
        key_failure_kind(400, Some("quota exceeded")),
        Some(KeyFailureKind::Quota)
    );
    assert_eq!(
        key_failure_kind(402, Some("insufficient balance")),
        Some(KeyFailureKind::Quota)
    );
    assert_eq!(
        key_failure_kind(429, Some("usage limit reached")),
        Some(KeyFailureKind::Quota)
    );
    assert_eq!(
        key_failure_kind(429, None),
        None,
        "a bare rate-limit 429 rotates without a persisted mark"
    );
    assert_eq!(key_failure_kind(429, Some("rate limit exceeded")), None);
    assert_eq!(key_failure_kind(500, Some("quota exceeded")), None);
    assert_eq!(key_failure_kind(404, None), None);
}

#[test]
fn settle_key_failure_marks_the_selected_key_in_memory_and_on_disk() {
    super::with_temp_home("attempt-policy-settle-failure", |_home| {
        let mut provider = seed_provider(provider_with_keys("p", vec![upstream_key("k1", true)]));
        let selected = select_attempt_key(&provider, 100, false, &HashSet::new())
            .expect("the unmarked key is selectable");
        settle_key_failure(
            &mut provider,
            &selected,
            KeyFailureKind::Authentication,
            "HTTP 401",
            500,
        );

        let in_memory = &provider.keys[0];
        assert!(in_memory.auto_marked);
        assert_eq!(in_memory.failure_kind, Some(KeyFailureKind::Authentication));
        assert_eq!(in_memory.marked_at, Some(500));
        assert_eq!(in_memory.reason.as_deref(), Some("HTTP 401"));
        assert!(in_memory.enabled, "the user's enabled intent is untouched");

        let persisted = crate::ai_gateway::storage::read_config()
            .expect("read config")
            .providers
            .remove(0);
        assert!(persisted.keys[0].auto_marked);
        assert_eq!(
            persisted.keys[0].failure_kind,
            Some(KeyFailureKind::Authentication)
        );
        assert_eq!(persisted.keys[0].marked_at, Some(500));
        assert_eq!(persisted.keys[0].reason.as_deref(), Some("HTTP 401"));
    });
}

#[test]
fn settle_key_probe_failure_rearms_the_cooldown_and_switches_kind_on_auth() {
    super::with_temp_home("attempt-policy-settle-probe-failure", |_home| {
        let now = 90_000u64;
        let mut provider = seed_provider(provider_with_keys(
            "p",
            vec![marked_key(
                "kp",
                KeyFailureKind::Quota,
                now - KEY_PROBE_COOLDOWN_SECS,
            )],
        ));
        let selected = select_attempt_key(&provider, now, false, &HashSet::new())
            .expect("the quota mark is probe-eligible");
        assert!(selected.probe);

        // `None` keeps the quota kind but moves the cooldown.
        settle_key_probe_failure(&mut provider, &selected, None, "still throttled", 95_000);
        let key = &provider.keys[0];
        assert!(key.auto_marked);
        assert_eq!(key.failure_kind, Some(KeyFailureKind::Quota));
        assert_eq!(key.marked_at, Some(95_000));
        assert_eq!(key.reason.as_deref(), Some("still throttled"));

        // `Some(Authentication)` switches the mark kind.
        settle_key_probe_failure(
            &mut provider,
            &selected,
            Some(KeyFailureKind::Authentication),
            "HTTP 401",
            96_000,
        );
        let key = &provider.keys[0];
        assert_eq!(key.failure_kind, Some(KeyFailureKind::Authentication));
        assert_eq!(key.marked_at, Some(96_000));
    });
}

#[test]
fn settle_key_probe_success_clears_the_runtime_state() {
    super::with_temp_home("attempt-policy-settle-probe-success", |_home| {
        let now = 90_000u64;
        let mut provider = seed_provider(provider_with_keys(
            "p",
            vec![marked_key(
                "kp",
                KeyFailureKind::Quota,
                now - KEY_PROBE_COOLDOWN_SECS,
            )],
        ));
        let selected = select_attempt_key(&provider, now, false, &HashSet::new())
            .expect("the quota mark is probe-eligible");
        assert!(selected.probe);
        settle_key_probe_success(&mut provider, &selected);

        let key = &provider.keys[0];
        assert!(!key.auto_marked);
        assert_eq!(key.failure_kind, None);
        assert_eq!(key.marked_at, None);
        assert_eq!(key.reason, None);

        let persisted = crate::ai_gateway::storage::read_config()
            .expect("read config")
            .providers
            .remove(0);
        assert!(!persisted.keys[0].auto_marked);
        assert_eq!(persisted.keys[0].failure_kind, None);
    });
}

// ---------------------------------------------------------------------------
// Request-scoped mapping health
// ---------------------------------------------------------------------------

#[test]
fn request_health_applies_threshold_and_success_reset_on_trimmed_keys() {
    super::with_temp_home("attempt-policy-health-threshold", |_home| {
        // The stored row carries padded model names; matching must trim both
        // the stored key and the requested target.
        seed_provider(provider_with_mapping("p1", "  a-local  ", "gpt-5.4"));
        let target = MappingTarget::new("p1", "a-local", "gpt-5.4");

        // Two separate requests below the threshold count, but do not disable.
        for expected in [1u32, 2] {
            let mut health = RequestHealth::new(false);
            health.record_failure(&target, FailureClass::Retryable, "boom", false);
            health.apply();
            let row = load_mapping("p1", "gpt-5.4");
            assert_eq!(row.consecutive_failures, expected);
            assert!(!row.auto_disabled);
        }

        // A successful request resets the counter and the last-error stamp.
        let mut health = RequestHealth::new(false);
        health.record_success(&target);
        health.apply();
        let row = load_mapping("p1", "gpt-5.4");
        assert_eq!(row.consecutive_failures, 0);
        assert_eq!(row.last_error_at, None);

        // The third consecutive retryable failure reaches the threshold.
        for expected in [1u32, 2, 3] {
            let mut health = RequestHealth::new(false);
            health.record_failure(&target, FailureClass::Retryable, "boom-3", false);
            health.apply();
            let row = load_mapping("p1", "gpt-5.4");
            assert_eq!(row.consecutive_failures, expected);
        }
        let row = load_mapping("p1", "gpt-5.4");
        assert!(row.auto_disabled);
        assert_eq!(row.disabled_reason.as_deref(), Some("boom-3"));
        assert!(row.disabled_at.is_some());
    });
}

#[test]
fn request_health_suppresses_retryable_transport_failures_under_the_resume_grace() {
    super::with_temp_home("attempt-policy-health-suppressed", |_home| {
        seed_provider(provider_with_mapping("p1", "a-local", "gpt-5.4"));
        let target = MappingTarget::new("p1", "a-local", "gpt-5.4");

        // A transport failure under the grace is neither counted nor stamped.
        let mut health = RequestHealth::new(true);
        health.record_failure(&target, FailureClass::Retryable, "connect reset", true);
        health.apply();
        let row = load_mapping("p1", "gpt-5.4");
        assert_eq!(row.consecutive_failures, 0);
        assert_eq!(row.last_error_at, None);

        // The same grace still counts a non-transport failure.
        let mut health = RequestHealth::new(true);
        health.record_failure(&target, FailureClass::Retryable, "HTTP 500", false);
        health.apply();
        let row = load_mapping("p1", "gpt-5.4");
        assert_eq!(row.consecutive_failures, 1);
        assert!(row.last_error_at.is_some());

        // Without the grace a transport failure counts normally.
        let mut health = RequestHealth::new(false);
        health.record_failure(&target, FailureClass::Retryable, "connect reset", true);
        health.apply();
        let row = load_mapping("p1", "gpt-5.4");
        assert_eq!(row.consecutive_failures, 2);
    });
}

// ---------------------------------------------------------------------------
// All-unavailable composition
// ---------------------------------------------------------------------------

#[test]
fn excluded_key_summary_lists_categories_in_fixed_order() {
    let provider = GatewayUpstreamProvider {
        id: "p".to_string(),
        keys: vec![
            marked_key("a1", KeyFailureKind::Authentication, 10),
            marked_key("a2", KeyFailureKind::Authentication, 11),
            marked_key("q1", KeyFailureKind::Quota, 12),
            upstream_key("d1", false),
            upstream_key("d2", false),
        ],
        ..GatewayUpstreamProvider::default()
    };
    assert_eq!(
        excluded_key_summary(&provider, true),
        "no usable upstream key (2 authentication failed, 1 quota exhausted, 2 user disabled)"
    );

    // When the whole request has no enabled key the per-key counts collapse to
    // the request-wide wording.
    assert_eq!(
        excluded_key_summary(&provider, false),
        "no usable upstream key (no enabled key)"
    );

    // A pool with no marks and no disabled key also reports the generic wording.
    let clean = GatewayUpstreamProvider {
        id: "c".to_string(),
        keys: vec![upstream_key("k", true)],
        ..GatewayUpstreamProvider::default()
    };
    assert_eq!(
        excluded_key_summary(&clean, true),
        "no usable upstream key (no enabled key)"
    );
}

#[test]
fn all_unavailable_message_composes_summary_and_scoped_hints() {
    assert_eq!(
        all_unavailable_message(&[]),
        "all providers unavailable: every candidate failed"
    );

    let failures = vec![
        ("Provider A".to_string(), "HTTP 500".to_string()),
        ("Provider B".to_string(), "HTTP 502".to_string()),
    ];
    assert_eq!(
        all_unavailable_message(&failures),
        "all providers unavailable: Provider A: HTTP 500; Provider B: HTTP 502"
    );

    // The quota hint appears only when every reported provider qualified.
    let quota = vec![
        ("A".to_string(), "HTTP 429: Quota Exceeded".to_string()),
        ("B".to_string(), "Quota Exceeded".to_string()),
    ];
    let message = all_unavailable_message(&quota);
    assert!(message.contains("所有服务商额度均已耗尽"), "{message}");

    let mixed = vec![
        ("A".to_string(), "HTTP 429: Quota Exceeded".to_string()),
        ("B".to_string(), "HTTP 500".to_string()),
    ];
    assert!(!all_unavailable_message(&mixed).contains("所有服务商额度均已耗尽"));

    // The authentication re-enable hint is independent of the quota hint.
    let auth = vec![(
        "A".to_string(),
        "no usable upstream key (1 authentication failed)".to_string(),
    )];
    let message = all_unavailable_message(&auth);
    assert!(
        message.contains("re-enable authentication-failed keys manually in the AI Gateway"),
        "{message}"
    );
}

#[test]
fn provider_failure_records_keep_one_entry_per_provider_with_the_latest_reason() {
    let mut failures: Vec<(String, String)> = Vec::new();
    record_provider_failure(&mut failures, "A", "first".to_string());
    record_provider_failure(&mut failures, "B", "b".to_string());
    record_provider_failure(&mut failures, "A", "second".to_string());
    assert_eq!(
        failures,
        vec![
            ("A".to_string(), "second".to_string()),
            ("B".to_string(), "b".to_string()),
        ],
        "one entry per provider, preserving first-seen order with the latest reason"
    );

    // An already reported provider keeps its real upstream reason instead of a
    // generic no-usable-key message; a new provider is appended.
    record_provider_failure_if_absent(&mut failures, "A", "no usable upstream key");
    assert_eq!(failures[0].1, "second");
    record_provider_failure_if_absent(&mut failures, "C", "no usable upstream key");
    assert_eq!(failures.len(), 3);
    assert_eq!(
        failures[2],
        ("C".to_string(), "no usable upstream key".to_string())
    );
}
