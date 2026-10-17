//! Step 2 behavior tests for serving Anthropic Messages upstreams through the
//! existing OpenAI entries (`20261011-ai-gateway-anthropic-messages`,
//! REQ-003 / REQ-004 / REQ-005; AC-005 … AC-011).
//!
//! These tests pin only observable boundaries:
//!
//! - the loopback HTTP entry points `POST /v1/chat/completions` and
//!   `POST /v1/responses`, asserted through the captured native upstream
//!   request (path/headers/body) and the caller-visible JSON/SSE payload;
//! - the loopback streaming seam `attempt_streaming`, asserted through the
//!   caller-visible SSE bytes and the buffered attempt log;
//! - conversion-aware routing/accounting through the existing HTTP boundary
//!   and the persisted usage-log rows.
//!
//! No private converter helper is called and no internal collaborator is
//! mocked, so the assertions stay independent of the implementation shape. The
//! fixtures declare the wire contract (Anthropic Messages request/response
//! shapes, OpenAI Chat/Responses shapes) and the tests fail behaviorally before
//! Step 2 lands: the conversion-aware candidate resolution currently yields the
//! all-unavailable `502` for every Anthropic mapping reached through a Chat or
//! Responses entry, unsupported conversion fields are forwarded instead of
//! rejected, and no converted SSE is emitted.
//!
//! `isolated_temp_home` is used for the direct streaming seam (single-threaded
//! runtime), while the loopback server tests use the process-wide `temp_home`
//! helper because the listener resolves configuration on worker threads.

use super::{
    call_gateway, free_port, isolated_temp_home, key_named, pool_key, raw_http_status_and_body,
    spawn_handle_connection, spawn_mock_upstream, temp_home, write_raw_gateway_config, MockReply,
};
use crate::ai_gateway::runtime_http::{
    attempt_streaming, read_http_request, start_server, stop_server, AttemptLog, ForwardCapture,
};
use crate::ai_gateway::storage::write_config;
use crate::ai_gateway::{
    GatewayConfig, GatewayUpstreamProvider, UsageResult, UsageTokens, GATEWAY_CONFIG_SCHEMA_VERSION,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Notify;

const LOCAL_KEY: &str = "local-key";
const LOCAL_MODEL: &str = "claude-sonnet-5-5";
const LOCAL_MODEL_B: &str = "claude-haiku-5-5";
const UPSTREAM_MODEL: &str = "commandcode/sonnet-5-5";
const UPSTREAM_SECRET: &str = "sk-anthropic-secret";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Raw Anthropic provider JSON declaring `anthropic_messages` at the provider
/// default and the individual mapping. Raw JSON keeps the fixture independent
/// of the enum variant the implementation adds.
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

/// One OpenAI-protocol provider with a single mapping, used for the
/// same-protocol regression and mutual-exclusion guards.
fn openai_provider_value(
    id: &str,
    base_url: &str,
    protocol: &str,
    local_model: &str,
    upstream_model: &str,
    key: &str,
) -> Value {
    json!({
        "id": id,
        "name": "OpenAI Provider",
        "base_url": base_url,
        "protocol": protocol,
        "enabled": true,
        "keys": [pool_key("key-default", "Default", key, true)],
        "default_model": null,
        "mappings": [{
            "local_model": local_model,
            "upstream_model": upstream_model,
            "protocol": protocol,
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

/// Raw persisted configuration carrying the given providers, one enabled local
/// key and no template scheduler work.
fn raw_config(port: u16, providers: Vec<Value>) -> Value {
    json!({
        "schema_version": GATEWAY_CONFIG_SCHEMA_VERSION,
        "enabled": true,
        "port": port,
        "providers": providers,
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

/// One native Anthropic assistant message body.
fn anthropic_message(content: Value, stop_reason: &str, usage: Value) -> Value {
    json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": UPSTREAM_MODEL,
        "content": content,
        "stop_reason": stop_reason,
        "usage": usage,
    })
}

fn chat_body() -> Value {
    json!({
        "model": LOCAL_MODEL,
        "messages": [
            {"role": "system", "content": "sys"},
            {"role": "user", "content": [
                {"type": "text", "text": "hi"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
            ]}
        ],
        "max_tokens": 64,
    })
}

fn responses_body() -> Value {
    json!({
        "model": LOCAL_MODEL,
        "instructions": "be terse",
        "input": [
            {"role": "user", "content": [
                {"type": "input_text", "text": "hi"},
                {"type": "input_image", "image_url": "https://example.com/photo.png"}
            ]}
        ],
    })
}

fn chat_tool_body() -> Value {
    json!({
        "model": LOCAL_MODEL,
        "messages": [
            {"role": "user", "content": "use the tool"},
            {"role": "assistant", "content": null, "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
            }]},
            {"role": "tool", "tool_call_id": "call_1", "content": "sunny"}
        ],
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "weather",
                "parameters": {
                    "type": "object",
                    "properties": {"city": {"type": "string"}},
                    "required": ["city"]
                }
            }
        }],
        "tool_choice": "auto",
        "parallel_tool_calls": false
    })
}

fn responses_tool_body() -> Value {
    json!({
        "model": LOCAL_MODEL,
        "input": [
            {"role": "user", "content": [{"type": "input_text", "text": "use the tool"}]}
        ],
        "tools": [{
            "type": "function",
            "name": "get_weather",
            "description": "weather",
            "parameters": {
                "type": "object",
                "properties": {"city": {"type": "string"}},
                "required": ["city"]
            }
        }],
        "tool_choice": "auto"
    })
}

/// Whether any captured upstream attempt exists, as a `String` diagnostic.
fn captured_len(log: &super::CapturedLog) -> usize {
    log.lock().expect("mock log").len()
}

// ---------------------------------------------------------------------------
// AC-005: ordinary conversations convert through both OpenAI entries
// ---------------------------------------------------------------------------

/// AC-005 / REQ-003: a Chat conversation with a system prompt, text and a
/// base64 data image is converted to a native Anthropic Messages request and
/// the native response is converted back into the caller's Chat shape with the
/// local model identity and usage semantics.
#[tokio::test]
async fn chat_entry_converts_ordinary_conversation_to_anthropic_messages() {
    let home = temp_home("conv-chat-ordinary");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "hello"}]),
                "end_turn",
                json!({"input_tokens": 10, "output_tokens": 5}),
            ),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-conv",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(chat_body()),
    )
    .await;
    assert_eq!(
        status, 200,
        "a Chat request served by an Anthropic mapping must succeed: {text}"
    );

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(captured.len(), 1, "exactly one upstream attempt");
    let attempt = &captured[0];
    assert_eq!(attempt.method, "POST");
    assert_eq!(
        attempt.path.matches("/v1/messages").count(),
        1,
        "the converted upstream URL must be exactly one /v1/messages: {}",
        attempt.path
    );
    assert!(
        !attempt.path.contains("/v1/v1"),
        "the version segment must never double: {}",
        attempt.path
    );

    let sent: Value = serde_json::from_slice(&attempt.body).expect("upstream body must be JSON");
    assert_eq!(
        sent["model"], UPSTREAM_MODEL,
        "the mapped upstream model must replace the local model"
    );
    assert_eq!(sent["max_tokens"], 64, "the output cap must be forwarded");
    let sent_text = serde_json::to_string(&sent).expect("serialize upstream body");
    assert!(sent_text.contains("\"sys\""), "the system prompt must convert: {sent_text}");
    assert!(sent_text.contains("\"hi\""), "the user text must convert: {sent_text}");
    assert!(
        sent_text.contains("\"media_type\":\"image/png\""),
        "a base64 data image must become an Anthropic image source: {sent_text}"
    );
    assert!(sent_text.contains("AAAA"), "the image payload must survive: {sent_text}");
    assert!(
        !sent_text.contains(LOCAL_MODEL),
        "the local model identity must not leak upstream: {sent_text}"
    );
    assert_eq!(
        attempt.headers.get("x-api-key").map(String::as_str),
        Some(UPSTREAM_SECRET),
        "the selected upstream key must be sent as x-api-key: {:?}",
        attempt.headers
    );

    let caller: Value = serde_json::from_str(&text).expect("caller body must be JSON");
    assert_eq!(
        caller["model"], LOCAL_MODEL,
        "the caller must see the local model identity: {text}"
    );
    assert!(
        serde_json::to_string(&caller)
            .expect("serialize caller body")
            .contains("hello"),
        "the converted assistant text must reach the caller: {text}"
    );
    assert!(
        caller["usage"].is_object(),
        "a converted Chat success must carry usage: {text}"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-005 / REQ-003: a stateless Responses conversation with `instructions`,
/// text and a URL image converts to a native Anthropic request and the native
/// response converts back into the caller's Responses shape.
#[tokio::test]
async fn responses_entry_converts_ordinary_conversation_to_anthropic_messages() {
    let home = temp_home("conv-responses-ordinary");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "hello"}]),
                "end_turn",
                json!({"input_tokens": 10, "output_tokens": 5}),
            ),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-conv",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(responses_body()),
    )
    .await;
    assert_eq!(
        status, 200,
        "a Responses request served by an Anthropic mapping must succeed: {text}"
    );

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(captured.len(), 1, "exactly one upstream attempt");
    let attempt = &captured[0];
    assert_eq!(
        attempt.path.matches("/v1/messages").count(),
        1,
        "the converted upstream URL must be exactly one /v1/messages: {}",
        attempt.path
    );
    let sent: Value = serde_json::from_slice(&attempt.body).expect("upstream body must be JSON");
    assert_eq!(sent["model"], UPSTREAM_MODEL);
    let sent_text = serde_json::to_string(&sent).expect("serialize upstream body");
    assert!(
        sent_text.contains("be terse"),
        "the Responses instructions must become the Anthropic system prompt: {sent_text}"
    );
    assert!(sent_text.contains("\"hi\""), "the input text must convert: {sent_text}");
    assert!(
        sent_text.contains("\"type\":\"url\""),
        "a URL image must become an Anthropic URL source: {sent_text}"
    );
    assert!(!sent_text.contains(LOCAL_MODEL));

    let caller: Value = serde_json::from_str(&text).expect("caller body must be JSON");
    assert_eq!(caller["model"], LOCAL_MODEL);
    assert!(
        serde_json::to_string(&caller)
            .expect("serialize caller body")
            .contains("hello"),
        "the converted assistant text must reach the caller: {text}"
    );
    assert!(caller["usage"].is_object(), "converted Responses usage: {text}");

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-005 / REQ-003: Chat output-cap precedence — `max_completion_tokens`
/// precedes `max_tokens`, and an omitted conversion cap defaults to 4096.
#[tokio::test]
async fn chat_conversion_cap_precedence_and_default_4096() {
    let home = temp_home("conv-chat-cap");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(json!([{"type": "text", "text": "ok"}]), "end_turn", json!({})),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-cap",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let requests = [
        // Both present: max_completion_tokens wins.
        json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "max_completion_tokens": 128, "max_tokens": 64}),
        // Only the legacy cap: it is preserved.
        json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "max_tokens": 7}),
        // Omitted: the conversion default is 4096.
        json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}]}),
    ];
    for body in requests {
        let (status, _ct, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(body),
        )
        .await;
        assert_eq!(status, 200, "cap fixture must be served: {text}");
    }

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(captured.len(), 3, "three upstream attempts");
    let caps: Vec<i64> = captured
        .iter()
        .map(|attempt| {
            let sent: Value = serde_json::from_slice(&attempt.body).expect("upstream JSON");
            sent["max_tokens"].as_i64().expect("numeric max_tokens")
        })
        .collect();
    assert_eq!(
        caps,
        vec![128, 7, 4096],
        "max_completion_tokens precedes max_tokens and an omitted cap defaults to 4096"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-005 / REQ-003: Responses uses `max_output_tokens`, including an explicit
/// nonnegative zero.
#[tokio::test]
async fn responses_conversion_cap_uses_max_output_tokens_including_zero() {
    let home = temp_home("conv-responses-cap");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(json!([{"type": "text", "text": "ok"}]), "end_turn", json!({})),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-cap",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    for cap in [77, 0] {
        let (status, _ct, text) = call_gateway(
            port,
            "POST",
            "/v1/responses",
            &[("authorization", "Bearer local-key")],
            Some(json!({"model": LOCAL_MODEL, "input": "hi", "max_output_tokens": cap})),
        )
        .await;
        assert_eq!(status, 200, "Responses cap fixture must be served: {text}");
    }

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(captured.len(), 2, "two upstream attempts");
    let caps: Vec<i64> = captured
        .iter()
        .map(|attempt| {
            let sent: Value = serde_json::from_slice(&attempt.body).expect("upstream JSON");
            sent["max_tokens"].as_i64().expect("numeric max_tokens")
        })
        .collect();
    assert_eq!(caps, vec![77, 0], "max_output_tokens maps to max_tokens and zero is preserved");

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-005 / REQ-003: negative, fractional and string caps fail explicitly
/// instead of being forwarded, and never reach an upstream.
#[tokio::test]
async fn invalid_conversion_caps_fail_before_upstream() {
    let home = temp_home("conv-cap-invalid");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(200, anthropic_message(json!([]), "end_turn", json!({})))
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-cap",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let chat_cases = [
        json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "max_tokens": -5}),
        json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "max_tokens": 1.5}),
        json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "max_completion_tokens": "64"}),
    ];
    for body in chat_cases {
        let (status, _ct, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(body),
        )
        .await;
        assert_eq!(
            status, 400,
            "an invalid conversion cap must fail explicitly: {status} {text}"
        );
        assert!(
            text.contains("max_tokens") || text.contains("max_completion_tokens"),
            "the failure must identify the cap field: {text}"
        );
    }

    assert_eq!(
        captured_len(&log),
        0,
        "no malformed converted request may reach an upstream"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

// ---------------------------------------------------------------------------
// AC-006: tools round-trip across both OpenAI entries
// ---------------------------------------------------------------------------

/// AC-006 / REQ-003: a Chat function schema, an assistant tool call and its
/// result convert to Anthropic `tools`/`tool_use`/`tool_result`, and the
/// native tool-use response converts back to Chat `tool_calls` with the
/// arguments in the caller-required JSON string form.
#[tokio::test]
async fn chat_entry_round_trips_function_tools_through_anthropic() {
    let home = temp_home("conv-chat-tools");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "get_weather",
                    "input": {"city": "SF"}
                }]),
                "tool_use",
                json!({"input_tokens": 8, "output_tokens": 4}),
            ),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-tools",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _ct, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(chat_tool_body()),
    )
    .await;
    assert_eq!(status, 200, "the tool round trip must succeed: {text}");

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(captured.len(), 1, "exactly one upstream attempt");
    let sent: Value = serde_json::from_slice(&captured[0].body).expect("upstream JSON");
    let sent_text = serde_json::to_string(&sent).expect("serialize upstream body");
    assert!(
        sent_text.contains("\"name\":\"get_weather\""),
        "the function schema must convert to an Anthropic tool: {sent_text}"
    );
    assert!(
        sent_text.contains("\"input_schema\""),
        "the Anthropic tool must carry its input schema: {sent_text}"
    );
    assert!(
        sent_text.contains("\"type\":\"tool_use\""),
        "the assistant tool call must become a tool_use block: {sent_text}"
    );
    assert!(
        sent_text.contains("\"id\":\"call_1\""),
        "the client tool-call id must be preserved in the request: {sent_text}"
    );
    assert!(
        sent_text.contains("\"type\":\"tool_result\""),
        "the tool result must become a tool_result block: {sent_text}"
    );
    assert!(
        sent_text.contains("sunny"),
        "the tool result content must survive: {sent_text}"
    );

    let caller: Value = serde_json::from_str(&text).expect("caller body must be JSON");
    let tool_call = &caller["choices"][0]["message"]["tool_calls"][0];
    assert_eq!(
        tool_call["function"]["name"], "get_weather",
        "the native tool-use name must reach the Chat caller: {text}"
    );
    assert!(
        tool_call["function"]["arguments"].as_str().is_some(),
        "Chat tool arguments must be a JSON string, not an object: {text}"
    );
    assert_eq!(
        caller["choices"][0]["finish_reason"], "tool_calls",
        "a tool_use stop must become the Chat tool_calls finish reason: {text}"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-006 / REQ-003: the Responses function schema converts to Anthropic tools
/// and a native tool-use response becomes a Responses `function_call` output
/// item whose arguments are a JSON string.
#[tokio::test]
async fn responses_entry_round_trips_function_tools_through_anthropic() {
    let home = temp_home("conv-responses-tools");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{
                    "type": "tool_use",
                    "id": "toolu_2",
                    "name": "get_weather",
                    "input": {"city": "SF"}
                }]),
                "tool_use",
                json!({"input_tokens": 8, "output_tokens": 4}),
            ),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-tools",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _ct, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(responses_tool_body()),
    )
    .await;
    assert_eq!(status, 200, "the Responses tool round trip must succeed: {text}");

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(captured.len(), 1, "exactly one upstream attempt");
    let sent: Value = serde_json::from_slice(&captured[0].body).expect("upstream JSON");
    let sent_text = serde_json::to_string(&sent).expect("serialize upstream body");
    assert!(
        sent_text.contains("\"name\":\"get_weather\"") && sent_text.contains("\"input_schema\""),
        "the Responses function tool must convert to an Anthropic tool: {sent_text}"
    );

    let caller: Value = serde_json::from_str(&text).expect("caller body must be JSON");
    let output = caller["output"]
        .as_array()
        .expect("Responses output must be an array");
    let function_call = output
        .iter()
        .find(|item| item["type"] == "function_call")
        .unwrap_or_else(|| panic!("the tool_use must become a function_call output item: {text}"));
    assert_eq!(function_call["name"], "get_weather");
    assert!(
        function_call["arguments"].as_str().is_some(),
        "Responses function-call arguments must be a JSON string: {text}"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

// ---------------------------------------------------------------------------
// AC-007: unsupported conversion fails field-specifically before any upstream
// ---------------------------------------------------------------------------

/// AC-007 / REQ-003: a Chat request carrying a field the Anthropic conversion
/// cannot represent fails with a field-specific 400 before any upstream
/// attempt.
#[tokio::test]
async fn unsupported_chat_conversion_fields_fail_field_specifically() {
    let home = temp_home("conv-unsupported-chat");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(200, anthropic_message(json!([]), "end_turn", json!({})))
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-unsup",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let cases = [
        ("n", json!({"model": LOCAL_MODEL, "n": 2, "messages": [{"role": "user", "content": "hi"}]})),
        (
            "response_format",
            json!({"model": LOCAL_MODEL, "response_format": {"type": "json_schema", "json_schema": {"name": "x", "schema": {"type": "object"}}}, "messages": [{"role": "user", "content": "hi"}]}),
        ),
    ];
    for (field, body) in cases {
        let (status, _ct, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(body),
        )
        .await;
        assert_eq!(
            status, 400,
            "unsupported Chat field `{field}` must be a field-specific 400: {status} {text}"
        );
        assert!(
            text.contains(field),
            "the 400 must identify `{field}`: {text}"
        );
    }

    assert_eq!(
        captured_len(&log),
        0,
        "no unsupported converted request may reach an upstream"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-007 / REQ-003: a Responses request carrying a field the stateless
/// Anthropic conversion cannot represent (or an unresolved non-message
/// reference) fails with a field-specific 400 before any upstream attempt.
#[tokio::test]
async fn unsupported_responses_conversion_fields_fail_field_specifically() {
    let home = temp_home("conv-unsupported-responses");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(200, anthropic_message(json!([]), "end_turn", json!({})))
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-unsup",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let cases = [
        ("previous_response_id", json!({"model": LOCAL_MODEL, "previous_response_id": "resp_1", "input": "hi"})),
        ("background", json!({"model": LOCAL_MODEL, "background": true, "input": "hi"})),
        ("store", json!({"model": LOCAL_MODEL, "store": true, "input": "hi"})),
        ("response_format", json!({"model": LOCAL_MODEL, "response_format": {"type": "json_schema", "json_schema": {"name": "x", "schema": {"type": "object"}}}, "input": "hi"})),
        ("reference", json!({"model": LOCAL_MODEL, "input": [{"type": "item_reference", "id": "item_1"}]})),
    ];
    for (field, body) in cases {
        let (status, _ct, text) = call_gateway(
            port,
            "POST",
            "/v1/responses",
            &[("authorization", "Bearer local-key")],
            Some(body),
        )
        .await;
        assert_eq!(
            status, 400,
            "unsupported Responses field `{field}` must be a field-specific 400: {status} {text}"
        );
        assert!(
            text.contains(field),
            "the 400 must identify `{field}`: {text}"
        );
    }

    assert_eq!(
        captured_len(&log),
        0,
        "no unsupported converted request may reach an upstream"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-007 counterexample / REQ-003: the same-protocol OpenAI forwarding path
/// must keep accepting the fields the converter rejects for Anthropic. An
/// OpenAI Chat upstream receives `n`/`response_format`, and an OpenAI Responses
/// upstream receives `store`/`previous_response_id`, unchanged.
#[tokio::test]
async fn same_protocol_openai_requests_keep_conversion_only_fields() {
    let home = temp_home("conv-openai-regression");
    let port = free_port().await;
    let (chat_url, chat_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "c", "choices": [{"message": {"role": "assistant", "content": "ok"}}]})))
            .await;
    let (responses_url, responses_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "r", "object": "response", "output": []})))
            .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![
            openai_provider_value("p-chat", &chat_url, "chat_completions", LOCAL_MODEL, "remote-chat", "sk-chat"),
            openai_provider_value("p-responses", &responses_url, "responses", LOCAL_MODEL_B, "remote-resp", "sk-resp"),
        ],
    ));
    start_server(None).await.expect("start the listener");

    let (chat_status, _ct, chat_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({
            "model": LOCAL_MODEL,
            "n": 2,
            "response_format": {"type": "json_object"},
            "messages": [{"role": "user", "content": "hi"}]
        })),
    )
    .await;
    assert_eq!(
        chat_status, 200,
        "an OpenAI Chat request must keep its own fields: {chat_text}"
    );
    let chat_sent: Value =
        serde_json::from_slice(&chat_log.lock().expect("chat log")[0].body).expect("chat JSON");
    assert_eq!(chat_sent["n"], 2, "the Chat passthrough must keep n: {chat_sent}");
    assert!(
        chat_sent["response_format"].is_object(),
        "the Chat passthrough must keep response_format: {chat_sent}"
    );

    let (responses_status, _ct, responses_text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({
            "model": LOCAL_MODEL_B,
            "store": true,
            "previous_response_id": "resp_1",
            "input": "hi"
        })),
    )
    .await;
    assert_eq!(
        responses_status, 200,
        "an OpenAI Responses request must keep its own fields: {responses_text}"
    );
    let responses_sent: Value =
        serde_json::from_slice(&responses_log.lock().expect("responses log")[0].body)
            .expect("responses JSON");
    assert_eq!(responses_sent["store"], true, "the Responses passthrough must keep store");
    assert_eq!(
        responses_sent["previous_response_id"], "resp_1",
        "the Responses passthrough must keep previous_response_id"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

// ---------------------------------------------------------------------------
// AC-008: converted SSE is incremental and complete
// ---------------------------------------------------------------------------

/// Anthropic SSE with text plus two tool-use blocks, split so UTF-8, JSON and
/// SSE delimiters all straddle a chunk boundary in the gated test below.
const ANTHROPIC_SSE_TOOLS: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"role\":\"assistant\",\"model\":\"commandcode/claude-sonnet-5-5\",\"content\":[],\"usage\":{\"input_tokens\":10,\"cache_read_input_tokens\":20,\"cache_creation_input_tokens\":3,\"output_tokens\":0}}}\n\n",
    "event: content_block_start\n",
    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
    "event: content_block_stop\n",
    "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: content_block_start\n",
    "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"get_weather\",\"input\":{}}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\":\"}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"SF\\\"}\"}}\n\n",
    "event: content_block_stop\n",
    "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":5}}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

/// Anthropic SSE carrying only a ping before a success terminator.
const ANTHROPIC_SSE_PING_ONLY: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":4,\"output_tokens\":0}}}\n\n",
    "event: ping\n",
    "data: {\"type\":\"ping\"}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

/// Anthropic SSE with start usage and two cumulative output snapshots that omit
/// the input tiers (AC-011).
const ANTHROPIC_SSE_USAGE: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"role\":\"assistant\",\"model\":\"commandcode/claude-sonnet-5-5\",\"content\":[],\"usage\":{\"input_tokens\":10,\"cache_read_input_tokens\":20,\"cache_creation_input_tokens\":3,\"output_tokens\":0}}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":null},\"usage\":{\"output_tokens\":2}}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":7}}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

/// Relay one Anthropic SSE payload through the existing streaming seam for an
/// OpenAI inbound path and return the caller-visible bytes, the capture and the
/// attempt log.
async fn relay_stream(
    base_url: &str,
    provider_value: &Value,
    path: &str,
    body: &Value,
) -> (String, ForwardCapture, Vec<AttemptLog>) {
    let _home = isolated_temp_home("converted-stream-relay");
    let mut provider_value = provider_value.clone();
    provider_value["base_url"] = json!(base_url);
    let provider: GatewayUpstreamProvider = serde_json::from_value(provider_value.clone())
        .expect("the `anthropic_messages` provider must deserialize");
    write_raw_gateway_config(&raw_config(0, vec![provider_value]));

    let body = serde_json::to_vec(body).expect("encode the inbound request");
    let (mut client, mut server) = tokio::io::duplex(256 * 1024);
    let mut attempts: Vec<AttemptLog> = Vec::new();
    let capture = attempt_streaming(
        &mut server,
        std::slice::from_ref(&provider),
        path,
        &body,
        Some(LOCAL_MODEL),
        &HashMap::new(),
        false,
        None,
        &mut attempts,
    )
    .await
    .expect("the converted stream must complete");
    drop(server);

    let mut output = Vec::new();
    client
        .read_to_end(&mut output)
        .await
        .expect("read the relayed converted stream");
    (String::from_utf8_lossy(&output).into_owned(), capture, attempts)
}

/// AC-008 / REQ-004: converting an Anthropic tools stream into Chat emits
/// ordered text and tool-call deltas with stable identities, and ends exactly
/// once with `[DONE]`.
#[tokio::test]
async fn converted_chat_stream_emits_incremental_text_and_tools_then_done_once() {
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Stream(ANTHROPIC_SSE_TOOLS.to_string())).await;
    let provider = anthropic_provider_value(
        "p-stream",
        &upstream_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let (output, capture, attempts) = relay_stream(
        &upstream_url,
        &provider,
        "/v1/chat/completions",
        &json!({"model": LOCAL_MODEL, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;

    assert!(
        output.contains("text/event-stream"),
        "the converted Chat stream must be SSE: {output}"
    );
    assert!(
        output.contains("\"Hel\"") && output.contains("\"lo\""),
        "ordered text deltas must reach the caller: {output}"
    );
    assert!(
        output.contains("toolu_1"),
        "the tool identity must survive conversion: {output}"
    );
    assert!(
        output.contains("get_weather"),
        "the tool name must survive conversion: {output}"
    );
    assert!(
        output.contains("{\\\"city\\\":") || output.contains("{\"city\":"),
        "the argument deltas must reach the caller: {output}"
    );
    assert_eq!(
        output.matches("[DONE]").count(),
        1,
        "a converted Chat stream must end with exactly one [DONE]: {output}"
    );
    assert!(
        !output.contains("event: message_start"),
        "raw Anthropic events must not leak to the caller: {output}"
    );
    assert_eq!(
        attempts.len(),
        1,
        "the converted stream must record exactly one upstream attempt: {output}"
    );
    assert_eq!(capture.status, 200, "a message_stop-terminated stream is a success");
    assert_eq!(attempts.last().expect("one attempt").result, UsageResult::Success);
}

/// AC-008 / REQ-004: converting an Anthropic tools stream into Responses emits
/// the incremental output-text and function-call argument events plus a
/// completion event, with no Chat `[DONE]`.
#[tokio::test]
async fn converted_responses_stream_emits_incremental_text_tools_and_completion() {
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Stream(ANTHROPIC_SSE_TOOLS.to_string())).await;
    let provider = anthropic_provider_value(
        "p-stream",
        &upstream_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let (output, _capture, attempts) = relay_stream(
        &upstream_url,
        &provider,
        "/v1/responses",
        &json!({"model": LOCAL_MODEL, "stream": true, "input": "hi"}),
    )
    .await;

    assert!(
        output.contains("text/event-stream"),
        "the converted Responses stream must be SSE: {output}"
    );
    assert!(
        output.contains("response.output_text.delta") || output.contains("output_text.delta"),
        "incremental text events must reach the Responses caller: {output}"
    );
    assert!(
        output.contains("function_call_arguments.delta"),
        "incremental tool argument events must reach the Responses caller: {output}"
    );
    assert!(
        output.contains("response.completed"),
        "a completed Responses stream must carry its completion event: {output}"
    );
    assert!(
        !output.contains("[DONE]"),
        "a Responses stream must not inject the Chat [DONE]: {output}"
    );
    assert_eq!(
        attempts.len(),
        1,
        "the converted stream must record exactly one upstream attempt: {output}"
    );
    assert_eq!(attempts.last().expect("one attempt").result, UsageResult::Success);
}

/// AC-008 / REQ-004: a ping-only native stream must not fabricate caller
/// content.
#[tokio::test]
async fn ping_only_native_stream_does_not_fabricate_content() {
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Stream(ANTHROPIC_SSE_PING_ONLY.to_string())).await;
    let provider = anthropic_provider_value(
        "p-ping",
        &upstream_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let (output, _capture, _attempts) = relay_stream(
        &upstream_url,
        &provider,
        "/v1/chat/completions",
        &json!({"model": LOCAL_MODEL, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;

    assert!(
        !output.contains("\"content\":\"Hel\"") && !output.contains("\"text\":\"hi\""),
        "a ping-only stream must not fabricate assistant content: {output}"
    );
    assert_eq!(
        output.matches("[DONE]").count(),
        1,
        "a ping-only Chat stream still terminates once with [DONE]: {output}"
    );
}

/// A gated Anthropic SSE upstream: send `first`, announce it, then wait for the
/// test to release it (or a bounded fallback) before sending `rest`. The
/// fallback guarantees forward progress for a non-incremental implementation so
/// the test fails an assertion instead of hanging.
struct GatedStream {
    url: String,
    release: Arc<Notify>,
    rest_sent: Arc<AtomicBool>,
}

async fn spawn_gated_anthropic_stream(first: &'static str, rest: &'static str) -> GatedStream {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind gated upstream");
    let addr = listener.local_addr().expect("gated upstream address");
    let release = Arc::new(Notify::new());
    let rest_sent = Arc::new(AtomicBool::new(false));
    let release_srv = release.clone();
    let rest_sent_srv = rest_sent.clone();
    tauri::async_runtime::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let _ = read_http_request(&mut stream).await;
        let header =
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
        let _ = stream.write_all(header.as_bytes()).await;
        let _ = stream.write_all(first.as_bytes()).await;
        let _ = stream.flush().await;
        tokio::select! {
            _ = release_srv.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
        }
        rest_sent_srv.store(true, Ordering::SeqCst);
        let _ = stream.write_all(rest.as_bytes()).await;
        let _ = stream.flush().await;
    });
    GatedStream {
        url: format!("http://{addr}"),
        release,
        rest_sent,
    }
}

/// AC-008 / REQ-004: the caller must receive the first converted delta while
/// the upstream is still producing output (sequencing, not a timing threshold).
#[tokio::test]
async fn converted_chat_stream_is_incremental_before_upstream_completion() {
    let _home = isolated_temp_home("converted-chat-incremental");
    let first = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"role\":\"assistant\",\"model\":\"commandcode/claude-sonnet-5-5\",\"content\":[],\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
    );
    let rest = concat!(
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    let gated = spawn_gated_anthropic_stream(first, rest).await;

    let provider_value = anthropic_provider_value(
        "p-inc",
        &gated.url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let provider: GatewayUpstreamProvider = serde_json::from_value(provider_value.clone())
        .expect("provider must deserialize");
    write_raw_gateway_config(&raw_config(0, vec![provider_value]));

    let body = serde_json::to_vec(&json!({
        "model": LOCAL_MODEL,
        "stream": true,
        "messages": [{"role": "user", "content": "hi"}]
    }))
    .expect("encode the inbound request");
    let (mut client, server) = tokio::io::duplex(256 * 1024);

    let forward = async move {
        let mut server = server;
        let mut attempts: Vec<AttemptLog> = Vec::new();
        let result = attempt_streaming(
            &mut server,
            std::slice::from_ref(&provider),
            "/v1/chat/completions",
            &body,
            Some(LOCAL_MODEL),
            &HashMap::new(),
            false,
            None,
            &mut attempts,
        )
        .await;
        (result, attempts)
    };

    let reader = async {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let mut incremental = false;
        loop {
            match client.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf);
                    if !incremental && text.contains("\"Hel\"") {
                        incremental = !gated.rest_sent.load(Ordering::SeqCst);
                        gated.release.notify_one();
                    }
                    if text.contains("[DONE]") {
                        break;
                    }
                }
            }
        }
        (buf, incremental)
    };

    let ((result, _attempts), (buf, incremental)) = tokio::join!(forward, reader);
    result.expect("the converted stream must complete");
    let output = String::from_utf8_lossy(&buf).into_owned();

    assert!(
        output.contains("text/event-stream"),
        "the caller must receive a converted SSE stream: {output}"
    );
    assert!(
        incremental,
        "the first converted delta must reach the caller before upstream completion: {output}"
    );
    assert!(output.contains("Hel") && output.contains("lo"), "both deltas: {output}");
    assert_eq!(
        output.matches("[DONE]").count(),
        1,
        "exactly one [DONE]: {output}"
    );
}

// ---------------------------------------------------------------------------
// AC-009: post-byte failure and cancellation settle without replay
// ---------------------------------------------------------------------------

/// AC-009 / REQ-004: a converted stream that fails after bytes have reached the
/// caller emits a caller-format error without a successful completion marker or
/// replay, and the attempt is recorded as a failure.
#[tokio::test]
async fn converted_chat_stream_failure_after_first_byte_has_no_completion() {
    let first = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"role\":\"assistant\",\"model\":\"commandcode/claude-sonnet-5-5\",\"content\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"partial\"}}\n\n",
    );
    let declared = first.len() + 256;
    let body = first.to_string();
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::PartialStream(body.clone(), declared)).await;
    let provider = anthropic_provider_value(
        "p-fail",
        &upstream_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let (output, capture, attempts) = relay_stream(
        &upstream_url,
        &provider,
        "/v1/chat/completions",
        &json!({"model": LOCAL_MODEL, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;

    assert!(
        output.contains("\"error\""),
        "a post-byte converted failure must emit a caller-format error: {output}"
    );
    assert!(
        !output.contains("[DONE]"),
        "a failed converted stream must never gain a completion marker: {output}"
    );
    assert_eq!(
        attempts.len(),
        1,
        "the post-byte failure must record exactly one upstream attempt, got {}: {output}",
        attempts.len()
    );
    assert_eq!(
        attempts.last().expect("one attempt").result,
        UsageResult::Failure,
        "the failed attempt must be recorded as a failure"
    );
    // R15: the captured start/delta usage stays on the failed attempt even
    // though the stream never completed.
    assert_eq!(
        attempts.last().expect("one attempt").usage,
        Some(UsageTokens {
            input_tokens: 6,
            ..UsageTokens::default()
        }),
        "the failed attempt must retain the field-wise merged start usage"
    );
    assert_eq!(capture.status, 502, "the terminal capture must be a failure");
}

/// AC-009 / REQ-004: a downstream cancellation of a converted streaming request
/// discards the whole buffer, so no usage row is persisted.
#[tokio::test]
async fn converted_stream_downstream_cancel_writes_zero_rows() {
    let home = isolated_temp_home("converted-cancel-stream");
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind held upstream");
    let upstream_url = format!("http://{}", listener.local_addr().expect("upstream address"));
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let upstream = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept held upstream");
        read_http_request(&mut stream)
            .await
            .expect("read held upstream request");
        let _ = entered_tx.send(());
        let _ = release_rx.await;
        // Never start the stream; the client disconnects while the upstream waits.
    });

    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", LOCAL_KEY));
    let provider: GatewayUpstreamProvider = serde_json::from_value(anthropic_provider_value(
        "p-cancel",
        &upstream_url,
        "local",
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    ))
    .expect("provider must deserialize");
    config.providers.push(provider);
    write_config(&config).expect("write relay config");

    let (client, mut handler) = spawn_handle_connection(true).await;
    let entered = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::select! {
            signal = entered_rx => signal,
            result = &mut handler => panic!("handler exited before upstream wait: {result:?}"),
        }
    })
    .await
    .expect("upstream must begin waiting");
    entered.expect("held upstream entry signal");
    drop(client);

    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), &mut handler).await;
    let _ = release_tx.send(());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), upstream).await;

    let records = super::default_usage_store().all_records().unwrap_or_default();
    assert!(
        records.is_empty(),
        "a cancelled converted streaming request must write zero log rows: {} observed",
        records.len()
    );
    drop(home);
}

// ---------------------------------------------------------------------------
// AC-010: conversion-aware resolution preserves the runtime boundaries
// ---------------------------------------------------------------------------

/// AC-010 / REQ-005: a Responses-only upstream is never selected by the Chat
/// entry, while the Responses entry still serves it (mutual exclusivity).
#[tokio::test]
async fn chat_entry_does_not_serve_responses_only_upstream() {
    let home = temp_home("boundary-chat-vs-responses");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "r", "object": "response", "output": []})))
            .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![openai_provider_value(
            "p-responses",
            &upstream_url,
            "responses",
            LOCAL_MODEL,
            "remote-resp",
            "sk-resp",
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (chat_status, _ct, chat_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}]})),
    )
    .await;
    assert_eq!(
        chat_status, 502,
        "a Responses-only upstream must not serve the Chat entry: {chat_text}"
    );

    let (responses_status, _ct, responses_text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": LOCAL_MODEL, "input": "hi"})),
    )
    .await;
    assert_eq!(
        responses_status, 200,
        "the Responses entry must still serve the Responses-only upstream: {responses_text}"
    );
    assert_eq!(
        captured_len(&log),
        1,
        "only the Responses request may reach the upstream"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-010 / REQ-005: a disabled Anthropic mapping row is never served through
/// the Chat entry and does not fall back to a default model.
#[tokio::test]
async fn disabled_anthropic_mapping_is_not_served_through_chat_entry() {
    let home = temp_home("boundary-disabled-anthropic");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(200, anthropic_message(json!([]), "end_turn", json!({})))
    })
    .await;
    let mut provider = anthropic_provider_value(
        "p-disabled",
        &upstream_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    provider["mappings"][0]["enabled"] = json!(false);
    provider["default_model"] = json!("remote-default");
    write_raw_gateway_config(&raw_config(port, vec![provider]));
    start_server(None).await.expect("start the listener");

    let (status, _ct, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}]})),
    )
    .await;
    assert_eq!(
        status, 502,
        "a disabled row must not serve and must not fall back to the default: {text}"
    );
    assert_eq!(captured_len(&log), 0, "no upstream request for a disabled row");

    stop_server().await.expect("stop the listener");
    drop(home);
}

// ---------------------------------------------------------------------------
// AC-011: actual upstream usage is normalized before conversion
// ---------------------------------------------------------------------------

/// AC-011 / REQ-005: a converted stream persists the start input/cache tiers
/// merged field-wise with the last cumulative output snapshot (10/20/3/7), and
/// exposes the inclusive Chat input total (33) with cache details.
#[tokio::test]
async fn converted_chat_stream_persists_actual_tiers_and_reports_inclusive_input() {
    let home = temp_home("conv-usage-tiers");
    let port = free_port().await;
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Stream(ANTHROPIC_SSE_USAGE.to_string())).await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-usage",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _ct, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": LOCAL_MODEL, "stream": true, "messages": [{"role": "user", "content": "hi"}]})),
    )
    .await;
    assert_eq!(status, 200, "the converted usage fixture must be served: {text}");

    let records = super::wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == LOCAL_MODEL)
        .unwrap_or_else(|| panic!("no usage row for {LOCAL_MODEL}: {records:?}"));
    assert_eq!(
        (
            record.input_tokens,
            record.cache_read_tokens,
            record.cache_write_tokens,
            record.output_tokens,
        ),
        (10, 20, 3, 7),
        "the persisted tiers must be the actual upstream decomposition"
    );
    assert_eq!(record.total_tokens, 40, "10 + 20 + 3 + 7 = 40");
    assert!(
        text.contains("33"),
        "the converted Chat usage must expose the inclusive input total 33: {text}"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-011 / REQ-005: a non-streaming converted success persists the same
/// exclusive tiers and returns the caller's inclusive usage.
#[tokio::test]
async fn converted_chat_non_streaming_persists_actual_tiers() {
    let home = temp_home("conv-usage-nonstream");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "ok"}]),
                "end_turn",
                json!({"input_tokens": 10, "cache_read_input_tokens": 20, "cache_creation_input_tokens": 3, "output_tokens": 7}),
            ),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-usage",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _ct, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}]})),
    )
    .await;
    assert_eq!(status, 200, "the non-streaming conversion must be served: {text}");

    let records = super::wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == LOCAL_MODEL)
        .unwrap_or_else(|| panic!("no usage row for {LOCAL_MODEL}: {records:?}"));
    assert_eq!(
        (
            record.input_tokens,
            record.cache_read_tokens,
            record.cache_write_tokens,
            record.output_tokens,
        ),
        (10, 20, 3, 7),
        "the persisted tiers must be the actual upstream decomposition"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// AC-010 / REQ-005: a raw non-JSON body on the Chat entry still becomes a
/// caller-format error and never reaches an upstream.
#[tokio::test]
async fn malformed_chat_body_is_a_client_error_without_upstream_attempt() {
    let home = temp_home("conv-malformed");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(200, anthropic_message(json!([]), "end_turn", json!({})))
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-malformed",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let raw = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: Bearer {LOCAL_KEY}\r\ncontent-type: application/json\r\ncontent-length: 4\r\nconnection: close\r\n\r\n{{abc"
    );
    let response = super::call_gateway_raw(port, &raw).await;
    let (status, _body) = raw_http_status_and_body(&response);
    assert!(
        status.contains(" 400"),
        "malformed JSON must be a 400: {status}"
    );
    assert_eq!(captured_len(&log), 0, "no malformed request may reach an upstream");

    stop_server().await.expect("stop the listener");
    drop(home);
}

// ---------------------------------------------------------------------------
// Review-repair round: R2 / R4 / R5 / R6
//
// These tests shape-assert the caller-visible JSON/SSE and the captured native
// request body. They never disable the existing incremental/order assertions.
// ---------------------------------------------------------------------------

/// Parse every JSON payload carried by an SSE stream (`data:` lines), skipping
/// the Chat `[DONE]` sentinel. Used to shape-assert converted events.
fn sse_events(output: &str) -> Vec<Value> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| *data != "[DONE]")
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .collect()
}

/// R2 / REQ-003 / AC-005: a non-streaming Responses conversion of assistant
/// text must wrap the text in a `message` output item whose `content` carries an
/// `output_text` block, matching the OpenAI Responses shape.
#[tokio::test]
async fn responses_non_streaming_text_is_a_message_output_item() {
    let home = temp_home("responses-text-schema");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "hello"}]),
                "end_turn",
                json!({"input_tokens": 10, "output_tokens": 5}),
            ),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-r2a",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _ct, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(responses_body()),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let caller: Value = serde_json::from_str(&text).expect("caller body must be JSON");
    let output = caller["output"]
        .as_array()
        .expect("Responses output must be an array");
    let message = output
        .iter()
        .find(|item| item["type"] == "message")
        .unwrap_or_else(|| panic!("Responses text must be a `message` output item: {text}"));
    let content = message["content"]
        .as_array()
        .expect("the message item must carry a content array");
    let text_block = content
        .iter()
        .find(|item| item["type"] == "output_text")
        .unwrap_or_else(|| {
            panic!("the message content must carry an `output_text` block: {text}")
        });
    assert!(
        text_block["text"].as_str().is_some_and(|value| value.contains("hello")),
        "the output_text block must carry the assistant text: {text}"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// R2 / REQ-004 / AC-008: a converted Responses stream must announce the
/// assistant `message` output item, keep the incremental text delta, and carry
/// `output` and `usage` on the completed response object.
#[tokio::test]
async fn converted_responses_stream_adds_message_item_and_completes_with_output_and_usage() {
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Stream(ANTHROPIC_SSE_TOOLS.to_string())).await;
    let provider = anthropic_provider_value(
        "p-r2b",
        &upstream_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let (output, _capture, _attempts) = relay_stream(
        &upstream_url,
        &provider,
        "/v1/responses",
        &json!({"model": LOCAL_MODEL, "stream": true, "input": "hi"}),
    )
    .await;

    let events = sse_events(&output);
    let added_message = events.iter().find(|event| {
        event["type"] == "response.output_item.added" && event["item"]["type"] == "message"
    });
    assert!(
        added_message.is_some(),
        "a converted Responses stream must add a `message` output item: {output}"
    );
    assert!(
        events
            .iter()
            .any(|event| event["type"] == "response.output_text.delta"),
        "the incremental text delta must survive: {output}"
    );
    let completed = events
        .iter()
        .find(|event| event["type"] == "response.completed")
        .unwrap_or_else(|| panic!("a completed Responses stream must complete: {output}"));
    assert!(
        completed["response"]["output"].is_array(),
        "response.completed.response must carry the output items: {output}"
    );
    assert!(
        completed["response"]["usage"].is_object(),
        "response.completed.response must carry usage: {output}"
    );
}

/// R4 / REQ-004: a native stream whose final event has no trailing blank line
/// (immediate EOF after `message_stop`) must still emit the caller completion
/// event: exactly one Chat `[DONE]` and a Responses `response.completed`.
#[tokio::test]
async fn converted_stream_without_trailing_blank_line_still_terminates_once() {
    const SSE_STOP_NO_TRAILING_BLANK: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"role\":\"assistant\",\"model\":\"commandcode/claude-sonnet-5-5\",\"content\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}",
    );

    let (chat_url, _chat_log) =
        spawn_mock_upstream(|_| MockReply::Stream(SSE_STOP_NO_TRAILING_BLANK.to_string())).await;
    let chat_provider = anthropic_provider_value(
        "p-r4a-chat",
        &chat_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let (chat_output, _capture, _attempts) = relay_stream(
        &chat_url,
        &chat_provider,
        "/v1/chat/completions",
        &json!({"model": LOCAL_MODEL, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    assert_eq!(
        chat_output.matches("[DONE]").count(),
        1,
        "an unterminated-but-complete Chat stream must still end with exactly one [DONE]: {chat_output}"
    );

    let (responses_url, _responses_log) =
        spawn_mock_upstream(|_| MockReply::Stream(SSE_STOP_NO_TRAILING_BLANK.to_string())).await;
    let responses_provider = anthropic_provider_value(
        "p-r4a-responses",
        &responses_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let (responses_output, _capture, _attempts) = relay_stream(
        &responses_url,
        &responses_provider,
        "/v1/responses",
        &json!({"model": LOCAL_MODEL, "stream": true, "input": "hi"}),
    )
    .await;
    assert!(
        responses_output.contains("response.completed"),
        "an unterminated-but-complete Responses stream must still complete: {responses_output}"
    );
}

/// R4 / REQ-004 / AC-009: an event whose JSON cannot be parsed after bytes have
/// already reached the caller is a post-byte failure: a caller-format error, no
/// success marker and no replay.
#[tokio::test]
async fn converted_stream_malformed_event_is_post_byte_failure() {
    const SSE_MALFORMED_EVENT: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"role\":\"assistant\",\"model\":\"commandcode/claude-sonnet-5-5\",\"content\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {this-is-not-json}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Stream(SSE_MALFORMED_EVENT.to_string())).await;
    let provider = anthropic_provider_value(
        "p-r4b",
        &upstream_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let (output, capture, attempts) = relay_stream(
        &upstream_url,
        &provider,
        "/v1/chat/completions",
        &json!({"model": LOCAL_MODEL, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;

    assert!(
        output.contains("\"error\""),
        "a malformed converted event must emit a caller-format error: {output}"
    );
    assert!(
        !output.contains("[DONE]"),
        "a malformed converted event must never gain a success marker: {output}"
    );
    assert_eq!(
        attempts.len(),
        1,
        "a post-byte converted failure must not replay: {output}"
    );
    assert_eq!(
        attempts.last().expect("one attempt").result,
        UsageResult::Failure,
        "the malformed converted stream must be recorded as a failure"
    );
    assert_eq!(capture.status, 502, "the terminal capture must be a failure");
}

/// R5 / REQ-003 / AC-006: the Chat entry maps `tool_choice:"none"` to an
/// explicit Anthropic `{"type":"none"}`, injects
/// `{"type":"auto","disable_parallel_tool_use":true}` when parallel calls are
/// disabled without a choice, and nests the flag inside an explicit choice.
#[tokio::test]
async fn chat_conversion_tool_choice_none_and_parallel_controls() {
    let home = temp_home("chat-tool-choice-parallel");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(json!([{"type": "text", "text": "ok"}]), "end_turn", json!({})),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-r5-chat",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let tools = json!([{
        "type": "function",
        "function": {"name": "get_weather", "parameters": {"type": "object"}}
    }]);
    let requests = [
        json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "tools": tools, "tool_choice": "none"}),
        json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "tools": tools, "parallel_tool_calls": false}),
        json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "tools": tools, "tool_choice": {"type": "function", "function": {"name": "get_weather"}}, "parallel_tool_calls": false}),
    ];
    for body in requests {
        let (status, _ct, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(body),
        )
        .await;
        assert_eq!(status, 200, "the tool-choice fixture must be served: {text}");
    }

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(captured.len(), 3, "three upstream attempts");
    let sent: Vec<Value> = captured
        .iter()
        .map(|attempt| serde_json::from_slice(&attempt.body).expect("upstream JSON"))
        .collect();

    assert_eq!(
        sent[0]["tool_choice"],
        json!({"type": "none"}),
        "tool_choice none must convert to an explicit none: {}",
        sent[0]
    );
    assert_eq!(
        sent[1]["tool_choice"],
        json!({"type": "auto", "disable_parallel_tool_use": true}),
        "parallel_tool_calls:false without a choice must inject an auto choice: {}",
        sent[1]
    );
    assert_eq!(
        sent[2]["tool_choice"],
        json!({"type": "tool", "name": "get_weather", "disable_parallel_tool_use": true}),
        "the parallel flag must be nested inside an explicit function choice: {}",
        sent[2]
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// R5 / REQ-003 / AC-006: the Responses entry keeps the tool-choice/parallel
/// controls at the `tool_choice` level (never a top-level body flag).
#[tokio::test]
async fn responses_conversion_tool_choice_and_parallel_nesting() {
    let home = temp_home("responses-tool-choice-parallel");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(json!([{"type": "text", "text": "ok"}]), "end_turn", json!({})),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-r5-responses",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let tools = json!([{
        "type": "function",
        "name": "get_weather",
        "parameters": {"type": "object"}
    }]);
    let requests = [
        json!({"model": LOCAL_MODEL, "input": "hi", "tools": tools, "tool_choice": "none"}),
        json!({"model": LOCAL_MODEL, "input": "hi", "tools": tools, "parallel_tool_calls": false}),
        json!({"model": LOCAL_MODEL, "input": "hi", "tools": tools, "tool_choice": {"type": "auto"}, "parallel_tool_calls": false}),
    ];
    for body in requests {
        let (status, _ct, text) = call_gateway(
            port,
            "POST",
            "/v1/responses",
            &[("authorization", "Bearer local-key")],
            Some(body),
        )
        .await;
        assert_eq!(status, 200, "the Responses tool-choice fixture must be served: {text}");
    }

    let captured = log.lock().expect("mock log").clone();
    assert_eq!(captured.len(), 3, "three upstream attempts");
    let sent: Vec<Value> = captured
        .iter()
        .map(|attempt| serde_json::from_slice(&attempt.body).expect("upstream JSON"))
        .collect();

    assert_eq!(
        sent[0]["tool_choice"],
        json!({"type": "none"}),
        "tool_choice none must convert to an explicit none: {}",
        sent[0]
    );
    assert_eq!(
        sent[1]["tool_choice"],
        json!({"type": "auto", "disable_parallel_tool_use": true}),
        "parallel_tool_calls:false without a choice must inject an auto choice: {}",
        sent[1]
    );
    assert_eq!(
        sent[2]["tool_choice"],
        json!({"type": "auto", "disable_parallel_tool_use": true}),
        "the parallel flag must be nested inside tool_choice: {}",
        sent[2]
    );
    assert!(
        sent[2].get("disable_parallel_tool_use").is_none(),
        "the parallel flag must not be a top-level body field: {}",
        sent[2]
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// R6 / REQ-003 / AC-007: `response_format:{"type":"text"}` is accepted and the
/// request reaches the upstream, while JSON object/schema and unknown output
/// formats remain field-specific 400s before any attempt.
#[tokio::test]
async fn chat_conversion_response_format_text_is_accepted_and_others_rejected() {
    let home = temp_home("chat-response-format");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(json!([{"type": "text", "text": "ok"}]), "end_turn", json!({})),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-r6",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    // Plain text output is acceptable and reaches the upstream.
    let (status, _ct, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({
            "model": LOCAL_MODEL,
            "messages": [{"role": "user", "content": "hi"}],
            "response_format": {"type": "text"}
        })),
    )
    .await;
    assert_eq!(
        status, 200,
        "response_format text must be accepted for Anthropic conversion: {text}"
    );
    assert_eq!(
        captured_len(&log),
        1,
        "the text response_format request must reach the upstream"
    );

    for format in [
        json!({"type": "json_object"}),
        json!({"type": "json_schema", "json_schema": {"name": "x", "schema": {"type": "object"}}}),
        json!({"type": "not_a_real_format"}),
    ] {
        let (status, _ct, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(json!({
                "model": LOCAL_MODEL,
                "messages": [{"role": "user", "content": "hi"}],
                "response_format": format
            })),
        )
        .await;
        assert_eq!(
            status, 400,
            "an unsupported response_format must be a field-specific 400: {format} {text}"
        );
        assert!(
            text.contains("response_format"),
            "the 400 must identify response_format: {text}"
        );
    }
    assert_eq!(
        captured_len(&log),
        1,
        "only the accepted text response_format request may reach the upstream"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// R6 / REQ-003 / AC-007: the Responses entry also accepts a plain text output
/// format while rejecting JSON/schema formats.
#[tokio::test]
async fn responses_conversion_response_format_text_is_accepted() {
    let home = temp_home("responses-response-format");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(json!([{"type": "text", "text": "ok"}]), "end_turn", json!({})),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-r6-responses",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _ct, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": LOCAL_MODEL, "input": "hi", "response_format": {"type": "text"}})),
    )
    .await;
    assert_eq!(
        status, 200,
        "response_format text must be accepted on the Responses entry: {text}"
    );
    assert_eq!(
        captured_len(&log),
        1,
        "the text response_format Responses request must reach the upstream"
    );

    let (status, _ct, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": LOCAL_MODEL, "input": "hi", "response_format": {"type": "json_schema", "json_schema": {"name": "x", "schema": {"type": "object"}}}})),
    )
    .await;
    assert_eq!(status, 400, "a schema response_format must be rejected: {text}");
    assert!(text.contains("response_format"), "the 400 must identify response_format: {text}");
    assert_eq!(captured_len(&log), 1, "no rejected request may reach the upstream");

    stop_server().await.expect("stop the listener");
    drop(home);
}

// ---------------------------------------------------------------------------
// Outbound stream-flag regression: the converted native Anthropic body must
// carry a JSON boolean `stream` that matches the inbound OpenAI request, and a
// non-streaming request must not invent one. Asserted only through the captured
// upstream HTTP body on the loopback entries.
// ---------------------------------------------------------------------------

/// The two upstream replies this regression needs: a valid native SSE stream for
/// the inbound `stream:true` cases and a JSON message for the omission cases.
#[derive(Clone, Copy)]
enum ConvertedUpstreamReply {
    Sse(&'static str),
    Message,
}

/// Drive one loopback conversion request against an `anthropic_messages`
/// mapping and return the captured outbound native Anthropic body. The global
/// `HOME` lock and the process-wide listener are released before any assertion
/// can panic, so one failing red case never strands the shared state the other
/// cases need.
async fn captured_converted_upstream_body(
    name: &str,
    path: &str,
    body: Value,
    reply: ConvertedUpstreamReply,
) -> Value {
    let home = temp_home(name);
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(move |_| match reply {
        ConvertedUpstreamReply::Sse(sse) => MockReply::Stream(sse.to_string()),
        ConvertedUpstreamReply::Message => MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "ok"}]),
                "end_turn",
                json!({"input_tokens": 1, "output_tokens": 1}),
            ),
        ),
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-stream-flag",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        path,
        &[("authorization", "Bearer local-key")],
        Some(body),
    )
    .await;

    let captured = log.lock().expect("mock log").clone();

    stop_server().await.expect("stop the listener");
    drop(home);

    assert_eq!(status, 200, "the converted request must be served: {text}");
    assert_eq!(captured.len(), 1, "exactly one upstream attempt");
    serde_json::from_slice(&captured[0].body).expect("upstream body must be JSON")
}

/// AC1: a Chat request with `stream:true` must forward a JSON boolean true in
/// the converted native Anthropic body. The mock returns SSE independently of
/// the flag, so the assertion reads the actual sent body and a dropped flag
/// cannot be hidden by the response shape.
#[tokio::test]
async fn converted_anthropic_stream_flag_is_forwarded_on_chat_entry() {
    let sent = captured_converted_upstream_body(
        "conv-stream-flag-chat",
        "/v1/chat/completions",
        json!({"model": LOCAL_MODEL, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
        ConvertedUpstreamReply::Sse(ANTHROPIC_SSE_TOOLS),
    )
    .await;

    assert_eq!(
        sent.get("stream"),
        Some(&Value::Bool(true)),
        "a Chat request with stream:true must forward a JSON boolean true to the Anthropic upstream: {sent}"
    );
}

/// AC2: a Responses request with `stream:true` must forward the same JSON
/// boolean true in the converted native Anthropic body, asserted on the actual
/// sent body.
#[tokio::test]
async fn converted_anthropic_stream_flag_is_forwarded_on_responses_entry() {
    let sent = captured_converted_upstream_body(
        "conv-stream-flag-responses",
        "/v1/responses",
        json!({"model": LOCAL_MODEL, "stream": true, "input": "hi"}),
        ConvertedUpstreamReply::Sse(ANTHROPIC_SSE_TOOLS),
    )
    .await;

    assert_eq!(
        sent.get("stream"),
        Some(&Value::Bool(true)),
        "a Responses request with stream:true must forward a JSON boolean true to the Anthropic upstream: {sent}"
    );
}

/// AC3: absent and explicit-false `stream` cases on both entries must leave the
/// converted body without a true stream flag, preserving the exact current
/// omission (`get("stream").is_none()`).
#[tokio::test]
async fn converted_anthropic_stream_flag_omitted_when_not_requested() {
    let cases = [
        (
            "/v1/chat/completions",
            json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}]}),
        ),
        (
            "/v1/chat/completions",
            json!({"model": LOCAL_MODEL, "stream": false, "messages": [{"role": "user", "content": "hi"}]}),
        ),
        ("/v1/responses", json!({"model": LOCAL_MODEL, "input": "hi"})),
        (
            "/v1/responses",
            json!({"model": LOCAL_MODEL, "stream": false, "input": "hi"}),
        ),
    ];
    for (index, (path, body)) in cases.into_iter().enumerate() {
        let sent = captured_converted_upstream_body(
            &format!("conv-stream-flag-omitted-{index}"),
            path,
            body,
            ConvertedUpstreamReply::Message,
        )
        .await;
        assert!(
            sent.get("stream").is_none(),
            "a non-streaming {path} request must not forward a stream flag: {sent}"
        );
    }
}

// ---------------------------------------------------------------------------
// Unified blank-text conversion normalization.
//
// One behavior: no Chat or Responses conversion may emit an empty or
// whitespace-only `text` block, and a message left with no blocks is a local
// 400 naming `messages.content` / `input.content` before any upstream attempt.
// The converted native `messages` are asserted by exact equality against an
// independent `json!` expectation; rejects are proven by the caller status and
// an unchanged capture count. Existing fixtures (`chat_tool_body`, `raw_config`,
// `anthropic_message`) are reused and no private converter helper is called.
// ---------------------------------------------------------------------------

/// Send one conversion request, require exactly one new upstream attempt, and
/// return the captured converted native body.
async fn converted_body(port: u16, log: &super::CapturedLog, path: &str, body: Value) -> Value {
    let before = captured_len(log);
    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        path,
        &[("authorization", "Bearer local-key")],
        Some(body),
    )
    .await;
    let captured = log.lock().expect("mock log").clone();
    assert_eq!(status, 200, "the conversion must be served: {status} {text}");
    assert_eq!(captured.len(), before + 1, "exactly one upstream attempt");
    serde_json::from_slice(&captured[before].body).expect("converted upstream body must be JSON")
}

/// One unified blank-text regression across every Chat/Responses input shape
/// that can reach a converted text block.
#[tokio::test]
async fn blank_conversion_text_blocks_are_dropped_or_rejected() {
    let home = temp_home("conversion-blank-text");
    let port = free_port().await;
    // Native SSE for the inbound `stream:true` cases and a plain native message
    // for every non-streaming case; the discriminator reads the converted
    // upstream body's forwarded `stream` flag.
    let (upstream_url, log) = spawn_mock_upstream(|captured| {
        let stream = serde_json::from_slice::<Value>(&captured.body)
            .ok()
            .and_then(|body| body.get("stream").and_then(Value::as_bool))
            .unwrap_or(false);
        if stream {
            MockReply::Stream(ANTHROPIC_SSE_TOOLS.to_string())
        } else {
            MockReply::Json(
                200,
                anthropic_message(
                    json!([{"type": "text", "text": "ok"}]),
                    "end_turn",
                    json!({"input_tokens": 1, "output_tokens": 1}),
                ),
            )
        }
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-blank",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    // Chat tool chain: swap the assistant content for every shape, for both
    // stream flags, and assert the whole converted `messages` by exact equality
    // (tool_use id/name/input and the linked tool_result tool_use_id/content).
    let tool_use = json!({"id": "call_1", "input": {"city": "SF"}, "name": "get_weather", "type": "tool_use"});
    let tool_result = json!({"content": "sunny", "tool_use_id": "call_1", "type": "tool_result"});
    let user_text = json!({"text": "use the tool", "type": "text"});
    for stream in [false, true] {
        for (label, content, surviving) in [
            ("empty_string", json!(""), None),
            ("null", json!(null), None),
            ("whitespace", json!(" \t\n "), None),
            ("nonblank_surrounded", json!("answer "), Some(json!({"text": "answer ", "type": "text"}))),
            ("mixed_array", json!([
                {"type": "text", "text": ""},
                {"type": "text", "text": "   "},
                {"type": "text", "text": "kept "}
            ]), Some(json!({"text": "kept ", "type": "text"}))),
        ] {
            let mut body = chat_tool_body();
            body["messages"][1]["content"] = content;
            body["stream"] = json!(stream);
            let sent = converted_body(port, &log, "/v1/chat/completions", body).await;
            let mut assistant = Vec::new();
            if let Some(block) = surviving {
                assistant.push(block);
            }
            assistant.push(tool_use.clone());
            assert_eq!(
                sent["messages"],
                json!([
                    {"content": [user_text.clone()], "role": "user"},
                    {"content": assistant, "role": "assistant"},
                    {"content": [tool_result.clone()], "role": "user"}
                ]),
                "chat assistant {label} stream={stream}: {sent}"
            );
            if stream {
                assert_eq!(sent.get("stream").and_then(Value::as_bool), Some(true), "stream flag: {sent}");
            } else {
                assert!(sent.get("stream").is_none(), "a non-streaming request must omit the flag: {sent}");
            }
        }
    }

    // Blank `instructions` must not produce a `system` message.
    for instructions in ["", " \t\n "] {
        let body = json!({
            "model": LOCAL_MODEL,
            "instructions": instructions,
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}]
        });
        let sent = converted_body(port, &log, "/v1/responses", body).await;
        assert_eq!(
            sent["messages"],
            json!([{"content": [{"text": "hi", "type": "text"}], "role": "user"}]),
            "blank instructions {instructions:?} must not produce a system block: {sent}"
        );
    }

    // A mixed Responses content array keeps only the nonblank text.
    let sent = converted_body(
        port,
        &log,
        "/v1/responses",
        json!({
            "model": LOCAL_MODEL,
            "input": [{"role": "user", "content": [
                {"type": "input_text", "text": ""},
                {"type": "input_text", "text": "   "},
                {"type": "input_text", "text": "  keep  "}
            ]}]
        }),
    )
    .await;
    assert_eq!(
        sent["messages"],
        json!([{"content": [{"text": "  keep  ", "type": "text"}], "role": "user"}]),
        "only the nonblank Responses text may survive: {sent}"
    );

    // A blank text block mixed with an image is dropped while the image stays.
    let sent = converted_body(
        port,
        &log,
        "/v1/chat/completions",
        json!({
            "model": LOCAL_MODEL,
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "   "},
                {"type": "image_url", "image_url": {"url": "https://example.com/photo.png"}}
            ]}]
        }),
    )
    .await;
    assert_eq!(
        sent["messages"],
        json!([{"content": [{
            "source": {"type": "url", "url": "https://example.com/photo.png"},
            "type": "image"
        }], "role": "user"}]),
        "a blank text block must not hide the image: {sent}"
    );

    // A message left with no blocks is a local 400 naming its content field,
    // with no upstream attempt.
    let rejects: Vec<(&str, &str, Value)> = vec![
        ("chat_empty_assistant_no_tools", "/v1/chat/completions", json!({"model": LOCAL_MODEL, "messages": [{"role": "assistant", "content": ""}]})),
        ("chat_whitespace_assistant_no_tools", "/v1/chat/completions", json!({"model": LOCAL_MODEL, "messages": [{"role": "assistant", "content": " \t\n "}]})),
        ("chat_empty_user", "/v1/chat/completions", json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": ""}]})),
        ("chat_whitespace_user", "/v1/chat/completions", json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": " \t\n "}]})),
        ("chat_blank_only_user_array", "/v1/chat/completions", json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": [{"type": "text", "text": ""}, {"type": "text", "text": "  "}]}]})),
        ("chat_blank_system", "/v1/chat/completions", json!({"model": LOCAL_MODEL, "messages": [{"role": "system", "content": ""}, {"role": "user", "content": "hi"}]})),
        ("chat_blank_developer", "/v1/chat/completions", json!({"model": LOCAL_MODEL, "messages": [{"role": "developer", "content": " \t\n "}, {"role": "user", "content": "hi"}]})),
        ("responses_empty_top_level_input", "/v1/responses", json!({"model": LOCAL_MODEL, "input": ""})),
        ("responses_whitespace_top_level_input", "/v1/responses", json!({"model": LOCAL_MODEL, "input": " \t\n "})),
        ("responses_empty_message", "/v1/responses", json!({"model": LOCAL_MODEL, "input": [{"role": "user", "content": ""}]})),
        ("responses_whitespace_message", "/v1/responses", json!({"model": LOCAL_MODEL, "input": [{"role": "user", "content": " \t\n "}]})),
        ("responses_blank_only_message_array", "/v1/responses", json!({"model": LOCAL_MODEL, "input": [{"role": "user", "content": [{"type": "input_text", "text": ""}, {"type": "input_text", "text": "\t\n"}]}]})),
        ("responses_blank_any_role_message", "/v1/responses", json!({"model": LOCAL_MODEL, "input": [{"role": "assistant", "content": ""}]})),
    ];
    for (label, path, body) in rejects {
        let before = captured_len(&log);
        let (status, _content_type, text) = call_gateway(
            port,
            "POST",
            path,
            &[("authorization", "Bearer local-key")],
            Some(body),
        )
        .await;
        assert_eq!(status, 400, "{label}: a block-less message must be a local 400: {text}");
        let error: Value = serde_json::from_str(&text).expect("the 400 body must be JSON");
        let field = if path == "/v1/responses" { "input.content" } else { "messages.content" };
        assert!(
            error["error"]["message"].as_str().unwrap_or_default().contains(field),
            "{label}: the 400 must name {field}: {text}"
        );
        assert_eq!(captured_len(&log), before, "{label}: no rejected request may reach an upstream");
    }

    stop_server().await.expect("stop the listener");
    drop(home);
}

// ---------------------------------------------------------------------------
// Step 2 (20261013-free-model-pool): a selected Messages candidate reached
// through the pool boundary reuses this adapter and the portable field matrix
// ---------------------------------------------------------------------------

/// REQ-005 / REQ-007: a pool whose selected candidate is an `anthropic_messages`
/// mapping must reject a nonportable Chat field (here `stop`) before any
/// upstream attempt, using the pool's portable contract rather than silently
/// forwarding it through the Messages adapter.
#[tokio::test]
async fn pool_messages_candidate_rejects_nonportable_chat_field_before_attempt() {
    let _home = temp_home("pool-messages-nonportable-field");
    let port = free_port().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let provider_id = format!("p-{tag}");
    let local = format!("local-{tag}");
    let upstream = format!("remote-{tag}");
    let alias = format!("free-model-{tag}");
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "ok"}]),
                "end_turn",
                json!({"input_tokens": 1, "output_tokens": 1}),
            ),
        )
    })
    .await;

    let provider = anthropic_provider_value(&provider_id, &upstream_url, &local, &upstream, UPSTREAM_SECRET);
    let mut config = raw_config(port, vec![provider]);
    config["model_pools"] = json!([{
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [{
            "provider_id": provider_id,
            "local_model": local,
            "upstream_model": upstream,
            "protocol": "anthropic_messages",
            "weight": 1,
        }],
    }]);
    config["model_pool_reserved_models"] = json!([]);
    write_raw_gateway_config(&config);
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "stop": "STOP",
        })),
    )
    .await;
    assert_eq!(
        status, 400,
        "a nonportable pool field must be rejected before the Messages conversion: {text}"
    );
    assert!(
        text.to_ascii_lowercase().contains("stop"),
        "the 400 must name the offending field: {text}"
    );
    assert_eq!(
        captured_len(&log),
        0,
        "no upstream attempt may run before the pool field validation"
    );

    stop_server().await.expect("stop the listener");
}

/// REQ-004: a selected `anthropic_messages` candidate reached through the pool's
/// Responses ingress still uses the existing adapter and returns the public
/// alias with the real source in accounting.
#[tokio::test]
async fn pool_messages_candidate_serves_responses_ingress_with_public_alias() {
    let _home = temp_home("pool-messages-responses-ingress");
    let port = free_port().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let provider_id = format!("p-{tag}");
    let local = format!("local-{tag}");
    let upstream = format!("remote-{tag}");
    let alias = format!("free-model-{tag}");
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "ok"}]),
                "end_turn",
                json!({"input_tokens": 2, "output_tokens": 3}),
            ),
        )
    })
    .await;

    let provider = anthropic_provider_value(&provider_id, &upstream_url, &local, &upstream, UPSTREAM_SECRET);
    let mut config = raw_config(port, vec![provider]);
    config["model_pools"] = json!([{
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [{
            "provider_id": provider_id,
            "local_model": local,
            "upstream_model": upstream,
            "protocol": "anthropic_messages",
            "weight": 1,
        }],
    }]);
    config["model_pool_reserved_models"] = json!([]);
    write_raw_gateway_config(&config);
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({
            "model": alias,
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}],
        })),
    )
    .await;
    assert_eq!(status, 200, "the pool Messages candidate must serve the Responses ingress: {text}");
    let body: Value = serde_json::from_str(&text).expect("a Responses JSON body");
    assert_eq!(body["object"], "response", "the caller keeps its Responses shape: {text}");
    assert_eq!(body["model"], alias, "the public pool alias must be returned: {text}");

    stop_server().await.expect("stop the listener");
}

// ===========================================================================
// Review-repair (2026-10-13): a selected Messages candidate cannot represent
// the OpenAI `strict` tool control, so a pool request that supplies it must be
// rejected by name before any attempt (REQ-005/REQ-007).
// ===========================================================================

/// `strict: true` on a function tool is not representable by the Messages
/// adapter; a pool that may select an `anthropic_messages` candidate must reject
/// it with a field-named 400 instead of silently dropping it.
#[tokio::test]
async fn pool_messages_candidate_rejects_strict_tool_control_before_attempt() {
    let _home = temp_home("pool-messages-strict-tool");
    let port = free_port().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let provider_id = format!("p-{tag}");
    let local = format!("local-{tag}");
    let upstream = format!("remote-{tag}");
    let alias = format!("free-model-{tag}");
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "ok"}]),
                "end_turn",
                json!({"input_tokens": 1, "output_tokens": 1}),
            ),
        )
    })
    .await;

    let provider =
        anthropic_provider_value(&provider_id, &upstream_url, &local, &upstream, UPSTREAM_SECRET);
    let mut config = raw_config(port, vec![provider]);
    config["model_pools"] = json!([{
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [{
            "provider_id": provider_id,
            "local_model": local,
            "upstream_model": upstream,
            "protocol": "anthropic_messages",
            "weight": 1,
        }],
    }]);
    config["model_pool_reserved_models"] = json!([]);
    write_raw_gateway_config(&config);
    start_server(None).await.expect("start the listener");

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "weather",
                    "description": "weather",
                    "parameters": {"type": "object", "properties": {"city": {"type": "string"}}},
                    "strict": true,
                }
            }],
            "tool_choice": "auto",
        })),
    )
    .await;
    assert_eq!(
        status, 400,
        "a selected Messages candidate cannot represent `strict`; reject it before selection: {text}"
    );
    assert!(
        text.to_ascii_lowercase().contains("strict"),
        "the 400 must name strict: {text}"
    );
    assert_eq!(
        captured_len(&log),
        0,
        "no upstream attempt may run before the strict rejection"
    );

    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// Step 2 (20261015-pool-only-reasoning): a pool Anthropic candidate receives its
// declared native controls after conversion. The ordinary adapter now translates
// recognized Chat/Responses reasoning controls into native `output_config`/
// `thinking` (`chat_conversion_translates_reasoning_effort_to_native_controls`
// and `responses_conversion_translates_reasoning_effort_and_ignores_summary`),
// while still rejecting unsupported values and members
// (`unsupported_reasoning_controls_fail_field_specifically`).
// ---------------------------------------------------------------------------

/// AC-006: a Chat request routed to a pool Anthropic candidate with an adaptive
/// effort profile is converted, then injected with `output_config.effort` and
/// `thinking.type = "adaptive"` while the recognized entry control is removed.
#[tokio::test]
async fn pool_anthropic_candidate_receives_native_controls_after_conversion() {
    let home = temp_home("pool-anth-native-controls");
    let port = free_port().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let alias = format!("free-anth-{tag}");
    let provider_id = format!("p-anth-{tag}");
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "ok"}]),
                "end_turn",
                json!({"input_tokens": 1, "output_tokens": 1}),
            ),
        )
    })
    .await;

    let provider = anthropic_provider_value(
        &provider_id,
        &upstream_url,
        LOCAL_MODEL,
        UPSTREAM_MODEL,
        UPSTREAM_SECRET,
    );
    let pool = json!({
        "id": format!("pool-anth-{tag}"),
        "name": "Anthropic",
        "model": alias,
        "enabled": true,
        "candidates": [{
            "provider_id": provider_id,
            "local_model": LOCAL_MODEL,
            "upstream_model": UPSTREAM_MODEL,
            "protocol": "anthropic_messages",
            "weight": 1,
            "reasoning": {
                "profile": "anthropic_effort_adaptive",
                "values": ["high"],
                "mappings": [{"effort": "high", "action": {"kind": "enum", "value": "high"}}],
            },
        }],
    });
    let mut config = raw_config(port, vec![provider]);
    config["model_pools"] = json!([pool]);
    config["model_pool_reserved_models"] = json!([]);
    write_raw_gateway_config(&config);
    start_server(None).await.expect("start the listener");

    let sent = converted_body(
        port,
        &log,
        "/v1/chat/completions",
        json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "reasoning_effort": "high",
        }),
    )
    .await;

    assert_eq!(
        sent.pointer("/output_config/effort").and_then(Value::as_str),
        Some("high"),
        "the native effort control must be injected: {sent}"
    );
    assert_eq!(
        sent.pointer("/thinking/type").and_then(Value::as_str),
        Some("adaptive"),
        "the adaptive thinking control must be injected: {sent}"
    );
    assert!(
        sent.get("reasoning_effort").is_none(),
        "the recognized entry control must be stripped before conversion: {sent}"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

// ---------------------------------------------------------------------------
// Direct fix: an ordinary (non-pool) OpenAI request served by an Anthropic
// Messages upstream translates the client reasoning control instead of
// rejecting it with a field-specific 400. Only the loopback HTTP boundary and
// the captured native upstream body are asserted; no private converter helper
// is called.
// ---------------------------------------------------------------------------

/// Chat `/v1/chat/completions`: a top-level `reasoning_effort` (or its
/// `reasoningEffort` alias) is trimmed and translated to the native
/// `output_config`/`thinking` controls, and the entry control never leaks
/// upstream.
#[tokio::test]
async fn chat_conversion_translates_reasoning_effort_to_native_controls() {
    let home = temp_home("conv-reasoning-chat");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "ok"}]),
                "end_turn",
                json!({"input_tokens": 1, "output_tokens": 1}),
            ),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-reasoning-chat",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    // `high` must become an exact `output_config` object and must not leak the
    // client-facing control.
    let high = converted_body(
        port,
        &log,
        "/v1/chat/completions",
        json!({
            "model": LOCAL_MODEL,
            "messages": [{"role": "user", "content": "hi"}],
            "reasoning_effort": "high",
        }),
    )
    .await;
    assert_eq!(
        high["output_config"],
        json!({"effort": "high"}),
        "reasoning_effort high must become output_config.effort high: {high}"
    );
    assert!(
        high.get("reasoning_effort").is_none(),
        "the entry reasoning control must not leak upstream: {high}"
    );

    // The camelCase alias is honored, and `minimal` maps to `low`.
    let minimal = converted_body(
        port,
        &log,
        "/v1/chat/completions",
        json!({
            "model": LOCAL_MODEL,
            "messages": [{"role": "user", "content": "hi"}],
            "reasoningEffort": "minimal",
        }),
    )
    .await;
    assert_eq!(
        minimal.pointer("/output_config/effort").and_then(Value::as_str),
        Some("low"),
        "the reasoningEffort alias must be honored and minimal must map to low: {minimal}"
    );

    // Surrounding whitespace is trimmed before matching a value.
    let trimmed = converted_body(
        port,
        &log,
        "/v1/chat/completions",
        json!({
            "model": LOCAL_MODEL,
            "messages": [{"role": "user", "content": "hi"}],
            "reasoning_effort": "  xhigh  ",
        }),
    )
    .await;
    assert_eq!(
        trimmed.pointer("/output_config/effort").and_then(Value::as_str),
        Some("xhigh"),
        "the reasoning effort value must be trimmed: {trimmed}"
    );

    // `none` disables thinking and must not add an `output_config`.
    let none = converted_body(
        port,
        &log,
        "/v1/chat/completions",
        json!({
            "model": LOCAL_MODEL,
            "messages": [{"role": "user", "content": "hi"}],
            "reasoning_effort": "none",
        }),
    )
    .await;
    assert_eq!(
        none["thinking"],
        json!({"type": "disabled"}),
        "reasoning_effort none must disable thinking: {none}"
    );
    assert!(
        none.get("output_config").is_none(),
        "a disabled-thinking request must not carry output_config: {none}"
    );

    // `max` is a recognized value forwarded verbatim.
    let max = converted_body(
        port,
        &log,
        "/v1/chat/completions",
        json!({
            "model": LOCAL_MODEL,
            "messages": [{"role": "user", "content": "hi"}],
            "reasoning_effort": "max",
        }),
    )
    .await;
    assert_eq!(
        max.pointer("/output_config/effort").and_then(Value::as_str),
        Some("max"),
        "reasoning_effort max must become output_config.effort max: {max}"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// Responses `/v1/responses`: a `reasoning` object with `effort` is translated
/// to the native `output_config`, an accepted `summary` member is ignored, and
/// a summary-only object adds neither `output_config` nor `thinking`.
#[tokio::test]
async fn responses_conversion_translates_reasoning_effort_and_ignores_summary() {
    let home = temp_home("conv-reasoning-responses");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            anthropic_message(
                json!([{"type": "text", "text": "ok"}]),
                "end_turn",
                json!({"input_tokens": 1, "output_tokens": 1}),
            ),
        )
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-reasoning-responses",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let with_summary = converted_body(
        port,
        &log,
        "/v1/responses",
        json!({
            "model": LOCAL_MODEL,
            "input": "hi",
            "reasoning": {"effort": "xhigh", "summary": "auto"},
        }),
    )
    .await;
    assert_eq!(
        with_summary.pointer("/output_config/effort").and_then(Value::as_str),
        Some("xhigh"),
        "the Responses reasoning effort must translate to output_config: {with_summary}"
    );
    assert!(
        with_summary.get("reasoning").is_none(),
        "the client reasoning object must not leak upstream: {with_summary}"
    );
    assert!(
        !serde_json::to_string(&with_summary)
            .expect("serialize the converted body")
            .contains("summary"),
        "the accepted-and-ignored summary member must not survive conversion: {with_summary}"
    );

    let summary_only = converted_body(
        port,
        &log,
        "/v1/responses",
        json!({
            "model": LOCAL_MODEL,
            "input": "hi",
            "reasoning": {"summary": "concise"},
        }),
    )
    .await;
    assert!(
        summary_only.get("output_config").is_none(),
        "a summary-only reasoning object must not add output_config: {summary_only}"
    );
    assert!(
        summary_only.get("thinking").is_none(),
        "a summary-only reasoning object must not add thinking: {summary_only}"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}

/// Unsupported client reasoning controls fail field-specifically with a 400
/// before any upstream attempt: an unknown effort value, a non-string effort
/// value, a non-object `reasoning`, a summary-only object accepted above, and
/// an unknown member.
#[tokio::test]
async fn unsupported_reasoning_controls_fail_field_specifically() {
    let home = temp_home("conv-reasoning-invalid");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(200, anthropic_message(json!([]), "end_turn", json!({})))
    })
    .await;
    write_raw_gateway_config(&raw_config(
        port,
        vec![anthropic_provider_value(
            "p-reasoning-invalid",
            &upstream_url,
            LOCAL_MODEL,
            UPSTREAM_MODEL,
            UPSTREAM_SECRET,
        )],
    ));
    start_server(None).await.expect("start the listener");

    let cases: Vec<(&str, &str, Value)> = vec![
        (
            "reasoning_effort",
            "/v1/chat/completions",
            json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "reasoning_effort": "bogus"}),
        ),
        (
            "reasoning_effort",
            "/v1/chat/completions",
            json!({"model": LOCAL_MODEL, "messages": [{"role": "user", "content": "hi"}], "reasoning_effort": 5}),
        ),
        (
            "reasoning",
            "/v1/responses",
            json!({"model": LOCAL_MODEL, "input": "hi", "reasoning": {"effort": "auto"}}),
        ),
        (
            "reasoning.mode",
            "/v1/responses",
            json!({"model": LOCAL_MODEL, "input": "hi", "reasoning": {"effort": "high", "mode": "pro"}}),
        ),
    ];
    for (field, path, body) in cases {
        let (status, _ct, text) = call_gateway(
            port,
            "POST",
            path,
            &[("authorization", "Bearer local-key")],
            Some(body),
        )
        .await;
        assert_eq!(
            status, 400,
            "an unsupported reasoning control `{field}` must be a field-specific 400: {status} {text}"
        );
        assert!(
            text.contains(field),
            "the 400 must identify `{field}`: {text}"
        );
    }

    assert_eq!(
        captured_len(&log),
        0,
        "no unsupported reasoning request may reach an upstream"
    );

    stop_server().await.expect("stop the listener");
    drop(home);
}
