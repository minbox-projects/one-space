//! Behavior tests for the explicit free-model pool boundary (Step 1 of
//! 20261013-free-model-pool: REQ-001/002/003/006/007/009).
//!
//! Every test drives only already-compiled entry points: the encrypted
//! configuration boundary (`write_raw_gateway_config`, `storage::read_config`,
//! `storage::modify_config`), the real loopback listener (`start_server` plus
//! `call_gateway`) and in-process mock upstreams (`spawn_mock_upstream`). Pool
//! records are supplied as raw JSON fixtures so a RED is a compiled, observable
//! failure — missing persistence, a 502 instead of a routed request, or an
//! unrelated ordinary default serving a pool alias — rather than a missing Rust
//! symbol. No internal collaborator is mocked.
//!
//! Chosen serialized fixture names (to be aligned with the Backend DTO):
//! top-level `model_pools` (array) and `model_pool_reserved_models` (string
//! array); each pool is `{id, name, model, enabled, candidates: [...]}` and each
//! candidate is `{provider_id, local_model, upstream_model, protocol, weight?}`,
//! where an omitted `weight` must default to 1.

use super::{
    call_gateway, default_usage_store, free_port, key_named, pool_key, pool_provider,
    read_raw_gateway_config, spawn_mock_upstream, temp_home, wait_for_usage_logs,
    write_raw_gateway_config, Captured, CapturedLog, MockReply,
};
use crate::ai_gateway::commands::{
    ai_gateway_delete_model_pool, ai_gateway_model_pools_status, ai_gateway_upsert_model_pool,
};
use crate::ai_gateway::model_pools::{cool_pool_candidate, plan_pool_request, PoolPlan};
use crate::ai_gateway::reasoning::{PoolOutputCap, PoolReasoningRequest};
use crate::ai_gateway::runtime_http::{start_server, stop_server};
use crate::ai_gateway::{
    GatewayConfig, GatewayUpstreamProvider, ModelPool, ModelPoolCandidate, PoolCandidateIdentity,
    PoolDefaultReasoningEffort, UpstreamProtocol, GATEWAY_CONFIG_SCHEMA_VERSION, UsageResult,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Unique suffix per test so the process-global weighted scheduler and
/// session-affinity table never carry another test's provider/alias identity.
fn tag() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// A schema-3 raw configuration that carries the frozen plan's pool fields plus
/// one local relay key, bypassing the typed `GatewayConfig` so unknown pool
/// fields survive the raw fixture round trip at baseline.
fn pool_config_v3(port: u16, providers: Vec<Value>, pools: Vec<Value>) -> Value {
    json!({
        "schema_version": GATEWAY_CONFIG_SCHEMA_VERSION,
        "port": port,
        "keys": [{
            "id": "k-local",
            "label": "k-local",
            "value": "local-key",
            "enabled": true,
            "created_at": 1,
        }],
        "providers": providers,
        "model_pools": pools,
        "model_pool_reserved_models": [],
    })
}

/// One raw mapping row carrying the trimmed local/upstream pair and effective
/// protocol a pool candidate references.
fn provider_mapping(local_model: &str, upstream_model: &str, protocol: &str) -> Value {
    json!({
        "local_model": local_model,
        "upstream_model": upstream_model,
        "protocol": protocol,
        "enabled": true,
    })
}

/// One raw pool candidate; `weight` is omitted when `None` to exercise the
/// documented default of 1.
fn pool_candidate(
    provider_id: &str,
    local_model: &str,
    upstream_model: &str,
    protocol: &str,
    weight: Option<u32>,
) -> Value {
    let mut candidate = json!({
        "provider_id": provider_id,
        "local_model": local_model,
        "upstream_model": upstream_model,
        "protocol": protocol,
    });
    if let Some(weight) = weight {
        candidate["weight"] = json!(weight);
    }
    candidate
}

/// The `model` value the relay forwarded to the mock upstream for one capture.
fn captured_model(captured: &Captured) -> String {
    serde_json::from_slice::<Value>(&captured.body)
        .ok()
        .and_then(|value| {
            value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn models_of(captured: &CapturedLog) -> Vec<String> {
    captured
        .lock()
        .expect("mock log")
        .iter()
        .map(captured_model)
        .collect()
}

/// One chat request for `alias` carrying a stable session header, so every
/// request already exercises the "identical session headers must not pin a pool
/// request" boundary.
async fn post_chat(port: u16, alias: &str) -> (u16, String, String) {
    call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-opencode-session", "sess-same"),
        ],
        Some(json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
        })),
    )
    .await
}

// ---------------------------------------------------------------------------
// REQ-001 / AC-001: encrypted persistence of the exact source boundary
// ---------------------------------------------------------------------------

/// AC-001: two different mappings from the same provider persist with their
/// exact provider/local/upstream/protocol identity and weights through the
/// encrypted profile file, an omitted weight persists as the default 1, and no
/// source key or endpoint is copied into the pool record.
#[test]
fn pool_candidates_persist_exact_source_tuples_without_copying_secrets() {
    super::with_temp_home("pool-persist-exact-tuples", |_home| {
        let provider_a = pool_provider(
            "p-a",
            "Provider A",
            "https://provider-a.example.invalid/v1",
            vec![pool_key("key-a", "Key A", "sk-secret-a", true)],
            vec![
                provider_mapping("local-a1", "remote-a1", "chat_completions"),
                provider_mapping("local-a2", "remote-a2", "responses"),
            ],
        );
        let provider_b = pool_provider(
            "p-b",
            "Provider B",
            "https://provider-b.example.invalid/v1",
            vec![pool_key("key-b", "Key B", "sk-secret-b", true)],
            vec![provider_mapping("local-b1", "remote-b1", "chat_completions")],
        );
        let pool = json!({
            "id": "pool-free",
            "name": "Free Modle",
            "model": "free-model",
            "enabled": true,
            "candidates": [
                pool_candidate("p-a", "local-a1", "remote-a1", "chat_completions", Some(3)),
                pool_candidate("p-a", "local-a2", "remote-a2", "responses", None),
            ],
        });
        write_raw_gateway_config(&pool_config_v3(0, vec![provider_a, provider_b], vec![pool]));

        // Force an ordinary typed configuration write, exercising the encrypted
        // serialization path the operator save uses.
        crate::ai_gateway::storage::modify_config::<()>(|config| {
            config.keys.push(key_named("k-extra", "extra-value"));
            Ok((true, ()))
        })
        .expect("the configuration write must succeed");

        let raw = read_raw_gateway_config();
        let providers = raw["providers"]
            .as_array()
            .expect("existing providers must persist");
        assert!(providers.iter().any(|p| p["id"] == "p-a"));
        assert!(providers.iter().any(|p| p["id"] == "p-b"));

        let pools = raw
            .get("model_pools")
            .and_then(Value::as_array)
            .expect("the pool collection must persist through the encrypted configuration");
        assert_eq!(pools.len(), 1, "exactly one pool must persist");
        let pool = &pools[0];
        assert_eq!(pool["id"], "pool-free");
        assert_eq!(pool["name"], "Free Modle");
        assert_eq!(pool["model"], "free-model");
        assert_eq!(pool["enabled"], true);

        let candidates = pool["candidates"].as_array().expect("candidates must persist");
        assert_eq!(candidates.len(), 2, "both same-provider mappings must persist");
        assert_eq!(candidates[0]["provider_id"], "p-a");
        assert_eq!(candidates[0]["local_model"], "local-a1");
        assert_eq!(candidates[0]["upstream_model"], "remote-a1");
        assert_eq!(candidates[0]["protocol"], "chat_completions");
        assert_eq!(candidates[0]["weight"], 3);
        assert_eq!(candidates[1]["provider_id"], "p-a");
        assert_eq!(candidates[1]["local_model"], "local-a2");
        assert_eq!(candidates[1]["upstream_model"], "remote-a2");
        assert_eq!(candidates[1]["protocol"], "responses");
        assert_eq!(
            candidates[1]["weight"], 1,
            "an omitted candidate weight must persist as the default 1"
        );

        let serialized = serde_json::to_string(pool).expect("serialize pool");
        assert!(
            !serialized.contains("sk-"),
            "the pool record must not copy a source key: {serialized}"
        );
        assert!(
            !serialized.contains("example.invalid"),
            "the pool record must not copy a source endpoint: {serialized}"
        );
        for candidate in candidates {
            assert!(
                candidate.get("base_url").is_none(),
                "a candidate must not carry an endpoint: {candidate}"
            );
            assert!(
                candidate.get("keys").is_none(),
                "a candidate must not carry source keys: {candidate}"
            );
        }
    });
}

/// AC-001: an existing configuration without any pool fields reads as an empty
/// pool collection and keeps its providers through the encrypted round trip.
#[test]
fn old_config_without_pool_fields_writes_an_empty_pool_collection() {
    super::with_temp_home("pool-old-config-absence", |_home| {
        let provider = pool_provider(
            "p-a",
            "Provider A",
            "https://provider-a.example.invalid/v1",
            vec![pool_key("key-a", "Key A", "sk-secret-a", true)],
            vec![provider_mapping("local-a1", "remote-a1", "chat_completions")],
        );
        // Deliberately no model_pools / model_pool_reserved_models fields.
        let legacy = json!({
            "schema_version": GATEWAY_CONFIG_SCHEMA_VERSION,
            "port": 0,
            "keys": [{
                "id": "k-local",
                "label": "k-local",
                "value": "local-key",
                "enabled": true,
                "created_at": 1,
            }],
            "providers": [provider],
        });
        write_raw_gateway_config(&legacy);

        crate::ai_gateway::storage::modify_config::<()>(|config| {
            config.keys.push(key_named("k-extra", "extra-value"));
            Ok((true, ()))
        })
        .expect("the configuration write must succeed");

        let raw = read_raw_gateway_config();
        let pools = raw.get("model_pools");
        assert!(
            pools.map(Value::is_array).unwrap_or(false),
            "a configuration without pool fields must read as an empty pool collection: {raw}"
        );
        assert!(
            pools.unwrap().as_array().unwrap().is_empty(),
            "the pool collection must be empty, not invented"
        );
        assert!(
            raw["providers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["id"] == "p-a"),
            "existing providers must survive the read/write round trip"
        );
    });
}

// ---------------------------------------------------------------------------
// REQ-002 / REQ-003 / AC-003 / AC-004: exact routing and isolated rotation
// ---------------------------------------------------------------------------

/// AC-003 / REQ-002: a pool alias routes only through its selected
/// same-provider mappings; the unselected sibling and the provider default are
/// never used, and the real provider credential reaches the upstream.
#[tokio::test]
async fn pool_routes_only_selected_same_provider_mappings() {
    let _home = temp_home("pool-exact-source-routing");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;

    let mut provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![
            provider_mapping("local-a", "remote-a", "chat_completions"),
            provider_mapping("local-b", "remote-b", "chat_completions"),
            provider_mapping("local-c", "remote-c", "chat_completions"),
        ],
    );
    provider["default_model"] = json!("remote-default");
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(provider_id.as_str(), "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    for _ in 0..2 {
        let (status, _content_type, text) = post_chat(port, &alias).await;
        assert_eq!(status, 200, "the pool alias must be served by a selected source: {text}");
    }

    let captured = log.lock().unwrap().clone();
    assert_eq!(
        captured.len(),
        2,
        "each successful request must reach exactly one selected source"
    );
    let mut models: Vec<String> = captured.iter().map(captured_model).collect();
    models.sort();
    assert_eq!(
        models,
        vec!["remote-a".to_string(), "remote-b".to_string()],
        "only the two selected same-provider mappings may serve, never the unselected sibling or the provider default"
    );
    for entry in &captured {
        assert_eq!(entry.path, "/v1/chat/completions");
        assert_eq!(
            entry.headers.get("authorization").map(String::as_str),
            Some("Bearer sk-upstream")
        );
    }

    stop_server().await.expect("stop the listener");
}

/// AC-004 / REQ-003: candidate weights drive per-request rotation, identical
/// session headers do not pin a pool request, and two pools over the same
/// provider keep independent scheduling state.
#[tokio::test]
async fn pool_rotation_is_isolated_per_pool_and_ignores_session_headers() {
    let _home = temp_home("pool-swrr-isolation");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias_a = format!("free-pool-a-{tag}");
    let alias_b = format!("free-pool-b-{tag}");
    let (provider_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![
            provider_mapping("local-a", "remote-a", "chat_completions"),
            provider_mapping("local-b", "remote-b", "chat_completions"),
        ],
    );
    let pool_a = json!({
        "id": format!("pool-a-{tag}"),
        "name": "Pool A",
        "model": alias_a,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "chat_completions", Some(3)),
            pool_candidate(provider_id.as_str(), "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    let pool_b = json!({
        "id": format!("pool-b-{tag}"),
        "name": "Pool B",
        "model": alias_b,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(provider_id.as_str(), "local-b", "remote-b", "chat_completions", Some(3)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool_a, pool_b]));
    start_server(None).await.expect("start the listener");

    for _ in 0..4 {
        let (status, _content_type, text) = post_chat(port, &alias_a).await;
        assert_eq!(status, 200, "pool A must serve: {text}");
    }
    for _ in 0..4 {
        let (status, _content_type, text) = post_chat(port, &alias_b).await;
        assert_eq!(status, 200, "pool B must serve: {text}");
    }

    let models = models_of(&log);
    assert_eq!(
        models.len(),
        8,
        "every request must reach exactly one selected candidate: {models:?}"
    );
    let pool_a_models = &models[0..4];
    let pool_b_models = &models[4..8];
    let count = |slice: &[String], target: &str| {
        slice.iter().filter(|model| model.as_str() == target).count()
    };
    assert_eq!(
        count(pool_a_models, "remote-a"),
        3,
        "pool A weights 3:1 must select remote-a three times per cycle: {pool_a_models:?}"
    );
    assert_eq!(
        count(pool_a_models, "remote-b"),
        1,
        "pool A weights 3:1 must select remote-b once per cycle: {pool_a_models:?}"
    );
    assert_eq!(
        count(pool_b_models, "remote-a"),
        1,
        "pool B must not inherit pool A's scheduling state: {pool_b_models:?}"
    );
    assert_eq!(
        count(pool_b_models, "remote-b"),
        3,
        "pool B weights 1:3 must select remote-b three times per cycle: {pool_b_models:?}"
    );

    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// REQ-006 / AC-008 / AC-009: scoped failure handling
// ---------------------------------------------------------------------------

/// AC-008 / REQ-006: a structured FreeUsageLimitError 429 cools only that
/// candidate, the sibling serves immediately, the shared key stays unmarked, and
/// a later request skips the cooled candidate.
#[tokio::test]
async fn candidate_free_limit_cools_only_that_candidate_and_leaves_shared_key_unmarked() {
    let _home = temp_home("pool-candidate-free-limit");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| {
        if captured_model(captured) == "remote-limited" {
            MockReply::Json(
                429,
                json!({
                    "error": {
                        "type": "FreeUsageLimitError",
                        "message": "free usage limit reached",
                    }
                }),
            )
        } else {
            MockReply::Json(200, json!({"id": "ok", "choices": []}))
        }
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            provider_mapping("local-limited", "remote-limited", "chat_completions"),
            provider_mapping("local-ok", "remote-ok", "chat_completions"),
        ],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-limited", "remote-limited", "chat_completions", Some(100)),
            pool_candidate(provider_id.as_str(), "local-ok", "remote-ok", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    for round in 0..2 {
        let (status, _content_type, text) = post_chat(port, &alias).await;
        assert_eq!(
            status, 200,
            "round {round}: the pool must fall back to the healthy sibling: {text}"
        );
    }

    let models = models_of(&log);
    let limited = models
        .iter()
        .filter(|model| model.as_str() == "remote-limited")
        .count();
    let ok = models
        .iter()
        .filter(|model| model.as_str() == "remote-ok")
        .count();
    assert_eq!(
        limited, 1,
        "the free-limited candidate must be attempted once and then cooled: {models:?}"
    );
    assert_eq!(
        ok, 2,
        "the healthy sibling must serve the fallback and the next request: {models:?}"
    );

    let stored = crate::ai_gateway::storage::read_config().expect("read config");
    let provider = stored
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .expect("the provider must persist");
    for key in &provider.keys {
        assert!(
            !key.auto_marked,
            "a candidate-scoped free limit must not mark the shared key: {key:?}"
        );
        assert_eq!(
            key.failure_kind, None,
            "the shared key must carry no failure kind"
        );
    }

    stop_server().await.expect("stop the listener");
}

/// AC-009 / REQ-006: a non-quota client 4xx from a selected candidate returns to
/// the caller without falling back to a sibling or the provider default.
#[tokio::test]
async fn pool_client_error_returns_to_caller_without_falling_back() {
    let _home = temp_home("pool-client-error-no-fallback");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| {
        if captured_model(captured) == "remote-bad" {
            MockReply::Json(400, json!({"error": {"message": "invalid request"}}))
        } else {
            MockReply::Json(200, json!({"id": "ok", "choices": []}))
        }
    })
    .await;

    let mut provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![
            provider_mapping("local-bad", "remote-bad", "chat_completions"),
            provider_mapping("local-ok", "remote-ok", "chat_completions"),
        ],
    );
    provider["default_model"] = json!("remote-default");
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-bad", "remote-bad", "chat_completions", Some(100)),
            pool_candidate(provider_id.as_str(), "local-ok", "remote-ok", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 400,
        "a non-quota client error must return to the caller, not fall back: {text}"
    );

    let models = models_of(&log);
    assert_eq!(
        models
            .iter()
            .filter(|model| model.as_str() == "remote-bad")
            .count(),
        1,
        "the failing candidate must be attempted once: {models:?}"
    );
    assert_eq!(
        models
            .iter()
            .filter(|model| model.as_str() == "remote-ok")
            .count(),
        0,
        "a client error must not fall back to a sibling: {models:?}"
    );

    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// REQ-007 / AC-010: alias exclusion and the native Messages boundary
// ---------------------------------------------------------------------------

/// AC-010 / REQ-007: a disabled pool alias and a retired reserved alias fail
/// closed with the standard all-providers-unavailable envelope and never reach
/// an unrelated ordinary provider default.
#[tokio::test]
async fn disabled_and_retired_pool_aliases_fail_closed() {
    let _home = temp_home("pool-alias-exclusion");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let disabled_alias = format!("free-model-{tag}");
    let retired_alias = format!("free-model-retired-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(200, json!({"id": "ordinary", "choices": []}))
    })
    .await;

    let mut provider = pool_provider(
        &provider_id,
        "Ordinary Provider",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![],
    );
    provider["default_model"] = json!("remote-default");

    let disabled_pool = json!({
        "id": format!("pool-disabled-{tag}"),
        "name": "Free Modle",
        "model": disabled_alias,
        "enabled": false,
        "candidates": [],
    });
    let mut config = pool_config_v3(port, vec![provider], vec![disabled_pool]);
    config["model_pool_reserved_models"] = json!([retired_alias]);

    write_raw_gateway_config(&config);
    start_server(None).await.expect("start the listener");

    for alias in [&disabled_alias, &retired_alias] {
        let (status, _content_type, text) = post_chat(port, alias).await;
        assert_eq!(
            status, 502,
            "alias {alias} must fail closed instead of using an unrelated default: {text}"
        );
        let body: Value = serde_json::from_str(&text).expect("an OpenAI error envelope");
        assert_eq!(
            body["error"]["code"], "all_providers_unavailable",
            "alias {alias} envelope: {text}"
        );
        assert!(
            body["error"]["message"]
                .as_str()
                .is_some_and(|message| !message.trim().is_empty()),
            "alias {alias} must carry a nonempty reason: {text}"
        );
    }
    assert_eq!(
        log.lock().unwrap().len(),
        0,
        "an excluded alias must never reach an ordinary default"
    );

    stop_server().await.expect("stop the listener");
}

/// AC-010 / REQ-007: a native Messages request for a pool alias returns the
/// existing Anthropic error envelope and makes no ordinary-route attempt, even
/// when an ordinary native Anthropic provider declares a usable default.
#[tokio::test]
async fn native_messages_pool_alias_returns_anthropic_envelope() {
    let _home = temp_home("pool-native-messages");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let (provider_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "content": []}))).await;

    let anthropic_id = format!("p-anth-{tag}");
    let mut anthropic = pool_provider(
        &anthropic_id,
        "Anthropic Provider",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-anth", true)],
        vec![],
    );
    anthropic["protocol"] = json!("anthropic_messages");
    anthropic["default_model"] = json!("claude-default");

    let openai_id = format!("p-openai-{tag}");
    let openai = pool_provider(
        &openai_id,
        "OpenAI Provider",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-openai", true)],
        vec![provider_mapping("local-free", "remote-free", "chat_completions")],
    );

    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(openai_id.as_str(), "local-free", "remote-free", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![anthropic, openai], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/messages",
        &[("x-api-key", "local-key")],
        Some(json!({
            "model": alias,
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hi"}],
        })),
    )
    .await;
    assert_eq!(
        status, 502,
        "a native Messages request for a pool alias must fail closed: {text}"
    );
    let body: Value = serde_json::from_str(&text).expect("an Anthropic error envelope");
    assert_eq!(
        body["type"], "error",
        "the existing Anthropic envelope must be used: {text}"
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.trim().is_empty()),
        "the Anthropic envelope must carry a nonempty reason: {text}"
    );
    assert_eq!(
        log.lock().unwrap().len(),
        0,
        "no ordinary Anthropic attempt may serve the pool alias"
    );

    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// REQ-009 / AC-012: alias plus real-source attempt accounting
// ---------------------------------------------------------------------------

/// AC-012 / REQ-009: a pool request logs the public alias as `local_model` and
/// the real selected provider id and upstream model for the attempt.
#[tokio::test]
async fn pool_attempt_log_records_alias_and_real_source() {
    let _home = temp_home("pool-usage-identity");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, _log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(status, 200, "the single-candidate pool must serve: {text}");

    let records = wait_for_usage_logs(1).await;
    let terminal = records
        .iter()
        .find(|record| record.terminal)
        .expect("a terminal usage row must exist");
    assert_eq!(
        terminal.local_model, alias,
        "the public alias must be logged as local_model: {terminal:?}"
    );
    assert_eq!(
        terminal.provider_id, provider_id,
        "the real provider id must be logged: {terminal:?}"
    );
    assert_eq!(
        terminal.upstream_model, "remote-a",
        "the real upstream model must be logged: {terminal:?}"
    );
    assert_eq!(terminal.status, 200);
    assert_eq!(terminal.result, UsageResult::Success);

    stop_server().await.expect("stop the listener");
}

// ===========================================================================
// Review-repair regressions (2026-10-13)
//
// The primary inspected the current Step 1 implementation and reported defects
// on supported paths. These tests drive only the public listener boundary, the
// persisted provider key/mapping state and the public pool status command; they
// are written first so a RED is a compiled, observable failure rather than a
// missing symbol. No private scheduler/cooldown assertion is made.
// ===========================================================================

/// The `authorization` header one captured upstream request carried.
fn auth_of(captured: &Captured) -> Option<&str> {
    captured.headers.get("authorization").map(String::as_str)
}

/// One persisted provider from the encrypted configuration.
fn stored_provider(id: &str) -> GatewayUpstreamProvider {
    crate::ai_gateway::storage::read_config()
        .expect("read persisted config")
        .providers
        .into_iter()
        .find(|provider| provider.id == id)
        .expect("the provider must persist")
}

/// A minimal native Anthropic Messages JSON success body, no tools.
fn messages_success_body() -> Value {
    json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "upstream",
        "content": [{"type": "text", "text": "ok"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 1, "output_tokens": 1},
    })
}

/// One Chat ingress request for `alias` carrying `marker` as the user text and
/// the same session header every pool request already sends.
async fn post_chat_marker(port: u16, alias: &str, marker: &str) -> (u16, String) {
    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-opencode-session", "sess-same"),
        ],
        Some(json!({
            "model": alias,
            "messages": [{"role": "user", "content": marker}],
        })),
    )
    .await;
    (status, text)
}

/// One Responses ingress request for `alias` carrying `marker` as the input.
async fn post_responses_marker(port: u16, alias: &str, marker: &str) -> (u16, String) {
    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[
            ("authorization", "Bearer local-key"),
            ("x-opencode-session", "sess-same"),
        ],
        Some(json!({
            "model": alias,
            "input": [{"role": "user", "content": [{"type": "input_text", "text": marker}]}],
        })),
    )
    .await;
    (status, text)
}

/// Compatibility / REQ-006: an ordinary (non-pool) request must keep the
/// existing same-candidate ordered key rotation. A bare 429 on the first key
/// must rotate to the next usable key in the same pass instead of being treated
/// as a pool-candidate scope failure that stops the pass.
#[tokio::test]
async fn ordinary_bare_429_still_rotates_to_the_next_key() {
    let _home = temp_home("pool-ordinary-bare-429-rotation");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| match auth_of(captured) {
        Some("Bearer sk-first") => {
            MockReply::Json(429, json!({"error": {"message": "temporarily unavailable"}}))
        }
        Some("Bearer sk-second") => MockReply::Json(200, json!({"id": "served-by-second"})),
        other => MockReply::Json(
            500,
            json!({"error": {"message": format!("unexpected authorization {other:?}")}}),
        ),
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Ordinary Provider",
        &provider_url,
        vec![
            pool_key("key-first", "First", "sk-first", true),
            pool_key("key-second", "Second", "sk-second", true),
        ],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    // No pool at all: this is the ordinary route.
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, "local-a").await;
    assert_eq!(
        status, 200,
        "the ordinary bare-429 rotation must fall back to the second key: {text}"
    );

    let captured = log.lock().unwrap().clone();
    let seen: Vec<Option<&str>> = captured.iter().map(auth_of).collect();
    assert_eq!(
        captured.len(),
        2,
        "a bare 429 must rotate to the second key in the same pass: {seen:?}"
    );
    assert_eq!(auth_of(&captured[0]), Some("Bearer sk-first"));
    assert_eq!(auth_of(&captured[1]), Some("Bearer sk-second"));

    let provider = stored_provider(&provider_id);
    for key in &provider.keys {
        assert!(
            !key.auto_marked,
            "a bare 429 must not mark a key: {key:?}"
        );
        assert_eq!(key.failure_kind, None, "a bare 429 must not mark a key");
    }

    stop_server().await.expect("stop the listener");
}

/// REQ-002 / REQ-007: a pool whose only selected candidate is an eligible
/// auto-disabled mapping must still be served by its half-open probe. The
/// unselected sibling and the provider default must receive zero calls and the
/// probed row's runtime health must clear after the successful probe.
#[tokio::test]
async fn pool_probe_only_candidate_serves_and_never_widens_the_selected_set() {
    let _home = temp_home("pool-probe-only-candidate");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| {
        match captured_model(captured).as_str() {
            "remote-a" => MockReply::Json(200, json!({"id": "served-by-a", "choices": []})),
            "remote-b" => MockReply::Json(200, json!({"id": "served-by-b", "choices": []})),
            "remote-default" => {
                MockReply::Json(200, json!({"id": "served-by-default", "choices": []}))
            }
            _ => MockReply::Json(500, json!({"error": {"message": "unexpected model"}})),
        }
    })
    .await;

    let mut disabled_mapping = provider_mapping("local-a", "remote-a", "chat_completions");
    disabled_mapping["auto_disabled"] = json!(true);
    disabled_mapping["disabled_reason"] = json!("HTTP 500 upstream error");
    disabled_mapping["disabled_at"] = json!(1);
    disabled_mapping["consecutive_failures"] = json!(3);

    let mut provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![
            disabled_mapping,
            provider_mapping("local-b", "remote-b", "chat_completions"),
        ],
    );
    provider["default_model"] = json!("remote-default");

    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "an eligible selected probe must serve the pool: {text}"
    );

    let captured = log.lock().unwrap().clone();
    let models: Vec<String> = captured.iter().map(captured_model).collect();
    assert_eq!(
        captured.len(),
        1,
        "only the selected candidate may be tried: {models:?}"
    );
    assert_eq!(
        models[0], "remote-a",
        "the probe must forward the selected mapping's upstream model: {models:?}"
    );

    let provider = stored_provider(&provider_id);
    let mapping = provider
        .mappings
        .iter()
        .find(|mapping| mapping.local_model == "local-a")
        .expect("the probed mapping must persist");
    assert!(
        !mapping.auto_disabled,
        "a successful probe must clear the row's runtime disable"
    );
    assert_eq!(
        mapping.consecutive_failures, 0,
        "a successful probe must reset the row's failure counter"
    );

    stop_server().await.expect("stop the listener");
}

/// REQ-003 / AC-004: pool scheduler accounting is isolated by ingress protocol.
/// Two equal-weight Anthropic Messages candidates served through the Chat and
/// Responses entries must each rotate through an independent equal cycle; under
/// a single shared cycle the two ingresses would each be pinned to a different
/// source.
#[tokio::test]
async fn pool_rotation_is_isolated_per_ingress_protocol() {
    let _home = temp_home("pool-ingress-isolation");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|_| MockReply::Json(200, messages_success_body())).await;

    let provider = pool_provider(
        &provider_id,
        "Messages Provider",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![
            provider_mapping("local-a", "remote-a", "anthropic_messages"),
            provider_mapping("local-b", "remote-b", "anthropic_messages"),
        ],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "anthropic_messages", Some(1)),
            pool_candidate(provider_id.as_str(), "local-b", "remote-b", "anthropic_messages", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    // Interleave the two ingresses; a shared scheduler would alternate sources
    // across requests, pinning each ingress to one source.
    for (marker, ingress) in [
        ("marker-chat-1", "chat"),
        ("marker-resp-1", "responses"),
        ("marker-chat-2", "chat"),
        ("marker-resp-2", "responses"),
    ] {
        let (status, text) = if ingress == "chat" {
            post_chat_marker(port, &alias, marker).await
        } else {
            post_responses_marker(port, &alias, marker).await
        };
        assert_eq!(status, 200, "{ingress} request '{marker}' must be served: {text}");
    }

    let captured = log.lock().unwrap().clone();
    assert_eq!(captured.len(), 4, "each request must reach exactly one source");
    let served_by = |marker: &str| -> String {
        captured
            .iter()
            .find(|captured| String::from_utf8_lossy(&captured.body).contains(marker))
            .map(captured_model)
            .unwrap_or_default()
    };
    let chat_1 = served_by("marker-chat-1");
    let chat_2 = served_by("marker-chat-2");
    let resp_1 = served_by("marker-resp-1");
    let resp_2 = served_by("marker-resp-2");
    let summary = format!("chat=({chat_1},{chat_2}) responses=({resp_1},{resp_2})");
    assert!(
        !chat_1.is_empty() && !chat_2.is_empty() && !resp_1.is_empty() && !resp_2.is_empty(),
        "every request must reach its selected source: {summary}"
    );
    assert_eq!(
        chat_1, resp_1,
        "each ingress must own an independent equal cycle starting at the same candidate: {summary}"
    );
    assert_ne!(
        chat_2, chat_1,
        "the chat ingress must rotate to the other candidate: {summary}"
    );
    assert_ne!(
        resp_2, resp_1,
        "the responses ingress must rotate to the other candidate: {summary}"
    );
    assert_eq!(
        chat_2, resp_2,
        "both ingresses must rotate identically in isolation: {summary}"
    );

    stop_server().await.expect("stop the listener");
}

/// REQ-006 / AC-008: a candidate-scoped failure with a valid `retry-after-ms: 0`
/// delay must be immediately eligible again; the zero delay must not be inflated
/// into an artificial one-second cooldown.
#[tokio::test]
async fn pool_candidate_zero_retry_delay_has_no_artificial_cooldown() {
    let _home = temp_home("pool-zero-delay-cooldown");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let (provider_url, _log) = spawn_mock_upstream(|captured| {
        if captured_model(captured) == "remote-limited" {
            MockReply::JsonWithHeaders(
                429,
                json!({"error": {"message": "free usage limit reached"}}),
                vec![("retry-after-ms".to_string(), "0".to_string())],
            )
        } else {
            MockReply::Json(200, json!({"id": "ok", "choices": []}))
        }
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            provider_mapping("local-limited", "remote-limited", "chat_completions"),
            provider_mapping("local-ok", "remote-ok", "chat_completions"),
        ],
    );
    let pool = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-limited", "remote-limited", "chat_completions", Some(100)),
            pool_candidate(provider_id.as_str(), "local-ok", "remote-ok", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(status, 200, "the sibling must serve the zero-delay fallback: {text}");

    let statuses =
        crate::ai_gateway::commands::ai_gateway_model_pools_status().expect("pool status");
    let pool = statuses
        .iter()
        .find(|pool| pool.id == pool_id)
        .expect("the pool status must be reported");
    let limited = pool
        .candidates
        .iter()
        .find(|candidate| candidate.upstream_model == "remote-limited")
        .expect("the limited candidate status must be reported");
    assert_eq!(
        limited.cooling_until, None,
        "a valid retry-after-ms: 0 must not create an artificial cooldown: {limited:?}"
    );
    assert!(
        limited.ready,
        "the zero-delay candidate must be immediately eligible: {limited:?}"
    );

    stop_server().await.expect("stop the listener");
}

/// REQ-006 / AC-008: a structured `FreeUsageLimitError` 429 is candidate-scoped
/// even when its message also names an account, so the sibling serves
/// immediately and the shared key stays unmarked.
#[tokio::test]
async fn structured_free_usage_limit_with_account_words_stays_candidate_scoped() {
    let _home = temp_home("pool-structured-free-limit-account-words");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| {
        if captured_model(captured) == "remote-limited" {
            MockReply::Json(
                429,
                json!({
                    "error": {
                        "type": "FreeUsageLimitError",
                        "message": "account free usage limit reached for this model",
                    }
                }),
            )
        } else {
            MockReply::Json(200, json!({"id": "ok", "choices": []}))
        }
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            provider_mapping("local-limited", "remote-limited", "chat_completions"),
            provider_mapping("local-ok", "remote-ok", "chat_completions"),
        ],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-limited", "remote-limited", "chat_completions", Some(100)),
            pool_candidate(provider_id.as_str(), "local-ok", "remote-ok", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "a structured free-usage limit must fall back to the sibling immediately: {text}"
    );

    let models = models_of(&log);
    assert_eq!(
        models
            .iter()
            .filter(|model| model.as_str() == "remote-limited")
            .count(),
        1,
        "the free-limited candidate must be attempted once: {models:?}"
    );
    assert_eq!(
        models
            .iter()
            .filter(|model| model.as_str() == "remote-ok")
            .count(),
        1,
        "the sibling must serve the immediate fallback: {models:?}"
    );

    let provider = stored_provider(&provider_id);
    for key in &provider.keys {
        assert!(
            !key.auto_marked,
            "a structured free limit must not mark the shared key: {key:?}"
        );
        assert_eq!(
            key.failure_kind, None,
            "the shared key must carry no failure kind: {key:?}"
        );
    }

    stop_server().await.expect("stop the listener");
}

// ===========================================================================
// Review-repair (2026-10-13), SPEC-3 / AC-009: credential, account and mapping
// isolation for a pool candidate, observed at the public loopback + status
// boundary only.
// ===========================================================================

/// AC-009 / REQ-006: an HTTP 401 from the first selected key marks only that
/// key as authentication-failed and the request continues in order on the next
/// usable key; the other key stays untouched and the mapping row is not
/// disabled.
#[tokio::test]
async fn pool_credential_401_marks_only_the_failed_key_and_rotates_to_the_next() {
    let _home = temp_home("pool-401-key-rotation");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| match auth_of(captured) {
        Some("Bearer sk-first") => MockReply::Json(
            401,
            json!({"error": {"message": "unauthorized: invalid credential"}}),
        ),
        Some("Bearer sk-second") => {
            MockReply::Json(200, json!({"id": "served-by-second", "choices": []}))
        }
        other => MockReply::Json(
            500,
            json!({"error": {"message": format!("unexpected authorization {other:?}")}}),
        ),
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![
            pool_key("key-first", "First", "sk-first", true),
            pool_key("key-second", "Second", "sk-second", true),
        ],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "a 401 on the first key must rotate to the next usable key: {text}"
    );

    let captured = log.lock().unwrap().clone();
    let seen: Vec<Option<&str>> = captured.iter().map(auth_of).collect();
    assert_eq!(
        captured.len(),
        2,
        "the request must continue on the next usable key: {seen:?}"
    );
    assert_eq!(auth_of(&captured[0]), Some("Bearer sk-first"));
    assert_eq!(auth_of(&captured[1]), Some("Bearer sk-second"));
    assert_eq!(captured_model(&captured[1]), "remote-a");

    let stored = stored_provider(&provider_id);
    let first = stored
        .keys
        .iter()
        .find(|key| key.id == "key-first")
        .expect("the first key must persist");
    let second = stored
        .keys
        .iter()
        .find(|key| key.id == "key-second")
        .expect("the second key must persist");
    assert!(first.auto_marked, "the 401 key must be marked: {first:?}");
    assert_eq!(
        first.failure_kind,
        Some(crate::ai_gateway::KeyFailureKind::Authentication),
        "a 401 must mark the key authentication-failed: {first:?}"
    );
    assert!(
        !second.auto_marked,
        "the other key must stay untouched: {second:?}"
    );
    assert_eq!(second.failure_kind, None);
    assert!(
        !stored.mappings[0].auto_disabled,
        "a credential 401 is key-scoped and must not disable the mapping row"
    );

    stop_server().await.expect("stop the listener");
}

/// AC-009 / REQ-006: a 403 whose text names a credential problem
/// (`is_authentication_error_message`) marks the key exactly like a 401 and
/// rotates in the same ordered way instead of disabling the mapping row.
#[tokio::test]
async fn pool_credential_text_403_marks_the_key_like_a_401() {
    let _home = temp_home("pool-403-credential-key-rotation");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| match auth_of(captured) {
        Some("Bearer sk-first") => {
            MockReply::Json(403, json!({"error": {"message": "invalid api key"}}))
        }
        Some("Bearer sk-second") => {
            MockReply::Json(200, json!({"id": "served-by-second", "choices": []}))
        }
        other => MockReply::Json(
            500,
            json!({"error": {"message": format!("unexpected authorization {other:?}")}}),
        ),
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![
            pool_key("key-first", "First", "sk-first", true),
            pool_key("key-second", "Second", "sk-second", true),
        ],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "a credential-text 403 must rotate to the next usable key: {text}"
    );

    let captured = log.lock().unwrap().clone();
    let seen: Vec<Option<&str>> = captured.iter().map(auth_of).collect();
    assert_eq!(captured.len(), 2, "the 403 must rotate keys: {seen:?}");
    assert_eq!(auth_of(&captured[0]), Some("Bearer sk-first"));
    assert_eq!(auth_of(&captured[1]), Some("Bearer sk-second"));

    let stored = stored_provider(&provider_id);
    let first = stored
        .keys
        .iter()
        .find(|key| key.id == "key-first")
        .expect("the first key must persist");
    assert!(first.auto_marked, "the credential-text 403 key must be marked");
    assert_eq!(
        first.failure_kind,
        Some(crate::ai_gateway::KeyFailureKind::Authentication),
        "a credential-text 403 must mark the key authentication-failed: {first:?}"
    );
    assert!(
        !stored.mappings[0].auto_disabled,
        "a credential-text 403 is key-scoped and must not disable the mapping row"
    );

    stop_server().await.expect("stop the listener");
}

/// AC-009 / REQ-006: a non-credential 403 keeps the key valid, auto-disables the
/// original mapping row with an HTTP 403 reason, lets the selected sibling serve
/// on the same key, and never cools the pool candidate.
#[tokio::test]
async fn pool_noncredential_403_disables_the_original_mapping_and_keeps_the_key() {
    let _home = temp_home("pool-noncredential-403");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| {
        if captured_model(captured) == "remote-a" {
            MockReply::Json(
                403,
                json!({"error": {"message": "forbidden: region mismatch"}}),
            )
        } else {
            MockReply::Json(200, json!({"id": "served-by-b", "choices": []}))
        }
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            provider_mapping("local-a", "remote-a", "chat_completions"),
            provider_mapping("local-b", "remote-b", "chat_completions"),
        ],
    );
    let pool = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "chat_completions", Some(100)),
            pool_candidate(provider_id.as_str(), "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "the sibling must serve after the non-credential 403: {text}"
    );

    let captured = log.lock().unwrap().clone();
    let models: Vec<String> = captured.iter().map(captured_model).collect();
    assert_eq!(
        models,
        vec!["remote-a".to_string(), "remote-b".to_string()],
        "the original mapping must fail first, then the sibling serves: {models:?}"
    );
    for entry in &captured {
        assert_eq!(
            auth_of(entry),
            Some("Bearer sk-shared"),
            "the sibling must serve using the same shared key"
        );
    }

    let stored = stored_provider(&provider_id);
    assert!(
        stored.mappings[0].auto_disabled,
        "a non-credential 403 must auto-disable the original mapping row: {:?}",
        stored.mappings[0]
    );
    assert!(
        stored.mappings[0]
            .disabled_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("403")),
        "the disable reason must name HTTP 403: {:?}",
        stored.mappings[0].disabled_reason
    );
    for key in &stored.keys {
        assert!(
            !key.auto_marked,
            "a non-credential 403 must not mark the valid key: {key:?}"
        );
        assert_eq!(key.failure_kind, None);
    }

    let statuses =
        crate::ai_gateway::commands::ai_gateway_model_pools_status().expect("pool status");
    let pool = statuses
        .iter()
        .find(|pool| pool.id == pool_id)
        .expect("the pool status must be reported");
    let candidate_a = pool
        .candidates
        .iter()
        .find(|candidate| candidate.upstream_model == "remote-a")
        .expect("candidate A status");
    assert_eq!(
        candidate_a.cooling_until, None,
        "a non-credential 403 disables the mapping and must not cool the pool candidate: {candidate_a:?}"
    );
    assert_eq!(
        candidate_a.availability, "mapping_unavailable",
        "the disabled mapping is the reason, not a candidate cooldown: {candidate_a:?}"
    );

    stop_server().await.expect("stop the listener");
}

/// AC-009 / REQ-006: an explicit account-exhaustion 429 keeps the existing
/// key/account policy (the exhausted key is quota-marked and the pass rotates
/// to the next usable key), does not candidate-cool, and the candidate stays
/// reachable on a later request through its healthy key.
#[tokio::test]
async fn pool_explicit_account_exhaustion_keeps_key_policy_without_candidate_cooling() {
    let _home = temp_home("pool-account-exhaustion");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| match auth_of(captured) {
        Some("Bearer sk-exhausted") => MockReply::Json(
            429,
            json!({"error": {"message": "insufficient account balance"}}),
        ),
        Some("Bearer sk-ok") => {
            MockReply::Json(200, json!({"id": "served-by-ok", "choices": []}))
        }
        other => MockReply::Json(
            500,
            json!({"error": {"message": format!("unexpected authorization {other:?}")}}),
        ),
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![
            pool_key("key-exhausted", "Exhausted", "sk-exhausted", true),
            pool_key("key-ok", "Ok", "sk-ok", true),
        ],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let pool = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-a", "remote-a", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "explicit account exhaustion must use the existing key rotation: {text}"
    );

    let captured = log.lock().unwrap().clone();
    let seen: Vec<Option<&str>> = captured.iter().map(auth_of).collect();
    assert_eq!(
        captured.len(),
        2,
        "the exhausted key must rotate to the next usable key: {seen:?}"
    );
    assert_eq!(auth_of(&captured[0]), Some("Bearer sk-exhausted"));
    assert_eq!(auth_of(&captured[1]), Some("Bearer sk-ok"));

    let stored = stored_provider(&provider_id);
    let exhausted = stored
        .keys
        .iter()
        .find(|key| key.id == "key-exhausted")
        .expect("the exhausted key must persist");
    let ok = stored
        .keys
        .iter()
        .find(|key| key.id == "key-ok")
        .expect("the healthy key must persist");
    assert!(
        exhausted.auto_marked,
        "an explicit account exhaustion must quota-mark the key: {exhausted:?}"
    );
    assert_eq!(
        exhausted.failure_kind,
        Some(crate::ai_gateway::KeyFailureKind::Quota),
        "an explicit account exhaustion must use the existing quota key mark: {exhausted:?}"
    );
    assert!(
        !ok.auto_marked,
        "the healthy key must stay usable: {ok:?}"
    );

    let statuses =
        crate::ai_gateway::commands::ai_gateway_model_pools_status().expect("pool status");
    let pool = statuses
        .iter()
        .find(|pool| pool.id == pool_id)
        .expect("the pool status must be reported");
    let candidate = pool
        .candidates
        .iter()
        .find(|candidate| candidate.upstream_model == "remote-a")
        .expect("candidate status");
    assert_eq!(
        candidate.cooling_until, None,
        "account exhaustion must not candidate-cool: {candidate:?}"
    );
    assert!(
        candidate.ready,
        "the candidate is still usable through its healthy key: {candidate:?}"
    );

    // A later request still reaches the same candidate through its healthy key;
    // only the exhausted key is skipped by the existing quota policy.
    let (second_status, _content_type, second_text) = post_chat(port, &alias).await;
    assert_eq!(
        second_status, 200,
        "the candidate must remain reachable on a later request: {second_text}"
    );
    let captured = log.lock().unwrap().clone();
    assert_eq!(
        captured.len(),
        3,
        "the second request must reach the candidate exactly once"
    );
    assert_eq!(auth_of(&captured[2]), Some("Bearer sk-ok"));

    stop_server().await.expect("stop the listener");
}

/// REQ-001 / AC-002: the public pool CRUD commands validate atomically, preserve
/// the encrypted configuration on every rejected write, retain a stored
/// reference whose source later became unavailable, reserve a deleted alias and
/// reject unknown-id mutations without writing.
#[test]
fn model_pool_upsert_validates_atomically_and_keeps_stored_unavailable_refs() {
    super::with_temp_home("pool-crud-validation-atomic", |_home| {
        let provider = pool_provider(
            "p-a",
            "Provider A",
            "https://provider-a.example.invalid/v1",
            vec![pool_key("key-a", "Key A", "sk-secret-a", true)],
            vec![provider_mapping("local-a1", "remote-a1", "chat_completions")],
        );
        let seeded_pool = json!({
            "id": "pool-seed",
            "name": "Seeded",
            "model": "free-seed",
            "enabled": true,
            "candidates": [
                pool_candidate("p-a", "local-a", "remote-a", "chat_completions", Some(1)),
                pool_candidate("p-a", "local-gone", "remote-gone", "chat_completions", Some(1)),
            ],
        });
        write_raw_gateway_config(&pool_config_v3(0, vec![provider], vec![seeded_pool]));

        let baseline = read_raw_gateway_config();
        let upsert = |value: Value| -> Result<GatewayConfig, String> {
            let pool: ModelPool =
                serde_json::from_value(value).expect("the pool fixture must deserialize");
            crate::ai_gateway::commands::ai_gateway_upsert_model_pool(pool)
        };
        let candidate = |local: &str, upstream: &str, weight: u32| {
            json!({
                "provider_id": "p-a",
                "local_model": local,
                "upstream_model": upstream,
                "protocol": "chat_completions",
                "weight": weight,
            })
        };
        let pool = |id: &str, name: &str, model: &str, candidates: Value| {
            json!({"id": id, "name": name, "model": model, "enabled": true, "candidates": candidates})
        };

        // Blank name.
        assert!(upsert(pool("pool-new", "  ", "free-new", json!([candidate("local-a", "remote-a", 1)]))).is_err());
        assert_eq!(
            read_raw_gateway_config(),
            baseline,
            "a rejected blank-name save must not change the encrypted config"
        );
        // Blank public model id.
        assert!(upsert(pool("pool-new", "New", "  ", json!([candidate("local-a", "remote-a", 1)]))).is_err());
        assert_eq!(read_raw_gateway_config(), baseline);
        // No candidates.
        assert!(upsert(pool("pool-new", "New", "free-new", json!([]))).is_err());
        assert_eq!(read_raw_gateway_config(), baseline);
        // Duplicate candidate tuple.
        assert!(upsert(pool(
            "pool-new",
            "New",
            "free-new",
            json!([candidate("local-a", "remote-a", 1), candidate("local-a", "remote-a", 1)])
        ))
        .is_err());
        assert_eq!(read_raw_gateway_config(), baseline);
        // Weight just below the floor and just above the ceiling.
        assert!(upsert(pool("pool-new", "New", "free-new", json!([candidate("local-a", "remote-a", 0)]))).is_err());
        assert!(upsert(pool("pool-new", "New", "free-new", json!([candidate("local-a", "remote-a", 101)]))).is_err());
        assert_eq!(read_raw_gateway_config(), baseline);
        // A brand-new reference that does not resolve.
        assert!(upsert(pool("pool-new", "New", "free-new", json!([candidate("local-x", "remote-x", 1)]))).is_err());
        assert_eq!(read_raw_gateway_config(), baseline);
        // Alias conflicts with an ordinary mapping and with another pool.
        assert!(upsert(pool("pool-new", "New", "local-a", json!([candidate("local-a", "remote-a", 1)]))).is_err());
        assert!(upsert(pool("pool-new", "New", "free-seed", json!([candidate("local-a", "remote-a", 1)]))).is_err());
        assert_eq!(read_raw_gateway_config(), baseline);

        // A stored reference whose source became unavailable may be retained.
        let retained = upsert(pool(
            "pool-seed",
            "Seeded",
            "free-seed",
            json!([
                candidate("local-a", "remote-a", 1),
                candidate("local-gone", "remote-gone", 1),
            ]),
        ))
        .expect("a stored unavailable reference may be retained");
        assert!(retained.model_pools.iter().any(|pool| pool.id == "pool-seed"));

        // Unknown-id mutations write nothing.
        let before = read_raw_gateway_config();
        assert!(
            crate::ai_gateway::commands::ai_gateway_set_model_pool_enabled("missing".to_string(), false)
                .is_err()
        );
        assert!(
            crate::ai_gateway::commands::ai_gateway_delete_model_pool("missing".to_string()).is_err()
        );
        assert_eq!(read_raw_gateway_config(), before);

        // Deleting an existing pool reserves its retired alias.
        crate::ai_gateway::commands::ai_gateway_delete_model_pool("pool-seed".to_string())
            .expect("delete the existing pool");
        let after = read_raw_gateway_config();
        assert!(
            after["model_pool_reserved_models"]
                .as_array()
                .expect("reservations must persist as an array")
                .iter()
                .any(|reserved| reserved == "free-seed"),
            "deleting a pool must reserve its retired alias: {after}"
        );
    });
}

// ===========================================================================
// Step 2 (20261013-free-model-pool) policy integration
//
// These tests exercise the Step 2 request contract at the public listener and
// command boundary: a converted Chat->Responses pool request must keep the real
// source and the original upstream usage in exactly one terminal row, and the
// public status DTO must keep reporting the wall-clock cooldown deadline.
// ===========================================================================

/// REQ-009 / AC-012: a converted pool request logs the public alias as
/// `local_model` and the real provider id and upstream model, and observes the
/// original upstream usage exactly once (no converted duplicate row).
#[tokio::test]
async fn pool_converted_request_logs_real_source_and_original_usage_once() {
    let _home = temp_home("pool-converted-usage-identity");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "resp_1",
                "object": "response",
                "model": "remote-r",
                "status": "completed",
                "output": [{
                    "id": "msg_1",
                    "type": "message",
                    "role": "assistant",
                    "status": "completed",
                    "content": [{"type": "output_text", "text": "ok", "annotations": []}]
                }],
                "usage": {"input_tokens": 11, "output_tokens": 4, "total_tokens": 15}
            }),
        )
    })
    .await;

    // The mapping declares `responses` while the provider default stays Chat;
    // the candidate's stored protocol is the mapping's effective protocol.
    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![provider_mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-r", "remote-r", "responses", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(status, 200, "the converted Chat->Responses pool request must succeed: {text}");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(
        records.len(),
        1,
        "one converted request must settle exactly one terminal row: {records:?}"
    );
    let row = &records[0];
    assert_eq!(row.local_model, alias, "the public alias must be the local model: {row:?}");
    assert_eq!(row.provider_id, provider_id, "the real provider id must be logged: {row:?}");
    assert_eq!(row.upstream_model, "remote-r", "the real upstream model must be logged: {row:?}");
    assert_eq!(row.result, UsageResult::Success, "the converted request is a success: {row:?}");
    assert_eq!(row.status, 200, "the terminal row status must be 200: {row:?}");
    assert_eq!(
        row.input_tokens, 11,
        "the original upstream input usage must be observed once: {row:?}"
    );
    assert_eq!(
        row.output_tokens, 4,
        "the original upstream output usage must be observed once: {row:?}"
    );

    stop_server().await.expect("stop the listener");
}

/// REQ-006 / AC-008: the public status DTO keeps reporting `cooling_until` as a
/// wall-clock epoch-seconds deadline, bounded by the 60-second default and never
/// in the past, while the internal monotonic deadline governs availability.
#[tokio::test]
async fn pool_candidate_status_reports_wall_clock_cooling_seconds() {
    let _home = temp_home("pool-status-wall-clock-cooldown");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let (provider_url, _log) = spawn_mock_upstream(|captured| {
        if captured_model(captured) == "remote-limited" {
            MockReply::JsonWithHeaders(
                429,
                json!({"error": {"message": "free usage limit reached"}}),
                vec![("retry-after-ms".to_string(), "60000".to_string())],
            )
        } else {
            MockReply::Json(200, json!({"id": "ok", "choices": []}))
        }
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            provider_mapping("local-limited", "remote-limited", "chat_completions"),
            provider_mapping("local-ok", "remote-ok", "chat_completions"),
        ],
    );
    let pool = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-limited", "remote-limited", "chat_completions", Some(100)),
            pool_candidate(provider_id.as_str(), "local-ok", "remote-ok", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(status, 200, "the sibling must serve the limited candidate: {text}");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs();
    let statuses =
        crate::ai_gateway::commands::ai_gateway_model_pools_status().expect("pool status");
    let pool = statuses
        .iter()
        .find(|pool| pool.id == pool_id)
        .expect("the pool status must be reported");
    let limited = pool
        .candidates
        .iter()
        .find(|candidate| candidate.upstream_model == "remote-limited")
        .expect("the limited candidate status must be reported");
    let until = limited
        .cooling_until
        .expect("a 60s retry-after-ms must report a cooldown deadline");
    assert!(
        until >= now,
        "the wall-clock deadline must not be in the past: until={until} now={now}"
    );
    assert!(
        until <= now + 60,
        "the wall-clock deadline must stay inside the 60s boundary: until={until} now={now}"
    );
    assert!(!limited.ready, "a cooling candidate must not be ready: {limited:?}");

    stop_server().await.expect("stop the listener");
}

// ===========================================================================
// Review-repair (2026-10-13): a selected cross-protocol mapping must still be
// reachable through the pool's half-open probe (REQ-002/REQ-007).
// ===========================================================================

/// A pool whose only selected candidate is an auto-disabled `responses` mapping,
/// reached through the Chat ingress, must probe that exact stored mapping once
/// (under its own protocol), clear the real row health and never touch an
/// ordinary unselected Chat default.
#[tokio::test]
async fn pool_cross_protocol_probe_only_candidate_serves_selected_mapping() {
    let _home = temp_home("pool-cross-protocol-probe");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let default_id = format!("p-default-{tag}");
    let alias = format!("free-model-{tag}");
    let (probe_url, probe_log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "resp_probe",
                "object": "response",
                "model": "remote-r",
                "status": "completed",
                "output": [{"id": "m", "type": "message", "role": "assistant", "status": "completed",
                    "content": [{"type": "output_text", "text": "ok", "annotations": []}]}],
                "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
            }),
        )
    })
    .await;
    let (default_url, default_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ordinary"}))).await;

    let mut disabled = provider_mapping("local-r", "remote-r", "responses");
    disabled["auto_disabled"] = json!(true);
    disabled["disabled_reason"] = json!("HTTP 500 upstream error");
    disabled["disabled_at"] = json!(1);
    disabled["consecutive_failures"] = json!(3);

    let provider = pool_provider(
        &provider_id,
        "Probe Provider",
        &probe_url,
        vec![pool_key("key-default", "Default", "sk-probe", true)],
        vec![disabled],
    );
    let mut unselected = pool_provider(
        &default_id,
        "Ordinary Provider",
        &default_url,
        vec![pool_key("key-default", "Default", "sk-default", true)],
        vec![],
    );
    unselected["default_model"] = json!("remote-default");

    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(provider_id.as_str(), "local-r", "remote-r", "responses", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider, unselected], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "the cross-protocol selected probe must serve the pool: {text}"
    );

    let calls = probe_log.lock().expect("probe log").clone();
    assert_eq!(calls.len(), 1, "exactly one selected probe: {text}");
    assert_eq!(
        calls[0].path, "/v1/responses",
        "the probe must use the selected mapping's stored protocol: {text}"
    );
    assert!(
        default_log.lock().expect("default log").is_empty(),
        "the unselected default must receive zero calls"
    );

    let stored = crate::ai_gateway::storage::read_config().expect("read config");
    let provider = stored
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .expect("the probed provider must persist");
    assert!(
        !provider.mappings[0].auto_disabled,
        "a successful probe must clear the real row health: {:?}",
        provider.mappings[0]
    );
    assert_eq!(
        provider.mappings[0].consecutive_failures, 0,
        "a successful probe must reset the real row failure counter"
    );

    stop_server().await.expect("stop the listener");
}

// ===========================================================================
// Step 2 (20261015-pool-only-reasoning): only reasoning-compatible candidates
// consume weighted scheduler slots (AC-005).
// ===========================================================================

/// One Chat request carrying an explicit `high` preference and an output cap.
async fn post_chat_capped(port: u16, alias: &str, max_tokens: u64) -> (u16, String) {
    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-opencode-session", "sess-same"),
        ],
        Some(json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "reasoning_effort": "high",
            "max_tokens": max_tokens,
        })),
    )
    .await;
    (status, text)
}

/// AC-005: a candidate whose declared budget cannot fit the request's effective
/// output cap is filtered before weighted selection, so it never consumes a
/// scheduler slot; once the cap admits it, the same weights make it participate.
#[tokio::test]
async fn pool_reasoning_incompatible_candidates_are_filtered_before_weighted_selection() {
    let _home = temp_home("pool-reasoning-swr-filter");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-cap-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| {
        if captured.path.ends_with("/messages") {
            MockReply::Json(
                200,
                json!({
                    "id": "msg_1", "type": "message", "role": "assistant", "model": "remote-cap-b",
                    "content": [{"type": "text", "text": "ok"}], "stop_reason": "end_turn",
                    "usage": {"input_tokens": 1, "output_tokens": 1},
                }),
            )
        } else {
            MockReply::Json(
                200,
                json!({
                    "id": "ok", "object": "chat.completion",
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"},
                        "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
                }),
            )
        }
    })
    .await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![
            provider_mapping("local-a", "remote-cap-a", "chat_completions"),
            provider_mapping("local-b", "remote-cap-b", "anthropic_messages"),
        ],
    );
    let pool = json!({
        "id": format!("pool-cap-{tag}"),
        "name": "Cap",
        "model": alias,
        "enabled": true,
        "default_reasoning_effort": "auto",
        "candidates": [
            {
                "provider_id": provider_id,
                "local_model": "local-a",
                "upstream_model": "remote-cap-a",
                "protocol": "chat_completions",
                "weight": 3,
                "reasoning": {
                    "profile": "openai_effort",
                    "values": ["high"],
                    "mappings": [{"effort": "high", "action": {"kind": "enum", "value": "high"}}],
                },
            },
            {
                "provider_id": provider_id,
                "local_model": "local-b",
                "upstream_model": "remote-cap-b",
                "protocol": "anthropic_messages",
                "weight": 1,
                "reasoning": {
                    "profile": "anthropic_manual_budget",
                    "budget": {"min": 1024, "max": 16384},
                    "mappings": [{"effort": "high", "action": {"kind": "budget", "budget_tokens": 8192}}],
                },
            },
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    // The small cap excludes the budget candidate entirely.
    for _ in 0..4 {
        let (status, text) = post_chat_capped(port, &alias, 1024).await;
        assert_eq!(
            status, 200,
            "the compatible candidate must serve under a small cap: {text}"
        );
    }
    let models = models_of(&log);
    assert_eq!(
        models.len(),
        4,
        "only the compatible candidate may serve: {models:?}"
    );
    assert!(
        models.iter().all(|model| model == "remote-cap-a"),
        "a cap-incompatible candidate must never consume a slot: {models:?}"
    );

    // A large cap admits the budget candidate under its own weight.
    for _ in 0..4 {
        let (status, text) = post_chat_capped(port, &alias, 16384).await;
        assert_eq!(
            status, 200,
            "both compatible candidates must serve under a large cap: {text}"
        );
    }
    let models = models_of(&log);
    assert!(
        models
            .iter()
            .filter(|model| model.as_str() == "remote-cap-b")
            .count()
            >= 1,
        "a now-compatible candidate must participate under its weight: {models:?}"
    );

    stop_server().await.expect("stop the listener");
}

// ===========================================================================
// Step 2 (20261015-pool-only-reasoning): a compatible selected probe and
// weighted selection obey the pool reasoning mapping (AC-007).
// ===========================================================================

/// One Chat request carrying an explicit `high` pool preference.
async fn post_chat_effort(port: u16, alias: &str, effort: &str) -> (u16, String, String) {
    call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-opencode-session", "sess-same"),
        ],
        Some(json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "reasoning_effort": effort,
        })),
    )
    .await
}

/// AC-007: the reasoning filter decides the selected set before weighted
/// selection. A currently healthy candidate that declares no `high` mapping must
/// not consume the slot, so the only compatible candidate is the auto-disabled
/// half-open probe; it serves with its own candidate-native control and its
/// successful probe clears the row's runtime health.
#[tokio::test]
async fn pool_reasoning_compatible_probe_serves_and_incompatible_candidate_is_excluded() {
    let _home = temp_home("pool-reasoning-probe-filter");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) = spawn_mock_upstream(|captured| {
        match captured_model(captured).as_str() {
            "remote-probe" => MockReply::Json(200, json!({"id": "served-by-probe", "choices": []})),
            "remote-sibling" => {
                MockReply::Json(200, json!({"id": "served-by-sibling", "choices": []}))
            }
            _ => MockReply::Json(500, json!({"error": {"message": "unexpected model"}})),
        }
    })
    .await;

    let mut probe_mapping = provider_mapping("local-probe", "remote-probe", "chat_completions");
    probe_mapping["auto_disabled"] = json!(true);
    probe_mapping["disabled_reason"] = json!("HTTP 500 upstream error");
    probe_mapping["disabled_at"] = json!(1);
    probe_mapping["consecutive_failures"] = json!(3);

    let provider = pool_provider(
        &provider_id,
        "Probe Provider",
        &provider_url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        vec![
            probe_mapping,
            provider_mapping("local-sibling", "remote-sibling", "chat_completions"),
        ],
    );

    let mut probe_candidate = pool_candidate(
        provider_id.as_str(),
        "local-probe",
        "remote-probe",
        "chat_completions",
        Some(1),
    );
    probe_candidate["reasoning"] = json!({
        "profile": "openai_effort",
        "values": ["high"],
        "mappings": [{"effort": "high", "action": {"kind": "enum", "value": "probe-native"}}],
    });
    // The healthy sibling carries the dominant weight, so a baseline without the
    // reasoning filter deterministically selects it instead of the probe.
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Probe Filter",
        "model": alias,
        "enabled": true,
        "candidates": [
            probe_candidate,
            pool_candidate(provider_id.as_str(), "local-sibling", "remote-sibling", "chat_completions", Some(100)),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat_effort(port, &alias, "high").await;
    let captured = log.lock().expect("mock log").clone();
    let stored = stored_provider(&provider_id);
    stop_server().await.expect("stop the listener");

    let models: Vec<String> = captured.iter().map(captured_model).collect();
    let mut failures: Vec<String> = Vec::new();
    if status != 200 {
        failures.push(format!(
            "the compatible selected probe must serve the pool: {status}: {text}"
        ));
    }
    if models != vec!["remote-probe".to_string()] {
        failures.push(format!(
            "only the compatible probe may be attempted: {models:?}"
        ));
    }
    if let Some(first) = captured.first() {
        let body: Value = serde_json::from_slice(&first.body).unwrap_or(Value::Null);
        if body.get("reasoning_effort").and_then(Value::as_str) != Some("probe-native") {
            failures.push(format!(
                "the probe must forward its candidate-native control: {body}"
            ));
        }
    }
    let mapping = stored
        .mappings
        .iter()
        .find(|mapping| mapping.local_model == "local-probe")
        .expect("the probed mapping must persist");
    if mapping.auto_disabled {
        failures.push(format!(
            "a successful probe must clear the row health: {mapping:?}"
        ));
    }
    if mapping.consecutive_failures != 0 {
        failures.push(format!(
            "a successful probe must reset the row failure counter: {mapping:?}"
        ));
    }
    assert!(
        failures.is_empty(),
        "compatible probe selection mismatches: {failures:#?}"
    );
}

/// AC-002: planning a pool request must not copy one whole provider for every
/// stored candidate when only one candidate can serve. The same one usable
/// candidate is planned from a pool with two stored candidates and from a pool
/// with twelve; the copied-provider count must not grow with the stored set.
///
/// The test-only counter increments for deep `GatewayUpstreamProvider` copies
/// made by pool planning, never for shared/`Arc` references.
#[test]
fn pool_planning_provider_copies_do_not_scale_with_stored_candidates() {
    let request = PoolReasoningRequest {
        requested: None,
        cap: PoolOutputCap {
            field: "max_tokens",
            tokens: 4096,
        },
    };

    let copies_for = |stored_candidates: usize| -> usize {
        let tag = tag();
        let provider_id = format!("p-copy-{tag}");
        let alias = format!("alias-copy-{tag}");
        let mappings: Vec<Value> = (0..stored_candidates)
            .map(|index| {
                let mut row = provider_mapping(
                    &format!("local-copy-{tag}-{index}"),
                    &format!("remote-copy-{tag}-{index}"),
                    "chat_completions",
                );
                if index != 0 {
                    row["enabled"] = json!(false);
                }
                row
            })
            .collect();
        let provider_value = pool_provider(
            &provider_id,
            "Copy Provider",
            "https://copy.example.invalid/v1",
            vec![pool_key("copy-key", "Copy Key", "sk-copy", true)],
            mappings,
        );
        let provider: GatewayUpstreamProvider =
            serde_json::from_value(provider_value).expect("copy provider fixture");
        let candidates: Vec<Value> = (0..stored_candidates)
            .map(|index| {
                pool_candidate(
                    &provider_id,
                    &format!("local-copy-{tag}-{index}"),
                    &format!("remote-copy-{tag}-{index}"),
                    "chat_completions",
                    Some(1),
                )
            })
            .collect();
        let pool: ModelPool = serde_json::from_value(json!({
            "id": format!("pool-copy-{tag}"),
            "name": "Copy Pool",
            "model": alias,
            "enabled": true,
            "candidates": candidates,
        }))
        .expect("copy pool fixture");
        let config = GatewayConfig {
            providers: vec![provider],
            model_pools: vec![pool],
            ..Default::default()
        };

        crate::ai_gateway::model_pools::reset_pool_provider_copy_count();
        match plan_pool_request(
            &config,
            &alias,
            UpstreamProtocol::ChatCompletions,
            &request,
            crate::ai_gateway::now_ts(),
        )
        .expect("a current pool alias must be planned")
        {
            PoolPlan::Ready { ordered, .. } => {
                assert_eq!(
                    ordered.len(),
                    1,
                    "only the single enabled mapping may serve the request"
                );
            }
            _ => panic!("the pool with one usable candidate must be ready"),
        }
        crate::ai_gateway::model_pools::pool_provider_copy_count()
    };

    let two = copies_for(2);
    let twelve = copies_for(12);
    assert_eq!(
        twelve, two,
        "pool planning must not copy one whole provider per stored candidate: \
         two stored copied {two}, twelve stored copied {twelve}"
    );
}

// ---------------------------------------------------------------------------
// Boundary A RED: optional default fallback configuration (AC-001 / AC-009)
// ---------------------------------------------------------------------------

/// AC-001/AC-009 RED: a raw pool carrying an optional `fallback` field must
/// survive the typed encrypted round trip with its exact identity and reasoning
/// declaration. Production currently ignores the unknown field, so the
/// assertion fails until the additive field is implemented.
#[test]
fn pool_optional_fallback_survives_typed_round_trip() {
    super::with_temp_home("pool-fallback-roundtrip", |_home| {
        let provider = pool_provider(
            "p-a",
            "Provider A",
            "https://provider-a.example.invalid/v1",
            vec![pool_key("key-a", "Key A", "sk-secret-a", true)],
            vec![
                provider_mapping("local-a1", "remote-a1", "chat_completions"),
                provider_mapping("local-fb", "remote-fb", "chat_completions"),
            ],
        );
        let fallback = pool_candidate("p-a", "local-fb", "remote-fb", "chat_completions", Some(1));
        let mut pool = json!({
            "id": "pool-fb",
            "name": "Fallback Pool",
            "model": "fallback-model",
            "enabled": true,
            "candidates": [
                pool_candidate("p-a", "local-a1", "remote-a1", "chat_completions", Some(3)),
            ],
        });
        pool["fallback"] = fallback;
        write_raw_gateway_config(&pool_config_v3(0, vec![provider], vec![pool]));

        crate::ai_gateway::storage::modify_config::<()>(|config| {
            config.keys.push(key_named("k-extra", "extra-value"));
            Ok((true, ()))
        })
        .expect("the configuration write must succeed");

        let raw = read_raw_gateway_config();
        let pools = raw
            .get("model_pools")
            .and_then(Value::as_array)
            .expect("pool collection must persist");
        assert_eq!(pools.len(), 1);
        let pool = &pools[0];
        assert_eq!(pool["fallback"]["provider_id"], "p-a");
        assert_eq!(pool["fallback"]["local_model"], "local-fb");
        assert_eq!(pool["fallback"]["upstream_model"], "remote-fb");
        assert_eq!(pool["fallback"]["protocol"], "chat_completions");
        assert_eq!(pool["fallback"]["weight"], 1);
    });
}

/// AC-001/AC-009 RED: a missing fallback reads as no fallback and never
/// rewrites the stored bytes on a typed round trip.
#[test]
fn pool_missing_fallback_reads_as_none_without_rewrite() {
    super::with_temp_home("pool-fallback-missing", |_home| {
        let provider = pool_provider(
            "p-a",
            "Provider A",
            "https://provider-a.example.invalid/v1",
            vec![pool_key("key-a", "Key A", "sk-secret-a", true)],
            vec![provider_mapping("local-a1", "remote-a1", "chat_completions")],
        );
        let pool = json!({
            "id": "pool-nofb",
            "name": "No Fallback Pool",
            "model": "no-fallback-model",
            "enabled": true,
            "candidates": [
                pool_candidate("p-a", "local-a1", "remote-a1", "chat_completions", Some(1)),
            ],
        });
        write_raw_gateway_config(&pool_config_v3(0, vec![provider], vec![pool]));
        let before = read_raw_gateway_config().to_string();

        crate::ai_gateway::storage::modify_config::<()>(|config| {
            config.keys.push(key_named("k-extra", "extra-value"));
            Ok((true, ()))
        })
        .expect("the configuration write must succeed");

        let after = read_raw_gateway_config();
        let serialized = after.to_string();
        assert!(
            !serialized.contains("\"fallback\""),
            "a missing fallback must not synthesize a fallback field: {serialized}"
        );
        let _ = before;
    });
}

// ---------------------------------------------------------------------------
// Boundary A RED: the optional default fallback through the public CRUD
// boundary (AC-001 / AC-009)
//
// Every fixture below is raw JSON, so the optional member survives the fixture
// while the typed contract still ignores it: a RED is an observable behavior
// failure (a dropped reference, an accepted invalid save or an unchanged
// ciphertext) and never a missing Rust symbol.
// ---------------------------------------------------------------------------

/// The stored pool with `id` in a raw configuration value.
fn pool_of<'a>(raw: &'a Value, id: &str) -> &'a Value {
    raw["model_pools"]
        .as_array()
        .expect("pool collection must persist")
        .iter()
        .find(|pool| pool["id"] == id)
        .unwrap_or_else(|| panic!("missing pool '{id}': {raw}"))
}

/// The encrypted configuration bytes on disk, so a rejected save is asserted
/// on the real ciphertext instead of on a re-parsed value.
fn encrypted_config_bytes() -> String {
    std::fs::read_to_string(crate::ai_gateway::storage::config_path().expect("config path"))
        .expect("read the encrypted configuration")
}

/// AC-001 / AC-009 RED: the public pool upsert command must save, replace and
/// clear one optional fallback reference through the encrypted configuration
/// boundary. The fallback's own weight defaults to 1 and accepts the inclusive
/// 1..=100 range, and the provider, key, price and sibling-pool values stay
/// semantically unchanged by the pool CRUD.
#[test]
fn pool_fallback_upsert_saves_replaces_and_clears_through_public_command() {
    super::with_temp_home("pool-fallback-public-upsert", |_home| {
        let provider_a = pool_provider(
            "p-a",
            "Provider A",
            "https://provider-a.example.invalid/v1",
            vec![pool_key("key-a", "Key A", "sk-secret-a", true)],
            vec![
                provider_mapping("local-a1", "remote-a1", "chat_completions"),
                provider_mapping("local-a2", "remote-a2", "chat_completions"),
            ],
        );
        let provider_b = pool_provider(
            "p-b",
            "Provider B",
            "https://provider-b.example.invalid/v1",
            vec![pool_key("key-b", "Key B", "sk-secret-b", true)],
            vec![provider_mapping("local-b1", "remote-b1", "chat_completions")],
        );
        let sibling = json!({
            "id": "pool-sibling",
            "name": "Sibling",
            "model": "sibling-model",
            "enabled": true,
            "candidates": [
                pool_candidate("p-b", "local-b1", "remote-b1", "chat_completions", Some(1)),
            ],
        });
        let mut raw = pool_config_v3(0, vec![provider_a, provider_b], vec![sibling]);
        raw["model_prices"] = json!([{
            "provider_id": "p-a",
            "upstream_model": "remote-a1",
            "input": 1.5,
            "cache_read": 0.0,
            "cache_write": 0.0,
            "output": 3.0,
        }]);
        write_raw_gateway_config(&raw);

        let upsert = |value: Value| -> Result<GatewayConfig, String> {
            let pool: ModelPool =
                serde_json::from_value(value).expect("the pool fixture must deserialize");
            crate::ai_gateway::commands::ai_gateway_upsert_model_pool(pool)
        };
        let pool_with = |fallback: Option<Value>| {
            let mut value = json!({
                "id": "pool-fb",
                "name": "Fallback Pool",
                "model": "fallback-model",
                "enabled": true,
                "candidates": [
                    pool_candidate("p-a", "local-a1", "remote-a1", "chat_completions", Some(1)),
                ],
            });
            if let Some(fallback) = fallback {
                value["fallback"] = fallback;
            }
            value
        };

        // Saving a valid fallback persists its exact identity, and an omitted
        // weight persists as the documented default of 1.
        upsert(pool_with(Some(pool_candidate(
            "p-a",
            "local-a2",
            "remote-a2",
            "chat_completions",
            None,
        ))))
        .expect("a valid fallback reference must save");
        let saved = read_raw_gateway_config();
        let stored = pool_of(&saved, "pool-fb");
        assert_eq!(stored["fallback"]["provider_id"], "p-a", "{saved}");
        assert_eq!(stored["fallback"]["local_model"], "local-a2", "{saved}");
        assert_eq!(stored["fallback"]["upstream_model"], "remote-a2", "{saved}");
        assert_eq!(stored["fallback"]["protocol"], "chat_completions", "{saved}");
        assert_eq!(stored["fallback"]["weight"], json!(1), "{saved}");

        // Replacing it swaps the stored identity, and the inclusive weight
        // ceiling is accepted.
        upsert(pool_with(Some(pool_candidate(
            "p-b",
            "local-b1",
            "remote-b1",
            "chat_completions",
            Some(100),
        ))))
        .expect("a replaced fallback reference must save");
        let replaced = read_raw_gateway_config();
        let stored = pool_of(&replaced, "pool-fb");
        assert_eq!(stored["fallback"]["provider_id"], "p-b", "{replaced}");
        assert_eq!(stored["fallback"]["local_model"], "local-b1", "{replaced}");
        assert_eq!(stored["fallback"]["weight"], json!(100), "{replaced}");

        // Clearing it removes the optional member entirely.
        upsert(pool_with(None)).expect("clearing a fallback must save");
        let cleared = read_raw_gateway_config();
        let stored = pool_of(&cleared, "pool-fb");
        assert!(
            stored.get("fallback").is_none(),
            "a cleared fallback must not persist a member: {stored}"
        );

        // The provider, key, price and sibling-pool values are unchanged.
        let providers = cleared["providers"].as_array().expect("providers must persist");
        assert_eq!(providers.len(), 2, "{cleared}");
        let provider_a_row = providers
            .iter()
            .find(|provider| provider["id"] == "p-a")
            .expect("provider a must persist");
        assert_eq!(
            provider_a_row["base_url"],
            "https://provider-a.example.invalid/v1",
            "{cleared}"
        );
        assert_eq!(provider_a_row["keys"][0]["id"], "key-a", "{cleared}");
        assert_eq!(provider_a_row["keys"][0]["value"], "sk-secret-a", "{cleared}");
        assert_eq!(
            provider_a_row["mappings"].as_array().expect("mappings").len(),
            2,
            "{cleared}"
        );
        let prices = cleared["model_prices"].as_array().expect("prices must persist");
        assert_eq!(prices.len(), 1, "{cleared}");
        assert_eq!(prices[0]["provider_id"], "p-a", "{cleared}");
        assert_eq!(prices[0]["upstream_model"], "remote-a1", "{cleared}");
        assert_eq!(prices[0]["input"], json!(1.5), "{cleared}");
        assert_eq!(prices[0]["output"], json!(3.0), "{cleared}");
        let sibling_row = pool_of(&cleared, "pool-sibling");
        assert_eq!(sibling_row["model"], "sibling-model", "{cleared}");
        assert_eq!(
            sibling_row["candidates"].as_array().expect("candidates").len(),
            1,
            "{cleared}"
        );
        assert_eq!(
            sibling_row["candidates"][0]["local_model"],
            "local-b1",
            "{cleared}"
        );
    });
}

/// AC-001 / AC-009 RED: a new unknown fallback reference, a blank fallback
/// identity, a fallback that duplicates a regular candidate, an empty regular
/// selection, a fallback weight outside 1..=100 and an invalid fallback
/// reasoning declaration must all be rejected, and every rejection must leave
/// the original encrypted configuration bytes unchanged.
#[test]
fn pool_fallback_rejects_invalid_references_and_weights_without_changing_encrypted_bytes() {
    super::with_temp_home("pool-fallback-rejections", |_home| {
        let provider = pool_provider(
            "p-a",
            "Provider A",
            "https://provider-a.example.invalid/v1",
            vec![pool_key("key-a", "Key A", "sk-secret-a", true)],
            vec![
                provider_mapping("local-a1", "remote-a1", "chat_completions"),
                provider_mapping("local-a2", "remote-a2", "chat_completions"),
            ],
        );
        write_raw_gateway_config(&pool_config_v3(0, vec![provider], vec![]));
        let baseline = encrypted_config_bytes();

        let upsert = |value: Value| -> Result<GatewayConfig, String> {
            let pool: ModelPool =
                serde_json::from_value(value).expect("the pool fixture must deserialize");
            crate::ai_gateway::commands::ai_gateway_upsert_model_pool(pool)
        };
        let regular = json!([
            pool_candidate("p-a", "local-a1", "remote-a1", "chat_completions", Some(1))
        ]);
        let fallback = |provider_id: &str,
                        local: &str,
                        upstream: &str,
                        protocol: &str,
                        weight: u32| {
            json!({
                "provider_id": provider_id,
                "local_model": local,
                "upstream_model": upstream,
                "protocol": protocol,
                "weight": weight,
            })
        };
        let pool_with = |candidates: Value, fallback: Value| {
            json!({
                "id": "pool-fb",
                "name": "Fallback Pool",
                "model": "fallback-model",
                "enabled": true,
                "candidates": candidates,
                "fallback": fallback,
            })
        };
        let reject = |label: &str, value: Value| {
            let result = upsert(value);
            assert!(result.is_err(), "{label} must be rejected");
            assert_eq!(
                encrypted_config_bytes(),
                baseline,
                "{label} must leave the encrypted configuration unchanged"
            );
        };

        // A new reference that resolves against no provider/mapping/protocol.
        reject(
            "an unknown fallback reference",
            pool_with(
                regular.clone(),
                fallback("p-missing", "local-x", "remote-x", "chat_completions", 1),
            ),
        );
        // The effective protocol is part of the identity, so a mismatched
        // protocol does not resolve either.
        reject(
            "a fallback whose protocol does not match its source",
            pool_with(
                regular.clone(),
                fallback("p-a", "local-a1", "remote-a1", "responses", 1),
            ),
        );
        // A blank identity member.
        reject(
            "a blank fallback local model",
            pool_with(
                regular.clone(),
                fallback("p-a", "  ", "remote-a1", "chat_completions", 1),
            ),
        );
        reject(
            "a blank fallback provider id",
            pool_with(
                regular.clone(),
                fallback("  ", "local-a1", "remote-a1", "chat_completions", 1),
            ),
        );
        reject(
            "a blank fallback upstream model",
            pool_with(
                regular.clone(),
                fallback("p-a", "local-a1", "  ", "chat_completions", 1),
            ),
        );
        // The same exact source identity may not appear in both roles.
        reject(
            "a fallback duplicating a regular candidate",
            pool_with(
                regular.clone(),
                fallback("p-a", "local-a1", "remote-a1", "chat_completions", 1),
            ),
        );
        // A pool still requires at least one regular candidate.
        reject(
            "an empty regular selection",
            pool_with(
                json!([]),
                fallback("p-a", "local-a1", "remote-a1", "chat_completions", 1),
            ),
        );
        // The weight range stays inclusive 1..=100.
        reject(
            "a fallback weight below the floor",
            pool_with(
                regular.clone(),
                fallback("p-a", "local-a1", "remote-a1", "chat_completions", 0),
            ),
        );
        reject(
            "a fallback weight above the ceiling",
            pool_with(
                regular.clone(),
                fallback("p-a", "local-a1", "remote-a1", "chat_completions", 101),
            ),
        );
        // An invalid reasoning declaration is rejected on the fallback too. The
        // fallback references its own distinct real mapping here, so the
        // rejection comes from the declaration and never from the duplicate
        // identity rule exercised above.
        reject(
            "an unknown fallback reasoning profile",
            pool_with(
                regular.clone(),
                json!({
                    "provider_id": "p-a",
                    "local_model": "local-a2",
                    "upstream_model": "remote-a2",
                    "protocol": "chat_completions",
                    "weight": 1,
                    "reasoning": {
                        "profile": "made_up_profile",
                        "mappings": [
                            {"effort": "high", "action": {"kind": "enum", "value": "high"}}
                        ],
                    },
                }),
            ),
        );
        reject(
            "a fallback reasoning profile unsupported by its protocol",
            pool_with(
                regular.clone(),
                json!({
                    "provider_id": "p-a",
                    "local_model": "local-a2",
                    "upstream_model": "remote-a2",
                    "protocol": "chat_completions",
                    "weight": 1,
                    "reasoning": {
                        "profile": "anthropic_manual_budget",
                        "budget": {"min": 1024, "max": 4096},
                        "mappings": [
                            {"effort": "high", "action": {"kind": "budget", "budget_tokens": 2048}}
                        ],
                    },
                }),
            ),
        );

        // The inclusive lower bound is accepted, so only the out-of-range
        // weights are rejected. The accepted fallback references its own real
        // mapping, because the same identity may not hold both roles.
        let accepted = upsert(pool_with(
            regular.clone(),
            fallback("p-a", "local-a2", "remote-a2", "chat_completions", 1),
        ));
        assert!(
            accepted.is_ok(),
            "the inclusive lower weight bound must be accepted: {accepted:?}"
        );
    });
}

/// AC-001 / AC-009 RED: a saved fallback whose source later disappears stays
/// retainable across an unrelated save, stays visible as unavailable with its
/// own reason, is never substituted by another source and stays clearable.
#[test]
fn pool_fallback_unavailable_source_stays_retainable_clearable_and_unsubstituted() {
    super::with_temp_home("pool-fallback-unavailable", |_home| {
        let provider = pool_provider(
            "p-a",
            "Provider A",
            "https://provider-a.example.invalid/v1",
            vec![pool_key("key-a", "Key A", "sk-secret-a", true)],
            vec![
                provider_mapping("local-a1", "remote-a1", "chat_completions"),
                provider_mapping("local-fb", "remote-fb", "chat_completions"),
            ],
        );
        let pool = json!({
            "id": "pool-fb",
            "name": "Fallback Pool",
            "model": "fallback-model",
            "enabled": true,
            "candidates": [
                pool_candidate("p-a", "local-a1", "remote-a1", "chat_completions", Some(1)),
            ],
            "fallback": pool_candidate("p-a", "local-fb", "remote-fb", "chat_completions", Some(1)),
        });
        write_raw_gateway_config(&pool_config_v3(0, vec![provider], vec![pool]));

        // The saved fallback's source later disappears: its mapping row is gone
        // while the stored reference stays in the configuration.
        let mut without_source = read_raw_gateway_config();
        let providers = without_source
            .get_mut("providers")
            .and_then(Value::as_array_mut)
            .expect("providers must persist");
        providers[0]["mappings"]
            .as_array_mut()
            .expect("mappings must persist")
            .retain(|mapping| mapping["local_model"] != "local-fb");
        write_raw_gateway_config(&without_source);

        // An unrelated save keeps the unchanged reference instead of dropping it
        // or substituting another source.
        let renamed = json!({
            "id": "pool-fb",
            "name": "Renamed Pool",
            "model": "fallback-model",
            "enabled": true,
            "candidates": [
                pool_candidate("p-a", "local-a1", "remote-a1", "chat_completions", Some(1)),
            ],
            "fallback": pool_candidate("p-a", "local-fb", "remote-fb", "chat_completions", Some(1)),
        });
        crate::ai_gateway::commands::ai_gateway_upsert_model_pool(
            serde_json::from_value(renamed).expect("the pool fixture must deserialize"),
        )
        .expect("an unchanged saved fallback must stay retainable");
        let after = read_raw_gateway_config();
        let stored = pool_of(&after, "pool-fb");
        assert_eq!(stored["fallback"]["provider_id"], "p-a", "{after}");
        assert_eq!(stored["fallback"]["local_model"], "local-fb", "{after}");
        assert_eq!(stored["fallback"]["upstream_model"], "remote-fb", "{after}");

        // The status keeps the fallback separate from the regular candidates and
        // reports its own availability and reason.
        let statuses = crate::ai_gateway::commands::ai_gateway_model_pools_status()
            .expect("pool status");
        let value = serde_json::to_value(&statuses).expect("serialize pool status");
        let entry = value
            .as_array()
            .expect("status array")
            .iter()
            .find(|pool| pool["id"] == "pool-fb")
            .unwrap_or_else(|| panic!("missing pool status row: {value}"))
            .clone();
        let fallback = &entry["fallback"];
        assert_eq!(fallback["provider_id"], "p-a", "{entry}");
        assert_eq!(fallback["local_model"], "local-fb", "{entry}");
        assert_eq!(fallback["upstream_model"], "remote-fb", "{entry}");
        assert_ne!(
            fallback["availability"], "ready",
            "an unavailable fallback must not report ready: {entry}"
        );
        assert!(
            fallback["reason"]
                .as_str()
                .is_some_and(|reason| !reason.trim().is_empty()),
            "an unavailable fallback must carry its own reason: {entry}"
        );
        let candidates = entry["candidates"].as_array().expect("regular candidate rows");
        assert_eq!(
            candidates.len(),
            1,
            "the regular candidate list must stay regular-only: {entry}"
        );
        assert!(
            candidates
                .iter()
                .all(|row| row["local_model"] != "local-fb"),
            "the fallback must not appear as a regular candidate: {entry}"
        );

        // Clearing the reference works and leaves no fallback behind.
        let cleared = json!({
            "id": "pool-fb",
            "name": "Renamed Pool",
            "model": "fallback-model",
            "enabled": true,
            "candidates": [
                pool_candidate("p-a", "local-a1", "remote-a1", "chat_completions", Some(1)),
            ],
        });
        crate::ai_gateway::commands::ai_gateway_upsert_model_pool(
            serde_json::from_value(cleared).expect("the pool fixture must deserialize"),
        )
        .expect("clearing a fallback must save");
        let after = read_raw_gateway_config();
        let stored = pool_of(&after, "pool-fb");
        assert!(
            stored.get("fallback").is_none(),
            "a cleared fallback must not persist a member: {stored}"
        );
    });
}

// ---------------------------------------------------------------------------
// Boundary B RED: the explicit last-stage fallback at the runtime routing
// boundary (AC-002 / AC-003 / AC-004 / AC-006 / AC-007 / AC-009)
//
// Every case below drives the real loopback listener, the encrypted
// configuration and the in-process mock upstreams. A pool record carries its
// optional fallback as a raw JSON member, so a RED is an observable routing
// outcome (a 502 instead of a served fallback, a missing source-attributed row
// or a replayed stream) and never a missing Rust symbol. No live provider and
// no real terminal projection is involved.
// ---------------------------------------------------------------------------

/// One pool record with exactly one regular candidate and one configured
/// fallback, each referencing its own real source identity.
fn pool_with_fallback(id: &str, alias: &str, regular: Value, fallback: Value) -> Value {
    json!({
        "id": id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [regular],
        "fallback": fallback,
    })
}

/// The stored identity of one pool candidate, for the in-memory cooldown seam.
fn candidate_identity(
    pool_id: &str,
    provider_id: &str,
    local_model: &str,
    upstream_model: &str,
) -> PoolCandidateIdentity {
    PoolCandidateIdentity {
        pool_id: pool_id.to_string(),
        provider_id: provider_id.to_string(),
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        protocol: UpstreamProtocol::ChatCompletions,
        reasoning: None,
    }
}

/// AC-002 / AC-003: a pool whose only regular candidate is cooling from an
/// earlier quota failure has an empty eligible regular set, so the configured
/// fallback serves the request and is the only source that is contacted.
#[tokio::test]
async fn pool_fallback_serves_when_the_only_regular_candidate_is_cooling() {
    let _home = temp_home("pool-fallback-cooling-regular");
    let port = free_port().await;
    let tag = tag();
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let (regular_url, regular_log) = spawn_mock_upstream(|_| {
        MockReply::Json(429, json!({"error": {"message": "quota exhausted"}}))
    })
    .await;
    let (fallback_url, fallback_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &pool_id,
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    // The regular candidate is cooling from an earlier quota failure, so it is
    // excluded at admission and only the fallback is eligible.
    cool_pool_candidate(
        &candidate_identity(
            &pool_id,
            &regular_provider_id,
            "local-regular",
            "remote-regular",
        ),
        Some(Duration::from_secs(60)),
    );
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "the configured fallback must serve an exhausted pool: {text}"
    );
    assert!(
        models_of(&regular_log).is_empty(),
        "the cooling regular candidate must not be attempted: {:?}",
        models_of(&regular_log)
    );
    let models = models_of(&fallback_log);
    assert_eq!(
        models.iter().filter(|model| model.as_str() == "remote-fallback").count(),
        1,
        "the fallback must receive exactly one pass: {models:?}"
    );
    stop_server().await.expect("stop the listener");
}

/// AC-002 / AC-004: a pool whose regular candidate references a source that
/// does not exist has an empty eligible regular runtime set, so the configured
/// fallback serves the request. Once the regular source appears, the next
/// independent request gives the eligible regular candidate priority again and
/// the fallback is never used a second time.
#[tokio::test]
async fn pool_fallback_serves_an_empty_regular_set_and_a_recovered_regular_regains_priority() {
    let _home = temp_home("pool-fallback-empty-regular-set");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            provider_id.as_str(),
            "local-late",
            "remote-late",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    // The regular source does not exist yet, so the eligible regular runtime
    // set is empty and the fallback serves the request.
    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "the configured fallback must serve an empty regular set: {text}"
    );
    let models = models_of(&log);
    assert_eq!(
        models.iter().filter(|model| model.as_str() == "remote-fallback").count(),
        1,
        "the fallback must serve exactly once: {models:?}"
    );
    assert_eq!(
        models.iter().filter(|model| model.as_str() == "remote-late").count(),
        0,
        "the missing regular source must never be attempted: {models:?}"
    );

    // The regular source appears. The next independent request gives the
    // eligible regular candidate priority again.
    let mut recovered = read_raw_gateway_config();
    let providers = recovered
        .get_mut("providers")
        .and_then(Value::as_array_mut)
        .expect("providers must persist");
    providers[0]["mappings"]
        .as_array_mut()
        .expect("mappings must persist")
        .push(provider_mapping("local-late", "remote-late", "chat_completions"));
    write_raw_gateway_config(&recovered);

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "the recovered regular candidate must serve: {text}"
    );
    let models = models_of(&log);
    assert_eq!(
        models.iter().filter(|model| model.as_str() == "remote-late").count(),
        1,
        "the recovered regular candidate must serve: {models:?}"
    );
    assert_eq!(
        models.iter().filter(|model| model.as_str() == "remote-fallback").count(),
        1,
        "the fallback must not serve a request an eligible regular candidate can serve: {models:?}"
    );
    stop_server().await.expect("stop the listener");
}

/// AC-003: when the regular candidate fails during forwarding, its own finite
/// attempts and permitted probe finish first and the configured fallback still
/// receives exactly one pass, without a second delayed retry cycle.
#[tokio::test]
async fn pool_fallback_serves_after_the_regular_candidate_exhausts_its_finite_retries() {
    let _home = temp_home("pool-fallback-after-retries");
    let port = free_port().await;
    let tag = tag();
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");
    let alias = format!("free-model-{tag}");
    let (regular_url, regular_log) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "upstream failure"}}))
    })
    .await;
    let (fallback_url, fallback_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "the fallback must serve after the regular retries are exhausted: {text}"
    );
    let regular_models = models_of(&regular_log);
    assert!(
        regular_models
            .iter()
            .any(|model| model.as_str() == "remote-regular"),
        "the regular candidate must be attempted before the fallback: {regular_models:?}"
    );
    let fallback_models = models_of(&fallback_log);
    assert_eq!(
        fallback_models
            .iter()
            .filter(|model| model.as_str() == "remote-fallback")
            .count(),
        1,
        "the fallback must receive exactly one pass with no second retry cycle: {fallback_models:?}"
    );
    stop_server().await.expect("stop the listener");
}

/// AC-003 / AC-004: healthy regular candidates keep their weighted order and
/// the configured fallback is never contacted while an eligible regular
/// candidate can serve the request.
#[tokio::test]
async fn pool_healthy_regular_candidates_never_reach_the_configured_fallback() {
    let _home = temp_home("pool-fallback-never-reached");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (provider_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            provider_mapping("local-a", "remote-a", "chat_completions"),
            provider_mapping("local-b", "remote-b", "chat_completions"),
            provider_mapping("local-fallback", "remote-fallback", "chat_completions"),
        ],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            provider_id.as_str(),
            "local-a",
            "remote-a",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "a healthy regular candidate must serve the request: {text}"
    );
    let models = models_of(&log);
    assert_eq!(
        models.iter().filter(|model| model.as_str() == "remote-a").count(),
        1,
        "the eligible regular candidate must serve exactly once: {models:?}"
    );
    assert_eq!(
        models.iter().filter(|model| model.as_str() == "remote-fallback").count(),
        0,
        "the fallback must never be contacted while a regular candidate is eligible: {models:?}"
    );
    stop_server().await.expect("stop the listener");
}

/// AC-006: the fallback attempt is attributed to the real source in exactly
/// one terminal row. The public alias stays the logged local model while the
/// provider and upstream model identify the fallback that actually served the
/// request, and the cooling regular candidate owns no row at all.
#[tokio::test]
async fn pool_fallback_attempt_is_attributed_to_the_real_source_in_one_terminal_row() {
    let _home = temp_home("pool-fallback-log-attribution");
    let port = free_port().await;
    let tag = tag();
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let (regular_url, _regular_log) = spawn_mock_upstream(|_| {
        MockReply::Json(429, json!({"error": {"message": "quota exhausted"}}))
    })
    .await;
    let (fallback_url, _fallback_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &pool_id,
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    cool_pool_candidate(
        &candidate_identity(
            &pool_id,
            &regular_provider_id,
            "local-regular",
            "remote-regular",
        ),
        Some(Duration::from_secs(60)),
    );
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 200,
        "the configured fallback must serve an exhausted pool: {text}"
    );
    let records = wait_for_usage_logs(1).await;
    let terminals: Vec<_> = records.iter().filter(|record| record.terminal).collect();
    assert_eq!(
        terminals.len(),
        1,
        "the request must have exactly one terminal row: {records:?}"
    );
    let terminal = terminals[0];
    assert_eq!(
        terminal.local_model, alias,
        "the public alias must stay the logged local model: {terminal:?}"
    );
    assert_eq!(
        terminal.provider_id, fallback_provider_id,
        "the real fallback provider must be logged: {terminal:?}"
    );
    assert_eq!(
        terminal.upstream_model, "remote-fallback",
        "the real fallback upstream model must be logged: {terminal:?}"
    );
    assert_eq!(terminal.status, 200, "the fallback attempt must be logged: {terminal:?}");
    assert_eq!(
        terminal.result,
        UsageResult::Success,
        "the served fallback must be logged as a success: {terminal:?}"
    );
    assert!(
        !records
            .iter()
            .any(|record| record.upstream_model == "remote-regular"),
        "the cooling regular candidate must own no usage row: {records:?}"
    );
    stop_server().await.expect("stop the listener");
}

/// AC-007: a stream whose regular candidate fails before its first client
/// byte is completed exactly once by the configured fallback, in the ingress
/// SSE format.
#[tokio::test]
async fn pool_fallback_serves_a_streaming_request_after_a_pre_output_failure() {
    let _home = temp_home("pool-fallback-stream-pre-byte");
    let port = free_port().await;
    let tag = tag();
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");
    let alias = format!("free-model-{tag}");
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"from-fallback\"}}]}\n\ndata: [DONE]\n\n"
        .to_string();
    let sse_for_mock = sse.clone();
    let (regular_url, regular_log) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "upstream failure"}}))
    })
    .await;
    let (fallback_url, fallback_log) =
        spawn_mock_upstream(move |_| MockReply::Stream(sse_for_mock.clone())).await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({
            "model": alias,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
        })),
    )
    .await;
    assert_eq!(
        status, 200,
        "the fallback must complete the pre-output failed stream: {text}"
    );
    assert!(
        content_type.contains("text/event-stream"),
        "the fallback must answer in the ingress SSE format: {content_type}"
    );
    assert!(
        text.contains("from-fallback"),
        "the fallback stream must reach the client: {text}"
    );
    assert_eq!(
        text.matches("[DONE]").count(),
        1,
        "the fallback stream must complete exactly once: {text}"
    );
    assert!(
        !models_of(&regular_log).is_empty(),
        "the regular candidate must have failed before the first byte: {:?}",
        models_of(&regular_log)
    );
    assert_eq!(
        models_of(&fallback_log)
            .iter()
            .filter(|model| model.as_str() == "remote-fallback")
            .count(),
        1,
        "the fallback must receive exactly one streaming pass: {:?}",
        models_of(&fallback_log)
    );
    stop_server().await.expect("stop the listener");
}

/// AC-007: a stream that already emitted client bytes and then fails is never
/// replayed through the configured fallback and never emits a second success
/// terminator, so the existing post-output boundary is preserved.
#[tokio::test]
async fn pool_stream_failure_after_client_output_never_replays_through_the_fallback() {
    let _home = temp_home("pool-fallback-stream-post-byte");
    let port = free_port().await;
    let tag = tag();
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");
    let alias = format!("free-model-{tag}");
    let partial = "data: {\"choices\":[{\"delta\":{\"content\":\"partial-regular\"}}]}\n\n".to_string();
    let declared = partial.len() + 500;
    let partial_for_mock = partial.clone();
    let (regular_url, regular_log) =
        spawn_mock_upstream(move |_| MockReply::PartialStream(partial_for_mock.clone(), declared))
            .await;
    let (fallback_url, fallback_log) = spawn_mock_upstream(|_| {
        MockReply::Stream(
            "data: {\"choices\":[{\"delta\":{\"content\":\"from-fallback\"}}]}\n\ndata: [DONE]\n\n"
                .to_string(),
        )
    })
    .await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({
            "model": alias,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
        })),
    )
    .await;
    assert_eq!(
        status, 200,
        "the SSE status is committed before the mid-stream failure: {text}"
    );
    assert!(
        text.contains("partial-regular"),
        "the partial bytes must reach the caller: {text}"
    );
    assert_eq!(
        text.matches("partial-regular").count(),
        1,
        "the failed stream must never be replayed: {text}"
    );
    assert!(
        !text.contains("from-fallback"),
        "no fallback may serve after the first client byte: {text}"
    );
    assert!(
        !text.contains("[DONE]"),
        "an abnormal stream must never emit a success terminator: {text}"
    );
    assert!(
        models_of(&fallback_log).is_empty(),
        "the fallback must never be contacted after the first client byte: {:?}",
        models_of(&fallback_log)
    );
    assert_eq!(
        models_of(&regular_log)
            .iter()
            .filter(|model| model.as_str() == "remote-regular")
            .count(),
        1,
        "only the failing regular candidate may be contacted: {:?}",
        models_of(&regular_log)
    );
    stop_server().await.expect("stop the listener");
}

/// AC-009: a pool without a configured fallback keeps its existing exhausted
/// behavior, so the new optional member never changes a legacy pool route.
#[tokio::test]
async fn pool_without_a_fallback_keeps_its_existing_exhaustion_response() {
    let _home = temp_home("pool-fallback-absent-legacy");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let (provider_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;

    let provider = pool_provider(
        &provider_id,
        "Provider One",
        &provider_url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let pool = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(
                provider_id.as_str(),
                "local-regular",
                "remote-regular",
                "chat_completions",
                Some(100),
            ),
        ],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    cool_pool_candidate(
        &candidate_identity(
            &pool_id,
            &provider_id,
            "local-regular",
            "remote-regular",
        ),
        Some(Duration::from_secs(60)),
    );
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    assert_eq!(
        status, 502,
        "a pool without a fallback must keep its existing exhausted response: {text}"
    );
    assert!(
        text.contains("all_providers_unavailable"),
        "the exhausted pool must keep the standard error code: {text}"
    );
    assert!(
        models_of(&log).is_empty(),
        "no source may be contacted for an exhausted pool without a fallback: {:?}",
        models_of(&log)
    );
    stop_server().await.expect("stop the listener");
}

// ===========================================================================
// Step 1 of `20261020-model-pool-default-fallback` (REQ-002/003/004/006;
// AC-003/AC-005/AC-006/AC-007/AC-009): the explicit fallback stage reached
// after the regular candidates are exhausted.
// ===========================================================================

/// The shared upstream attempt order of one request, so the exact sequence of
/// the regular candidate's attempts and the fallback's single pass is
/// observable.
type AttemptOrder = Arc<Mutex<Vec<&'static str>>>;

/// Run the relay's own connection handler on the calling runtime, so a paused
/// clock drives the retry waits and the client can be disconnected at an exact
/// moment.
async fn spawn_relay_connection(
    path: &str,
    body: Value,
) -> (TcpStream, tokio::task::JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind relay loopback");
    let addr = listener.local_addr().expect("relay loopback address");
    let accept = tokio::spawn(async move {
        listener
            .accept()
            .await
            .expect("accept relay loopback")
            .0
    });
    let mut client = TcpStream::connect(addr).await.expect("connect relay loopback");
    let server = accept.await.expect("relay accept task");
    let handler = tokio::spawn(crate::ai_gateway::runtime_http::handle_connection(server));
    tokio::task::yield_now().await;
    let payload = serde_json::to_vec(&body).expect("encode relay request");
    let request = format!(
        "POST {path} HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: Bearer local-key\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        payload.len()
    );
    let mut frame = request.into_bytes();
    frame.extend_from_slice(&payload);
    client
        .write_all(&frame)
        .await
        .expect("write relay request");
    client.flush().await.expect("flush relay request");
    (client, handler)
}

/// Read one complete relay response and split its status code from its body.
async fn read_relay_response(client: &mut TcpStream) -> (u16, String) {
    let mut raw = Vec::new();
    client
        .read_to_end(&mut raw)
        .await
        .expect("read relay response");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    (status, text)
}

/// One recorded upstream arrival with the virtual instant of its request.
#[derive(Clone)]
struct TimedArrival {
    model: String,
    at: tokio::time::Instant,
}

/// Spawn a loopback upstream on the calling runtime that records the virtual
/// arrival instant of every request. The shared mock helper answers on the
/// application runtime, whose clock is never paused, so a paused-clock case
/// measures the moment a stage began with this helper instead.
async fn spawn_timed_mock_upstream<F>(behavior: F) -> (String, Arc<Mutex<Vec<TimedArrival>>>)
where
    F: Fn(&Captured) -> MockReply + Send + Sync + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind timed mock");
    let addr = listener.local_addr().expect("timed mock addr");
    let arrivals: Arc<Mutex<Vec<TimedArrival>>> = Arc::new(Mutex::new(Vec::new()));
    let arrivals_for_server = arrivals.clone();
    let behavior = Arc::new(behavior);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let arrivals = arrivals_for_server.clone();
            let behavior = behavior.clone();
            tokio::spawn(async move {
                let Ok(request) = crate::ai_gateway::runtime_http::read_http_request(&mut stream).await
                else {
                    return;
                };
                let captured = Captured {
                    method: request.method,
                    path: request.path,
                    headers: request.headers,
                    body: request.body,
                };
                let model = captured_model(&captured);
                arrivals.lock().expect("timed mock arrivals").push(TimedArrival {
                    model,
                    at: tokio::time::Instant::now(),
                });
                let (status, value, extra_headers) = match behavior(&captured) {
                    MockReply::Json(status, value) => (status, value, Vec::new()),
                    MockReply::JsonWithHeaders(status, value, headers) => (status, value, headers),
                    _other => panic!("the timed mock upstream answers a JSON reply only"),
                };
                let body = serde_json::to_vec(&value).unwrap_or_default();
                let mut header = format!(
                    "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
                    body.len(),
                );
                for (name, header_value) in &extra_headers {
                    header.push_str(&format!("{name}: {header_value}\r\n"));
                }
                header.push_str("\r\n");
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(&body).await;
            });
        }
    });
    (format!("http://{}", addr), arrivals)
}

/// One Chat completion body served by the configured fallback.
fn fallback_success_body() -> Value {
    json!({
        "id": "chatcmpl-fallback",
        "object": "chat.completion",
        "model": "remote-fallback",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "from-fallback"},
            "finish_reason": "stop"}],
    })
}

/// One provider whose only mapping is unrelated to the pool's regular
/// reference, so the regular candidate resolves as unavailable and the
/// configured fallback is the only eligible source of the request.
fn provider_without_the_pool_mapping(id: &str, name: &str, base_url: &str) -> Value {
    pool_provider(
        id,
        name,
        base_url,
        vec![pool_key("key-regular", "Regular", "sk-regular-secret", true)],
        vec![provider_mapping(
            "local-other",
            "remote-other",
            "chat_completions",
        )],
    )
}

/// AC-003 / REQ-002: the shared 120-second actual retry-wait budget stops the
/// regular candidate's retries, and the configured fallback still receives its
/// first pass. The wait runs on the existing paused clock seam, so the request
/// costs no real minutes, and the captured upstream order proves the fallback
/// ran after the regular candidate's exhausted retries without a second
/// delayed retry cycle of its own.
#[tokio::test(start_paused = true)]
async fn pool_fallback_serves_after_the_regular_retry_wait_budget_is_spent() {
    let _home = temp_home("pool-fallback-wait-budget");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");
    let order: AttemptOrder = Arc::new(Mutex::new(Vec::new()));
    let _ticker = super::spawn_paused_clock_ticker();

    let (regular_url, regular_log) = {
        let order = order.clone();
        spawn_mock_upstream(move |_| {
            order.lock().expect("attempt order").push("regular");
            MockReply::JsonWithHeaders(
                500,
                json!({"error": {"message": "upstream failure"}}),
                vec![("retry-after-ms".to_string(), "60000".to_string())],
            )
        })
        .await
    };
    let (fallback_url, fallback_arrivals) = {
        let order = order.clone();
        spawn_timed_mock_upstream(move |_| {
            order.lock().expect("attempt order").push("fallback");
            MockReply::Json(200, fallback_success_body())
        })
        .await
    };

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular-secret", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback-secret", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));

    let started = tokio::time::Instant::now();
    let (mut client, handler) = spawn_relay_connection(
        "/v1/chat/completions",
        json!({"model": alias, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    let (status, text) = read_relay_response(&mut client).await;
    let elapsed = started.elapsed();
    handler.await.expect("relay task").expect("relay handler");

    assert_eq!(
        status, 200,
        "the fallback must serve after the regular wait budget is spent: {text}"
    );
    assert!(
        text.contains("from-fallback"),
        "the fallback response must reach the client: {text}"
    );
    let order = order.lock().expect("attempt order").clone();
    assert_eq!(
        order,
        vec!["regular", "regular", "regular", "fallback"],
        "the fallback must run after the regular candidate's exhausted retries: {order:?}"
    );
    assert_eq!(
        models_of(&regular_log).len(),
        3,
        "the regular candidate must be attempted once plus its two 60s waits inside the inclusive 120s budget: {:?}",
        models_of(&regular_log)
    );
    assert_eq!(
        fallback_arrivals.lock().expect("timed mock arrivals").len(),
        1,
        "the fallback must receive exactly one pass without a delayed retry cycle of its own"
    );
    // The paused clock also advances while a real loopback round trip is in
    // flight, so the moment the fallback pass began is measured on the mock's
    // own arrival instead of the whole-request elapsed time.
    let arrivals = fallback_arrivals.lock().expect("timed mock arrivals").clone();
    assert_eq!(
        arrivals[0].model, "remote-fallback",
        "the fallback pass must forward the fallback's own upstream model"
    );
    let budget_spent = arrivals[0].at - started;
    assert!(
        budget_spent >= Duration::from_secs(119) && budget_spent <= Duration::from_secs(140),
        "the fallback pass must start right after the inclusive 120s wait budget is spent, paused elapsed was {budget_spent:?}"
    );
    assert!(
        elapsed <= Duration::from_secs(180),
        "the request must stay bounded by the wait budget, paused elapsed was {elapsed:?}"
    );
}

/// AC-007 / REQ-004: a client that disconnects while the regular candidate is
/// waiting for its next retry ends the request at once. No later attempt runs,
/// no terminal row is written and no mapping or key health is published, so a
/// configured fallback can never be entered after the caller is gone.
#[tokio::test]
async fn pool_fallback_is_never_attempted_after_a_disconnect_during_the_regular_retry_wait() {
    let _home = temp_home("pool-fallback-disconnect-retry");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");
    let arrived = Arc::new(tokio::sync::Notify::new());

    let (regular_url, regular_log) = {
        let arrived = arrived.clone();
        spawn_mock_upstream(move |_| {
            arrived.notify_one();
            MockReply::JsonWithHeaders(
                500,
                json!({"error": {"message": "upstream failure"}}),
                vec![("retry-after-ms".to_string(), "30000".to_string())],
            )
        })
        .await
    };
    let (fallback_url, fallback_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, fallback_success_body())).await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular-secret", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback-secret", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));

    let (client, mut handler) = spawn_relay_connection(
        "/v1/chat/completions",
        json!({"model": alias, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), arrived.notified())
        .await
        .expect("the regular attempt must begin");
    drop(client);
    let exited = tokio::time::timeout(Duration::from_secs(5), &mut handler).await;
    assert!(
        exited.is_ok(),
        "a disconnected client must end the request instead of waiting out its retry delay"
    );

    assert_eq!(
        models_of(&regular_log).len(),
        1,
        "a cancelled request must not retry: {:?}",
        models_of(&regular_log)
    );
    assert!(
        models_of(&fallback_log).is_empty(),
        "no fallback attempt may follow a disconnected client: {:?}",
        models_of(&fallback_log)
    );
    let records = default_usage_store().all_records().unwrap_or_default();
    assert!(
        records.is_empty(),
        "a cancelled request must write no terminal row: {records:?}"
    );
    let stored = stored_provider(&regular_provider_id);
    assert!(
        !stored.mappings[0].auto_disabled,
        "a cancellation must not disable the mapping row: {:?}",
        stored.mappings[0]
    );
    assert_eq!(
        stored.mappings[0].consecutive_failures, 0,
        "a cancellation must not count a mapping failure: {:?}",
        stored.mappings[0]
    );
    assert!(
        !stored.keys[0].auto_marked,
        "a cancellation must not mark the key: {:?}",
        stored.keys[0]
    );
}

/// AC-003 / REQ-003: an in-request credential failure that leaves every
/// regular key unusable ends the regular stage, and the configured fallback
/// still receives exactly one pass with its own finite in-pass key rotation.
#[tokio::test]
async fn pool_fallback_serves_after_in_request_failures_leave_every_regular_key_unusable() {
    let _home = temp_home("pool-fallback-keyless-regular");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");

    let (regular_url, regular_log) = spawn_mock_upstream(|_| {
        MockReply::Json(401, json!({"error": {"message": "unauthorized"}}))
    })
    .await;
    let (fallback_url, fallback_log) = spawn_mock_upstream(|captured| match auth_of(captured) {
        Some("Bearer sk-fallback-first") => {
            MockReply::Json(401, json!({"error": {"message": "unauthorized"}}))
        }
        Some("Bearer sk-fallback-second") => MockReply::Json(200, fallback_success_body()),
        other => MockReply::Json(
            500,
            json!({"error": {"message": format!("unexpected authorization {other:?}")}}),
        ),
    })
    .await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular-secret", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![
            pool_key("key-fallback-first", "Fallback First", "sk-fallback-first", true),
            pool_key("key-fallback-second", "Fallback Second", "sk-fallback-second", true),
        ],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    stop_server().await.expect("stop the listener");

    assert_eq!(
        status, 200,
        "the fallback must serve after every regular key became unusable: {text}"
    );
    assert!(
        text.contains("from-fallback"),
        "the fallback response must reach the client: {text}"
    );
    let regular = regular_log.lock().expect("mock log").clone();
    assert_eq!(
        regular.len(),
        1,
        "the keyless regular candidate must not be retried: {}",
        regular.len()
    );
    let fallback = fallback_log.lock().expect("mock log").clone();
    assert_eq!(
        fallback.len(),
        2,
        "the fallback must rotate in-pass over its own usable keys: {}",
        fallback.len()
    );
    assert_eq!(auth_of(&fallback[0]), Some("Bearer sk-fallback-first"));
    assert_eq!(auth_of(&fallback[1]), Some("Bearer sk-fallback-second"));
    assert_eq!(captured_model(&fallback[1]), "remote-fallback");

    let stored_regular = stored_provider(&regular_provider_id);
    assert!(
        stored_regular.keys[0].auto_marked,
        "the failing regular key must be marked: {:?}",
        stored_regular.keys[0]
    );
    assert!(
        !stored_regular.mappings[0].auto_disabled,
        "a credential failure is key-scoped and must not disable the mapping row: {:?}",
        stored_regular.mappings[0]
    );
    let stored_fallback = stored_provider(&fallback_provider_id);
    assert!(
        stored_fallback.keys[0].auto_marked,
        "the failing fallback key must be marked: {:?}",
        stored_fallback.keys[0]
    );
    assert!(
        !stored_fallback.keys[1].auto_marked,
        "the fallback's other key must stay untouched: {:?}",
        stored_fallback.keys[1]
    );
}

/// AC-006 / REQ-003: a model-scoped quota failure of the fallback cools that
/// candidate only. The shared key stays unmarked, the fallback's own mapping
/// row keeps no failure count, and the sibling mapping of the same provider
/// stays enabled and untouched.
#[tokio::test]
async fn pool_fallback_candidate_failure_cools_only_the_fallback_candidate() {
    let _home = temp_home("pool-fallback-candidate-scope");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");

    let (fallback_url, fallback_log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            429,
            json!({"error": {"type": "FreeUsageLimitError", "message": "free usage limit reached"}}),
        )
    })
    .await;
    let regular_provider = provider_without_the_pool_mapping(
        &regular_provider_id,
        "Regular Provider",
        "https://regular.example.invalid/v1",
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback-secret", true)],
        vec![
            provider_mapping("local-fallback", "remote-fallback", "chat_completions"),
            provider_mapping("local-sibling", "remote-sibling", "chat_completions"),
        ],
    );
    let pool = pool_with_fallback(
        &pool_id,
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    stop_server().await.expect("stop the listener");

    assert_eq!(
        status, 502,
        "an exhausted fallback must answer with the standard unavailability envelope: {text}"
    );
    assert!(
        text.contains("all_providers_unavailable"),
        "the exhausted fallback must keep the standard error code: {text}"
    );
    assert!(
        text.contains("local-fallback") && text.contains("remote-fallback"),
        "the failure reason must name the fallback that was attempted: {text}"
    );
    assert!(
        !text.contains("sk-fallback-secret"),
        "the failure reason must stay free of credentials: {text}"
    );
    assert_eq!(
        models_of(&fallback_log).len(),
        1,
        "the fallback must receive exactly one pass: {:?}",
        models_of(&fallback_log)
    );

    let status_row = pool_status_row(&pool_id);
    let fallback_status = status_row
        .fallback
        .expect("the pool must report its separate fallback status");
    assert_eq!(
        fallback_status.availability, "cooling",
        "the failed fallback candidate must own the cooldown: {fallback_status:?}"
    );
    assert!(
        fallback_status.cooling_until.is_some(),
        "the cooling fallback must publish its deadline: {fallback_status:?}"
    );
    assert!(
        !status_row.candidates[0].ready,
        "the excluded regular candidate must stay unavailable: {:?}",
        status_row.candidates[0]
    );

    let stored = stored_provider(&fallback_provider_id);
    assert!(
        !stored.keys[0].auto_marked,
        "a candidate-scoped failure must not mark the shared key: {:?}",
        stored.keys[0]
    );
    let fallback_mapping = stored
        .mappings
        .iter()
        .find(|mapping| mapping.local_model == "local-fallback")
        .expect("the fallback mapping must persist");
    assert!(
        !fallback_mapping.auto_disabled,
        "a candidate-scoped failure must not disable the mapping row: {fallback_mapping:?}"
    );
    assert_eq!(
        fallback_mapping.consecutive_failures, 0,
        "a candidate-scoped failure must not count a mapping failure: {fallback_mapping:?}"
    );
    let sibling = stored
        .mappings
        .iter()
        .find(|mapping| mapping.local_model == "local-sibling")
        .expect("the sibling mapping must persist");
    assert!(
        sibling.enabled && !sibling.auto_disabled,
        "the same-provider sibling mapping must stay enabled: {sibling:?}"
    );
    assert_eq!(
        sibling.consecutive_failures, 0,
        "the same-provider sibling mapping must own no failure: {sibling:?}"
    );
}

/// AC-006 / REQ-003: an account-scoped quota failure of the fallback keeps the
/// existing key policy instead of a candidate cooldown, and it damages neither
/// the fallback's mapping row nor the sibling mapping of the same provider.
#[tokio::test]
async fn pool_fallback_account_exhaustion_marks_the_key_without_cooling_the_candidate() {
    let _home = temp_home("pool-fallback-account-scope");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");

    let (fallback_url, fallback_log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            429,
            json!({"error": {"message": "insufficient credit on this account"}}),
        )
    })
    .await;
    let regular_provider = provider_without_the_pool_mapping(
        &regular_provider_id,
        "Regular Provider",
        "https://regular.example.invalid/v1",
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback-secret", true)],
        vec![
            provider_mapping("local-fallback", "remote-fallback", "chat_completions"),
            provider_mapping("local-sibling", "remote-sibling", "chat_completions"),
        ],
    );
    let pool = pool_with_fallback(
        &pool_id,
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    stop_server().await.expect("stop the listener");

    assert_eq!(
        status, 502,
        "an exhausted fallback must answer with the standard unavailability envelope: {text}"
    );
    assert!(
        text.contains("all_providers_unavailable"),
        "the exhausted fallback must keep the standard error code: {text}"
    );
    assert!(
        !text.contains("sk-fallback-secret"),
        "the failure reason must stay free of credentials: {text}"
    );
    assert_eq!(
        models_of(&fallback_log).len(),
        1,
        "the fallback must receive exactly one pass: {:?}",
        models_of(&fallback_log)
    );

    let status_row = pool_status_row(&pool_id);
    let fallback_status = status_row
        .fallback
        .expect("the pool must report its separate fallback status");
    assert_ne!(
        fallback_status.availability, "cooling",
        "an account-scoped failure must not cool the candidate: {fallback_status:?}"
    );

    let stored = stored_provider(&fallback_provider_id);
    assert!(
        stored.keys[0].auto_marked,
        "an account-scoped quota failure must keep the existing key policy: {:?}",
        stored.keys[0]
    );
    let sibling = stored
        .mappings
        .iter()
        .find(|mapping| mapping.local_model == "local-sibling")
        .expect("the sibling mapping must persist");
    assert!(
        !sibling.auto_disabled,
        "the same-provider sibling mapping must stay enabled: {sibling:?}"
    );
    assert_eq!(
        sibling.consecutive_failures, 0,
        "the same-provider sibling mapping must own no failure: {sibling:?}"
    );
}

/// AC-006 / REQ-005: an exhausted fallback answers with the existing 502
/// envelope, a bounded nonsecret reason that names the fallback, and exactly
/// one terminal row that keeps the last observed upstream status. A network
/// failure keeps the existing logged status of 0.
#[tokio::test(start_paused = true)]
async fn pool_exhausted_fallback_reports_the_last_upstream_status_and_a_bounded_reason() {
    let _home = temp_home("pool-fallback-exhausted-status");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");
    let _ticker = super::spawn_paused_clock_ticker();
    // A closed loopback port makes the fallback attempt fail with a transport
    // error, whose existing logged status is 0.
    let closed_url = format!("http://127.0.0.1:{}/v1", free_port().await);

    let regular_provider = provider_without_the_pool_mapping(
        &regular_provider_id,
        "Regular Provider",
        "https://regular.example.invalid/v1",
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &closed_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback-secret", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));

    let (mut client, handler) = spawn_relay_connection(
        "/v1/chat/completions",
        json!({"model": alias, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    let (status, text) = read_relay_response(&mut client).await;
    handler.await.expect("relay task").expect("relay handler");

    assert_eq!(
        status, 502,
        "an exhausted fallback must answer with the standard unavailability envelope: {text}"
    );
    assert!(
        text.contains("all_providers_unavailable"),
        "the exhausted fallback must keep the standard error code: {text}"
    );
    assert!(
        text.contains("local-fallback") && text.contains("remote-fallback"),
        "the failure reason must name the fallback that was attempted: {text}"
    );
    assert!(
        !text.contains("sk-fallback-secret"),
        "the failure reason must stay free of credentials: {text}"
    );
    assert!(
        text.len() < 1000,
        "the failure reason must stay bounded: {text}"
    );

    let records = wait_for_usage_logs(1).await;
    let terminals: Vec<_> = records.iter().filter(|record| record.terminal).collect();
    assert_eq!(
        terminals.len(),
        1,
        "the request must have exactly one terminal row: {records:?}"
    );
    let terminal = terminals[0];
    assert_eq!(
        terminal.local_model, alias,
        "the public alias must stay the logged local model: {terminal:?}"
    );
    assert_eq!(
        terminal.provider_id, fallback_provider_id,
        "the real fallback provider must be logged: {terminal:?}"
    );
    assert_eq!(
        terminal.upstream_model, "remote-fallback",
        "the real fallback upstream model must be logged: {terminal:?}"
    );
    assert_eq!(
        terminal.status, 0,
        "a network failure must keep the existing logged status of 0: {terminal:?}"
    );
    assert_eq!(
        terminal.result,
        UsageResult::Failure,
        "the exhausted fallback must be logged as a failure: {terminal:?}"
    );
    let reason = terminal
        .error_message
        .clone()
        .expect("the terminal row must carry the failure reason");
    assert!(
        !reason.contains("sk-fallback-secret"),
        "the logged reason must stay free of credentials: {reason}"
    );
    assert!(
        reason.len() < 1000,
        "the logged reason must stay bounded: {reason}"
    );
}

/// One typed pool record with exactly one regular candidate and an optional
/// fallback, for the public upsert command.
fn typed_pool_with_fallback(
    id: &str,
    alias: &str,
    regular: ModelPoolCandidate,
    fallback: Option<ModelPoolCandidate>,
) -> ModelPool {
    ModelPool {
        id: id.to_string(),
        name: "Free Modle".to_string(),
        model: alias.to_string(),
        enabled: true,
        candidates: vec![regular],
        default_reasoning_effort: PoolDefaultReasoningEffort::Auto,
        fallback,
    }
}

/// One typed pool candidate reference.
fn typed_candidate(
    provider_id: &str,
    local_model: &str,
    upstream_model: &str,
    protocol: UpstreamProtocol,
    weight: u32,
) -> ModelPoolCandidate {
    ModelPoolCandidate {
        provider_id: provider_id.to_string(),
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        protocol,
        weight,
        reasoning: None,
    }
}

/// The pool status row of one stored pool id.
fn pool_status_row(pool_id: &str) -> crate::ai_gateway::model_pools::ModelPoolStatus {
    ai_gateway_model_pools_status()
        .expect("read the pool status")
        .into_iter()
        .find(|status| status.id == pool_id)
        .unwrap_or_else(|| panic!("missing the status row of pool '{pool_id}'"))
}

/// AC-009 / REQ-006: a model-scoped failure of the configured fallback creates
/// the fallback-owned cooldown. A rejected save changes neither the encrypted
/// bytes nor that runtime state, and a successful replacement, clearing or
/// deletion prunes only the removed identity, so a re-added reference starts
/// ready instead of inheriting a stale cooldown.
#[tokio::test]
async fn pool_fallback_cooldown_survives_a_rejected_save_and_is_pruned_only_for_the_removed_identity() {
    let _home = temp_home("pool-fallback-cooldown-pruning");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let pool_id = format!("pool-{tag}");
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");
    let other_provider_id = format!("p-other-{tag}");

    let (fallback_url, fallback_log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            429,
            json!({"error": {"type": "FreeUsageLimitError", "message": "free usage limit reached"}}),
        )
    })
    .await;
    let (other_url, other_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, fallback_success_body())).await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        "https://regular.example.invalid/v1",
        vec![pool_key("key-regular", "Regular", "sk-regular-secret", true)],
        vec![provider_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback-secret", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let other_provider = pool_provider(
        &other_provider_id,
        "Other Provider",
        &other_url,
        vec![pool_key("key-other", "Other", "sk-other-secret", true)],
        vec![provider_mapping(
            "local-other-fallback",
            "remote-other-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &pool_id,
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider, other_provider],
        vec![pool],
    ));
    // The regular candidate is excluded, so the configured fallback is the only
    // source the request can reach.
    cool_pool_candidate(
        &candidate_identity(
            &pool_id,
            &regular_provider_id,
            "local-regular",
            "remote-regular",
        ),
        Some(Duration::from_secs(60)),
    );
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    stop_server().await.expect("stop the listener");

    assert_eq!(
        status, 502,
        "the failing fallback must answer with the standard unavailability envelope: {text}"
    );
    assert_eq!(
        models_of(&fallback_log).len(),
        1,
        "the fallback must receive exactly one pass: {:?}",
        models_of(&fallback_log)
    );
    let cooled = pool_status_row(&pool_id);
    let cooling = cooled
        .fallback
        .clone()
        .expect("the pool must report its separate fallback status");
    assert_eq!(
        cooling.availability, "cooling",
        "the failed fallback must own the cooldown: {cooling:?}"
    );

    // A rejected save changes neither the encrypted bytes nor the runtime
    // state the fallback owns.
    let bytes_before = encrypted_config_bytes();
    let rejected = ai_gateway_upsert_model_pool(typed_pool_with_fallback(
        &pool_id,
        &alias,
        typed_candidate(
            &regular_provider_id,
            "local-regular",
            "remote-regular",
            UpstreamProtocol::ChatCompletions,
            100,
        ),
        Some(typed_candidate(
            &fallback_provider_id,
            "local-fallback",
            "remote-fallback",
            UpstreamProtocol::ChatCompletions,
            0,
        )),
    ));
    assert!(
        rejected.is_err(),
        "an out-of-range fallback weight must be rejected: {rejected:?}"
    );
    assert_eq!(
        encrypted_config_bytes(),
        bytes_before,
        "a rejected save must leave the encrypted configuration bytes unchanged"
    );
    let after_rejection = pool_status_row(&pool_id);
    assert_eq!(
        after_rejection
            .fallback
            .expect("the fallback status must persist")
            .availability,
        "cooling",
        "a rejected save must leave the fallback's runtime state unchanged"
    );

    // A successful replacement prunes only the removed identity, so the
    // regular candidate keeps its own cooldown while the removed fallback
    // starts ready again when it is configured once more.
    ai_gateway_upsert_model_pool(typed_pool_with_fallback(
        &pool_id,
        &alias,
        typed_candidate(
            &regular_provider_id,
            "local-regular",
            "remote-regular",
            UpstreamProtocol::ChatCompletions,
            100,
        ),
        Some(typed_candidate(
            &other_provider_id,
            "local-other-fallback",
            "remote-other-fallback",
            UpstreamProtocol::ChatCompletions,
            1,
        )),
    ))
    .expect("the replacement must be accepted");
    let replaced = pool_status_row(&pool_id);
    let replacement_status = replaced
        .fallback
        .clone()
        .expect("the replacement fallback must be reported");
    assert_eq!(
        replacement_status.availability,
        "ready",
        "the replacement fallback must not inherit a cooldown: {replacement_status:?}"
    );
    assert_eq!(
        replaced.candidates[0].availability, "cooling",
        "the removed identity's pruning must not touch the regular candidate: {:?}",
        replaced.candidates[0]
    );
    ai_gateway_upsert_model_pool(typed_pool_with_fallback(
        &pool_id,
        &alias,
        typed_candidate(
            &regular_provider_id,
            "local-regular",
            "remote-regular",
            UpstreamProtocol::ChatCompletions,
            100,
        ),
        Some(typed_candidate(
            &fallback_provider_id,
            "local-fallback",
            "remote-fallback",
            UpstreamProtocol::ChatCompletions,
            1,
        )),
    ))
    .expect("the re-added fallback must be accepted");
    let re_added = pool_status_row(&pool_id);
    let re_added_status = re_added
        .fallback
        .clone()
        .expect("the re-added fallback must be reported");
    assert_eq!(
        re_added_status.availability,
        "ready",
        "the removed identity's cooldown must be pruned, not resurrected: {re_added_status:?}"
    );

    // Clearing the fallback and deleting the pool prune the same removed
    // identity, so a later configuration of the same reference starts ready.
    ai_gateway_upsert_model_pool(typed_pool_with_fallback(
        &pool_id,
        &alias,
        typed_candidate(
            &regular_provider_id,
            "local-regular",
            "remote-regular",
            UpstreamProtocol::ChatCompletions,
            100,
        ),
        None,
    ))
    .expect("the cleared fallback must be accepted");
    let cleared = pool_status_row(&pool_id);
    assert!(
        cleared.fallback.is_none(),
        "a cleared fallback must not be reported: {cleared:?}"
    );
    ai_gateway_delete_model_pool(pool_id.clone()).expect("the pool must be deleted");
    ai_gateway_upsert_model_pool(typed_pool_with_fallback(
        &pool_id,
        &alias,
        typed_candidate(
            &regular_provider_id,
            "local-regular",
            "remote-regular",
            UpstreamProtocol::ChatCompletions,
            100,
        ),
        Some(typed_candidate(
            &fallback_provider_id,
            "local-fallback",
            "remote-fallback",
            UpstreamProtocol::ChatCompletions,
            1,
        )),
    ))
    .expect("the recreated pool must be accepted");
    let recreated = pool_status_row(&pool_id);
    let recreated_status = recreated
        .fallback
        .clone()
        .expect("the recreated fallback must be reported");
    assert_eq!(
        recreated_status.availability,
        "ready",
        "a deleted pool must not leave its fallback cooldown behind: {recreated_status:?}"
    );
    assert!(
        models_of(&other_log).is_empty(),
        "no other source may be contacted by these saves: {:?}",
        models_of(&other_log)
    );
}

/// One mapping row whose runtime state makes it eligible for the half-open
/// probe: the row is auto-disabled, its failure threshold is reached and its
/// disable timestamp is far enough in the past.
fn probe_eligible_mapping(local_model: &str, upstream_model: &str, protocol: &str) -> Value {
    let mut mapping = provider_mapping(local_model, upstream_model, protocol);
    mapping["auto_disabled"] = json!(true);
    mapping["disabled_reason"] = json!("HTTP 500 upstream error");
    mapping["disabled_at"] = json!(1);
    mapping["consecutive_failures"] = json!(3);
    mapping
}

/// AC-003 / REQ-002: the regular stage's permitted half-open probe belongs to
/// the regular stage. When that probe fails, the request still reaches the
/// configured fallback, which serves the request with its own one pass, so a
/// probe-eligible regular row never suppresses the fallback stage.
#[tokio::test]
async fn pool_fallback_serves_after_the_regular_stage_used_its_permitted_probe() {
    let _home = temp_home("pool-fallback-after-probe");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");

    let (regular_url, regular_log) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "probe failure"}}))
    })
    .await;
    let (fallback_url, fallback_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, fallback_success_body())).await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular-secret", true)],
        // The regular candidate's only mapping is auto-disabled, so the
        // request's regular stage is exactly one half-open probe.
        vec![probe_eligible_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback-secret", true)],
        vec![provider_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    stop_server().await.expect("stop the listener");

    assert_eq!(
        status, 200,
        "the fallback must serve after the regular stage's probe failed: {text}"
    );
    assert!(
        text.contains("from-fallback"),
        "the fallback response must reach the client: {text}"
    );
    assert_eq!(
        models_of(&regular_log).len(),
        1,
        "the regular stage must be exactly its one half-open probe: {:?}",
        models_of(&regular_log)
    );
    assert_eq!(
        models_of(&fallback_log),
        vec!["remote-fallback".to_string()],
        "the fallback must receive exactly one pass with its own upstream model: {:?}",
        models_of(&fallback_log)
    );
}

/// AC-003 / REQ-002: the half-open probe allowance is shared by the whole
/// request. The regular stage spends it on its own probe-eligible row, so the
/// fallback stage may not spend it a second time: the request performs one
/// probe in total, keeps the fail-closed unavailability envelope and publishes
/// no health for either row.
#[tokio::test]
async fn pool_fallback_shares_the_request_wide_probe_allowance() {
    let _home = temp_home("pool-fallback-shared-probe");
    let port = free_port().await;
    let tag = tag();
    let alias = format!("free-model-{tag}");
    let regular_provider_id = format!("p-reg-{tag}");
    let fallback_provider_id = format!("p-fb-{tag}");

    let (regular_url, regular_log) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "probe failure"}}))
    })
    .await;
    let (fallback_url, fallback_log) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "probe failure"}}))
    })
    .await;

    let regular_provider = pool_provider(
        &regular_provider_id,
        "Regular Provider",
        &regular_url,
        vec![pool_key("key-regular", "Regular", "sk-regular-secret", true)],
        vec![probe_eligible_mapping(
            "local-regular",
            "remote-regular",
            "chat_completions",
        )],
    );
    let fallback_provider = pool_provider(
        &fallback_provider_id,
        "Fallback Provider",
        &fallback_url,
        vec![pool_key("key-fallback", "Fallback", "sk-fallback-secret", true)],
        vec![probe_eligible_mapping(
            "local-fallback",
            "remote-fallback",
            "chat_completions",
        )],
    );
    let pool = pool_with_fallback(
        &format!("pool-{tag}"),
        &alias,
        pool_candidate(
            regular_provider_id.as_str(),
            "local-regular",
            "remote-regular",
            "chat_completions",
            Some(100),
        ),
        pool_candidate(
            fallback_provider_id.as_str(),
            "local-fallback",
            "remote-fallback",
            "chat_completions",
            Some(1),
        ),
    );
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![regular_provider, fallback_provider],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = post_chat(port, &alias).await;
    stop_server().await.expect("stop the listener");

    assert_eq!(
        status, 502,
        "a request whose only probe allowance is spent must fail closed: {text}"
    );
    assert!(
        text.contains("all_providers_unavailable"),
        "the exhausted pool must keep the standard error code: {text}"
    );
    let probes = models_of(&regular_log).len() + models_of(&fallback_log).len();
    assert_eq!(
        probes, 1,
        "the request-wide probe allowance must be spent once: regular {:?}, fallback {:?}",
        models_of(&regular_log),
        models_of(&fallback_log)
    );

    let stored_regular = stored_provider(&regular_provider_id);
    let regular_mapping = &stored_regular.mappings[0];
    assert!(
        regular_mapping.auto_disabled,
        "a failed probe must keep the regular row disabled: {regular_mapping:?}"
    );
    let stored_fallback = stored_provider(&fallback_provider_id);
    let fallback_mapping = &stored_fallback.mappings[0];
    assert!(
        fallback_mapping.auto_disabled,
        "a row that was never probed must keep its own state: {fallback_mapping:?}"
    );
}
