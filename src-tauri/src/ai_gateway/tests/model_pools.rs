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
    call_gateway, free_port, key_named, pool_key, pool_provider, read_raw_gateway_config,
    spawn_mock_upstream, temp_home, wait_for_usage_logs, write_raw_gateway_config, Captured,
    CapturedLog, MockReply,
};
use crate::ai_gateway::runtime_http::{start_server, stop_server};
use crate::ai_gateway::{
    GatewayConfig, GatewayUpstreamProvider, ModelPool, GATEWAY_CONFIG_SCHEMA_VERSION, UsageResult,
};
use serde_json::{json, Value};

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
            vec![provider_mapping("local-a", "remote-a", "chat_completions")],
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
