//! Behavior tests for same-turn model-pool continuation preference.
//!
//! Every test drives only already-compiled entry points: the encrypted
//! configuration boundary (`write_raw_gateway_config`), the real loopback
//! listener (`start_server` plus `call_gateway`) and in-process mock upstreams
//! (`spawn_mock_upstream`). Pool records are supplied as raw JSON fixtures so a
//! RED is a compiled, observable routing outcome — which candidate actually
//! served the request — rather than a missing Rust symbol. No internal
//! collaborator is mocked and no not-yet-existing product symbol is referenced.
//!
//! The behavior under test: within one recognized user turn, an enabled pool
//! candidate that successfully delivered a tool call is preferred for the
//! matching tool continuations, instead of advancing the ordinary weighted
//! rotation. A new user turn, a request without a recognized session, an
//! unrelated scope or a no-longer-eligible candidate returns to normal routing.

use super::{
    call_gateway, free_port, pool_key, pool_provider, spawn_mock_upstream, temp_home,
    write_raw_gateway_config, Captured, CapturedLog, MockReply,
};
use crate::ai_gateway::runtime_http::{start_server, stop_server};
use crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Unique suffix per test so the process-global pool scheduler and any hold
/// state never carry another test's pool/provider/alias identity.
fn tag() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

// ---------------------------------------------------------------------------
// Raw configuration and pool fixtures
// ---------------------------------------------------------------------------

/// One local relay key entry for the raw schema-3 configuration.
fn local_key(id: &str, value: &str) -> Value {
    json!({
        "id": id,
        "label": id,
        "value": value,
        "enabled": true,
        "created_at": 1,
    })
}

/// A schema-3 raw configuration carrying the pool fields plus explicit local
/// relay keys, bypassing the typed `GatewayConfig` so unknown pool fields
/// survive the raw fixture round trip.
fn pool_config(port: u16, keys: Vec<Value>, providers: Vec<Value>, pools: Vec<Value>) -> Value {
    json!({
        "schema_version": GATEWAY_CONFIG_SCHEMA_VERSION,
        "port": port,
        "keys": keys,
        "providers": providers,
        "model_pools": pools,
        "model_pool_reserved_models": [],
    })
}

/// The single local relay key most tests use.
fn single_key() -> Vec<Value> {
    vec![local_key("k-local", "local-key")]
}

/// One raw mapping row carrying the local/upstream pair and effective protocol
/// a pool candidate references.
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

// ---------------------------------------------------------------------------
// Mock upstreams
// ---------------------------------------------------------------------------

/// A Chat Completions success body carrying one assistant function call.
fn chat_tool_call_body(call_id: &str) -> Value {
    json!({
        "id": format!("chatcmpl-{call_id}"),
        "object": "chat.completion",
        "model": "upstream",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": Value::Null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    })
}

/// A Responses success body carrying one `function_call` output item.
fn responses_tool_call_body(call_id: &str) -> Value {
    json!({
        "id": format!("resp-{call_id}"),
        "object": "response",
        "model": "upstream",
        "status": "completed",
        "output": [{
            "id": format!("fc-{call_id}"),
            "type": "function_call",
            "call_id": call_id,
            "name": "get_weather",
            "arguments": "{\"city\":\"SF\"}",
            "status": "completed"
        }],
        "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
    })
}

/// A Chat upstream that answers every request with a tool call whose identity is
/// drawn from the shared counter, so every response in a chain is unique.
async fn spawn_chat_tool_upstream(next: Arc<AtomicUsize>) -> (String, CapturedLog) {
    spawn_mock_upstream(move |_captured| {
        let n = next.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Json(200, chat_tool_call_body(&format!("call_{n}")))
    })
    .await
}

/// A Responses upstream that answers every request with a `function_call` whose
/// identity is drawn from the shared counter.
async fn spawn_responses_tool_upstream(next: Arc<AtomicUsize>) -> (String, CapturedLog) {
    spawn_mock_upstream(move |_captured| {
        let n = next.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Json(200, responses_tool_call_body(&format!("call_{n}")))
    })
    .await
}

/// An upstream that succeeds with a plain text Chat answer and never emits a
/// tool call, for the standalone-user guard.
async fn spawn_plain_chat_upstream() -> (String, CapturedLog) {
    spawn_mock_upstream(|_captured| {
        MockReply::Json(
            200,
            json!({
                "id": "chatcmpl-plain",
                "object": "chat.completion",
                "model": "upstream",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "ok"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
            }),
        )
    })
    .await
}

/// A Chat upstream that succeeds on its first request and answers every later
/// request with a 500 before any output byte, for the fallback-preference case.
async fn spawn_flaky_chat_tool_upstream(next: Arc<AtomicUsize>) -> (String, CapturedLog) {
    let calls = Arc::new(AtomicUsize::new(0));
    spawn_mock_upstream(move |_captured| {
        let call = calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            let n = next.fetch_add(1, Ordering::SeqCst) + 1;
            MockReply::Json(200, chat_tool_call_body(&format!("call_{n}")))
        } else {
            MockReply::Json(500, json!({"error": {"message": "upstream unavailable"}}))
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// Request helpers and capture inspection
// ---------------------------------------------------------------------------

/// The portable tool surface a weather function call needs.
fn weather_tools() -> Value {
    json!([{
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
    }])
}

/// POST one JSON body to the listener with an explicit local credential and the
/// optional recognized session/thread headers.
async fn post_json(
    port: u16,
    path: &str,
    auth: &str,
    session: Option<&str>,
    thread: Option<&str>,
    body: Value,
) -> (u16, String) {
    let mut headers: Vec<(&str, &str)> = vec![("authorization", auth)];
    if let Some(session) = session {
        headers.push(("x-opencode-session", session));
    }
    if let Some(thread) = thread {
        headers.push(("thread-id", thread));
    }
    let (status, _content_type, text) =
        call_gateway(port, "POST", path, &headers, Some(body)).await;
    (status, text)
}

/// The first assistant function call of a Chat completion response.
fn chat_call(value: &Value) -> Value {
    value["choices"][0]["message"]["tool_calls"][0].clone()
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("response must be JSON: {error}: {text}"))
}

fn parsed_body(captured: &Captured) -> Option<Value> {
    serde_json::from_slice::<Value>(&captured.body).ok()
}

/// The upstream model the relay forwarded for one captured request.
fn forwarded_model(captured: &Captured) -> String {
    parsed_body(captured)
        .and_then(|value| {
            value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// How many tool-result facts one forwarded request already carries, across both
/// the Chat (`role: tool`) and Responses (`function_call_output`) shapes.
fn tool_result_count(captured: &Captured) -> usize {
    let Some(value) = parsed_body(captured) else {
        return 0;
    };
    let mut count = 0usize;
    if let Some(messages) = value.get("messages").and_then(Value::as_array) {
        count += messages
            .iter()
            .filter(|message| message.get("role").and_then(Value::as_str) == Some("tool"))
            .count();
    }
    if let Some(input) = value.get("input").and_then(Value::as_array) {
        count += input
            .iter()
            .filter(|item| {
                item.get("type").and_then(Value::as_str) == Some("function_call_output")
            })
            .count();
    }
    count
}

fn with_results(log: &CapturedLog, results: usize) -> Option<Captured> {
    log.lock()
        .expect("mock log")
        .iter()
        .find(|captured| tool_result_count(captured) == results)
        .cloned()
}

fn for_marker(log: &CapturedLog, marker: &str, results: usize) -> Option<Captured> {
    log.lock()
        .expect("mock log")
        .iter()
        .find(|captured| {
            String::from_utf8_lossy(&captured.body).contains(marker)
                && tool_result_count(captured) == results
        })
        .cloned()
}

/// Candidate identity names used in assertion messages.
const A: &str = "candidate-a";
const B: &str = "candidate-b";

// ---------------------------------------------------------------------------
// RED: non-streaming continuation preference
// ---------------------------------------------------------------------------

/// A Chat tool chain served by the first candidate must keep attempting that
/// candidate for both appended tool continuations, instead of advancing the
/// ordinary weighted rotation to the sibling.
#[tokio::test]
async fn chat_non_streaming_tool_chain_prefers_the_preceding_candidate() {
    let _home = temp_home("pool-context-hold-chat-chain");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-chat-{tag}");
    let session = format!("sess-chat-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_b, log_b) = spawn_chat_tool_upstream(next.clone()).await;

    let provider_a_value = pool_provider(
        &provider_a,
        "Provider A",
        &url_a,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let provider_b_value = pool_provider(
        &provider_b,
        "Provider B",
        &url_b,
        vec![pool_key("key-b", "Key B", "sk-b", true)],
        vec![provider_mapping("local-b", "remote-b", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-chat-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    // Build the chain from the real responses: the seed request, then two
    // continuations that append the assistant tool call and its tool result
    // without a new user message.
    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    let mut call_ids: Vec<String> = Vec::new();
    for round in 0..3 {
        let body = json!({
            "model": alias,
            "messages": messages.clone(),
            "tools": weather_tools(),
        });
        let (status, text) = post_json(
            port,
            "/v1/chat/completions",
            "Bearer local-key",
            Some(&session),
            None,
            body,
        )
        .await;
        assert_eq!(
            status, 200,
            "round {round} must be served by a pool candidate: {text}"
        );
        let value = parse(&text);
        assert_eq!(value["model"], alias, "the client must see the public alias");
        let call = chat_call(&value);
        let call_id = call["id"]
            .as_str()
            .expect("the response must carry a call id")
            .to_string();
        call_ids.push(call_id.clone());
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [call.clone()]}));
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));
    }

    for results in 1..=2usize {
        let served_a = with_results(&log_a, results);
        let served_b = with_results(&log_b, results);
        let summary = format!(
            "results={results} a={:?} b={:?}",
            served_a.as_ref().map(forwarded_model),
            served_b.as_ref().map(forwarded_model)
        );
        assert!(
            served_a.is_some(),
            "continuation {results} must attempt the preceding successful candidate {A} first: {summary}"
        );
        assert!(
            served_b.is_none(),
            "continuation {results} must not rotate to {B}: {summary}"
        );
        assert_eq!(
            forwarded_model(&served_a.expect("checked above")),
            "remote-a",
            "the continuation must keep the real upstream model of the held candidate"
        );
    }

    let continuation =
        with_results(&log_a, 1).expect("the first continuation must reach candidate A");
    let body = parsed_body(&continuation).expect("the forwarded continuation must be JSON");
    assert_eq!(body["model"], "remote-a");
    let messages = body["messages"].as_array().expect("messages must be forwarded");
    let assistant = messages
        .iter()
        .find(|message| message.get("tool_calls").is_some())
        .expect("the assistant tool call must be forwarded");
    assert_eq!(assistant["tool_calls"][0]["id"], json!(call_ids[0]));
    assert_eq!(assistant["tool_calls"][0]["function"]["name"], "get_weather");
    assert_eq!(
        assistant["tool_calls"][0]["function"]["arguments"],
        "{\"city\":\"SF\"}"
    );
    let tool = messages
        .iter()
        .find(|message| message["role"] == "tool")
        .expect("the matching tool result must be forwarded");
    assert_eq!(tool["tool_call_id"], json!(call_ids[0]));
    assert_eq!(tool["content"], "sunny");

    stop_server().await.expect("stop the listener");
}

/// A Responses tool chain (function_call / function_call_output history) served
/// by the first candidate must keep attempting that candidate for both appends,
/// instead of advancing the ordinary weighted rotation.
#[tokio::test]
async fn responses_non_streaming_tool_chain_prefers_the_preceding_candidate() {
    let _home = temp_home("pool-context-hold-responses-chain");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-resp-{tag}");
    let session = format!("sess-resp-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_responses_tool_upstream(next.clone()).await;
    let (url_b, log_b) = spawn_responses_tool_upstream(next.clone()).await;

    let provider_a_value = pool_provider(
        &provider_a,
        "Provider A",
        &url_a,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![provider_mapping("local-a", "remote-a", "responses")],
    );
    let provider_b_value = pool_provider(
        &provider_b,
        "Provider B",
        &url_b,
        vec![pool_key("key-b", "Key B", "sk-b", true)],
        vec![provider_mapping("local-b", "remote-b", "responses")],
    );
    let pool = json!({
        "id": format!("pool-resp-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "responses", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "responses", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let mut input = vec![json!({
        "role": "user",
        "content": [{"type": "input_text", "text": "weather?"}],
    })];
    let mut call_ids: Vec<String> = Vec::new();
    for round in 0..3 {
        let body = json!({"model": alias, "input": input.clone()});
        let (status, text) = post_json(
            port,
            "/v1/responses",
            "Bearer local-key",
            Some(&session),
            None,
            body,
        )
        .await;
        assert_eq!(
            status, 200,
            "round {round} must be served by a pool candidate: {text}"
        );
        let value = parse(&text);
        assert_eq!(value["model"], alias, "the client must see the public alias");
        let item = value["output"]
            .as_array()
            .and_then(|output| {
                output
                    .iter()
                    .find(|item| item["type"] == "function_call")
                    .cloned()
            })
            .expect("the response must carry a function call");
        let call_id = item["call_id"].as_str().expect("call_id").to_string();
        call_ids.push(call_id.clone());
        input.push(json!({
            "type": "function_call",
            "call_id": call_id.clone(),
            "name": item["name"].clone(),
            "arguments": item["arguments"].clone(),
        }));
        input.push(json!({
            "type": "function_call_output",
            "call_id": call_id,
            "output": "sunny",
        }));
    }

    for results in 1..=2usize {
        let served_a = with_results(&log_a, results);
        let served_b = with_results(&log_b, results);
        let summary = format!(
            "results={results} a={:?} b={:?}",
            served_a.as_ref().map(forwarded_model),
            served_b.as_ref().map(forwarded_model)
        );
        assert!(
            served_a.is_some(),
            "continuation {results} must attempt the preceding successful candidate {A} first: {summary}"
        );
        assert!(
            served_b.is_none(),
            "continuation {results} must not rotate to {B}: {summary}"
        );
        assert_eq!(forwarded_model(&served_a.expect("checked above")), "remote-a");
    }

    let continuation =
        with_results(&log_a, 1).expect("the first continuation must reach candidate A");
    let body = parsed_body(&continuation).expect("the forwarded continuation must be JSON");
    assert_eq!(body["model"], "remote-a");
    let input = body["input"].as_array().expect("input must be forwarded");
    let call = input
        .iter()
        .find(|item| item["type"] == "function_call")
        .expect("the function call must be forwarded");
    assert_eq!(call["call_id"], json!(call_ids[0]));
    assert_eq!(call["name"], "get_weather");
    assert_eq!(call["arguments"], "{\"city\":\"SF\"}");
    let output = input
        .iter()
        .find(|item| item["type"] == "function_call_output")
        .expect("the function call output must be forwarded");
    assert_eq!(output["call_id"], json!(call_ids[0]));
    assert_eq!(output["output"], "sunny");

    stop_server().await.expect("stop the listener");
}

/// When the held candidate fails before any output byte, the fallback candidate
/// that actually delivered the tool call becomes the immediate next preference.
#[tokio::test]
async fn successful_fallback_becomes_the_next_continuation_preference() {
    let _home = temp_home("pool-context-hold-fallback-preference");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-fallback-{tag}");
    let session = format!("sess-fallback-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_flaky_chat_tool_upstream(next.clone()).await;
    let (url_b, log_b) = spawn_chat_tool_upstream(next.clone()).await;

    let provider_a_value = pool_provider(
        &provider_a,
        "Provider A",
        &url_a,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let provider_b_value = pool_provider(
        &provider_b,
        "Provider B",
        &url_b,
        vec![pool_key("key-b", "Key B", "sk-b", true)],
        vec![provider_mapping("local-b", "remote-b", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-fallback-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    for round in 0..3 {
        let body = json!({
            "model": alias,
            "messages": messages.clone(),
            "tools": weather_tools(),
        });
        let (status, text) = post_json(
            port,
            "/v1/chat/completions",
            "Bearer local-key",
            Some(&session),
            None,
            body,
        )
        .await;
        assert_eq!(
            status, 200,
            "round {round} must be served before/after failover: {text}"
        );
        let call = chat_call(&parse(&text));
        let call_id = call["id"].as_str().expect("call id").to_string();
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [call.clone()]}));
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));
    }

    let a_first_continuation = with_results(&log_a, 1);
    assert!(
        a_first_continuation.is_some(),
        "the first continuation must attempt the held candidate {A} first, letting its 500 select the fallback"
    );
    assert_eq!(
        forwarded_model(&a_first_continuation.expect("checked above")),
        "remote-a"
    );
    assert!(
        with_results(&log_a, 2).is_none(),
        "the second continuation must prefer the successful fallback {B}, not re-attempt {A}"
    );
    let b_first = with_results(&log_b, 1).expect("the fallback must serve the first continuation");
    assert_eq!(forwarded_model(&b_first), "remote-b");
    let b_second = with_results(&log_b, 2).expect("the fallback must serve the second continuation");
    assert_eq!(forwarded_model(&b_second), "remote-b");

    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// Guards: cases that already route normally and must keep doing so
// ---------------------------------------------------------------------------

/// Appending a new user message — even with identical text — starts a new turn
/// and reselects by the ordinary weighted rotation.
#[tokio::test]
async fn a_new_user_message_reselects_the_ordinary_weighted_rotation() {
    let _home = temp_home("pool-context-hold-new-user");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-new-user-{tag}");
    let session = format!("sess-new-user-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_b, log_b) = spawn_chat_tool_upstream(next.clone()).await;

    let provider_a_value = pool_provider(
        &provider_a,
        "Provider A",
        &url_a,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let provider_b_value = pool_provider(
        &provider_b,
        "Provider B",
        &url_b,
        vec![pool_key("key-b", "Key B", "sk-b", true)],
        vec![provider_mapping("local-b", "remote-b", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-new-user-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let (status, text) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hello"}],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the seed request must be served: {text}");
    let call = chat_call(&parse(&text));
    let call_id = call["id"].as_str().expect("call id").to_string();
    assert_eq!(
        forwarded_model(&with_results(&log_a, 0).expect("the seed must reach candidate A")),
        "remote-a"
    );

    let (status, text2) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": null, "tool_calls": [call.clone()]},
                {"role": "tool", "tool_call_id": call_id, "content": "sunny"},
                {"role": "user", "content": "hello"},
            ],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the new-user request must be served: {text2}");
    assert!(
        with_results(&log_a, 1).is_none(),
        "identical user text after a new user message must not continue the previous candidate"
    );
    assert_eq!(
        forwarded_model(
            &with_results(&log_b, 1).expect("the new turn must rotate to candidate B")
        ),
        "remote-b"
    );

    stop_server().await.expect("stop the listener");
}

/// A repeated standalone user request with a stable session header carries no
/// tool facts and keeps the ordinary weighted rotation.
#[tokio::test]
async fn repeated_standalone_user_requests_keep_ordinary_weighted_rotation() {
    let _home = temp_home("pool-context-hold-standalone");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-standalone-{tag}");
    let session = format!("sess-standalone-{tag}");
    let (url_a, log_a) = spawn_plain_chat_upstream().await;
    let (url_b, log_b) = spawn_plain_chat_upstream().await;

    let provider_a_value = pool_provider(
        &provider_a,
        "Provider A",
        &url_a,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let provider_b_value = pool_provider(
        &provider_b,
        "Provider B",
        &url_b,
        vec![pool_key("key-b", "Key B", "sk-b", true)],
        vec![provider_mapping("local-b", "remote-b", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-standalone-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    for _ in 0..2 {
        let (status, text) = post_json(
            port,
            "/v1/chat/completions",
            "Bearer local-key",
            Some(&session),
            None,
            json!({
                "model": alias,
                "messages": [{"role": "user", "content": "hi"}],
            }),
        )
        .await;
        assert_eq!(status, 200, "the standalone request must be served: {text}");
    }

    let a = log_a.lock().expect("mock log").clone();
    let b = log_b.lock().expect("mock log").clone();
    assert_eq!(a.len(), 1, "the first standalone request must reach candidate A");
    assert_eq!(b.len(), 1, "the second standalone request must rotate to candidate B");
    assert_eq!(forwarded_model(&a[0]), "remote-a");
    assert_eq!(forwarded_model(&b[0]), "remote-b");

    stop_server().await.expect("stop the listener");
}

/// A tool chain without any recognized session header uses the ordinary pool
/// rotation for every request.
#[tokio::test]
async fn a_tool_chain_without_a_session_header_uses_ordinary_rotation() {
    let _home = temp_home("pool-context-hold-headerless");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-headerless-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_b, log_b) = spawn_chat_tool_upstream(next.clone()).await;

    let provider_a_value = pool_provider(
        &provider_a,
        "Provider A",
        &url_a,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let provider_b_value = pool_provider(
        &provider_b,
        "Provider B",
        &url_b,
        vec![pool_key("key-b", "Key B", "sk-b", true)],
        vec![provider_mapping("local-b", "remote-b", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-headerless-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    for round in 0..3 {
        let body = json!({
            "model": alias,
            "messages": messages.clone(),
            "tools": weather_tools(),
        });
        let (status, text) = post_json(
            port,
            "/v1/chat/completions",
            "Bearer local-key",
            None,
            None,
            body,
        )
        .await;
        assert_eq!(status, 200, "round {round} must be served: {text}");
        let call = chat_call(&parse(&text));
        let call_id = call["id"].as_str().expect("call id").to_string();
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [call.clone()]}));
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));
    }

    assert!(with_results(&log_a, 0).is_some(), "the first request must reach candidate A");
    assert!(with_results(&log_b, 1).is_some(), "the first continuation must rotate to candidate B");
    assert!(with_results(&log_a, 2).is_some(), "the second continuation must rotate back to candidate A");
    assert_eq!(
        forwarded_model(&with_results(&log_b, 1).expect("candidate B served")),
        "remote-b"
    );
    assert_eq!(
        forwarded_model(&with_results(&log_a, 2).expect("candidate A served")),
        "remote-a"
    );

    stop_server().await.expect("stop the listener");
}

/// A chain completed in one scope does not change selection in a different
/// scope. Each sub-case uses its own pool so the ordinary rotation is
/// predictable, and the second request is an otherwise colliding continuation.
#[tokio::test]
async fn hold_scopes_do_not_leak_across_session_thread_key_and_ingress() {
    let _home = temp_home("pool-context-hold-scope-isolation");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_b, log_b) = spawn_chat_tool_upstream(next.clone()).await;

    let provider_a_value = pool_provider(
        &provider_a,
        "Provider A",
        &url_a,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![
            provider_mapping("local-a-chat", "remote-a-chat", "chat_completions"),
            provider_mapping("local-a-resp", "remote-a-resp", "responses"),
        ],
    );
    let provider_b_value = pool_provider(
        &provider_b,
        "Provider B",
        &url_b,
        vec![pool_key("key-b", "Key B", "sk-b", true)],
        vec![
            provider_mapping("local-b-chat", "remote-b-chat", "chat_completions"),
            provider_mapping("local-b-resp", "remote-b-resp", "responses"),
        ],
    );

    let alias_sess = format!("free-scope-sess-{tag}");
    let alias_thread = format!("free-scope-thread-{tag}");
    let alias_key = format!("free-scope-key-{tag}");
    let alias_proto = format!("free-scope-proto-{tag}");
    let pool_sess = json!({
        "id": format!("pool-scope-sess-{tag}"),
        "name": "Scope Session",
        "model": alias_sess,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a-chat", "remote-a-chat", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b-chat", "remote-b-chat", "chat_completions", Some(1)),
        ],
    });
    let pool_thread = json!({
        "id": format!("pool-scope-thread-{tag}"),
        "name": "Scope Thread",
        "model": alias_thread,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a-chat", "remote-a-chat", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b-chat", "remote-b-chat", "chat_completions", Some(1)),
        ],
    });
    let pool_key = json!({
        "id": format!("pool-scope-key-{tag}"),
        "name": "Scope Key",
        "model": alias_key,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a-chat", "remote-a-chat", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b-chat", "remote-b-chat", "chat_completions", Some(1)),
        ],
    });
    let pool_proto = json!({
        "id": format!("pool-scope-proto-{tag}"),
        "name": "Scope Protocol",
        "model": alias_proto,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a-chat", "remote-a-chat", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b-chat", "remote-b-chat", "chat_completions", Some(1)),
        ],
    });

    write_raw_gateway_config(&pool_config(
        port,
        vec![
            local_key("k-local", "local-key"),
            local_key("k-local-2", "local-key-2"),
        ],
        vec![provider_a_value, provider_b_value],
        vec![pool_sess, pool_thread, pool_key, pool_proto],
    ));
    start_server(None).await.expect("start the listener");

    // (a) differing session values
    let marker = format!("marker-sess-{tag}");
    let session_a = format!("session-a-{tag}");
    let session_b = format!("session-b-{tag}");
    let (status, text) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session_a),
        None,
        json!({
            "model": alias_sess,
            "messages": [{"role": "user", "content": marker}],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the session-scope seed must be served: {text}");
    assert_eq!(
        forwarded_model(&for_marker(&log_a, &marker, 0).expect("the seed must reach candidate A")),
        "remote-a-chat"
    );
    let call = chat_call(&parse(&text));
    let call_id = call["id"].as_str().expect("call id").to_string();
    let (status, text2) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session_b),
        None,
        json!({
            "model": alias_sess,
            "messages": [
                {"role": "user", "content": marker},
                {"role": "assistant", "content": null, "tool_calls": [call.clone()]},
                {"role": "tool", "tool_call_id": call_id, "content": "sunny"},
            ],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the other-session request must be served: {text2}");
    assert!(
        for_marker(&log_a, &marker, 1).is_none(),
        "a different session value must not inherit the seed scope's preference"
    );
    assert_eq!(
        forwarded_model(
            &for_marker(&log_b, &marker, 1).expect("the other session must rotate to B")
        ),
        "remote-b-chat"
    );

    // (b) differing supplied thread identities within one session value
    let marker = format!("marker-thread-{tag}");
    let session = format!("session-thread-{tag}");
    let (status, text) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        Some("thread-1"),
        json!({
            "model": alias_thread,
            "messages": [{"role": "user", "content": marker}],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the thread-scope seed must be served: {text}");
    assert_eq!(
        forwarded_model(&for_marker(&log_a, &marker, 0).expect("the seed must reach candidate A")),
        "remote-a-chat"
    );
    let call = chat_call(&parse(&text));
    let call_id = call["id"].as_str().expect("call id").to_string();
    let (status, text2) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        Some("thread-2"),
        json!({
            "model": alias_thread,
            "messages": [
                {"role": "user", "content": marker},
                {"role": "assistant", "content": null, "tool_calls": [call.clone()]},
                {"role": "tool", "tool_call_id": call_id, "content": "sunny"},
            ],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the other-thread request must be served: {text2}");
    assert!(
        for_marker(&log_a, &marker, 1).is_none(),
        "a different thread identity must not inherit the seed scope's preference"
    );
    assert_eq!(
        forwarded_model(
            &for_marker(&log_b, &marker, 1).expect("the other thread must rotate to B")
        ),
        "remote-b-chat"
    );

    // (c) differing matched enabled local keys
    let marker = format!("marker-key-{tag}");
    let session = format!("session-key-{tag}");
    let (status, text) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias_key,
            "messages": [{"role": "user", "content": marker}],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the key-scope seed must be served: {text}");
    assert_eq!(
        forwarded_model(&for_marker(&log_a, &marker, 0).expect("the seed must reach candidate A")),
        "remote-a-chat"
    );
    let call = chat_call(&parse(&text));
    let call_id = call["id"].as_str().expect("call id").to_string();
    let (status, text2) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key-2",
        Some(&session),
        None,
        json!({
            "model": alias_key,
            "messages": [
                {"role": "user", "content": marker},
                {"role": "assistant", "content": null, "tool_calls": [call.clone()]},
                {"role": "tool", "tool_call_id": call_id, "content": "sunny"},
            ],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the other-key request must be served: {text2}");
    assert!(
        for_marker(&log_a, &marker, 1).is_none(),
        "a different matched local key must not inherit the seed scope's preference"
    );
    assert_eq!(
        forwarded_model(
            &for_marker(&log_b, &marker, 1).expect("the other key must rotate to B")
        ),
        "remote-b-chat"
    );

    // (d) differing ingress protocols with otherwise colliding history
    let marker = format!("marker-proto-{tag}");
    let prime = format!("marker-prime-{tag}");
    let session = format!("session-proto-{tag}");
    let (status, text) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias_proto,
            "messages": [{"role": "user", "content": marker}],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the chat-ingress seed must be served: {text}");
    let call = chat_call(&parse(&text));
    let call_id = call["id"].as_str().expect("call id").to_string();
    // Prime the Responses cycle so the colliding continuation is the second
    // request of that ingress and would rotate to the other candidate.
    let (status, text2) = post_json(
        port,
        "/v1/responses",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias_proto,
            "input": [{"role": "user", "content": [{"type": "input_text", "text": prime}]}],
        }),
    )
    .await;
    assert_eq!(status, 200, "the responses prime must be served: {text2}");
    let (status, text3) = post_json(
        port,
        "/v1/responses",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias_proto,
            "input": [
                {"role": "user", "content": [{"type": "input_text", "text": marker}]},
                {"type": "function_call", "call_id": call_id, "name": call["function"]["name"], "arguments": call["function"]["arguments"]},
                {"type": "function_call_output", "call_id": call_id, "output": "sunny"},
            ],
        }),
    )
    .await;
    assert_eq!(status, 200, "the responses continuation must be served: {text3}");
    assert!(
        for_marker(&log_a, &marker, 1).is_none(),
        "a continuation on another ingress must not inherit the chat scope's preference"
    );
    assert_eq!(
        forwarded_model(
            &for_marker(&log_b, &marker, 1).expect("the responses ingress must rotate to B")
        ),
        "remote-b-chat"
    );

    stop_server().await.expect("stop the listener");
}

/// Two sibling mappings of the same provider stay distinct candidates: a chain
/// on the first must not pin the next ordinary turn to either sibling.
#[tokio::test]
async fn sibling_mappings_of_one_provider_stay_distinct_on_reselection() {
    let _home = temp_home("pool-context-hold-siblings");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-siblings-{tag}");
    let session = format!("sess-siblings-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url, log) = spawn_chat_tool_upstream(next.clone()).await;

    let provider_value = pool_provider(
        &provider_id,
        "Sibling Provider",
        &url,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![
            provider_mapping("local-a", "remote-a", "chat_completions"),
            provider_mapping("local-b", "remote-b", "chat_completions"),
        ],
    );
    let pool = json!({
        "id": format!("pool-siblings-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_id, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_id, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(port, single_key(), vec![provider_value], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, text) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [{"role": "user", "content": "weather?"}],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the seed request must be served: {text}");
    let call = chat_call(&parse(&text));
    let call_id = call["id"].as_str().expect("call id").to_string();
    assert_eq!(
        forwarded_model(&with_results(&log, 0).expect("the seed must reach the first sibling")),
        "remote-a"
    );

    let (status, text2) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "weather?"},
                {"role": "assistant", "content": null, "tool_calls": [call.clone()]},
                {"role": "tool", "tool_call_id": call_id, "content": "sunny"},
                {"role": "user", "content": "weather?"},
            ],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the new turn must be served: {text2}");
    assert_eq!(
        forwarded_model(&with_results(&log, 1).expect("the new turn must rotate to the sibling")),
        "remote-b"
    );

    stop_server().await.expect("stop the listener");
}

/// A held candidate that is no longer a pool member is not preferred: the
/// remaining healthy candidate serves and selection never escapes to an
/// ordinary provider default outside the pool.
#[tokio::test]
async fn a_removed_candidate_is_not_preferred_and_the_pool_never_escapes() {
    let _home = temp_home("pool-context-hold-removed-candidate");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let pool_id = format!("pool-removed-{tag}");
    let alias = format!("free-removed-{tag}");
    let session = format!("sess-removed-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url, log) = spawn_chat_tool_upstream(next.clone()).await;

    let mut provider_value = pool_provider(
        &provider_id,
        "Provider One",
        &url,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![
            provider_mapping("local-a", "remote-a", "chat_completions"),
            provider_mapping("local-b", "remote-b", "chat_completions"),
        ],
    );
    provider_value["default_model"] = json!("remote-default");

    let pool_before = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_id, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_id, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_value.clone()],
        vec![pool_before],
    ));
    start_server(None).await.expect("start the listener");

    let (status, text) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [{"role": "user", "content": "weather?"}],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the seed request must be served: {text}");
    let call = chat_call(&parse(&text));
    let call_id = call["id"].as_str().expect("call id").to_string();
    assert_eq!(
        forwarded_model(&with_results(&log, 0).expect("the seed must reach candidate A")),
        "remote-a"
    );

    // Remove the held candidate from the pool; only the sibling remains.
    let pool_after = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_id, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_value],
        vec![pool_after],
    ));

    let (status, text2) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "weather?"},
                {"role": "assistant", "content": null, "tool_calls": [call.clone()]},
                {"role": "tool", "tool_call_id": call_id, "content": "sunny"},
            ],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the continuation must be served by the pool: {text2}");
    let served = with_results(&log, 1).expect("the remaining candidate must serve");
    assert_eq!(forwarded_model(&served), "remote-b");
    assert!(
        log.lock()
            .expect("mock log")
            .iter()
            .all(|captured| forwarded_model(captured) != "remote-default"),
        "the pool must never escape to the ordinary provider default"
    );

    stop_server().await.expect("stop the listener");
}

/// An oversized retained history still forwards successfully with normal
/// routing and produces no new validation error.
#[tokio::test]
async fn an_oversized_history_still_forwards_with_normal_routing() {
    let _home = temp_home("pool-context-hold-oversized");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-oversized-{tag}");
    let session = format!("sess-oversized-{tag}");
    let (url_a, log_a) = spawn_plain_chat_upstream().await;
    let (url_b, _log_b) = spawn_plain_chat_upstream().await;

    let provider_a_value = pool_provider(
        &provider_a,
        "Provider A",
        &url_a,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![provider_mapping("local-a", "remote-a", "chat_completions")],
    );
    let provider_b_value = pool_provider(
        &provider_b,
        "Provider B",
        &url_b,
        vec![pool_key("key-b", "Key B", "sk-b", true)],
        vec![provider_mapping("local-b", "remote-b", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-oversized-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let large = "x".repeat(1_100_000);
    let (status, text) = post_json(
        port,
        "/v1/chat/completions",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [{"role": "user", "content": large}],
        }),
    )
    .await;
    assert_eq!(
        status, 200,
        "an oversized history must forward without a new validation error: {text}"
    );
    let served = with_results(&log_a, 0).expect("the oversized request must use normal routing to candidate A");
    assert_eq!(forwarded_model(&served), "remote-a");
    assert!(
        served.body.len() > 1_000_000,
        "the oversized history must be forwarded intact, got {} bytes",
        served.body.len()
    );

    stop_server().await.expect("stop the listener");
}
