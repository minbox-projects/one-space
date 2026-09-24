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
    let mut provider = super::provider("go-provider");
    provider.api_key = api_key.to_string();
    provider.base_url = base_url.to_string();
    provider
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
