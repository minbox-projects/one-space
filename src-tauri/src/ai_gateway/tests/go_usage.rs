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
fn resolve_go_usage_request_follows_the_effective_opencode_go_policy() {
    let bound = config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://any.example.com/v1",
        "opencode-go",
    ));
    let request = resolve_go_usage_request(&bound, &bound.providers[0])
        .expect("a Go-bound provider must be accepted");
    assert_eq!(request.url, GO_USAGE_URL);
    assert_eq!(request.api_key, SECRET_KEY);

    for provider in [
        // An unbound Go-looking URL never resolves the policy.
        go_provider(SECRET_KEY, "https://opencode.ai/zen/go"),
        // A blank key on a bound provider is still rejected.
        bound_go_provider("", "https://opencode.ai/zen/go", "opencode-go"),
    ] {
        let config = config_with_provider(provider);
        let error = resolve_go_usage_request(&config, &config.providers[0])
            .expect_err("invalid Go usage request must be rejected");
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
    let config = config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/go",
        "opencode-go",
    ));
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
        let config = config_with_provider(bound_go_provider(
            SECRET_KEY,
            "https://opencode.ai/zen/go",
            "opencode-go",
        ));
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
    let config = config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/go",
        "opencode-go",
    ));
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
    crate::ai_gateway::storage::write_config(&config_with_provider(bound_go_provider(
        "",
        "https://opencode.ai/zen/go",
        "opencode-go",
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
            "template_id": "opencode-go",
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

// ---------------------------------------------------------------------------
// REQ-006 (20260925-gateway-routing-and-config-hardening): the Go-usage source
// key prefers the first enabled *unmarked* key, then the first enabled key, then
// the first stored key.
// ---------------------------------------------------------------------------

/// A runtime-marked key pool entry for the Go-usage fixtures.
fn go_pool_key_marked(
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
async fn ac028_go_usage_source_prefers_the_first_enabled_unmarked_key() {
    let _home = super::isolated_temp_home("go-usage-ac028");
    write_go_pool(vec![
        go_pool_key_marked("key-marked", "sk-marked", true, "quota", 1_700_000_000),
        go_pool_key("key-unmarked", "sk-unmarked", true),
    ]);

    let calls = Arc::new(AtomicUsize::new(0));
    let fetch_calls = Arc::clone(&calls);
    let result = ai_gateway_provider_go_usage_with(
        "go-pool".to_string(),
        None,
        1_000,
        &Mutex::new(GoUsageCache::new()),
        move |url, key| {
            fetch_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(url, GO_USAGE_URL);
                assert_eq!(
                    key, "sk-unmarked",
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
async fn ac029_go_usage_source_falls_back_to_the_first_enabled_key_when_all_are_marked() {
    let _home = super::isolated_temp_home("go-usage-ac029");
    write_go_pool(vec![
        go_pool_key_marked("key-a", "sk-first", true, "quota", 1_700_000_000),
        go_pool_key_marked("key-b", "sk-second", true, "authentication", 1_700_000_100),
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
                    key, "sk-first",
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

/// AC-028 / AC-031 / REQ-006: the Go-usage resolver follows the ladder and its
/// cache still invalidates when a mark change makes a different source key the
/// first enabled unmarked one.
#[tokio::test]
async fn ac031_go_usage_source_cache_invalidates_on_a_mark_driven_source_change() {
    let _home = super::isolated_temp_home("go-usage-ac031");
    write_go_pool(vec![
        go_pool_key("key-a", "sk-first", true),
        go_pool_key("key-b", "sk-second", true),
    ]);

    let cache = Mutex::new(GoUsageCache::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let first_calls = Arc::clone(&calls);
    ai_gateway_provider_go_usage_with(
        "go-pool".to_string(),
        None,
        1_000,
        &cache,
        move |_, key| {
            first_calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(key, "sk-first");
                Ok(FIXTURE.to_string())
            }
        },
    )
    .await
    .expect("the first enabled unmarked key resolves the source");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Marking the cached source pushes the resolver to the second enabled key;
    // the cache entry must not be reused because its source value changed.
    write_go_pool(vec![
        go_pool_key_marked("key-a", "sk-first", true, "quota", 1_700_000_000),
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
                assert_eq!(
                    key, "sk-second",
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
// AC-012, AC-013): OpenCode Go usage dispatch resolves from the effective
// template quota policy, never from the provider URL. An unbound Go-looking
// provider stays generic, a broken binding falls back to generic, a mismatched
// direct command is rejected before any fetch, and a policy/binding change does
// not reuse an incompatible monitor snapshot.
// ===========================================================================

fn bound_go_provider(api_key: &str, base_url: &str, template_id: &str) -> GatewayUpstreamProvider {
    let mut provider = go_provider(api_key, base_url);
    provider.template_id = Some(template_id.to_string());
    provider
}

/// AC-005 / AC-012 counterexample: an unbound provider whose URL looks like Go
/// must use the generic (none) policy and never fetch Go usage.
#[tokio::test]
async fn unbound_go_looking_provider_never_fetches_go_usage() {
    let config = config_with_provider(go_provider(SECRET_KEY, "https://opencode.ai/zen/go/v1"));
    let calls = Arc::new(AtomicUsize::new(0));
    let fetch_calls = Arc::clone(&calls);

    let result = provider_go_usage_with(
        &config,
        "go-provider",
        false,
        1_000,
        &Mutex::new(GoUsageCache::new()),
        move |_, _| {
            fetch_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await;

    assert!(
        result.is_err(),
        "an unbound provider must not receive Go usage from its URL"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "an unbound provider must be rejected before any fetch"
    );
}

/// AC-005 / AC-012: a provider bound to OpenCode Go queries the Go usage
/// endpoint even when its own base URL is not a Go endpoint.
#[tokio::test]
async fn provider_bound_to_opencode_go_uses_usage_regardless_of_base_url() {
    let config = config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://my-gateway.example.com/v1",
        "opencode-go",
    ));

    let result = provider_go_usage_with(
        &config,
        "go-provider",
        false,
        1_000,
        &Mutex::new(GoUsageCache::new()),
        |url, key| async move {
            assert_eq!(
                url, GO_USAGE_URL,
                "the policy must select the Go usage endpoint"
            );
            assert_eq!(key, SECRET_KEY);
            Ok(FIXTURE.to_string())
        },
    )
    .await
    .expect("a Go-bound provider must resolve the usage request");
    assert_eq!(result, expected_fixture());
}

/// AC-006: a broken template binding falls back to generic and rejects the
/// direct Go usage command before any fetch.
#[tokio::test]
async fn broken_template_binding_rejects_go_usage_before_fetch() {
    let config = config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/go/v1",
        "missing-template",
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let fetch_calls = Arc::clone(&calls);

    let result = provider_go_usage_with(
        &config,
        "go-provider",
        false,
        1_000,
        &Mutex::new(GoUsageCache::new()),
        move |_, _| {
            fetch_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await;

    assert!(
        result.is_err(),
        "a broken binding must reject the Go usage command"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a broken binding must be rejected before any fetch"
    );
}

/// AC-006 counterexample: a provider whose effective policy is CommandCode must
/// be rejected by the Go usage command before any fetch, even with a Go URL.
#[tokio::test]
async fn go_usage_command_rejects_a_commandcode_policy_before_fetch() {
    let config = config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/go/v1",
        "commandcode",
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let fetch_calls = Arc::clone(&calls);

    let result = provider_go_usage_with(
        &config,
        "go-provider",
        false,
        1_000,
        &Mutex::new(GoUsageCache::new()),
        move |_, _| {
            fetch_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        },
    )
    .await;

    assert!(
        result.is_err(),
        "a CommandCode-policy provider must be rejected by the Go usage command"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a strategy mismatch must be rejected before any fetch"
    );
}

/// AC-013: a successful Go usage snapshot is not reused after the provider's
/// binding changes, while the same-policy five-minute cache still works.
#[tokio::test]
async fn go_usage_cache_is_not_reused_across_a_binding_change() {
    let mut config = config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/go/v1",
        "opencode-go",
    ));
    let cache = Mutex::new(GoUsageCache::new());
    let calls = Arc::new(AtomicUsize::new(0));

    let first_calls = Arc::clone(&calls);
    let first =
        provider_go_usage_with(&config, "go-provider", false, 1_000, &cache, move |_, _| {
            first_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        })
        .await
        .expect("a Go-bound provider must fetch on a cache miss");
    assert_eq!(first, expected_fixture());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    provider_go_usage_with(&config, "go-provider", false, 1_001, &cache, |_, _| async {
        panic!("a same-policy fresh cache hit must not fetch")
    })
    .await
    .expect("a same-policy cache hit must be reused");

    config.providers[0].template_id = None;
    let second_calls = Arc::clone(&calls);
    let second =
        provider_go_usage_with(&config, "go-provider", false, 1_002, &cache, move |_, _| {
            second_calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(FIXTURE.to_string()) }
        })
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
// Step 5: matching in-flight Go-usage lookups share one network operation.
//
// The injected fetch closure is the only network boundary. These tests count
// actual closure invocations and compare returned snapshots; they do not
// inspect cache or flight-map internals. A semaphore parks a started fetch
// until the test has observed the concurrent contender's decision, so the
// assertions use operation counts rather than wall-clock ceilings.
// ---------------------------------------------------------------------------

struct GoUsageFlightControl {
    calls: AtomicUsize,
    arrived: tokio::sync::Notify,
    release: tokio::sync::Semaphore,
}

impl GoUsageFlightControl {
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
            .expect("Go usage flight release semaphore must stay open");
        permit.forget();
    }
}

fn go_usage_body(percent: f64) -> String {
    format!(
        r#"{{"usage":{{"rolling":{{"status":"ok","percent":{percent}}},"weekly":{{}},"monthly":{{}}}}}}"#
    )
}

fn spawn_go_usage_lookup(
    config: Arc<GatewayConfig>,
    cache: Arc<Mutex<GoUsageCache>>,
    control: Arc<GoUsageFlightControl>,
    barrier: Arc<tokio::sync::Barrier>,
    force_refresh: bool,
    now_ms: u64,
    outcome: Result<String, String>,
) -> tokio::task::JoinHandle<Result<ProviderGoUsage, String>> {
    tokio::spawn(async move {
        barrier.wait().await;
        provider_go_usage_with(
            config.as_ref(),
            "go-provider",
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
async fn concurrent_matching_cold_go_usage_lookups_share_one_network_fetch() {
    let config = Arc::new(config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/go",
        "opencode-go",
    )));
    let cache = Arc::new(Mutex::new(GoUsageCache::new()));
    let control = Arc::new(GoUsageFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let first = spawn_go_usage_lookup(
        Arc::clone(&config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(FIXTURE.to_string()),
    );
    let second = spawn_go_usage_lookup(
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

    let first = first.await.expect("first Go usage task must not panic");
    let second = second.await.expect("second Go usage task must not panic");
    let first = first.expect("first concurrent Go usage lookup must succeed");
    let second = second.expect("second concurrent Go usage lookup must succeed");
    assert_eq!(first, expected_fixture());
    assert_eq!(second, expected_fixture());
    assert_eq!(
        observed, 1,
        "two concurrent matching cold Go usage lookups must invoke the network fetch once, observed {observed}"
    );
    assert_eq!(
        control.calls.load(Ordering::SeqCst),
        1,
        "no later fetch may run for the same settled identity"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_matching_forced_go_usage_refreshes_join_one_flight() {
    let config = Arc::new(config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/go",
        "opencode-go",
    )));
    let mut cache = GoUsageCache::new();
    cache.store(
        "go-provider",
        SECRET_KEY,
        "https://opencode.ai/zen/go",
        1_000,
        expected_fixture(),
    );
    let cache = Arc::new(Mutex::new(cache));
    let control = Arc::new(GoUsageFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let first = spawn_go_usage_lookup(
        Arc::clone(&config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        true,
        1_001,
        Ok(FIXTURE.to_string()),
    );
    let second = spawn_go_usage_lookup(
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

    let first = first
        .await
        .expect("first forced Go usage task must not panic");
    let second = second
        .await
        .expect("second forced Go usage task must not panic");
    assert_eq!(
        first.expect("first forced Go usage refresh must succeed"),
        expected_fixture()
    );
    assert_eq!(
        second.expect("second forced Go usage refresh must succeed"),
        expected_fixture()
    );
    assert_eq!(
        observed, 1,
        "a forced Go usage refresh must bypass completed success but join a matching in-flight fetch, observed {observed}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_go_usage_lookups_with_distinct_base_urls_do_not_share() {
    let cache = Arc::new(Mutex::new(GoUsageCache::new()));
    let control = Arc::new(GoUsageFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let first_config = Arc::new(config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/first",
        "opencode-go",
    )));
    let second_config = Arc::new(config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/second",
        "opencode-go",
    )));

    let first = spawn_go_usage_lookup(
        Arc::clone(&first_config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(go_usage_body(1.0)),
    );
    let second = spawn_go_usage_lookup(
        Arc::clone(&second_config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(go_usage_body(2.0)),
    );

    control.arrived.notified().await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let observed = control.calls.load(Ordering::SeqCst);
    control.release.add_permits(8);

    let first = first
        .await
        .expect("first base-URL Go usage task must not panic")
        .expect("first base-URL Go usage lookup must succeed");
    let second = second
        .await
        .expect("second base-URL Go usage task must not panic")
        .expect("second base-URL Go usage lookup must succeed");
    assert_eq!(
        observed, 2,
        "different base URLs are different Go usage identities and must not share a fetch, observed {observed}"
    );
    assert_eq!(first.usage.rolling.percent, 1.0);
    assert_eq!(second.usage.rolling.percent, 2.0);
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_go_usage_lookups_with_distinct_keys_do_not_share() {
    let cache = Arc::new(Mutex::new(GoUsageCache::new()));
    let control = Arc::new(GoUsageFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let first_config = Arc::new(config_with_provider(bound_go_provider(
        "sk-first-go-source",
        "https://opencode.ai/zen/go",
        "opencode-go",
    )));
    let second_config = Arc::new(config_with_provider(bound_go_provider(
        "sk-second-go-source",
        "https://opencode.ai/zen/go",
        "opencode-go",
    )));

    let first = spawn_go_usage_lookup(
        Arc::clone(&first_config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(go_usage_body(3.0)),
    );
    let second = spawn_go_usage_lookup(
        Arc::clone(&second_config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Ok(go_usage_body(4.0)),
    );

    control.arrived.notified().await;
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    let observed = control.calls.load(Ordering::SeqCst);
    control.release.add_permits(8);

    let first = first
        .await
        .expect("first key Go usage task must not panic")
        .expect("first key Go usage lookup must succeed");
    let second = second
        .await
        .expect("second key Go usage task must not panic")
        .expect("second key Go usage lookup must succeed");
    assert_eq!(
        observed, 2,
        "different pinned keys are different Go usage identities and must not share a fetch, observed {observed}"
    );
    assert_eq!(first.usage.rolling.percent, 3.0);
    assert_eq!(second.usage.rolling.percent, 4.0);
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_go_usage_failure_settles_and_a_later_lookup_retries() {
    let config = Arc::new(config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/go",
        "opencode-go",
    )));
    let cache = Arc::new(Mutex::new(GoUsageCache::new()));
    let control = Arc::new(GoUsageFlightControl::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let first = spawn_go_usage_lookup(
        Arc::clone(&config),
        Arc::clone(&cache),
        Arc::clone(&control),
        Arc::clone(&barrier),
        false,
        1_000,
        Err("HTTP 503 Service Unavailable".to_string()),
    );
    let second = spawn_go_usage_lookup(
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
        .expect("first failed Go usage task must not panic")
        .is_err());
    assert!(second
        .await
        .expect("second failed Go usage task must not panic")
        .is_err());

    let retry_control = Arc::clone(&control);
    let retry = provider_go_usage_with(
        config.as_ref(),
        "go-provider",
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
    .expect("a settled failed Go usage flight must allow a later lookup to retry");
    assert_eq!(retry, expected_fixture());

    let total_calls = control.calls.load(Ordering::SeqCst);
    assert!(
        concurrent_calls == 1 && total_calls == 2,
        "one matching failed Go usage flight must settle and allow one retry, concurrent={concurrent_calls}, total={total_calls}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_go_usage_owner_does_not_strand_a_later_lookup() {
    let config = Arc::new(config_with_provider(bound_go_provider(
        SECRET_KEY,
        "https://opencode.ai/zen/go",
        "opencode-go",
    )));
    let cache = Arc::new(Mutex::new(GoUsageCache::new()));
    let control = Arc::new(GoUsageFlightControl::new());

    let owner_config = Arc::clone(&config);
    let owner_cache = Arc::clone(&cache);
    let owner_control = Arc::clone(&control);
    let owner = tokio::spawn(async move {
        provider_go_usage_with(
            owner_config.as_ref(),
            "go-provider",
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
    let retry = provider_go_usage_with(
        config.as_ref(),
        "go-provider",
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
        .expect("a cancelled Go usage flight must not strand a later lookup")
        .expect("the later Go usage lookup must retry successfully");
    assert_eq!(retry, expected_fixture());
    assert_eq!(
        control.calls.load(Ordering::SeqCst),
        2,
        "the cancelled Go usage fetch and one retry are the only network operations"
    );
}
