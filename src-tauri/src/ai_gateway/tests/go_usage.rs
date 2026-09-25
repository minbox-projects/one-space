//! Behavior tests for the OpenCode Go usage boundary.
//!
//! The injected fetch functions keep these tests deterministic and offline.

use crate::ai_gateway::go_usage::{
    ai_gateway_provider_go_usage_with, is_go_usage_cache_fresh, parse_go_usage,
    provider_go_usage_with, resolve_go_usage_request, GoUsage, GoUsageCache, GoUsageWindow,
    ProviderGoUsage, GO_USAGE_CACHE_TTL_MS, GO_USAGE_URL,
};
use crate::ai_gateway::{GatewayConfig, GatewayUpstreamProvider, UsageLogStore};
use serde_json::json;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const FIXTURE: &str = r#"{
    "usage": {
        "rolling": {"status": "ok", "percent": 18.5, "resetsAt": "2026-09-25T03:30:30.663Z"},
        "weekly": {"status": "rate-limited", "percent": 100, "resetsAt": 1758600000000},
        "monthly": {"percent": 0.25, "unrecognized": true}
    },
    "unrecognizedField": true
}"#;

const SECRET_KEY: &str = "sk-secret-go-usage-key";

fn go_provider(api_key: &str, base_url: &str) -> GatewayUpstreamProvider {
    // Raw JSON carries the legacy single credential (current behavior) and the
    // ordered pool the provider type will adopt, so the same fixture serves both.
    serde_json::from_value(json!({
        "id": "go-provider",
        "name": "Provider go-provider",
        "base_url": base_url,
        "api_key": api_key,
        "keys": [super::pool_key("key-default", "Default", api_key, true)],
        "protocol": "chat_completions",
        "enabled": true,
    }))
    .expect("go-usage provider fixture must deserialize")
}

fn config_with_provider(provider: GatewayUpstreamProvider) -> GatewayConfig {
    let mut config = GatewayConfig::default();
    config.providers.push(provider);
    config
}

fn expected_fixture() -> ProviderGoUsage {
    ProviderGoUsage {
        usage: GoUsage {
            rolling: GoUsageWindow {
                status: "ok".to_string(),
                percent: 18.5,
                resets_at: Some(json!("2026-09-25T03:30:30.663Z")),
            },
            weekly: GoUsageWindow {
                status: "rate-limited".to_string(),
                percent: 100.0,
                resets_at: Some(json!(1758600000000_u64)),
            },
            monthly: GoUsageWindow {
                status: "ok".to_string(),
                percent: 0.25,
                resets_at: None,
            },
        },
    }
}

#[test]
fn go_usage_endpoint_and_cache_ttl_are_fixed() {
    assert_eq!(GO_USAGE_URL, "https://opencode.ai/zen/go/v1/usage");
    assert_eq!(GO_USAGE_CACHE_TTL_MS, 300_000);
}

#[test]
fn parses_go_usage_fixture_and_keeps_reset_values() {
    assert_eq!(parse_go_usage(FIXTURE).expect("fixture parses"), expected_fixture());
}

#[test]
fn missing_usage_is_rejected_and_missing_or_null_window_values_default() {
    let error = parse_go_usage(r#"{"other":true}"#).expect_err("usage is required");
    assert!(error.contains("usage"));

    let parsed = parse_go_usage(
        r#"{"usage":{"rolling":{"percent":null},"weekly":{},"monthly":{"status":"ok","percent":2}}}"#,
    )
    .expect("null and missing percent values default");
    assert_eq!(parsed.usage.rolling.percent, 0.0);
    assert_eq!(parsed.usage.rolling.status, "ok");
    assert_eq!(parsed.usage.rolling.resets_at, None);
    assert_eq!(parsed.usage.weekly.percent, 0.0);
    assert_eq!(parsed.usage.weekly.status, "ok");
}

#[test]
fn resolves_only_opencode_go_endpoints_without_leaking_credentials() {
    for base_url in [
        "https://opencode.ai/zen/go",
        "https://OPENCODE.AI:8443/a/Zen/Go/custom",
    ] {
        let request = resolve_go_usage_request(&go_provider(SECRET_KEY, base_url))
            .expect("OpenCode Go endpoint accepted");
        assert_eq!(request.url, GO_USAGE_URL);
        assert_eq!(request.api_key, SECRET_KEY);
    }

    for (api_key, base_url, expected_error) in [
        ("", "https://opencode.ai/zen/go", "no API key configured for this provider"),
        (SECRET_KEY, "  ", "provider base URL is blank"),
        (SECRET_KEY, "not a url", "provider base URL is invalid"),
        (SECRET_KEY, "https://example.com/zen/go", "provider is not an OpenCode Go endpoint"),
        (SECRET_KEY, "https://opencode.ai/zen/v1", "provider is not an OpenCode Go endpoint"),
    ] {
        let error = resolve_go_usage_request(&go_provider(api_key, base_url))
            .expect_err("invalid OpenCode Go endpoint must be rejected");
        assert_eq!(error, expected_error);
        assert!(!error.contains(SECRET_KEY), "error leaked the API key: {error}");
    }
}

#[test]
fn cache_freshness_ends_at_ttl_and_tracks_credentials_and_base_url() {
    let cached_at = 10_000;
    assert!(is_go_usage_cache_fresh(cached_at, cached_at));
    assert!(is_go_usage_cache_fresh(cached_at, cached_at + GO_USAGE_CACHE_TTL_MS - 1));
    assert!(!is_go_usage_cache_fresh(cached_at, cached_at + GO_USAGE_CACHE_TTL_MS));

    let snapshot = expected_fixture();
    let mut cache = GoUsageCache::new();
    cache.store("provider", SECRET_KEY, "https://opencode.ai/zen/go", 100, snapshot.clone());
    assert_eq!(cache.get_fresh("provider", SECRET_KEY, "https://opencode.ai/zen/go", 101, false), Some(snapshot.clone()));
    assert_eq!(cache.get_fresh("provider", SECRET_KEY, "https://opencode.ai/zen/go", 101, true), None);
    assert_eq!(cache.get_fresh("provider", "rotated", "https://opencode.ai/zen/go", 101, false), None);
    assert_eq!(cache.get_fresh("provider", SECRET_KEY, "https://opencode.ai/zen/v1", 101, false), None);
    assert_eq!(cache.get_fresh("provider", SECRET_KEY, "https://opencode.ai/zen/go", 100 + GO_USAGE_CACHE_TTL_MS, false), None);
}

#[tokio::test]
async fn provider_fetches_on_miss_caches_success_and_forces_refresh() {
    let config = config_with_provider(go_provider(SECRET_KEY, "https://opencode.ai/zen/go"));
    let cache = Mutex::new(GoUsageCache::new());
    let calls = Arc::new(AtomicUsize::new(0));

    let fetch_calls = Arc::clone(&calls);
    let result = provider_go_usage_with(&config, "go-provider", false, 1000, &cache, move |url, key| {
        fetch_calls.fetch_add(1, Ordering::SeqCst);
        async move {
            assert_eq!(url, GO_USAGE_URL);
            assert_eq!(key, SECRET_KEY);
            Ok(FIXTURE.to_string())
        }
    })
    .await
    .expect("cache miss fetch succeeds");
    assert_eq!(result, expected_fixture());

    let hit = provider_go_usage_with(&config, "go-provider", false, 1001, &cache, |_, _| async {
        panic!("fresh cache hit must not fetch")
    })
    .await
    .expect("cache hit succeeds");
    assert_eq!(hit, expected_fixture());

    let forced_calls = Arc::clone(&calls);
    provider_go_usage_with(&config, "go-provider", true, 1002, &cache, move |_, _| {
        forced_calls.fetch_add(1, Ordering::SeqCst);
        async { Ok(FIXTURE.to_string()) }
    })
    .await
    .expect("forced refresh succeeds");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn failed_fetch_and_parse_are_not_cached() {
    for response in [Err("upstream HTTP 503".to_string()), Ok(r#"{"other":true}"#.to_string())] {
        let config = config_with_provider(go_provider(SECRET_KEY, "https://opencode.ai/zen/go"));
        let cache = Mutex::new(GoUsageCache::new());
        let calls = AtomicUsize::new(0);
        for _ in 0..2 {
            let response = response.clone();
            let error = provider_go_usage_with(&config, "go-provider", false, 2000, &cache, |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                async move { response }
            })
            .await
            .expect_err("failed fetch or parse propagates");
            assert!(!error.contains(SECRET_KEY), "error leaked the API key: {error}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2, "failures must not be cached");
    }
}

#[tokio::test]
async fn storage_command_path_reuses_cache_and_does_not_write_config_or_usage_logs() {
    let _home = super::isolated_temp_home("go-usage-read-only");
    let config = config_with_provider(go_provider(SECRET_KEY, "https://opencode.ai/zen/go"));
    crate::ai_gateway::storage::write_config(&config).expect("write initial gateway config");
    let config_file = crate::ai_gateway::storage::config_path().expect("gateway config path");
    let bytes_before = fs::read(&config_file).expect("read config bytes");
    let store = UsageLogStore::default_store().expect("default usage store");
    let rows_before = store.count().expect("count usage rows");

    let cache = Mutex::new(GoUsageCache::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let first_calls = Arc::clone(&calls);
    let first = ai_gateway_provider_go_usage_with("go-provider".to_string(), None, 4000, &cache, move |url, key| {
        first_calls.fetch_add(1, Ordering::SeqCst);
        async move {
            assert_eq!(url, GO_USAGE_URL);
            assert_eq!(key, SECRET_KEY);
            Ok(FIXTURE.to_string())
        }
    })
    .await
    .expect("storage-backed usage query succeeds");
    assert_eq!(first, expected_fixture());

    let cached_calls = Arc::clone(&calls);
    ai_gateway_provider_go_usage_with("go-provider".to_string(), None, 4001, &cache, move |_, _| {
        cached_calls.fetch_add(1, Ordering::SeqCst);
        async { Err("cache should prevent fetch".to_string()) }
    })
    .await
    .expect("storage command seam reuses cache");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let bytes_after = fs::read(&config_file).expect("read config after query");
    assert_eq!(bytes_after, bytes_before, "usage query must not write config");
    assert_eq!(store.count().expect("count rows after query"), rows_before, "usage query must not append usage logs");
}

#[tokio::test]
async fn storage_command_path_rejects_empty_key_before_fetching() {
    let _home = super::isolated_temp_home("go-usage-empty-key");
    crate::ai_gateway::storage::write_config(&config_with_provider(go_provider(
        "",
        "https://opencode.ai/zen/go",
    )))
    .expect("write empty-key gateway config");
    let calls = AtomicUsize::new(0);
    let error = ai_gateway_provider_go_usage_with(
        "go-provider".to_string(),
        None,
        5000,
        &Mutex::new(GoUsageCache::new()),
        |_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await
    .expect_err("empty API key must be rejected");
    assert_eq!(error, "no API key configured for this provider");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

// ---------------------------------------------------------------------------
// Pinned Go-usage source over the ordered key pool (REQ-010)
//
// Raw schema-2 fixtures keep the key-pool contract while the typed provider has
// no key pool yet; the storage command seam observes which key was resolved.
// ---------------------------------------------------------------------------

fn go_pool_key(id: &str, value: &str, enabled: bool) -> serde_json::Value {
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

fn write_go_pool(keys: Vec<serde_json::Value>) {
    super::write_raw_gateway_config(&json!({
        "schema_version": 2,
        "providers": [{
            "id": "go-pool",
            "name": "Go Pool",
            "base_url": "https://opencode.ai/zen/go",
            "protocol": "chat_completions",
            "keys": keys,
            "mappings": [],
        }],
    }));
}

/// REQ-010: the Go-usage block always queries the first enabled key in list
/// order and invalidates its cache when that key changes.
#[tokio::test]
async fn go_usage_source_is_first_enabled_key_and_invalidates_on_key_change() {
    let _home = super::isolated_temp_home("go-usage-pool-source");
    write_go_pool(vec![
        go_pool_key("key-disabled", "sk-disabled", false),
        go_pool_key("key-a", "sk-first", true),
        go_pool_key("key-b", "sk-second", true),
    ]);

    let cache = Mutex::new(GoUsageCache::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let first_calls = Arc::clone(&calls);
    let first = ai_gateway_provider_go_usage_with(
        "go-pool".to_string(),
        None,
        1_000,
        &cache,
        move |url, key| {
            first_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(url, GO_USAGE_URL);
                assert_eq!(key, "sk-first", "the first enabled key must be the source");
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("the first enabled key must resolve the Go-usage request");
    assert_eq!(first, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    write_go_pool(vec![
        go_pool_key("key-a", "sk-first", false),
        go_pool_key("key-b", "sk-second", true),
    ]);
    let second_calls = Arc::clone(&calls);
    ai_gateway_provider_go_usage_with(
        "go-pool".to_string(),
        None,
        1_001,
        &cache,
        move |_, key| {
            second_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(key, "sk-second", "the new first enabled key must be queried");
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

/// REQ-010 boundary: with every key disabled the first key in list order stays
/// the pinned Go-usage source.
#[tokio::test]
async fn go_usage_source_uses_first_key_when_all_are_disabled() {
    let _home = super::isolated_temp_home("go-usage-pool-all-disabled");
    write_go_pool(vec![
        go_pool_key("key-a", "sk-first-disabled", false),
        go_pool_key("key-b", "sk-second-disabled", false),
    ]);

    let calls = AtomicUsize::new(0);
    ai_gateway_provider_go_usage_with(
        "go-pool".to_string(),
        None,
        1_000,
        &Mutex::new(GoUsageCache::new()),
        |_, key| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(
                    key, "sk-first-disabled",
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
async fn go_usage_source_empty_pool_keeps_the_no_key_error() {
    let _home = super::isolated_temp_home("go-usage-pool-empty");
    write_go_pool(vec![]);

    let calls = AtomicUsize::new(0);
    let error = ai_gateway_provider_go_usage_with(
        "go-pool".to_string(),
        None,
        1_000,
        &Mutex::new(GoUsageCache::new()),
        |_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await
    .expect_err("an empty key pool must fail before fetching");
    assert_eq!(error, "no API key configured for this provider");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
