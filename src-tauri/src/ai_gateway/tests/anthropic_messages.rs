//! Step 1 behavior tests for the native Anthropic Messages entry
//! (`20261011-ai-gateway-anthropic-messages`, REQ-001 / REQ-002 / REQ-004 /
//! REQ-005).
//!
//! These tests pin only public and in-crate behavior: the persisted
//! `anthropic_messages` protocol accepted by the configuration DTOs, the
//! existing loopback HTTP entry `POST /v1/messages` (routing, local
//! authentication, model substitution, upstream `x-api-key` and
//! beta/version header handling) and the native SSE lifecycle/usage
//! observation through the existing streaming transport seam.
//!
//! Before Step 1 lands they must fail behaviorally rather than by a compiler
//! error: `anthropic_messages` is rejected by every DTO, the native path
//! resolves to the unknown-path 404, and native streams are accumulated with
//! OpenAI usage semantics. The focused behavioral failures are
//! `persisted_upstream_protocol_accepts_anthropic_messages` and
//! `native_messages_route_is_recognized_and_reports_native_local_errors`.

use super::{
    call_gateway, call_gateway_raw, free_port, isolated_temp_home, key_named, pool_key,
    raw_http_status_and_body, spawn_mock_upstream, temp_home, upstream_provider,
    write_raw_gateway_config, MockReply,
};
use crate::ai_gateway::runtime_http::{
    attempt_streaming, start_server, stop_server, AttemptLog, ForwardCapture,
};
use crate::ai_gateway::storage::{read_config, write_config};
use crate::ai_gateway::{
    GatewayConfig, GatewayUpstreamProvider, UpstreamProtocol, UsageResult, UsageTokens,
    GATEWAY_CONFIG_SCHEMA_VERSION,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::io::AsyncReadExt;

const LOCAL_KEY: &str = "local-key";
const LOCAL_CLAUDE: &str = "local-claude";
const UPSTREAM_CLAUDE: &str = "claude-sonnet-5-5";
const UPSTREAM_SECRET: &str = "sk-anthropic-secret";

/// Raw provider JSON declaring `anthropic_messages` at both the provider
/// default and the individual mapping. Raw JSON is used deliberately so the
/// fixture is independent of the enum variant the implementation must add.
fn anthropic_provider_value(
    id: &str,
    base_url: &str,
    local_model: &str,
    upstream_model: &str,
    key: &str,
) -> Value {
    json!({
        "id": id,
        "name": "Anthropic Provider",
        "base_url": base_url,
        "protocol": "anthropic_messages",
        "enabled": true,
        "keys": [pool_key("key-default", "Default", key, true)],
        "default_model": null,
        "mappings": [{
            "local_model": local_model,
            "upstream_model": upstream_model,
            "protocol": "anthropic_messages",
            "display_name": null,
            "enabled": true,
            "reasoning_efforts": [],
            "auto_disabled": false,
            "disabled_reason": null,
            "disabled_at": null,
            "consecutive_failures": 0,
            "last_error_at": null
        }]
    })
}

/// A placeholder-based native provider (the base URL is replaced by the test's
/// loopback mock).
fn native_provider_value(id: &str) -> Value {
    anthropic_provider_value(
        id,
        "https://placeholder.invalid/v1",
        LOCAL_CLAUDE,
        UPSTREAM_CLAUDE,
        UPSTREAM_SECRET,
    )
}

/// Raw persisted configuration carrying one provider, one enabled local key
/// and no template scheduler work.
fn raw_config_with_provider(port: u16, provider: Value) -> Value {
    json!({
        "schema_version": GATEWAY_CONFIG_SCHEMA_VERSION,
        "enabled": true,
        "port": port,
        "providers": [provider],
        "keys": [{
            "id": "k1",
            "label": "k1",
            "value": LOCAL_KEY,
            "enabled": true,
            "created_at": 1
        }],
        "default_key_id": null,
        "terminal_syncs": [],
        "usage_retention_days": 90,
        "model_prices": [],
        "provider_templates": [],
        "template_auto_refresh_minutes": 0
    })
}

/// AC-001 / REQ-001: the persisted upstream protocol value `anthropic_messages`
/// is accepted at the provider default and at the individual mapping location
/// and serializes back byte-identically.
#[test]
fn persisted_upstream_protocol_accepts_anthropic_messages() {
    let protocol: UpstreamProtocol = serde_json::from_value(json!("anthropic_messages"))
        .expect("the persisted `anthropic_messages` protocol must deserialize");
    assert_eq!(
        serde_json::to_value(protocol).expect("serialize the protocol"),
        json!("anthropic_messages"),
        "the persisted protocol value must round-trip byte-identically"
    );

    let provider: GatewayUpstreamProvider = serde_json::from_value(anthropic_provider_value(
        "p-anthropic",
        "https://anthropic.example/v1",
        LOCAL_CLAUDE,
        UPSTREAM_CLAUDE,
        UPSTREAM_SECRET,
    ))
    .expect("a provider declaring `anthropic_messages` must deserialize");
    assert_eq!(
        serde_json::to_value(provider.protocol).expect("serialize the provider protocol"),
        json!("anthropic_messages"),
        "the provider default protocol must survive deserialization"
    );
    assert_eq!(
        serde_json::to_value(provider.mappings[0].protocol)
            .expect("serialize the mapping protocol"),
        json!("anthropic_messages"),
        "the individual mapping protocol must survive deserialization"
    );
}

/// AC-001 / REQ-001: a persisted configuration carrying `anthropic_messages`
/// survives the encrypted write/read persistence path unchanged.
#[test]
fn anthropic_messages_protocol_survives_config_persistence() {
    let _home = isolated_temp_home("anthropic-messages-persist");
    let raw = raw_config_with_provider(
        17688,
        anthropic_provider_value(
            "p-anthropic",
            "https://anthropic.example/v1",
            LOCAL_CLAUDE,
            UPSTREAM_CLAUDE,
            UPSTREAM_SECRET,
        ),
    );
    write_raw_gateway_config(&raw);

    let loaded = read_config().expect("the persisted `anthropic_messages` config must load");
    assert_eq!(
        serde_json::to_value(&loaded.providers[0].protocol).expect("serialize loaded protocol"),
        json!("anthropic_messages")
    );
    assert_eq!(
        serde_json::to_value(&loaded.providers[0].mappings[0].protocol)
            .expect("serialize loaded mapping protocol"),
        json!("anthropic_messages")
    );
}

/// AC-003 / REQ-002: `POST /v1/messages` (with a query string) is a recognized
/// native route that authenticates locally and answers native errors before any
/// upstream contact. It must never fall through to the unknown-path 404.
#[tokio::test]
async fn native_messages_route_is_recognized_and_reports_native_local_errors() {
    let home = temp_home("native-messages-auth");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", LOCAL_KEY));
    config.providers.push(upstream_provider(
        "p1",
        "Provider One",
        &upstream_url,
        "sk",
        Some("remote-default"),
    ));
    write_config(&config).expect("seed the local config");
    start_server(None).await.expect("start the native listener");

    // Missing local credentials: a native 401, never the unknown-path 404.
    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/messages?beta=true",
        &[],
        Some(json!({"model": "m", "messages": []})),
    )
    .await;
    assert_eq!(
        status, 401,
        "the native route must authenticate locally before any upstream attempt: {text}"
    );
    assert!(
        text.contains("\"error\""),
        "the native 401 must carry a native error body: {text}"
    );

    // Valid local credentials reach candidate resolution; a chat-only config
    // has no native candidate, so the native 502 replaces the old 404.
    for credential in [
        ("authorization", "Bearer local-key"),
        ("x-api-key", LOCAL_KEY),
    ] {
        let (status, _content_type, text) = call_gateway(
            port,
            "POST",
            "/v1/messages",
            &[credential],
            Some(json!({"model": "m", "messages": []})),
        )
        .await;
        assert_eq!(
            status, 502,
            "valid local credential {credential:?} must reach routing: {text}"
        );
        assert!(
            text.contains("\"error\""),
            "the no-candidate native error must be a native error body: {text}"
        );
    }

    // Malformed JSON with a valid key is a native 400, not a 404.
    let raw = "POST /v1/messages HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: Bearer local-key\r\ncontent-type: application/json\r\ncontent-length: 4\r\nconnection: close\r\n\r\n{abc";
    let raw_response = call_gateway_raw(port, raw).await;
    let (raw_status, raw_body) = raw_http_status_and_body(&raw_response);
    assert!(
        raw_status.contains(" 400"),
        "malformed JSON on the native route must be a 400: {raw_status}"
    );
    assert!(
        raw_body.contains("invalid_request"),
        "the malformed-JSON error must be an invalid_request: {raw_body}"
    );

    assert!(
        log.lock().expect("mock log").is_empty(),
        "no rejected native request may reach an upstream"
    );

    stop_server().await.expect("stop the native listener");
    drop(home);
}

/// AC-003 / REQ-002: a native JSON request reaches exactly one `/v1/messages`
/// upstream, substitutes the selected upstream pool key as `x-api-key`,
/// rewrites only the model and preserves the native beta/version headers while
/// never forwarding the local credential.
#[tokio::test]
async fn native_messages_forwards_exact_path_model_and_upstream_api_key() {
    let home = temp_home("native-messages-forward");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({"id": "msg_1", "type": "message", "content": []}),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config_with_provider(
        port,
        anthropic_provider_value(
            "p-native",
            &upstream_url,
            LOCAL_CLAUDE,
            UPSTREAM_CLAUDE,
            UPSTREAM_SECRET,
        ),
    ));
    start_server(None).await.expect("start the native listener");

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/messages?beta=true",
        &[
            ("authorization", "Bearer local-key"),
            ("x-api-key", "local-key"),
            ("anthropic-beta", "beta-tools-2025"),
            ("anthropic-version", "2024-06-01"),
        ],
        Some(json!({
            "model": LOCAL_CLAUDE,
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "hi"}]
        })),
    )
    .await;
    assert_eq!(
        status, 200,
        "the native route must serve the mapped Messages model: {text}"
    );

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(
        captured.len(),
        1,
        "exactly one upstream attempt, got {}",
        captured.len()
    );
    let attempt = &captured[0];
    assert_eq!(attempt.method, "POST");
    assert_eq!(
        attempt.path.matches("/v1/messages").count(),
        1,
        "the upstream URL must contain exactly one /v1/messages, got {}",
        attempt.path
    );
    assert!(
        !attempt.path.contains("/v1/v1"),
        "the version segment must never double: {}",
        attempt.path
    );

    let sent: Value = serde_json::from_slice(&attempt.body).expect("upstream body must be JSON");
    assert_eq!(
        sent["model"], UPSTREAM_CLAUDE,
        "the mapped upstream model must replace the local model"
    );
    assert_eq!(sent["max_tokens"], 64);
    assert_eq!(sent["messages"], json!([{"role": "user", "content": "hi"}]));

    assert_eq!(
        attempt.headers.get("x-api-key").map(String::as_str),
        Some(UPSTREAM_SECRET),
        "the selected upstream key must be sent as x-api-key: {:?}",
        attempt.headers
    );
    assert_eq!(
        attempt.headers.get("anthropic-beta").map(String::as_str),
        Some("beta-tools-2025"),
        "the incoming anthropic-beta must survive: {:?}",
        attempt.headers
    );
    assert_eq!(
        attempt.headers.get("anthropic-version").map(String::as_str),
        Some("2024-06-01"),
        "an explicit anthropic-version must survive: {:?}",
        attempt.headers
    );
    assert_ne!(
        attempt.headers.get("authorization").map(String::as_str),
        Some("Bearer local-key"),
        "the local relay credential must never be forwarded: {:?}",
        attempt.headers
    );
    assert!(
        !attempt.headers.values().any(|value| value.contains(LOCAL_KEY)),
        "no upstream header may carry the local credential: {:?}",
        attempt.headers
    );

    stop_server().await.expect("stop the native listener");
    drop(home);
}

/// AC-003 / REQ-002: the gateway supplies `anthropic-version: 2023-06-01` only
/// when the caller omits it, preserves an explicit version, and never invents
/// an absent `anthropic-beta`.
#[tokio::test]
async fn native_messages_defaults_anthropic_version_only_when_absent() {
    let home = temp_home("native-messages-version-default");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({"id": "msg_1", "type": "message", "content": []}),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config_with_provider(
        port,
        anthropic_provider_value(
            "p-native-default",
            &upstream_url,
            LOCAL_CLAUDE,
            UPSTREAM_CLAUDE,
            UPSTREAM_SECRET,
        ),
    ));
    start_server(None).await.expect("start the native listener");

    // No anthropic-version: the gateway supplies the documented default.
    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/messages",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": LOCAL_CLAUDE, "messages": []})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    // An explicit version is preserved instead of being reset to the default.
    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/messages",
        &[
            ("authorization", "Bearer local-key"),
            ("anthropic-version", "2025-01-01"),
        ],
        Some(json!({"model": LOCAL_CLAUDE, "messages": []})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(captured.len(), 2);
    assert_eq!(
        captured[0]
            .headers
            .get("anthropic-version")
            .map(String::as_str),
        Some("2023-06-01"),
        "a missing anthropic-version must be supplied as 2023-06-01: {:?}",
        captured[0].headers
    );
    assert!(
        !captured[0].headers.contains_key("anthropic-beta"),
        "a missing anthropic-beta must not be injected: {:?}",
        captured[0].headers
    );
    assert_eq!(
        captured[1]
            .headers
            .get("anthropic-version")
            .map(String::as_str),
        Some("2025-01-01"),
        "an explicit anthropic-version must be preserved: {:?}",
        captured[1].headers
    );

    stop_server().await.expect("stop the native listener");
    drop(home);
}

/// Native SSE stream terminating with `message_stop`.
const NATIVE_SSE_OK: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-5-5\",\"usage\":{\"input_tokens\":10,\"cache_read_input_tokens\":20,\"cache_creation_input_tokens\":3,\"output_tokens\":0}}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":null},\"usage\":{\"output_tokens\":2}}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":7}}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

/// Native SSE stream that ends (EOF) before any `message_stop`.
const NATIVE_SSE_NO_STOP: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":2}}\n\n",
);

/// Native SSE stream carrying an in-band error event (and a terminator, so the
/// error recognition is observed independently of the missing-stop rule).
const NATIVE_SSE_ERROR: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
    "event: error\n",
    "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

/// Relay one native SSE payload through the existing streaming transport seam
/// and return the caller-visible bytes, the capture and the attempt log.
async fn relay_native_stream(
    provider_value: &Value,
    sse: &'static str,
) -> (String, ForwardCapture, Vec<AttemptLog>) {
    let _home = isolated_temp_home("native-stream-relay");
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::Stream(sse.to_string())).await;

    let mut provider_value = provider_value.clone();
    provider_value["base_url"] = json!(upstream_url);
    let provider: GatewayUpstreamProvider = serde_json::from_value(provider_value.clone())
        .expect("the persisted `anthropic_messages` provider must deserialize");
    // A persisted configuration is required so the streaming settlement path
    // has a target; the request resolves its candidate from the passed slice.
    write_raw_gateway_config(&raw_config_with_provider(0, provider_value));

    let body = serde_json::to_vec(&json!({
        "model": LOCAL_CLAUDE,
        "max_tokens": 16,
        "stream": true,
        "messages": [{"role": "user", "content": "hi"}]
    }))
    .expect("encode the native request");
    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    let mut attempts: Vec<AttemptLog> = Vec::new();
    let capture = attempt_streaming(
        &mut server,
        std::slice::from_ref(&provider),
        "/v1/messages",
        &body,
        Some(LOCAL_CLAUDE),
        &HashMap::new(),
        false,
        None,
        &mut attempts,
    )
    .await
    .expect("the native stream must complete");
    drop(server);

    let mut output = Vec::new();
    client
        .read_to_end(&mut output)
        .await
        .expect("read the relayed native stream");
    (String::from_utf8_lossy(&output).into_owned(), capture, attempts)
}

/// AC-004 / REQ-004: native success bytes are relayed verbatim with no OpenAI
/// `[DONE]`, and the split start/delta usage merges field-wise (start input and
/// cache tiers plus the last cumulative output snapshot).
#[tokio::test]
async fn native_messages_stream_is_passthrough_with_split_usage_merge() {
    let (output, capture, attempts) =
        relay_native_stream(&native_provider_value("p-native-stream"), NATIVE_SSE_OK).await;

    assert_eq!(
        output.as_str(),
        NATIVE_SSE_OK,
        "native SSE bytes must be preserved verbatim"
    );
    assert!(
        !output.contains("[DONE]"),
        "no OpenAI [DONE] may be injected into a native stream"
    );
    assert_eq!(
        capture.status, 200,
        "a message_stop-terminated native stream is a success"
    );
    assert_eq!(attempts.len(), 1, "one completed attempt");
    assert_eq!(attempts[0].result, UsageResult::Success);
    assert_eq!(
        attempts[0].usage,
        Some(UsageTokens {
            input_tokens: 10,
            cache_read_tokens: 20,
            cache_write_tokens: 3,
            output_tokens: 7,
        }),
        "message_start input/cache tiers merge field-wise with the last cumulative output snapshot"
    );
    assert!(
        attempts[0].usage_valid,
        "the merged native usage must be a valid canonical decomposition"
    );
}

/// AC-004 / AC-009 / REQ-004: EOF without `message_stop` is not a successful
/// native stream and still injects no OpenAI `[DONE]`.
#[tokio::test]
async fn native_messages_stream_without_message_stop_is_not_success() {
    let (output, capture, attempts) =
        relay_native_stream(&native_provider_value("p-native-nostop"), NATIVE_SSE_NO_STOP).await;

    assert_eq!(
        attempts.last().expect("one completed attempt").result,
        UsageResult::Failure,
        "a native stream that ends without `message_stop` must not be a success"
    );
    assert_ne!(
        capture.status, 200,
        "the missing lifecycle terminator must not be recorded as a success"
    );
    assert!(
        !output.contains("[DONE]"),
        "no OpenAI [DONE] may be injected into a native stream"
    );
}

/// AC-004 / REQ-004: an in-band native error event is recognized and never
/// misclassified as a successful stream.
#[tokio::test]
async fn native_messages_in_band_error_event_is_not_success() {
    let (output, _capture, attempts) =
        relay_native_stream(&native_provider_value("p-native-error"), NATIVE_SSE_ERROR).await;

    assert_eq!(
        attempts.last().expect("one completed attempt").result,
        UsageResult::Failure,
        "an in-band native error event must not be classified as success"
    );
    assert!(
        !output.contains("[DONE]"),
        "no OpenAI [DONE] may be injected into a native stream"
    );
}
