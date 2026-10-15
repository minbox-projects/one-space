//! Step 1 behavior slice for `20261015-deepseek-cache-diagnostics`
//! (REQ-001 / AC-001, REQ-002 / AC-002).
//!
//! Two boundaries are covered here:
//!
//! 1. Native DeepSeek cache accounting. `prompt_cache_hit_tokens` /
//!    `prompt_cache_miss_tokens` are not recognized by the current parser, so
//!    the native-only 100/98/2 fixture produces the old numeric result
//!    (ordinary input 100, cache read 0, 0% aggregate, an input-token-only
//!    charge) instead of the correct exclusive decomposition (ordinary 2,
//!    cache read 98, a 98% token-weighted aggregate and a charge that applies
//!    the cache-read tier - not the input tier - to the cached tokens).
//!
//!    The same exclusive result must hold for the nested-only shape (already
//!    understood today) and for a consistent native+nested dual shape, and it
//!    must survive both a buffered JSON response and a fragmented SSE stream
//!    driven by a real mock upstream through the loopback gateway.
//!
//! 2. The existing Chat forwarding contract. Model-only rewriting, native
//!    message/tool/reasoning/isolation values, session headers and the
//!    local-vs-upstream authentication split are characterized against the
//!    current correct behavior. These tests are expected to pass before and
//!    after implementation; they never manufacture a RED.
//!
//! Deliberately out of scope (deferred to the post-GREEN validation matrix):
//! session affinity, stream failure semantics and downstream cancellation.
//! This slice does not edit product code and does not change shared fixtures.

use crate::ai_gateway::runtime_http::{read_http_request, start_server, stop_server};
use crate::ai_gateway::storage::write_config;
use crate::ai_gateway::{canonical_usage_from_value, SseUsageAccumulator, TimeRange, UsageResult};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use super::{
    call_gateway, config_with_key, default_usage_store, free_port, mapping, priced,
    spawn_json_sequence_mock, spawn_mock_upstream, temp_home, tokens, upstream_provider,
    wait_for_usage_logs, MockReply,
};

// ---------------------------------------------------------------------------
// Section A: canonical normalization (pure parser boundary)
// ---------------------------------------------------------------------------

/// AC-001 / REQ-001: a native-only usage object is an exclusive decomposition.
/// `prompt_tokens` is the inclusive input; the native hit is the cached subset,
/// so ordinary input is the checked remainder and the cache read is counted
/// once. This is the slice's primary RED: today the native fields are ignored
/// and the row is logged as ordinary 100 / cache read 0.
#[test]
fn native_only_usage_is_exclusive_and_not_double_counted() {
    let usage = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_cache_hit_tokens": 98,
        "prompt_cache_miss_tokens": 2,
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(
        canonical.valid,
        "a consistent native 100/98/2 shape is a valid decomposition"
    );
    assert_eq!(canonical.tokens.input_tokens, 2, "ordinary = 100 - 98");
    assert_eq!(canonical.tokens.cache_read_tokens, 98);
    assert_eq!(canonical.tokens.cache_write_tokens, 0);
    assert_eq!(canonical.tokens.output_tokens, 7);
    assert_eq!(
        canonical.tokens.total(),
        107,
        "each tier is counted exactly once"
    );
    assert_eq!(
        canonical.reported_output_tokens,
        Some(7),
        "the explicit output count is independently valid"
    );
}

/// Characterization: the nested-only shape already produces the exclusive
/// result; adding native support must not change it.
#[test]
fn nested_only_usage_stays_exclusive() {
    let usage = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_tokens_details": { "cached_tokens": 98 },
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(canonical.valid);
    assert_eq!(canonical.tokens, tokens(2, 98, 0, 7));
}

/// AC-001 / REQ-001: equal native and nested hits are consistent and the hit is
/// counted once, never doubled to 196.
#[test]
fn consistent_native_and_nested_hits_count_once() {
    let usage = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_tokens_details": { "cached_tokens": 98 },
        "prompt_cache_hit_tokens": 98,
        "prompt_cache_miss_tokens": 2,
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(canonical.valid, "equal native and nested hits are consistent");
    assert_eq!(canonical.tokens, tokens(2, 98, 0, 7));
    assert_eq!(
        canonical.tokens.cache_read_tokens, 98,
        "counted once (98), never summed with the nested copy (196)"
    );
}

/// AC-001 / REQ-001: unequal native and nested hits are a contradictory shape
/// and fall back conservatively - never a silently merged exclusive split.
#[test]
fn conflicting_native_and_nested_hits_are_invalid() {
    let usage = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_tokens_details": { "cached_tokens": 50 },
        "prompt_cache_hit_tokens": 98,
        "prompt_cache_miss_tokens": 2,
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(
        !canonical.valid,
        "native 98 vs nested 50 must be classified invalid"
    );
    assert_eq!(
        canonical.tokens,
        tokens(100, 0, 0, 7),
        "the conservative fallback bills the reported input once with no cache tiers"
    );
    assert_eq!(canonical.tokens.total(), 107, "no double count, no negative");
}

/// AC-001 boundary: a native hit greater than the reported total input is
/// invalid, never clamped into a valid-looking percentage.
#[test]
fn native_hit_greater_than_reported_input_is_invalid() {
    let usage = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_cache_hit_tokens": 150,
        "prompt_cache_miss_tokens": 0,
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(!canonical.valid, "150 > 100 is an impossible decomposition");
    assert_eq!(canonical.tokens, tokens(100, 0, 0, 7));
    assert_eq!(
        canonical.tokens.cache_read_tokens, 0,
        "an impossible hit is never clamped to a valid cache read"
    );
}

/// AC-001 boundary: native hit values that are not non-negative integers are
/// invalid rather than coerced (the generic field coercion must not be reused
/// for these new fields).
#[test]
fn non_integer_native_hit_values_are_invalid_not_coerced() {
    for native in [json!(-1), json!(98.5), json!("abc"), json!(true)] {
        let usage = json!({
            "prompt_tokens": 100,
            "completion_tokens": 7,
            "prompt_cache_hit_tokens": native,
            "prompt_cache_miss_tokens": 2,
        });
        let canonical = canonical_usage_from_value(&usage);
        assert!(
            !canonical.valid,
            "native hit {native} must be invalid, not coerced"
        );
        assert_eq!(
            canonical.tokens,
            tokens(100, 0, 0, 7),
            "native hit {native} falls back to the reported input with no cache tiers"
        );
    }
}

/// REQ-001: the newly recognized native fields accept the existing supported
/// integer-string representation, exactly like the other token fields.
#[test]
fn integer_string_native_fields_are_accepted() {
    let usage = json!({
        "prompt_tokens": "100",
        "completion_tokens": 7,
        "prompt_cache_hit_tokens": "98",
        "prompt_cache_miss_tokens": "2",
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(canonical.valid);
    assert_eq!(canonical.tokens, tokens(2, 98, 0, 7));
}

/// AC-001 boundary: an explicit zero is present while `null` is absent. The
/// difference is observable only when the nested read disagrees: a present zero
/// conflicts, an absent null does not.
#[test]
fn explicit_zero_native_hit_is_present_while_null_is_absent() {
    // null native fields are absent, so the nested read is the only source.
    let null_native = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_tokens_details": { "cached_tokens": 98 },
        "prompt_cache_hit_tokens": null,
        "prompt_cache_miss_tokens": null,
    });
    let canonical = canonical_usage_from_value(&null_native);
    assert!(canonical.valid);
    assert_eq!(canonical.tokens, tokens(2, 98, 0, 7));

    // A present explicit zero is a real claim and conflicts with a different
    // nested read.
    let zero_native = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_tokens_details": { "cached_tokens": 98 },
        "prompt_cache_hit_tokens": 0,
        "prompt_cache_miss_tokens": 100,
    });
    let canonical = canonical_usage_from_value(&zero_native);
    assert!(
        !canonical.valid,
        "a present zero (0) conflicts with a non-zero nested read (98)"
    );
    assert_eq!(canonical.tokens, tokens(100, 0, 0, 7));

    // Equal explicit zero on both sides is consistent and counts once.
    let zero_both = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_tokens_details": { "cached_tokens": 0 },
        "prompt_cache_hit_tokens": 0,
        "prompt_cache_miss_tokens": 100,
    });
    let canonical = canonical_usage_from_value(&zero_both);
    assert!(
        canonical.valid,
        "equal explicit zero is consistent and stays valid"
    );
    assert_eq!(canonical.tokens, tokens(100, 0, 0, 7));

    // Zero with no nested read is a valid uncached row, not a fallback.
    let zero_only = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_cache_hit_tokens": 0,
        "prompt_cache_miss_tokens": 100,
    });
    let canonical = canonical_usage_from_value(&zero_only);
    assert!(canonical.valid);
    assert_eq!(canonical.tokens, tokens(100, 0, 0, 7));
}

/// AC-001 boundary: a native hit equal to the reported input is a valid 100%
/// row (ordinary input 0) when the positive denominator holds.
#[test]
fn native_hit_equal_to_input_is_a_valid_full_cache_row() {
    let usage = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_cache_hit_tokens": 100,
        "prompt_cache_miss_tokens": 0,
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(canonical.valid);
    assert_eq!(canonical.tokens, tokens(0, 100, 0, 7));
}

/// AC-001 boundary: a present native miss that disagrees with
/// `reported - hit` is invalid; the parser never silently prefers the larger
/// reported hit.
#[test]
fn native_miss_conflicting_with_total_minus_hit_is_invalid() {
    let usage = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_cache_hit_tokens": 98,
        "prompt_cache_miss_tokens": 5,
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(
        !canonical.valid,
        "miss 5 disagrees with reported(100) - hit(98) = 2"
    );
    assert_eq!(
        canonical.tokens,
        tokens(100, 0, 0, 7),
        "an inconsistent miss never selects a larger cached hit"
    );
}

/// REQ-001: a miss-only field must not invent a cached count (it must never
/// become `reported - miss` = 98). With no selected hit the required miss equals
/// the full reported input, so `miss = 2 != reported(100) - hit(0) = 100` is an
/// invalid decomposition, not a cold-cache observation.
#[test]
fn miss_only_native_field_does_not_invent_a_cached_count() {
    let usage = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_cache_miss_tokens": 2,
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(
        !canonical.valid,
        "miss 2 disagrees with reported(100) - selected cache hit(0) = 100"
    );
    assert_eq!(
        canonical.tokens,
        tokens(100, 0, 0, 7),
        "a hit-less miss count leaves the reported input ordinary"
    );
    assert_eq!(
        canonical.tokens.cache_read_tokens, 0,
        "no cache read is derived from the miss count"
    );
    assert_eq!(
        canonical.tokens.total(),
        107,
        "the miss count is never added as a second input tier"
    );

    // Valid contrast: with no hit the selected cache hit is 0, so a miss equal
    // to the reported input is consistent and stays a valid uncached row.
    let consistent_miss_only = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_cache_miss_tokens": 100,
    });
    let canonical = canonical_usage_from_value(&consistent_miss_only);
    assert!(
        canonical.valid,
        "miss equal to reported minus the selected hit(0) is consistent"
    );
    assert_eq!(
        canonical.tokens,
        tokens(100, 0, 0, 7),
        "a consistent miss-only row bills the reported input once, no cache tier"
    );
    assert_eq!(canonical.tokens.cache_read_tokens, 0);
}

/// REQ-001: native hits combined with the independent Anthropic
/// `cache_read_input_tokens` tier are a conflicting shape even without nested
/// details.
#[test]
fn native_hit_with_anthropic_independent_read_is_a_conflicting_shape() {
    let usage = json!({
        "prompt_tokens": 100,
        "completion_tokens": 7,
        "prompt_cache_hit_tokens": 98,
        "prompt_cache_miss_tokens": 2,
        "cache_read_input_tokens": 50,
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(
        !canonical.valid,
        "native hits must not merge with the independent Anthropic read tier"
    );
    assert_eq!(canonical.tokens, tokens(100, 0, 0, 7));
}

/// REQ-001: the existing inclusive-input precedence continues to hold for the
/// native shape - `input_tokens` wins over `prompt_tokens`.
#[test]
fn input_tokens_win_over_prompt_tokens_for_native_hits() {
    let usage = json!({
        "prompt_tokens": 999,
        "input_tokens": 100,
        "completion_tokens": 7,
        "prompt_cache_hit_tokens": 98,
        "prompt_cache_miss_tokens": 2,
    });
    let canonical = canonical_usage_from_value(&usage);
    assert!(canonical.valid);
    assert_eq!(canonical.tokens, tokens(2, 98, 0, 7));
    assert_eq!(
        canonical.tokens.input_tokens + canonical.tokens.cache_read_tokens,
        100,
        "the inclusive total is the selected input_tokens"
    );
}

/// REQ-001 / AC-001: output validity is independent of cache validity. A valid
/// explicit output count survives an invalid cache shape, and a missing output
/// stays unknown even when native cache accounting is valid.
#[test]
fn reported_output_tokens_are_independent_of_cache_validity() {
    let valid_cache_string_output = json!({
        "prompt_tokens": 100,
        "output_tokens": "7",
        "prompt_cache_hit_tokens": 98,
        "prompt_cache_miss_tokens": 2,
    });
    let canonical = canonical_usage_from_value(&valid_cache_string_output);
    assert!(canonical.valid);
    assert_eq!(canonical.tokens.output_tokens, 7);
    assert_eq!(canonical.reported_output_tokens, Some(7));

    let invalid_cache_valid_output = json!({
        "prompt_tokens": 100,
        "output_tokens": 7,
        "prompt_cache_hit_tokens": -1,
        "prompt_cache_miss_tokens": 101,
    });
    let canonical = canonical_usage_from_value(&invalid_cache_valid_output);
    assert!(!canonical.valid, "the cache shape is invalid");
    assert_eq!(
        canonical.reported_output_tokens,
        Some(7),
        "cache invalidity must not erase the independently valid output count"
    );

    let valid_cache_missing_output = json!({
        "prompt_tokens": 100,
        "prompt_cache_hit_tokens": 98,
        "prompt_cache_miss_tokens": 2,
    });
    let canonical = canonical_usage_from_value(&valid_cache_missing_output);
    assert!(canonical.valid);
    assert_eq!(
        canonical.reported_output_tokens, None,
        "a missing output count stays unknown, not a fabricated zero"
    );
}

// ---------------------------------------------------------------------------
// Section B: fragmented SSE parsing
// ---------------------------------------------------------------------------

/// REQ-001: the same native normalization is applied to incrementally observed
/// SSE. The usage JSON spans arbitrary chunk boundaries; the last valid usage
/// object is normalized exactly like the buffered path.
#[test]
fn sse_accumulator_extracts_native_usage_across_fragmented_chunks() {
    let mut accumulator = SseUsageAccumulator::default();
    accumulator.feed(b"data: {\"id\":\"x\",\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n");
    accumulator.feed(b"data: {\"usage\":{\"prompt_tokens\":100,\"prompt_cache_hit_tokens\":98,");
    accumulator.feed(b"\"prompt_cache_miss_tokens\":2,\"completion_tokens\":7}}\n\n");
    accumulator.feed(b"data: [DONE]\n\n");

    let canonical = accumulator
        .canonical_usage()
        .expect("the final usage event must be recognized");
    assert!(canonical.valid);
    assert_eq!(canonical.tokens, tokens(2, 98, 0, 7));
    assert_eq!(accumulator.usage(), Some(tokens(2, 98, 0, 7)));
}

// ---------------------------------------------------------------------------
// Section C: persistence, aggregation and pricing through the real gateway
// ---------------------------------------------------------------------------

/// AC-001: native-only, nested-only and consistent dual-field usage for input
/// 100 / hit 98 / miss 2 all produce ordinary input 2, cache read 98 and the
/// same 98% token-weighted aggregate through the buffered gateway path, with no
/// duplicated input or cache charges and with the upstream body preserved
/// byte-for-byte.
#[tokio::test]
async fn buffered_native_nested_and_dual_usage_log_identical_exclusive_tiers() {
    let home = temp_home("deepseek-cache-buffered");
    let port = free_port().await;

    let native_only = json!({
        "id": "native",
        "choices": [{"message": {"role": "assistant", "content": "ok"}}],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 7,
            "prompt_cache_hit_tokens": 98,
            "prompt_cache_miss_tokens": 2,
        },
    });
    let nested_only = json!({
        "id": "nested",
        "choices": [{"message": {"role": "assistant", "content": "ok"}}],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 7,
            "prompt_tokens_details": { "cached_tokens": 98 },
        },
    });
    let dual = json!({
        "id": "dual",
        "choices": [{"message": {"role": "assistant", "content": "ok"}}],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 7,
            "prompt_tokens_details": { "cached_tokens": 98 },
            "prompt_cache_hit_tokens": 98,
            "prompt_cache_miss_tokens": 2,
        },
    });

    let (upstream_url, _attempts) = spawn_json_sequence_mock(vec![
        (200, native_only.clone()),
        (200, nested_only.clone()),
        (200, dual.clone()),
    ])
    .await;

    let mut config = config_with_key(port);
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    write_config(&config).unwrap();
    start_server(None).await.unwrap();

    for body in [&native_only, &nested_only, &dual] {
        let (status, _content_type, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(json!({"model": "local-a"})),
        )
        .await;
        assert_eq!(status, 200, "unexpected response: {text}");
        let expected_body = String::from_utf8(serde_json::to_vec(body).unwrap()).unwrap();
        assert_eq!(
            text, expected_body,
            "a served buffered response must pass through byte-for-byte"
        );
    }

    let records = wait_for_usage_logs(3).await;
    assert_eq!(records.len(), 3);
    // Independent constants: ordinary 2, cache read 98, output 7 over the
    // configured $1 / $0.50 / $2 / $4 per-million rates.
    let expected_amount = (2.0 * 1.0 + 98.0 * 0.5 + 0.0 * 2.0 + 7.0 * 4.0) / 1_000_000.0;
    for record in &records {
        assert_eq!(record.result, UsageResult::Success);
        assert_eq!(record.input_tokens, 2, "ordinary input is the exclusive remainder");
        assert_eq!(record.cache_read_tokens, 98, "the cache read is counted once");
        assert_eq!(record.cache_write_tokens, 0);
        assert_eq!(record.output_tokens, 7);
        assert_eq!(record.total_tokens, 107);
        assert!(
            (record.amount.expect("priced row") - expected_amount).abs() < 1e-12,
            "native/nested/dual must not duplicate the input or cache charge: {:?}",
            record.amount
        );
        let breakdown = record
            .cost_breakdown
            .expect("a priced row carries a cost breakdown");
        assert!(
            (breakdown.input_cost - 2.0 / 1_000_000.0).abs() < 1e-12,
            "input cost is charged on the 2 ordinary tokens"
        );
        assert!(
            (breakdown.cache_read_cost - 98.0 * 0.5 / 1_000_000.0).abs() < 1e-12,
            "cache-read cost is charged on the 98 cached tokens"
        );
        assert!((breakdown.cache_write_cost - 0.0).abs() < 1e-12);
        assert!((breakdown.output_cost - 7.0 * 4.0 / 1_000_000.0).abs() < 1e-12);
    }

    let stats = default_usage_store()
        .usage_stats(&TimeRange::default(), false)
        .unwrap();
    assert_eq!(stats.totals.cache_hit_tokens, 294, "three rows x 98 cached");
    assert_eq!(
        stats.totals.cache_eligible_tokens, 300,
        "three rows x (2 ordinary + 98 cached)"
    );
    let rate = stats
        .totals
        .cache_hit_rate_percent
        .expect("a positive denominator exists");
    assert!(
        (rate - 98.0).abs() < 1e-9,
        "the token-weighted aggregate is 98%: got {rate}"
    );

    stop_server().await.unwrap();
    drop(home);
}

/// Splits one usage event into several flushed fragments (the same shape the
/// original native case used), so the gateway observes a genuinely fragmented
/// SSE stream for any native/nested/dual usage object. Every value serializes
/// to ASCII, so byte slicing is also character slicing.
fn fragmented_usage_fragments(usage: Value) -> Vec<String> {
    let event = format!(
        "data: {}\n\n",
        serde_json::to_string(&json!({
            "id": "x",
            "choices": [{"delta": {}}],
            "usage": usage,
        }))
        .unwrap()
    );
    let bytes = event.as_bytes();
    let first = bytes.len() / 3;
    let second = (bytes.len() * 2) / 3;
    vec![
        "data: {\"id\":\"x\",\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n".to_string(),
        String::from_utf8(bytes[..first].to_vec()).expect("ascii fragment"),
        String::from_utf8(bytes[first..second].to_vec()).expect("ascii fragment"),
        String::from_utf8(bytes[second..].to_vec()).expect("ascii fragment"),
        "data: [DONE]\n\n".to_string(),
    ]
}

/// AC-001: one usage shape survives a genuinely fragmented SSE stream; the
/// forwarded bytes are identical to the upstream fragments, the persisted row
/// matches the buffered path (ordinary 2, cache read 98, output 7, a single
/// charge on the cache-read tier) and the token-weighted aggregate is 98%.
async fn fragmented_stream_shape_logs_once(home_name: &str, usage: Value) {
    let home = temp_home(home_name);
    let port = free_port().await;

    let fragments = fragmented_usage_fragments(usage);
    let expected_body: String = fragments.concat();
    let upstream_url = spawn_fragmented_sse_upstream(fragments).await;

    let mut config = config_with_key(port);
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    write_config(&config).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "stream": true})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");
    assert_eq!(
        text, expected_body,
        "fragmented forwarded bytes must be identical to the upstream stream"
    );

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.result, UsageResult::Success);
    assert_eq!(record.input_tokens, 2, "ordinary = 100 - 98");
    assert_eq!(record.cache_read_tokens, 98, "the cache read is counted once");
    assert_eq!(record.cache_write_tokens, 0);
    assert_eq!(record.output_tokens, 7);
    assert_eq!(record.total_tokens, 107);
    let expected_amount = (2.0 * 1.0 + 98.0 * 0.5 + 0.0 * 2.0 + 7.0 * 4.0) / 1_000_000.0;
    assert!(
        (record.amount.expect("priced row") - expected_amount).abs() < 1e-12,
        "the streamed hit is charged once on the cache-read tier, never duplicated"
    );

    let stats = default_usage_store()
        .usage_stats(&TimeRange::default(), false)
        .unwrap();
    assert_eq!(stats.totals.cache_hit_tokens, 98, "one row x 98 cached");
    assert_eq!(
        stats.totals.cache_eligible_tokens, 100,
        "one row x (2 ordinary + 98 cached)"
    );
    let rate = stats
        .totals
        .cache_hit_rate_percent
        .expect("a positive denominator exists");
    assert!(
        (rate - 98.0).abs() < 1e-9,
        "the token-weighted aggregate is 98%: got {rate}"
    );

    stop_server().await.unwrap();
    drop(home);
}

/// AC-001: the native-only shape, streamed and fragmented (the original case).
#[tokio::test]
async fn fragmented_stream_native_usage_is_logged_once_and_bytes_are_preserved() {
    fragmented_stream_shape_logs_once(
        "deepseek-cache-stream-native",
        json!({
            "prompt_tokens": 100,
            "completion_tokens": 7,
            "prompt_cache_hit_tokens": 98,
            "prompt_cache_miss_tokens": 2,
        }),
    )
    .await;
}

/// AC-001 / SPEC-004: the nested-only shape is the same exclusive decomposition,
/// aggregate and single charge through the same fragmented stream.
#[tokio::test]
async fn fragmented_stream_nested_only_usage_is_logged_once_and_bytes_are_preserved() {
    fragmented_stream_shape_logs_once(
        "deepseek-cache-stream-nested",
        json!({
            "prompt_tokens": 100,
            "completion_tokens": 7,
            "prompt_tokens_details": { "cached_tokens": 98 },
        }),
    )
    .await;
}

/// AC-001 / SPEC-004: a consistent dual native+nested shape counts the 98 hit
/// once through the fragmented stream, forwards the original bytes and produces
/// the same exclusive tiers, 98% aggregate and a single charge.
#[tokio::test]
async fn fragmented_stream_consistent_dual_usage_is_logged_once_and_bytes_are_preserved() {
    fragmented_stream_shape_logs_once(
        "deepseek-cache-stream-dual",
        json!({
            "prompt_tokens": 100,
            "completion_tokens": 7,
            "prompt_cache_hit_tokens": 98,
            "prompt_cache_miss_tokens": 2,
            "prompt_tokens_details": { "cached_tokens": 98 },
        }),
    )
    .await;
}

// ---------------------------------------------------------------------------
// Section D: existing Chat forwarding contract (characterization)
// ---------------------------------------------------------------------------

/// AC-002 / REQ-002: a native Chat tool conversation keeps every JSON value
/// through model rewriting, the ordered tool definitions and the isolation
/// fields are preserved, session headers reach upstream, and the provider
/// credential replaces - never exposes - the local relay key.
#[tokio::test]
async fn chat_native_fields_survive_model_rewrite_and_auth_is_upstream() {
    let home = temp_home("deepseek-cache-forward-contract");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({"id": "chatcmpl", "choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
        )
    })
    .await;

    let mut config = config_with_key(port);
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "upstream-secret", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    write_config(&config).unwrap();
    start_server(None).await.unwrap();

    let request_body = json!({
        "model": "local-a",
        "messages": [
            {"role": "system", "content": "sys"},
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "", "reasoning_content": ""},
            {"role": "assistant", "content": null, "reasoning_content": null},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call-1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":\"x\"}"}}
            ]},
            {"role": "tool", "tool_call_id": "call-1", "content": "{\"result\":1}"}
        ],
        "tools": [
            {"type": "function", "function": {"name": "first", "description": "one", "parameters": {"type": "object"}}},
            {"type": "function", "function": {"name": "second", "description": "two", "parameters": {"type": "object"}}}
        ],
        "tool_choice": "auto",
        "user_id": "user-123",
        "metadata": {"trace": "t-1", "nested": {"keep": [1, 2, 3]}},
        "temperature": 0.0,
        "stream": false
    });

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-opencode-session", "sess-abc"),
            ("x-session-id", "sess-xyz"),
        ],
        Some(request_body.clone()),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let captured = log.lock().unwrap().clone();
    assert_eq!(captured.len(), 1);
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(sent["model"], "remote-a", "only the mapped model is rewritten");

    let mut sent_rest = sent.clone();
    sent_rest.as_object_mut().unwrap().remove("model");
    let mut expected_rest = request_body.clone();
    expected_rest.as_object_mut().unwrap().remove("model");
    assert_eq!(
        sent_rest, expected_rest,
        "every native Chat field must keep its JSON value apart from the mapped model"
    );

    // The specific fields named by REQ-002, asserted individually.
    assert_eq!(sent["messages"][2]["reasoning_content"], json!(""));
    assert_eq!(sent["messages"][2]["content"], json!(""));
    assert_eq!(sent["messages"][3]["reasoning_content"], Value::Null);
    assert_eq!(sent["messages"][3]["content"], Value::Null);
    assert_eq!(
        sent["messages"][4]["tool_calls"][0]["function"]["name"],
        json!("lookup")
    );
    assert_eq!(sent["messages"][5]["tool_call_id"], json!("call-1"));
    assert_eq!(sent["messages"][5]["role"], json!("tool"));
    assert_eq!(sent["tools"][0]["function"]["name"], json!("first"));
    assert_eq!(sent["tools"][1]["function"]["name"], json!("second"));
    assert_eq!(sent["user_id"], json!("user-123"));
    assert_eq!(sent["metadata"]["nested"]["keep"], json!([1, 2, 3]));

    let headers = &captured[0].headers;
    assert_eq!(
        headers.get("authorization").map(String::as_str),
        Some("Bearer upstream-secret"),
        "upstream must receive exactly the provider credential: {headers:?}"
    );
    assert_ne!(
        headers.get("authorization").map(String::as_str),
        Some("Bearer local-key"),
        "the local relay key must never leak upstream: {headers:?}"
    );
    assert_eq!(
        headers.get("x-opencode-session").map(String::as_str),
        Some("sess-abc"),
        "vendor session headers must survive forwarding: {headers:?}"
    );
    assert_eq!(
        headers.get("x-session-id").map(String::as_str),
        Some("sess-xyz"),
        "session headers must survive forwarding: {headers:?}"
    );

    stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

/// A mock upstream that writes one SSE response in several flushed fragments
/// (with a short pause between them) so the relay observes a genuinely
/// fragmented stream. The exact total content-length is declared so the gateway
/// forwards every byte unchanged.
async fn spawn_fragmented_sse_upstream(fragments: Vec<String>) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind fragmented mock");
    let addr = listener.local_addr().expect("fragmented mock addr");
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let _ = read_http_request(&mut stream).await;
        let total: usize = fragments.iter().map(String::len).sum();
        let header = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {total}\r\nconnection: close\r\n\r\n"
        );
        let _ = stream.write_all(header.as_bytes()).await;
        for fragment in &fragments {
            if stream.write_all(fragment.as_bytes()).await.is_err() {
                return;
            }
            let _ = stream.flush().await;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    format!("http://{}", addr)
}
