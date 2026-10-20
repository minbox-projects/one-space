//! Behavior tests for the CommandCode provider quota boundary.
//!
//! These tests use fixed response bodies and an injected fetch function so the
//! parser, request guard, cache, failure behavior, and read-only boundary are
//! observable without making network requests.

use crate::ai_gateway::quota::{
    ai_gateway_provider_quota_with, is_quota_cache_fresh, parse_provider_quota,
    provider_quota_with, resolve_quota_request, ProviderQuota, QuotaCache,
    QuotaCredits, QuotaWindow, QuotaWindowLimits, COMMANDCODE_QUOTA_URL,
    QUOTA_CACHE_TTL_MS,
};
use crate::ai_gateway::{GatewayConfig, GatewayUpstreamProvider, UsageLogStore};
use serde_json::json;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const FIXTURE: &str = r#"{
    "credits": {
        "monthlyCredits": 42.5,
        "purchasedCredits": 1.25,
        "freeCredits": 0,
        "belowThreshold": false
    },
    "windowLimits": {
        "limited": true,
        "fiveHour": {
            "used": 0.5,
            "cap": 14,
            "exceeded": false,
            "resetAt": 1758600000000
        },
        "weekly": {
            "used": 11,
            "cap": 35,
            "exceeded": false,
            "resetAt": "2026-09-25T03:30:30.663Z"
        }
    },
    "unrecognizedField": {"preservedBySource": true}
}"#;

const SECRET_KEY: &str = "sk-secret-quota-key";

fn quota_provider(api_key: &str, base_url: &str) -> GatewayUpstreamProvider {
    // Raw JSON carries the legacy single credential (current behavior) and the
    // ordered pool the provider type will adopt, so the same fixture serves both.
    serde_json::from_value(json!({
        "id": "quota-provider",
        "name": "Provider quota-provider",
        "base_url": base_url,
        "api_key": api_key,
        "keys": [super::pool_key("key-default", "Default", api_key, true)],
        "protocol": "chat_completions",
        "enabled": true,
    }))
    .expect("quota provider fixture must deserialize")
}

fn config_with_provider(provider: GatewayUpstreamProvider) -> GatewayConfig {
    let mut config = GatewayConfig::default();
    config.providers.push(provider);
    config
}

fn expected_fixture() -> ProviderQuota {
    ProviderQuota {
        credits: QuotaCredits {
            monthly_credits: 42.5,
            purchased_credits: 1.25,
            free_credits: 0.0,
            below_threshold: false,
        },
        window_limits: Some(QuotaWindowLimits {
            limited: true,
            five_hour: Some(QuotaWindow {
                used: 0.5,
                cap: 14.0,
                exceeded: false,
                reset_at: Some(json!(1758600000000_u64)),
            }),
            weekly: Some(QuotaWindow {
                used: 11.0,
                cap: 35.0,
                exceeded: false,
                reset_at: Some(json!("2026-09-25T03:30:30.663Z")),
            }),
        }),
    }
}

#[test]
fn quota_endpoint_and_cache_ttl_are_fixed() {
    assert_eq!(
        COMMANDCODE_QUOTA_URL,
        "https://api.commandcode.ai/alpha/billing/credits"
    );
    assert_eq!(QUOTA_CACHE_TTL_MS, 300_000);
}

// AC-001 / REQ-003: the verified fixture parses numerics and preserves reset
// values in their original JSON representation while ignoring extra fields.
#[test]
fn parses_quota_fixture_and_preserves_epoch_and_string_reset_values() {
    let parsed = parse_provider_quota(FIXTURE).expect("fixture must parse");
    assert_eq!(parsed, expected_fixture());
}

#[test]
fn missing_and_null_numeric_fields_default_and_optional_fields_remain_absent() {
    let parsed = parse_provider_quota(
        r#"{
            "credits": {
                "monthlyCredits": 0,
                "purchasedCredits": null
            },
            "windowLimits": {
                "fiveHour": {"used": null, "cap": 0, "resetAt": null},
                "weekly": {"used": 1}
            }
        }"#,
    )
    .expect("missing/null numeric fields must default");

    assert_eq!(
        parsed.credits,
        QuotaCredits {
            monthly_credits: 0.0,
            purchased_credits: 0.0,
            free_credits: 0.0,
            below_threshold: false,
        }
    );
    let windows = parsed.window_limits.expect("windowLimits was provided");
    assert!(!windows.limited, "missing booleans default to false");
    assert_eq!(
        windows.five_hour,
        Some(QuotaWindow {
            used: 0.0,
            cap: 0.0,
            exceeded: false,
            reset_at: None,
        })
    );
    assert_eq!(
        windows.weekly,
        Some(QuotaWindow {
            used: 1.0,
            cap: 0.0,
            exceeded: false,
            reset_at: None,
        })
    );
    assert_eq!(
        parse_provider_quota(r#"{"credits":{}}"#)
            .expect("minimal credits object parses")
            .window_limits,
        None,
        "missing windowLimits remains absent"
    );
}

#[test]
fn rejects_missing_or_null_credits_object() {
    assert!(parse_provider_quota(r#"{"foo":1}"#).is_err());
    assert!(parse_provider_quota(r#"{"credits":null}"#).is_err());
}

// AC-005 / AC-006: the parser preserves values; display rules are not applied
// by this boundary, including non-positive caps and exceeded usage.
#[test]
fn parses_non_positive_caps_and_exceeded_values_without_clamping() {
    let parsed = parse_provider_quota(
        r#"{
            "credits": {},
            "windowLimits": {
                "limited": true,
                "fiveHour": {"used": 8, "cap": 0, "exceeded": true},
                "weekly": {"used": 12, "cap": -1, "exceeded": false}
            }
        }"#,
    )
    .expect("raw window values must parse");
    let windows = parsed.window_limits.unwrap();
    assert_eq!(
        windows.five_hour,
        Some(QuotaWindow {
            used: 8.0,
            cap: 0.0,
            exceeded: true,
            reset_at: None,
        })
    );
    assert_eq!(
        windows.weekly,
        Some(QuotaWindow {
            used: 12.0,
            cap: -1.0,
            exceeded: false,
            reset_at: None,
        })
    );
}

#[test]
fn quota_serialization_round_trip_uses_camel_case_json_keys() {
    let parsed = parse_provider_quota(FIXTURE).expect("fixture must parse");
    let encoded = serde_json::to_value(&parsed).expect("serialize quota");
    assert_eq!(encoded["credits"]["monthlyCredits"], json!(42.5));
    assert_eq!(encoded["credits"]["purchasedCredits"], json!(1.25));
    assert_eq!(encoded["credits"]["freeCredits"], json!(0.0));
    assert_eq!(encoded["credits"]["belowThreshold"], json!(false));
    assert_eq!(encoded["windowLimits"]["fiveHour"]["resetAt"], json!(1758600000000_u64));
    assert_eq!(
        encoded["windowLimits"]["weekly"]["resetAt"],
        json!("2026-09-25T03:30:30.663Z")
    );
    assert!(encoded.get("window_limits").is_none());
    assert!(encoded["windowLimits"].get("five_hour").is_none());
    assert!(encoded["credits"].get("monthly_credits").is_none());
    let decoded: ProviderQuota = serde_json::from_value(encoded).expect("deserialize quota");
    assert_eq!(decoded, parsed);
}

// REQ-005 / AC-005 / AC-012: request resolution follows the effective template
// quota policy, not the provider URL, and always targets the single fixed
// endpoint for an accepted policy.
#[test]
fn resolve_quota_request_follows_the_effective_commandcode_policy() {
    let bound = config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://any.example.com/v1",
        "commandcode",
    ));
    let request = resolve_quota_request(&bound, &bound.providers[0])
        .expect("a CommandCode-bound provider must be accepted");
    assert_eq!(request.url, COMMANDCODE_QUOTA_URL);
    assert_eq!(request.api_key, SECRET_KEY);

    for provider in [
        // An unbound CommandCode-looking URL never resolves the policy.
        quota_provider(SECRET_KEY, "https://API.CommandCode.AI:8443/a/path"),
        // A blank key on a bound provider is still rejected.
        bound_quota_provider("   ", "https://api.commandcode.ai/v1", "commandcode"),
    ] {
        let config = config_with_provider(provider);
        let error = resolve_quota_request(&config, &config.providers[0])
            .expect_err("invalid quota request must be rejected");
        assert!(
            !error.contains(SECRET_KEY),
            "request-resolution error leaked the stored key: {error}"
        );
    }
}

#[test]
fn freshness_ends_at_the_exact_ttl_boundary() {
    let cached_at = 10_000;
    assert!(is_quota_cache_fresh(cached_at, cached_at));
    assert!(is_quota_cache_fresh(
        cached_at,
        cached_at + QUOTA_CACHE_TTL_MS - 1
    ));
    assert!(!is_quota_cache_fresh(
        cached_at,
        cached_at + QUOTA_CACHE_TTL_MS
    ));
}

#[test]
fn quota_cache_requires_matching_provider_credentials_and_freshness() {
    let snapshot = expected_fixture();
    let mut cache = QuotaCache::new();
    cache.store(
        "provider-id",
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
        100,
        snapshot.clone(),
    );

    assert_eq!(
        cache.get_fresh(
            "provider-id",
            SECRET_KEY,
            "https://api.commandcode.ai/provider/v1",
            101,
            false
        ),
        Some(snapshot.clone())
    );
    assert_eq!(
        cache.get_fresh(
            "provider-id",
            SECRET_KEY,
            "https://api.commandcode.ai/provider/v1",
            101,
            true
        ),
        None,
        "forced refresh bypasses the snapshot"
    );
    assert_eq!(
        cache.get_fresh(
            "provider-id",
            "sk-rotated-key",
            "https://api.commandcode.ai/provider/v1",
            101,
            false
        ),
        None,
        "a changed API key invalidates the snapshot"
    );
    assert_eq!(
        cache.get_fresh(
            "provider-id",
            SECRET_KEY,
            "https://api.commandcode.ai/other/path",
            101,
            false
        ),
        None,
        "a changed base URL invalidates the snapshot"
    );
    assert_eq!(
        cache.get_fresh(
            "provider-id",
            SECRET_KEY,
            "https://api.commandcode.ai/provider/v1",
            100 + QUOTA_CACHE_TTL_MS,
            false
        ),
        None,
        "an entry is stale at exactly the TTL"
    );
}

// AC-004: cache hits avoid fetching and a forced refresh invokes the injected
// fetch exactly once more.
#[tokio::test]
async fn provider_quota_fetches_on_miss_caches_success_and_forces_refresh() {
    let config = config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
        "commandcode",
    ));
    let cache = Mutex::new(QuotaCache::new());
    let calls = Arc::new(AtomicUsize::new(0));

    let first_calls = Arc::clone(&calls);
    let first = provider_quota_with(
        &config,
        "quota-provider",
        false,
        1_000,
        &cache,
        move |url, api_key| {
            first_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(url, COMMANDCODE_QUOTA_URL);
                assert_eq!(api_key, SECRET_KEY);
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("cache miss fetch must succeed");
    assert_eq!(first, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let should_not_fetch = Arc::clone(&calls);
    let cached = provider_quota_with(
        &config,
        "quota-provider",
        false,
        1_001,
        &cache,
        move |_, _| {
            should_not_fetch.fetch_add(1, Ordering::SeqCst);
            async { Ok("unexpected fetch".to_string()) }
        },
    )
    .await
    .expect("fresh success must be cached");
    assert_eq!(cached, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "cache hit must not fetch");

    let forced_calls = Arc::clone(&calls);
    let forced = provider_quota_with(
        &config,
        "quota-provider",
        true,
        1_002,
        &cache,
        move |_, _| {
            forced_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await
    .expect("forced fetch must succeed");
    assert_eq!(forced, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

// AC-012: invalid credentials and non-CommandCode providers fail before fetch.
#[tokio::test]
async fn invalid_provider_credentials_and_unknown_ids_fail_without_fetching() {
    let calls = AtomicUsize::new(0);
    let cache = Mutex::new(QuotaCache::new());

    for provider in [
        quota_provider("  \t", "https://api.commandcode.ai/v1"),
        quota_provider(SECRET_KEY, "https://api.openai.com/v1"),
    ] {
        let config = config_with_provider(provider);
        let error = provider_quota_with(
            &config,
            "quota-provider",
            false,
            0,
            &cache,
            |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Ok(FIXTURE.to_string()) }
            },
        )
        .await
        .expect_err("invalid provider must fail");
        assert!(
            !error.contains(SECRET_KEY),
            "dispatch error leaked the stored key: {error}"
        );
    }

    let valid_config = config_with_provider(quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/v1",
    ));
    let missing_provider = provider_quota_with(
        &valid_config,
        "not-present",
        false,
        0,
        &cache,
        |_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await;
    assert!(missing_provider.is_err(), "unknown providers must fail");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

// AC-009: HTTP, non-JSON and unusable-payload failures neither expose the key
// nor populate the success cache.
#[tokio::test]
async fn fetch_and_parse_failures_are_not_cached_or_returned_with_the_key() {
    let invalid_responses: [(Result<String, String>, Option<&str>); 3] = [
        (
            Err("HTTP 429 Too Many Requests".to_string()),
            Some("HTTP 429"),
        ),
        (Ok("upstream returned non-JSON".to_string()), None),
        (Ok(r#"{"foo":1}"#.to_string()), None),
    ];

    for (response, expected_error_fragment) in invalid_responses {
        let config = config_with_provider(bound_quota_provider(
            SECRET_KEY,
            "https://api.commandcode.ai/provider/v1",
            "commandcode",
        ));
        let cache = Mutex::new(QuotaCache::new());
        let calls = Arc::new(AtomicUsize::new(0));

        for now_ms in [20_000, 20_001] {
            let response = response.clone();
            let fetch_calls = Arc::clone(&calls);
            let error = provider_quota_with(
                &config,
                "quota-provider",
                false,
                now_ms,
                &cache,
                move |_, _| {
                    fetch_calls.fetch_add(1, Ordering::SeqCst);
                    async move { response }
                },
            )
            .await
            .expect_err("invalid response must fail");
            assert!(
                !error.contains(SECRET_KEY),
                "quota error leaked the stored key: {error}"
            );
            if let Some(fragment) = expected_error_fragment {
                assert!(
                    error.contains(fragment),
                    "fetch error should propagate {fragment:?}: {error}"
                );
            }
        }

        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "the second call must fetch again after a failed fetch or parse"
        );
    }
}

struct TempHome {
    path: std::path::PathBuf,
    guard: Option<crate::config::test_home::TestHomeGuard>,
}

impl TempHome {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "onespace-ai-gateway-quota-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create isolated home");
        let guard = crate::config::test_home::TestHomeGuard::set(&path);
        Self {
            path,
            guard: Some(guard),
        }
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        self.guard.take();
        let _ = fs::remove_dir_all(&self.path);
    }
}

// AC-010 / REQ-007: successful and failed read-only quota queries leave the
// encrypted configuration bytes and usage-log row count unchanged.
#[tokio::test]
async fn quota_queries_do_not_write_config_or_usage_logs() {
    let _home = TempHome::new();
    let config = config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
        "commandcode",
    ));
    crate::ai_gateway::storage::write_config(&config).expect("write initial gateway config");
    let config_file = crate::ai_gateway::storage::config_path().expect("gateway config path");
    let bytes_before = fs::read(&config_file).expect("read encrypted gateway config");
    let store = UsageLogStore::default_store().expect("default usage store");
    let rows_before = store.count().expect("count initial usage rows");

    let cache = Mutex::new(QuotaCache::new());
    let success = provider_quota_with(
        &config,
        "quota-provider",
        false,
        50_000,
        &cache,
        |_, _| async { Ok(FIXTURE.to_string()) },
    )
    .await
    .expect("read-only query succeeds");
    assert_eq!(success, expected_fixture());

    let error = provider_quota_with(
        &config,
        "quota-provider",
        true,
        50_001,
        &cache,
        |_, _| async { Err("HTTP 503 Service Unavailable".to_string()) },
    )
    .await
    .expect_err("injected failure propagates");
    assert!(!error.contains(SECRET_KEY));

    let bytes_after = fs::read(&config_file).expect("read gateway config after query");
    assert_eq!(bytes_after, bytes_before, "quota reads must not write config");
    assert_eq!(
        store.count().expect("count usage rows after query"),
        rows_before,
        "quota reads must not append usage rows"
    );
}

// AC-004 / AC-010 / AC-012: the production command-path seam reads persisted
// configuration, reuses successful cache entries, and remains read-only.
#[tokio::test]
async fn quota_command_path_is_read_only_and_uses_the_shared_cache() {
    let _home = TempHome::new();
    let config = config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
        "commandcode",
    ));
    crate::ai_gateway::storage::write_config(&config).expect("write initial gateway config");
    let config_file = crate::ai_gateway::storage::config_path().expect("gateway config path");
    let bytes_before = fs::read(&config_file).expect("read encrypted gateway config");
    let store = UsageLogStore::default_store().expect("default usage store");
    let rows_before = store.count().expect("count initial usage rows");

    let cache = Mutex::new(QuotaCache::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let success_calls = Arc::clone(&calls);
    let success = ai_gateway_provider_quota_with(
        "quota-provider".to_string(),
        None,
        50_000,
        &cache,
        move |url, api_key| {
            success_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(url, COMMANDCODE_QUOTA_URL);
                assert_eq!(api_key, SECRET_KEY);
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("read-only command-path query succeeds");
    assert_eq!(success, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let cached_calls = Arc::clone(&calls);
    let cached = ai_gateway_provider_quota_with(
        "quota-provider".to_string(),
        None,
        50_001,
        &cache,
        move |_, _| {
            cached_calls.fetch_add(1, Ordering::SeqCst);
            async { Err("unexpected cache-miss fetch".to_string()) }
        },
    )
    .await
    .expect("second command-path call reuses the cached quota");
    assert_eq!(cached, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "cache hit must not fetch");

    let failure_calls = Arc::clone(&calls);
    let error = ai_gateway_provider_quota_with(
        "quota-provider".to_string(),
        Some(true),
        50_002,
        &cache,
        move |_, _| {
            failure_calls.fetch_add(1, Ordering::SeqCst);
            async {
                Err("failed to fetch https://api.commandcode.ai/alpha/billing/credits: HTTP 503"
                    .to_string())
            }
        },
    )
    .await
    .expect_err("injected command-path failure propagates");
    assert!(!error.contains(SECRET_KEY), "quota error leaked the stored key: {error}");
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let bytes_after = fs::read(&config_file).expect("read gateway config after quota queries");
    assert_eq!(bytes_after, bytes_before, "quota reads must not write config");
    assert_eq!(
        store.count().expect("count usage rows after quota queries"),
        rows_before,
        "quota reads must not append usage rows"
    );
}

// AC-012: a persisted CommandCode provider with no API key is rejected before fetch.
#[tokio::test]
async fn quota_command_path_rejects_empty_api_key_without_fetching() {
    let _home = TempHome::new();
    let config = config_with_provider(quota_provider(
        "",
        "https://api.commandcode.ai/provider/v1",
    ));
    crate::ai_gateway::storage::write_config(&config).expect("write empty-key gateway config");

    let calls = AtomicUsize::new(0);
    let error = ai_gateway_provider_quota_with(
        "quota-provider".to_string(),
        None,
        50_000,
        &Mutex::new(QuotaCache::new()),
        |_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await
    .expect_err("empty persisted API key must be rejected");

    assert!(!error.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0, "invalid credentials must not fetch");
}

// ---------------------------------------------------------------------------
// Pinned quota source over the ordered key pool (REQ-010)
//
// The provider is written as raw schema-2 JSON because the typed provider has
// no key pool yet; the command seam reads the persisted configuration, so the
// injected fetch observes which key the quota source resolved.
// ---------------------------------------------------------------------------

fn pool_key_value(id: &str, value: &str, enabled: bool) -> serde_json::Value {
    json!({
        "id": id,
        "name": id,
        "value": value,
        "enabled": enabled,
        "auto_marked": false,
        "failure_kind": null,
        "marked_at": null,
        "reason": null,
    })
}

fn write_quota_pool(keys: Vec<serde_json::Value>) {
    super::write_raw_gateway_config(&json!({
        "schema_version": 2,
        "providers": [{
            "id": "quota-pool",
            "name": "Quota Pool",
            "base_url": "https://api.commandcode.ai/provider/v1",
            "protocol": "chat_completions",
            "template_id": "commandcode",
            "keys": keys,
            "mappings": [],
        }],
    }));
}

/// REQ-010: the quota block always queries the first enabled key in list order
/// (skipping a disabled earlier key) and a change of that key invalidates the
/// cache.
#[tokio::test]
async fn quota_source_is_first_enabled_key_and_invalidates_on_key_change() {
    let _home = TempHome::new();
    write_quota_pool(vec![
        pool_key_value("key-disabled", "sk-disabled", false),
        pool_key_value("key-a", "sk-first", true),
        pool_key_value("key-b", "sk-second", true),
    ]);

    let cache = Mutex::new(QuotaCache::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let first_calls = Arc::clone(&calls);
    let first = ai_gateway_provider_quota_with(
        "quota-pool".to_string(),
        None,
        1_000,
        &cache,
        move |url, api_key| {
            first_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(url, COMMANDCODE_QUOTA_URL);
                assert_eq!(api_key, "sk-first", "the first enabled key must be the source");
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("the first enabled key must resolve the quota request");
    assert_eq!(first, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // The first enabled key changes: the cached snapshot must not be reused.
    write_quota_pool(vec![
        pool_key_value("key-a", "sk-first", false),
        pool_key_value("key-b", "sk-second", true),
    ]);
    let second_calls = Arc::clone(&calls);
    ai_gateway_provider_quota_with(
        "quota-pool".to_string(),
        None,
        1_001,
        &cache,
        move |_, api_key| {
            second_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(api_key, "sk-second", "the new first enabled key must be queried");
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("a changed source key must invalidate the cache and refetch");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "a changed source key must force a fresh fetch"
    );
}

/// REQ-010 boundary: when every key is disabled the first key in list order is
/// still the quota source.
#[tokio::test]
async fn quota_source_uses_the_first_key_when_all_are_disabled() {
    let _home = TempHome::new();
    write_quota_pool(vec![
        pool_key_value("key-a", "sk-first-disabled", false),
        pool_key_value("key-b", "sk-second-disabled", false),
    ]);

    let calls = AtomicUsize::new(0);
    ai_gateway_provider_quota_with(
        "quota-pool".to_string(),
        None,
        1_000,
        &Mutex::new(QuotaCache::new()),
        |_, api_key| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(
                    api_key, "sk-first-disabled",
                    "the first key must remain the pinned source when all are disabled"
                );
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("an all-disabled pool still resolves the first key");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// REQ-010 boundary: an empty pool keeps the existing no-key error and never
/// fetches.
#[tokio::test]
async fn quota_source_empty_pool_keeps_the_no_key_error() {
    let _home = TempHome::new();
    write_quota_pool(vec![]);

    let calls = AtomicUsize::new(0);
    let error = ai_gateway_provider_quota_with(
        "quota-pool".to_string(),
        None,
        1_000,
        &Mutex::new(QuotaCache::new()),
        |_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await
    .expect_err("an empty key pool must fail before fetching");
    assert!(
        error.to_lowercase().contains("api key"),
        "the existing no-key error must be preserved: {error}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

// ---------------------------------------------------------------------------
// REQ-006 (20260925-gateway-routing-and-config-hardening): the quota source key
// prefers the first enabled *unmarked* key, then the first enabled key, then the
// first stored key.
// ---------------------------------------------------------------------------

/// A runtime-marked key pool entry for the quota fixtures.
fn quota_pool_key_marked(
    id: &str,
    value: &str,
    enabled: bool,
    failure_kind: &str,
    marked_at: u64,
) -> serde_json::Value {
    json!({
        "id": id,
        "name": id,
        "value": value,
        "enabled": enabled,
        "auto_marked": true,
        "failure_kind": failure_kind,
        "marked_at": marked_at,
        "reason": "weekly usage limit",
    })
}

/// AC-028 / REQ-006: a marked earlier enabled key defers to the first enabled
/// unmarked key.
#[tokio::test]
async fn ac028_quota_source_prefers_the_first_enabled_unmarked_key() {
    let _home = TempHome::new();
    write_quota_pool(vec![
        quota_pool_key_marked("key-marked", "sk-marked", true, "quota", 1_700_000_000),
        pool_key_value("key-unmarked", "sk-unmarked", true),
    ]);

    let calls = Arc::new(AtomicUsize::new(0));
    let fetch_calls = Arc::clone(&calls);
    let result = ai_gateway_provider_quota_with(
        "quota-pool".to_string(),
        None,
        1_000,
        &Mutex::new(QuotaCache::new()),
        move |url, api_key| {
            fetch_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(url, COMMANDCODE_QUOTA_URL);
                assert_eq!(
                    api_key, "sk-unmarked",
                    "the first enabled unmarked key must be the source"
                );
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("a marked first key must defer to the unmarked key");
    assert_eq!(result, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// AC-029 / REQ-006: when every enabled key is runtime-marked, the first enabled
/// key's value is used.
#[tokio::test]
async fn ac029_quota_source_falls_back_to_the_first_enabled_key_when_all_are_marked() {
    let _home = TempHome::new();
    write_quota_pool(vec![
        quota_pool_key_marked("key-a", "sk-first", true, "quota", 1_700_000_000),
        quota_pool_key_marked("key-b", "sk-second", true, "authentication", 1_700_000_100),
    ]);

    let calls = AtomicUsize::new(0);
    ai_gateway_provider_quota_with(
        "quota-pool".to_string(),
        None,
        1_000,
        &Mutex::new(QuotaCache::new()),
        |_, api_key| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(
                    api_key, "sk-first",
                    "when every enabled key is marked the first enabled key stays the source"
                );
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("an all-marked pool still resolves the first enabled key");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// AC-028 / AC-031 / REQ-006: the quota resolver follows the ladder and its
/// cache still invalidates when a mark change makes a different source key the
/// first enabled unmarked one.
#[tokio::test]
async fn ac031_quota_source_cache_invalidates_on_a_mark_driven_source_change() {
    let _home = TempHome::new();
    write_quota_pool(vec![
        pool_key_value("key-a", "sk-first", true),
        pool_key_value("key-b", "sk-second", true),
    ]);

    let cache = Mutex::new(QuotaCache::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let first_calls = Arc::clone(&calls);
    ai_gateway_provider_quota_with(
        "quota-pool".to_string(),
        None,
        1_000,
        &cache,
        move |_, api_key| {
            first_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(api_key, "sk-first");
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("the first enabled unmarked key resolves the source");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Marking the cached source pushes the resolver to the second enabled key;
    // the cache entry must not be reused because its source value changed.
    write_quota_pool(vec![
        quota_pool_key_marked("key-a", "sk-first", true, "quota", 1_700_000_000),
        pool_key_value("key-b", "sk-second", true),
    ]);
    let second_calls = Arc::clone(&calls);
    ai_gateway_provider_quota_with(
        "quota-pool".to_string(),
        None,
        1_001,
        &cache,
        move |_, api_key| {
            second_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(
                    api_key, "sk-second",
                    "the newly unmarked enabled key must be queried"
                );
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("a mark-driven source change must invalidate the cache");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "a changed source key must force a fresh fetch"
    );
}

// ===========================================================================
// 20261010-provider-template-bound-strategies — REQ-005 (AC-005, AC-006,
// AC-012, AC-013): CommandCode quota dispatch resolves from the provider's
// effective template policy, never from its URL. An unbound provider is generic
// (no monitor), a broken binding falls back to generic and rejects a mismatched
// direct command before any fetch, and a policy/binding change cannot reuse an
// incompatible monitor snapshot.
// ===========================================================================

fn bound_quota_provider(
    api_key: &str,
    base_url: &str,
    template_id: &str,
) -> GatewayUpstreamProvider {
    let mut provider = quota_provider(api_key, base_url);
    provider.template_id = Some(template_id.to_string());
    provider
}

/// AC-005 / AC-012 counterexample: an unbound provider whose URL looks like
/// CommandCode must still use the generic (none) policy and never fetch.
#[tokio::test]
async fn unbound_commandcode_looking_provider_never_fetches_commandcode_quota() {
    let config = config_with_provider(quota_provider(SECRET_KEY, "https://api.commandcode.ai/v1"));
    let calls = Arc::new(AtomicUsize::new(0));
    let fetch_calls = Arc::clone(&calls);

    let result = provider_quota_with(
        &config,
        "quota-provider",
        false,
        1_000,
        &Mutex::new(QuotaCache::new()),
        move |_, _| {
            fetch_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await;

    assert!(
        result.is_err(),
        "an unbound provider must not receive CommandCode quota from its URL"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "an unbound provider must be rejected before any fetch"
    );
}

/// AC-005 / AC-012: a provider bound to the CommandCode template uses the
/// CommandCode endpoint even when its own base URL is not CommandCode.
#[tokio::test]
async fn provider_bound_to_commandcode_uses_quota_regardless_of_base_url() {
    let config = config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://my-gateway.example.com/v1",
        "commandcode",
    ));

    let result = provider_quota_with(
        &config,
        "quota-provider",
        false,
        1_000,
        &Mutex::new(QuotaCache::new()),
        |url, key| async move {
            assert_eq!(
                url, COMMANDCODE_QUOTA_URL,
                "the policy must select the CommandCode credits endpoint"
            );
            assert_eq!(key, SECRET_KEY);
            Ok(FIXTURE.to_string())
        },
    )
    .await
    .expect("a CommandCode-bound provider must resolve the quota request");
    assert_eq!(result, expected_fixture());
}

/// AC-006: a broken template binding falls back to generic and rejects the
/// direct quota command before any fetch, even with a CommandCode-looking URL.
#[tokio::test]
async fn broken_template_binding_rejects_commandcode_quota_before_fetch() {
    let config = config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/v1",
        "missing-template",
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let fetch_calls = Arc::clone(&calls);

    let result = provider_quota_with(
        &config,
        "quota-provider",
        false,
        1_000,
        &Mutex::new(QuotaCache::new()),
        move |_, _| {
            fetch_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await;

    assert!(
        result
            .expect_err("a broken binding must reject the CommandCode quota command")
            .len()
            > 0,
        "the rejection must carry an actionable reason"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a broken binding must be rejected before any fetch"
    );
}

/// AC-006 counterexample: a provider whose effective policy is OpenCode Go must
/// be rejected by the CommandCode quota command before any fetch, even when its
/// URL is CommandCode-like.
#[tokio::test]
async fn commandcode_quota_command_rejects_a_non_commandcode_policy_before_fetch() {
    let config = config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/v1",
        "opencode-go",
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let fetch_calls = Arc::clone(&calls);

    let result = provider_quota_with(
        &config,
        "quota-provider",
        false,
        1_000,
        &Mutex::new(QuotaCache::new()),
        move |_, _| {
            fetch_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await;

    assert!(
        result.is_err(),
        "a Go-policy provider must be rejected by the CommandCode quota command"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a strategy mismatch must be rejected before any fetch"
    );
}

/// AC-013: a successful CommandCode snapshot is not reused after the provider's
/// binding changes; the same-policy five-minute cache still works.
#[tokio::test]
async fn quota_cache_is_not_reused_across_a_binding_change() {
    let mut config = config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/v1",
        "commandcode",
    ));
    let cache = Mutex::new(QuotaCache::new());
    let calls = Arc::new(AtomicUsize::new(0));

    let first_calls = Arc::clone(&calls);
    let first = provider_quota_with(
        &config,
        "quota-provider",
        false,
        1_000,
        &cache,
        move |_, _| {
            first_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await
    .expect("a CommandCode-bound provider must fetch on a cache miss");
    assert_eq!(first, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Same policy, still fresh: the snapshot is served from cache.
    provider_quota_with(
        &config,
        "quota-provider",
        false,
        1_001,
        &cache,
        |_, _| async { panic!("a same-policy fresh cache hit must not fetch") },
    )
    .await
    .expect("a same-policy cache hit must be reused");

    // Change the binding to the generic policy; the stale snapshot must not be
    // returned and the mismatch must not trigger a fetch either.
    config.providers[0].template_id = None;
    let second_calls = Arc::clone(&calls);
    let second = provider_quota_with(
        &config,
        "quota-provider",
        false,
        1_002,
        &cache,
        move |_, _| {
            second_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await;
    assert!(
        second.is_err(),
        "an incompatible cached snapshot must not be reused after a binding change"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a binding change must neither reuse nor refetch the stale snapshot"
    );
}

// ---------------------------------------------------------------------------
// Step 5: matching in-flight quota lookups share one network operation.
//
// The injected fetch closure is the only network boundary. These tests count
// actual closure invocations and compare returned snapshots; they do not
// inspect cache or flight-map internals. A semaphore parks a started fetch
// until the test has observed the concurrent contender's decision, so the
// assertions use operation counts rather than wall-clock ceilings.
// ---------------------------------------------------------------------------

struct QuotaFlightControl {
    calls: AtomicUsize,
    arrived: tokio::sync::Notify,
    release: tokio::sync::Semaphore,
}

impl QuotaFlightControl {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            arrived: tokio::sync::Notify::new(),
            release: tokio::sync::Semaphore::new(0),
        }
    }

    async fn wait_for_release(&self) {
        let permit = self
            .release
            .acquire()
            .await
            .expect("quota flight release semaphore must stay open");
        permit.forget();
    }
}

fn quota_body(monthly_credits: f64) -> String {
    format!(
        r#"{{"credits":{{"monthlyCredits":{monthly_credits},"purchasedCredits":0,"freeCredits":0,"belowThreshold":false}}}}"#
    )
}

fn spawn_quota_lookup(
    config: Arc<GatewayConfig>,
    cache: Arc<Mutex<QuotaCache>>,
    control: Arc<QuotaFlightControl>,
    barrier: Arc<tokio::sync::Barrier>,
    force_refresh: bool,
    now_ms: u64,
    outcome: Result<String, String>,
) -> tokio::task::JoinHandle<Result<ProviderQuota, String>> {
    tokio::spawn(async move {
        barrier.wait().await;
        provider_quota_with(
            config.as_ref(),
            "quota-provider",
            force_refresh,
            now_ms,
            cache.as_ref(),
            move |_, _| {
                let control = Arc::clone(&control);
                async move {
                    control.calls.fetch_add(1, Ordering::SeqCst);
                    control.arrived.notify_one();
                    control.wait_for_release().await;
                    outcome
                }
            },
        )
        .await
    })
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_matching_cold_quota_lookups_share_one_network_fetch() {
    let config = Arc::new(config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
        "commandcode",
    )));
    let cache = Arc::new(Mutex::new(QuotaCache::new()));
    let control = Arc::new(QuotaFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let first = spawn_quota_lookup(
        Arc::clone(&config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(FIXTURE.to_string()),
    );
    let second = spawn_quota_lookup(
        Arc::clone(&config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(FIXTURE.to_string()),
    );

    control.arrived.notified().await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let observed = control.calls.load(Ordering::SeqCst);
    control.release.add_permits(8);

    let first = first.await.expect("first lookup task must not panic");
    let second = second.await.expect("second lookup task must not panic");
    let first = first.expect("first concurrent lookup must succeed");
    let second = second.expect("second concurrent lookup must succeed");
    assert_eq!(first, expected_fixture());
    assert_eq!(second, expected_fixture());
    assert_eq!(
        observed, 1,
        "two concurrent matching cold quota lookups must invoke the network fetch once, observed {observed}"
    );
    assert_eq!(
        control.calls.load(Ordering::SeqCst),
        1,
        "no later fetch may run for the same settled identity"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_matching_forced_quota_refreshes_join_one_flight() {
    let config = Arc::new(config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
        "commandcode",
    )));
    let mut cache = QuotaCache::new();
    cache.store(
        "quota-provider",
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
        1_000,
        expected_fixture(),
    );
    let cache = Arc::new(Mutex::new(cache));
    let control = Arc::new(QuotaFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let first = spawn_quota_lookup(
        Arc::clone(&config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        true,
        1_001,
        Ok(FIXTURE.to_string()),
    );
    let second = spawn_quota_lookup(
        Arc::clone(&config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        true,
        1_001,
        Ok(FIXTURE.to_string()),
    );

    control.arrived.notified().await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let observed = control.calls.load(Ordering::SeqCst);
    control.release.add_permits(8);

    let first = first.await.expect("first forced refresh task must not panic");
    let second = second.await.expect("second forced refresh task must not panic");
    assert_eq!(
        first.expect("first forced refresh must succeed"),
        expected_fixture()
    );
    assert_eq!(
        second.expect("second forced refresh must succeed"),
        expected_fixture()
    );
    assert_eq!(
        observed, 1,
        "a forced refresh must bypass completed success but join a matching in-flight fetch, observed {observed}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_quota_lookups_with_distinct_base_urls_do_not_share() {
    let cache = Arc::new(Mutex::new(QuotaCache::new()));
    let control = Arc::new(QuotaFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let first_config = Arc::new(config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/first/v1",
        "commandcode",
    )));
    let second_config = Arc::new(config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/second/v1",
        "commandcode",
    )));

    let first = spawn_quota_lookup(
        Arc::clone(&first_config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(quota_body(1.0)),
    );
    let second = spawn_quota_lookup(
        Arc::clone(&second_config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(quota_body(2.0)),
    );

    control.arrived.notified().await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let observed = control.calls.load(Ordering::SeqCst);
    control.release.add_permits(8);

    let first = first
        .await
        .expect("first base-URL task must not panic")
        .expect("first base-URL lookup must succeed");
    let second = second
        .await
        .expect("second base-URL task must not panic")
        .expect("second base-URL lookup must succeed");
    assert_eq!(
        observed, 2,
        "different base URLs are different source identities and must not share a fetch, observed {observed}"
    );
    assert_eq!(first.credits.monthly_credits, 1.0);
    assert_eq!(second.credits.monthly_credits, 2.0);
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_quota_lookups_with_distinct_keys_do_not_share() {
    let cache = Arc::new(Mutex::new(QuotaCache::new()));
    let control = Arc::new(QuotaFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let first_config = Arc::new(config_with_provider(bound_quota_provider(
        "sk-first-source",
        "https://api.commandcode.ai/provider/v1",
        "commandcode",
    )));
    let second_config = Arc::new(config_with_provider(bound_quota_provider(
        "sk-second-source",
        "https://api.commandcode.ai/provider/v1",
        "commandcode",
    )));

    let first = spawn_quota_lookup(
        Arc::clone(&first_config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(quota_body(3.0)),
    );
    let second = spawn_quota_lookup(
        Arc::clone(&second_config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(quota_body(4.0)),
    );

    control.arrived.notified().await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let observed = control.calls.load(Ordering::SeqCst);
    control.release.add_permits(8);

    let first = first
        .await
        .expect("first key task must not panic")
        .expect("first key lookup must succeed");
    let second = second
        .await
        .expect("second key task must not panic")
        .expect("second key lookup must succeed");
    assert_eq!(
        observed, 2,
        "different pinned keys are different source identities and must not share a fetch, observed {observed}"
    );
    assert_eq!(first.credits.monthly_credits, 3.0);
    assert_eq!(second.credits.monthly_credits, 4.0);
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_quota_failure_settles_and_a_later_lookup_retries() {
    let config = Arc::new(config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
        "commandcode",
    )));
    let cache = Arc::new(Mutex::new(QuotaCache::new()));
    let control = Arc::new(QuotaFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let first = spawn_quota_lookup(
        Arc::clone(&config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Err("HTTP 503 Service Unavailable".to_string()),
    );
    let second = spawn_quota_lookup(
        Arc::clone(&config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Err("HTTP 503 Service Unavailable".to_string()),
    );

    control.arrived.notified().await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let concurrent_calls = control.calls.load(Ordering::SeqCst);
    control.release.add_permits(8);

    assert!(first
        .await
        .expect("first failed lookup task must not panic")
        .is_err());
    assert!(second
        .await
        .expect("second failed lookup task must not panic")
        .is_err());

    let retry_control = Arc::clone(&control);
    let retry = provider_quota_with(
        config.as_ref(),
        "quota-provider",
        false,
        1_001,
        cache.as_ref(),
        move |_, _| {
            let control = Arc::clone(&retry_control);
            async move {
                control.calls.fetch_add(1, Ordering::SeqCst);
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("a settled failed flight must allow a later lookup to retry");
    assert_eq!(retry, expected_fixture());

    let total_calls = control.calls.load(Ordering::SeqCst);
    assert!(
        concurrent_calls == 1 && total_calls == 2,
        "one matching failed flight must settle and allow one retry, concurrent={concurrent_calls}, total={total_calls}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_quota_owner_does_not_strand_a_later_lookup() {
    let config = Arc::new(config_with_provider(bound_quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
        "commandcode",
    )));
    let cache = Arc::new(Mutex::new(QuotaCache::new()));
    let control = Arc::new(QuotaFlightControl::new());

    let owner_config = Arc::clone(&config);
    let owner_cache = Arc::clone(&cache);
    let owner_control = Arc::clone(&control);
    let owner = tokio::spawn(async move {
        provider_quota_with(
            owner_config.as_ref(),
            "quota-provider",
            false,
            1_000,
            owner_cache.as_ref(),
            move |_, _| {
                let control = Arc::clone(&owner_control);
                async move {
                    control.calls.fetch_add(1, Ordering::SeqCst);
                    control.arrived.notify_one();
                    control.wait_for_release().await;
                    Ok(FIXTURE.to_string())
                }
            },
        )
        .await
    });

    control.arrived.notified().await;
    owner.abort();
    assert!(owner.await.expect_err("owner task must be cancelled").is_cancelled());

    let retry_control = Arc::clone(&control);
    let retry = provider_quota_with(
        config.as_ref(),
        "quota-provider",
        false,
        1_001,
        cache.as_ref(),
        move |_, _| {
            let control = Arc::clone(&retry_control);
            async move {
                control.calls.fetch_add(1, Ordering::SeqCst);
                Ok(FIXTURE.to_string())
            }
        },
    );
    let retry = tokio::time::timeout(std::time::Duration::from_secs(2), retry)
        .await
        .expect("a cancelled flight must not strand a later lookup")
        .expect("the later lookup must retry successfully");
    assert_eq!(retry, expected_fixture());
    assert_eq!(
        control.calls.load(Ordering::SeqCst),
        2,
        "the cancelled fetch and one retry are the only network operations"
    );
}
