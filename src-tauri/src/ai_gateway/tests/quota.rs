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
    let mut provider = super::provider("quota-provider");
    provider.api_key = api_key.to_string();
    provider.base_url = base_url.to_string();
    provider
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

// REQ-005 / AC-009: host matching ignores casing, path and port, while request
// construction always targets the single fixed endpoint.
#[test]
fn resolves_quota_request_for_commandcode_host_only() {
    for base_url in [
        "https://api.commandcode.ai/provider/v1",
        "https://API.CommandCode.AI:8443/a/path",
    ] {
        let request = resolve_quota_request(&quota_provider(SECRET_KEY, base_url))
            .expect("CommandCode host must be accepted");
        assert_eq!(request.url, COMMANDCODE_QUOTA_URL);
        assert_eq!(request.api_key, SECRET_KEY);
    }

    for (api_key, base_url) in [
        ("   ", "https://api.commandcode.ai/v1"),
        (SECRET_KEY, "https://api.openai.com/v1"),
        (SECRET_KEY, "  "),
        (SECRET_KEY, "not a url"),
    ] {
        let error = resolve_quota_request(&quota_provider(api_key, base_url))
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
    let config = config_with_provider(quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
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
        let config = config_with_provider(quota_provider(
            SECRET_KEY,
            "https://api.commandcode.ai/provider/v1",
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
    let config = config_with_provider(quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
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
    let config = config_with_provider(quota_provider(
        SECRET_KEY,
        "https://api.commandcode.ai/provider/v1",
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
