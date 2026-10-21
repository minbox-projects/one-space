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
    call_gateway, default_usage_store, free_port, pool_key, pool_provider, read_raw_gateway_config,
    spawn_mock_upstream, temp_home, write_raw_gateway_config, Captured, CapturedLog, MockReply,
};
use crate::ai_gateway::commands::ai_gateway_model_pools_status;
use crate::ai_gateway::model_pools::cool_pool_candidate;
use crate::ai_gateway::pool_context_hold::{CallerHistory, HoldScope, HoldStore};
use crate::ai_gateway::runtime_http::{read_http_request, start_server, stop_server};
use crate::ai_gateway::{
    GatewayConfig, GatewayUpstreamProvider, PoolCandidateIdentity, UpstreamProtocol,
    GATEWAY_CONFIG_SCHEMA_VERSION,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Instant;

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

/// A fallback-stage success closes any stale primary continuation preference and
/// publishes no fallback preference; a later healthy or recovered regular
/// candidate regains priority on the next request.
#[tokio::test]
async fn pool_fallback_stage_success_closes_stale_primary_continuation_preference_and_publishes_none() {
    let _home = temp_home("pool-context-hold-fallback-stage-preference");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let provider_f = format!("pf-{tag}");
    let alias = format!("free-fallback-{tag}");
    let session = format!("sess-fallback-{tag}");
    let pool_id = format!("pool-fallback-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_b, log_b) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_f, log_f) = spawn_chat_tool_upstream(next.clone()).await;

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
    let provider_f_value = pool_provider(
        &provider_f,
        "Provider Fallback",
        &url_f,
        vec![pool_key("key-f", "Key F", "sk-f", true)],
        vec![provider_mapping("local-f", "remote-f", "chat_completions")],
    );
    let pool = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
        "fallback_models": ["local-f"],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value, provider_f_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    for round in 0..3usize {
        if round == 1 {
            cool_pool_candidate(
                &PoolCandidateIdentity {
                    pool_id: pool_id.clone(),
                    provider_id: provider_a.clone(),
                    local_model: "local-a".to_string(),
                    upstream_model: "remote-a".to_string(),
                    protocol: UpstreamProtocol::ChatCompletions,
                    reasoning: None,
                },
                Some(Duration::from_secs(2)),
            );
            cool_pool_candidate(
                &PoolCandidateIdentity {
                    pool_id: pool_id.clone(),
                    provider_id: provider_b.clone(),
                    local_model: "local-b".to_string(),
                    upstream_model: "remote-b".to_string(),
                    protocol: UpstreamProtocol::ChatCompletions,
                    reasoning: None,
                },
                Some(Duration::from_secs(2)),
            );
        }
        if round == 2 {
            tokio::time::sleep(Duration::from_millis(2500)).await;
        }
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
            "round {round} must be served by a pool source: {text}"
        );
        let value = parse(&text);
        assert_eq!(value["model"], alias, "the client must see the public alias");
        let call = chat_call(&value);
        let call_id = call["id"]
            .as_str()
            .expect("the response must carry a call id")
            .to_string();
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [call]}));
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));
    }

    // The excluded continuation was served by the configured fallback.
    let fallback_round_one = with_results(&log_f, 1).map(|captured| forwarded_model(&captured));
    assert_eq!(
        fallback_round_one.as_deref(),
        Some("remote-f"),
        "the excluded continuation must be served by the fallback"
    );
    // A fallback-stage success publishes no fallback preference and closes any
    // stale primary preference, so the fallback is not contacted afterwards.
    assert!(
        with_results(&log_f, 2).is_none(),
        "a fallback-stage success must not publish a fallback preference: {}",
        with_results(&log_f, 2).map(|c| forwarded_model(&c)).unwrap_or_default()
    );
    // A later healthy or recovered regular candidate regains priority.
    assert!(
        with_results(&log_a, 2).is_some() || with_results(&log_b, 2).is_some(),
        "an eligible regular candidate must have priority after recovery"
    );

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

// ---------------------------------------------------------------------------
// Streaming behavior: local SSE builders, parsers and gated mock upstreams
// ---------------------------------------------------------------------------

/// One Chat SSE tool-call stream split across two writes, with a mix of LF and
/// CRLF event boundaries and the terminal `data: [DONE]`. The first write ends
/// in the middle of an event body, so it exercises incremental re-assembly.
fn chat_sse_tool_call_chunks(call_id: &str) -> Vec<Vec<u8>> {
    let first = json!({
        "id": format!("chatcmpl-{call_id}"),
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{
            "index": 0,
            "delta": {
                "role": "assistant",
                "tool_calls": [{
                    "index": 0,
                    "id": call_id,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": ""}
                }]
            },
            "finish_reason": Value::Null
        }]
    });
    let second = json!({
        "id": format!("chatcmpl-{call_id}"),
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{
            "index": 0,
            "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "{\"city\":\"SF\"}"}}]},
            "finish_reason": Value::Null
        }]
    });
    let third = json!({
        "id": format!("chatcmpl-{call_id}"),
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]
    });
    let full = format!("data: {first}\n\ndata: {second}\r\n\r\ndata: {third}\n\ndata: [DONE]\n\n");
    let split = full.find("\"choices\"").expect("choices field present") + 3;
    vec![
        full.as_bytes()[..split].to_vec(),
        full.as_bytes()[split..].to_vec(),
    ]
}

/// The same Chat tool-call stream as a single buffer for upstreams where
/// fragmentation is not the behavior under test.
fn chat_sse_tool_call(call_id: &str) -> String {
    chat_sse_tool_call_chunks(call_id)
        .into_iter()
        .map(|chunk| String::from_utf8(chunk).expect("ascii sse"))
        .collect()
}

/// The first Chat tool-call event on its own, for a stream that deliberately
/// never completes.
fn first_chat_tool_event(call_id: &str) -> Vec<u8> {
    let value = json!({
        "id": format!("chatcmpl-{call_id}"),
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{
            "index": 0,
            "delta": {
                "role": "assistant",
                "tool_calls": [{
                    "index": 0,
                    "id": call_id,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": ""}
                }]
            },
            "finish_reason": Value::Null
        }]
    });
    format!("data: {value}\n\n").into_bytes()
}

/// A Chat SSE stream that ends with a plain assistant text answer and no tool
/// call.
fn chat_sse_plain(text: &str) -> String {
    let body = json!({
        "id": "chatcmpl-plain",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{"index": 0, "delta": {"role": "assistant", "content": text}, "finish_reason": Value::Null}]
    });
    let stop = json!({
        "id": "chatcmpl-plain",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
    });
    format!("data: {body}\n\ndata: {stop}\n\ndata: [DONE]\n\n")
}

/// A Chat SSE stream whose only non-text events are a comment, a metadata event
/// and a usage-only chunk: none of them may count as tool evidence.
fn chat_sse_heartbeat_then_text() -> String {
    let usage = json!({
        "id": "chatcmpl-heartbeat",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [],
        "usage": {"prompt_tokens": 1, "completion_tokens": 0, "total_tokens": 1}
    });
    let metadata = json!({
        "id": "chatcmpl-heartbeat",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [],
        "metadata": {"provider": "mock", "region": "test"}
    });
    let text = json!({
        "id": "chatcmpl-heartbeat",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{"index": 0, "delta": {"role": "assistant", "content": "ok"}, "finish_reason": Value::Null}]
    });
    let stop = json!({
        "id": "chatcmpl-heartbeat",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
    });
    format!(
        ": keep-alive\n\nevent: response.metadata\ndata: {metadata}\n\ndata: {usage}\n\ndata: {text}\n\ndata: {stop}\n\ndata: [DONE]\n\n"
    )
}

/// A Responses SSE stream that announces one function call, streams its
/// arguments and completes.
fn responses_sse_tool_call(call_id: &str) -> String {
    let item_id = format!("fc-{call_id}");
    let response_id = format!("resp-{call_id}");
    let event = |kind: &str, value: Value| format!("event: {kind}\ndata: {value}\n\n");
    let mut stream = String::new();
    stream.push_str(&event(
        "response.created",
        json!({"type": "response.created", "response": {"id": response_id.clone(), "status": "in_progress"}}),
    ));
    stream.push_str(&event(
        "response.output_item.added",
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {
                "id": item_id.clone(),
                "type": "function_call",
                "call_id": call_id,
                "name": "get_weather",
                "arguments": "",
                "status": "in_progress"
            }
        }),
    ));
    stream.push_str(&event(
        "response.function_call_arguments.delta",
        json!({
            "type": "response.function_call_arguments.delta",
            "item_id": item_id.clone(),
            "output_index": 0,
            "delta": "{\"city\":\"SF\"}"
        }),
    ));
    stream.push_str(&event(
        "response.output_item.done",
        json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": {
                "id": item_id.clone(),
                "type": "function_call",
                "call_id": call_id,
                "name": "get_weather",
                "arguments": "{\"city\":\"SF\"}",
                "status": "completed"
            }
        }),
    ));
    stream.push_str(&event(
        "response.completed",
        json!({
            "type": "response.completed",
            "response": {
                "id": response_id,
                "status": "completed",
                "output": [{
                    "id": item_id,
                    "type": "function_call",
                    "call_id": call_id,
                    "name": "get_weather",
                    "arguments": "{\"city\":\"SF\"}",
                    "status": "completed"
                }],
                "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
            }
        }),
    ));
    stream
}

/// A plain non-streaming Chat completion body.
fn chat_plain_body() -> Value {
    json!({
        "id": "chatcmpl-plain-ns",
        "object": "chat.completion",
        "model": "upstream",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "ok"},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    })
}

/// The JSON values of every `data:` line of a client-visible SSE body, ignoring
/// the terminal `[DONE]` marker.
fn client_sse_values(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .filter(|data| !data.is_empty() && *data != "[DONE]")
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .collect()
}

/// Client-visible Chat tool-call identities of a streamed response.
fn client_chat_tool_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for value in client_sse_values(text) {
        let Some(choices) = value.get("choices").and_then(Value::as_array) else {
            continue;
        };
        for choice in choices {
            let Some(calls) = choice
                .get("delta")
                .and_then(|delta| delta.get("tool_calls"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for call in calls {
                if let Some(id) = call.get("id").and_then(Value::as_str) {
                    if !id.is_empty() {
                        ids.push(id.to_string());
                    }
                }
            }
        }
    }
    ids
}

/// Client-visible Responses function-call identities of a streamed response.
fn client_responses_call_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(data.trim()) else {
            continue;
        };
        if let Some(item) = value.get("item") {
            if let Some(id) = item.get("call_id").and_then(Value::as_str) {
                if !id.is_empty() {
                    ids.push(id.to_string());
                }
            }
        }
        if let Some(output) = value
            .get("response")
            .and_then(|response| response.get("output"))
            .and_then(Value::as_array)
        {
            for item in output {
                if item.get("type").and_then(Value::as_str) == Some("function_call") {
                    if let Some(id) = item.get("call_id").and_then(Value::as_str) {
                        if !id.is_empty() {
                            ids.push(id.to_string());
                        }
                    }
                }
            }
        }
    }
    ids
}

/// Whether a streamed response carried the ingress protocol's completion marker.
fn client_stream_completed(path: &str, text: &str) -> bool {
    if path == "/v1/responses" {
        text.contains("response.completed")
    } else {
        text.contains("data: [DONE]")
    }
}

/// Whether one forwarded request asked for streaming.
fn request_wants_stream(captured: &Captured) -> bool {
    parsed_body(captured)
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// The first captured request matching a predicate.
fn find_request<F>(log: &CapturedLog, predicate: F) -> Option<Captured>
where
    F: Fn(&Captured) -> bool,
{
    log.lock()
        .expect("mock log")
        .iter()
        .find(|captured| predicate(captured))
        .cloned()
}

/// A scripted local upstream reply.
enum ScriptedReply {
    /// Write SSE headers, then these chunks in order, one write each.
    Sse(Vec<Vec<u8>>),
    /// Write SSE headers and the first event, then hold the connection open until
    /// the release signal, so the caller can observe a stream that began but
    /// never finished.
    HoldAfterFirst {
        first: Vec<u8>,
        release: Arc<tokio::sync::Notify>,
    },
    /// Write SSE headers, announce arrival, wait for the release signal, then
    /// write every chunk and close. It holds a *successful* stream open so the
    /// test can let it finish after a newer request.
    HoldThenRelease {
        arrived: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        chunks: Vec<Vec<u8>>,
    },
}

/// A local upstream that records every request and answers it from a per-request
/// script, allowing fragmented SSE writes and a deliberately held stream. Used
/// only where the fixed `MockReply` variants cannot express the transport.
async fn spawn_scripted_upstream<F>(script: F) -> (String, CapturedLog)
where
    F: Fn(usize, &Captured) -> ScriptedReply + Send + Sync + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind scripted upstream");
    let addr = listener.local_addr().expect("scripted upstream addr");
    let log: CapturedLog = Arc::new(Mutex::new(Vec::new()));
    let log_for_server = log.clone();
    let script = Arc::new(script);
    let counter = Arc::new(AtomicUsize::new(0));
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let log = log_for_server.clone();
            let script = script.clone();
            let counter = counter.clone();
            tauri::async_runtime::spawn(async move {
                let Ok(request) = read_http_request(&mut stream).await else {
                    return;
                };
                let captured = Captured {
                    method: request.method,
                    path: request.path,
                    headers: request.headers,
                    body: request.body,
                };
                log.lock().expect("mock log").push(captured.clone());
                let index = counter.fetch_add(1, Ordering::SeqCst);
                match script(index, &captured) {
                    ScriptedReply::Sse(chunks) => {
                        let total: usize = chunks.iter().map(Vec::len).sum();
                        let header = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {total}\r\nconnection: close\r\n\r\n"
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        for chunk in &chunks {
                            let _ = stream.write_all(chunk).await;
                            let _ = stream.flush().await;
                        }
                    }
                    ScriptedReply::HoldAfterFirst { first, release } => {
                        let header = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 1000000\r\nconnection: close\r\n\r\n";
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(&first).await;
                        let _ = stream.flush().await;
                        release.notified().await;
                    }
                    ScriptedReply::HoldThenRelease {
                        arrived,
                        release,
                        chunks,
                    } => {
                        let total: usize = chunks.iter().map(Vec::len).sum();
                        let header = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {total}\r\nconnection: close\r\n\r\n"
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.flush().await;
                        arrived.notify_one();
                        release.notified().await;
                        for chunk in &chunks {
                            let _ = stream.write_all(chunk).await;
                            let _ = stream.flush().await;
                        }
                    }
                }
            });
        }
    });
    (format!("http://{addr}"), log)
}

/// Send one streaming request over a raw socket and drop the connection once the
/// response has started, so the gateway observes a mid-stream client disconnect
/// rather than a pre-output cancellation.
async fn raw_stream_then_disconnect(
    port: u16,
    path: &str,
    auth: &str,
    session: &str,
    body: Value,
    awaited: &str,
) {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect gateway");
    let body = serde_json::to_vec(&body).expect("encode body");
    let request = format!(
        "POST {path} HTTP/1.1\r\nhost: 127.0.0.1\r\ncontent-type: application/json\r\nauthorization: {auth}\r\nx-opencode-session: {session}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write headers");
    stream.write_all(&body).await.expect("write body");
    stream.flush().await.expect("flush request");
    let mut buffer = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let read = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut tmp))
            .await
            .expect("the gateway must begin streaming")
            .expect("read from gateway");
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&tmp[..read]);
        if String::from_utf8_lossy(&buffer).contains(awaited) {
            break;
        }
    }
    drop(stream);
}

// ---------------------------------------------------------------------------
// Streaming continuity and failover (current behavior rotates away)
// ---------------------------------------------------------------------------

/// A streamed Chat tool chain must keep attempting the candidate that delivered
/// the preceding tool call for both appended continuations, instead of advancing
/// the weighted rotation. The upstream bytes are fragmented mid-line and mix LF
/// and CRLF event boundaries.
#[tokio::test]
async fn streaming_chat_tool_chain_prefers_the_preceding_candidate() {
    let _home = temp_home("pool-stream-chat-chain");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-chat-{tag}");
    let session = format!("sess-stream-chat-{tag}");
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_scripted_upstream(move |_index, _captured| {
        let n = counter_a.fetch_add(1, Ordering::SeqCst) + 1;
        ScriptedReply::Sse(chat_sse_tool_call_chunks(&format!("call_a_{n}")))
    })
    .await;
    let (url_b, log_b) = spawn_scripted_upstream(move |_index, _captured| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        ScriptedReply::Sse(chat_sse_tool_call_chunks(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-chat-{tag}"),
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

    let path = "/v1/chat/completions";
    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    let mut call_ids: Vec<String> = Vec::new();
    for round in 0..3 {
        let body = json!({
            "model": alias,
            "messages": messages.clone(),
            "tools": weather_tools(),
            "stream": true,
        });
        let (status, text) = post_json(port, path, "Bearer local-key", Some(&session), None, body).await;
        assert_eq!(status, 200, "round {round} must be served: {text}");
        assert!(
            text.contains(&format!("\"model\":\"{alias}\"")),
            "round {round} must keep the public alias: {text}"
        );
        assert!(
            client_stream_completed(path, &text),
            "round {round} must end with the completion marker: {text}"
        );
        let ids = client_chat_tool_ids(&text);
        let call_id = ids
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("round {round} must stream a tool call: {text}"));
        call_ids.push(call_id.clone());
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [{
            "id": call_id,
            "type": "function",
            "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
        }]}));
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));
    }

    for results in 1..=2usize {
        let served_a = with_results(&log_a, results);
        let served_b = with_results(&log_b, results);
        assert!(
            served_a.is_some(),
            "streaming continuation {results} must attempt the preceding candidate: a={} b={}",
            served_a.is_some(),
            served_b.is_some()
        );
        assert!(
            served_b.is_none(),
            "streaming continuation {results} must not rotate to the sibling"
        );
    }

    let continuation = with_results(&log_a, 1).expect("the first continuation must reach candidate A");
    assert!(
        String::from_utf8_lossy(&continuation.body).contains(&call_ids[0]),
        "the forwarded continuation must retain the observed tool identity"
    );

    stop_server().await.expect("stop the listener");
}

/// A streamed Responses function-call chain must keep attempting the candidate
/// that delivered the preceding call for both appended continuations.
#[tokio::test]
async fn streaming_responses_tool_chain_prefers_the_preceding_candidate() {
    let _home = temp_home("pool-stream-responses-chain");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-resp-{tag}");
    let session = format!("sess-stream-resp-{tag}");
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_mock_upstream(move |_| {
        let n = counter_a.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(responses_sse_tool_call(&format!("call_a_{n}")))
    })
    .await;
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(responses_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-resp-{tag}"),
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

    let path = "/v1/responses";
    let mut input = vec![json!({
        "role": "user",
        "content": [{"type": "input_text", "text": "weather?"}],
    })];
    for round in 0..3 {
        let body = json!({"model": alias, "input": input.clone(), "stream": true});
        let (status, text) = post_json(port, path, "Bearer local-key", Some(&session), None, body).await;
        assert_eq!(status, 200, "round {round} must be served: {text}");
        assert!(
            client_stream_completed(path, &text),
            "round {round} must complete with response.completed: {text}"
        );
        let ids = client_responses_call_ids(&text);
        let call_id = ids
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("round {round} must expose a function call: {text}"));
        input.push(json!({
            "type": "function_call",
            "call_id": call_id.clone(),
            "name": "get_weather",
            "arguments": "{\"city\":\"SF\"}",
        }));
        input.push(json!({
            "type": "function_call_output",
            "call_id": call_id,
            "output": "sunny",
        }));
    }

    for results in 1..=2usize {
        assert!(
            with_results(&log_a, results).is_some(),
            "streaming Responses continuation {results} must attempt the preceding candidate"
        );
        assert!(
            with_results(&log_b, results).is_none(),
            "streaming Responses continuation {results} must not rotate to the sibling"
        );
    }

    stop_server().await.expect("stop the listener");
}

/// A streamed Chat chain whose pool candidate is served by a Responses-protocol
/// upstream keeps its continuity and its client-visible tool identity through
/// the existing conversion adapter.
#[tokio::test]
async fn streaming_chat_chain_through_a_responses_upstream_prefers_the_preceding_candidate() {
    let _home = temp_home("pool-stream-adapter-chain");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-adapter-{tag}");
    let session = format!("sess-stream-adapter-{tag}");
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_mock_upstream(move |_| {
        let n = counter_a.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(responses_sse_tool_call(&format!("call_a_{n}")))
    })
    .await;
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(responses_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-adapter-{tag}"),
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

    let path = "/v1/chat/completions";
    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    for round in 0..3 {
        let body = json!({
            "model": alias,
            "messages": messages.clone(),
            "tools": weather_tools(),
            "stream": true,
        });
        let (status, text) = post_json(port, path, "Bearer local-key", Some(&session), None, body).await;
        assert_eq!(status, 200, "round {round} must be served: {text}");
        assert!(
            client_stream_completed(path, &text),
            "the adapted Chat stream must end with [DONE]: {text}"
        );
        let ids = client_chat_tool_ids(&text);
        let call_id = ids.first().cloned().unwrap_or_else(|| {
            panic!("the adapted Chat stream must expose a stable tool identity: {text}")
        });
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [{
            "id": call_id,
            "type": "function",
            "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
        }]}));
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));
    }

    for results in 1..=2usize {
        assert!(
            with_results(&log_a, results).is_some(),
            "adapted continuation {results} must attempt the preceding candidate"
        );
        assert!(
            with_results(&log_b, results).is_none(),
            "adapted continuation {results} must not rotate to the sibling"
        );
    }

    stop_server().await.expect("stop the listener");
}

/// After the streamed candidate fails before any client byte, the sibling that
/// streams the tool call becomes the immediate next continuation preference.
#[tokio::test]
async fn streaming_failover_becomes_the_next_continuation_preference() {
    let _home = temp_home("pool-stream-failover");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-failover-{tag}");
    let session = format!("sess-stream-failover-{tag}");
    let seed_call = format!("call_a_seed_{tag}");
    let calls_a = Arc::new(AtomicUsize::new(0));
    let seed_call_for_mock = seed_call.clone();
    let (url_a, log_a) = spawn_mock_upstream(move |_| {
        let n = calls_a.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            MockReply::Stream(chat_sse_tool_call(&seed_call_for_mock))
        } else {
            MockReply::Json(500, json!({"error": {"message": "upstream unavailable"}}))
        }
    })
    .await;
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-failover-{tag}"),
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

    let path = "/v1/chat/completions";
    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    let mut tool_calls: Vec<String> = Vec::new();
    for round in 0..3 {
        let body = json!({
            "model": alias,
            "messages": messages.clone(),
            "tools": weather_tools(),
            "stream": true,
        });
        let (status, text) = post_json(port, path, "Bearer local-key", Some(&session), None, body).await;
        assert_eq!(status, 200, "round {round} must be served after failover: {text}");
        let ids = client_chat_tool_ids(&text);
        let call_id = ids
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("round {round} must stream a tool call: {text}"));
        tool_calls.push(call_id.clone());
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [{
            "id": call_id,
            "type": "function",
            "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
        }]}));
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));
    }

    let a_first = with_results(&log_a, 1);
    assert!(
        a_first.is_some(),
        "the held candidate must be attempted first and fail before the fallback: a={} b={}",
        a_first.is_some(),
        with_results(&log_b, 1).is_some()
    );
    assert_eq!(
        forwarded_model(&a_first.expect("checked above")),
        "remote-a"
    );
    assert!(
        with_results(&log_a, 2).is_none(),
        "the continuation after failover must prefer the successful sibling, not re-attempt the failed candidate"
    );
    assert_eq!(
        forwarded_model(&with_results(&log_b, 1).expect("the fallback must serve the first continuation")),
        "remote-b"
    );
    assert_eq!(
        forwarded_model(&with_results(&log_b, 2).expect("the fallback must serve the second continuation")),
        "remote-b"
    );
    assert_eq!(tool_calls.len(), 3);
    assert_eq!(tool_calls[0], seed_call);
    assert!(tool_calls[1].starts_with("call_b_"));
    assert!(tool_calls[2].starts_with("call_b_"));

    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// Streaming guards: boundaries a delivered implementation must preserve
// ---------------------------------------------------------------------------

/// Heartbeat, metadata and usage-only events create no tool evidence: the stream
/// still completes and a later would-be continuation follows the ordinary
/// rotation.
#[tokio::test]
async fn streaming_metadata_and_usage_events_create_no_tool_evidence() {
    let _home = temp_home("pool-stream-metadata");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-metadata-{tag}");
    let session = format!("sess-stream-metadata-{tag}");
    let (url_a, log_a) =
        spawn_mock_upstream(|_| MockReply::Stream(chat_sse_heartbeat_then_text())).await;
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-metadata-{tag}"),
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

    let path = "/v1/chat/completions";
    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": [{"role": "user", "content": "hi"}], "stream": true}),
    )
    .await;
    assert_eq!(status, 200, "the heartbeat stream must complete: {text}");
    assert!(client_stream_completed(path, &text), "the stream must end with [DONE]: {text}");
    assert!(
        client_chat_tool_ids(&text).is_empty(),
        "metadata/usage events must not surface a tool call: {text}"
    );
    assert!(
        with_results(&log_a, 0).is_some(),
        "the seed must be served by the first candidate"
    );

    let fabricated = format!("call_fabricated_{tag}");
    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": fabricated,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": fabricated, "content": "sunny"},
            ],
            "stream": true,
        }),
    )
    .await;
    assert_eq!(status, 200, "the later request must be served: {text}");
    assert!(
        with_results(&log_b, 1).is_some() && with_results(&log_a, 1).is_none(),
        "a metadata-only stream must not hold the next request on the first candidate"
    );

    stop_server().await.expect("stop the listener");
}

/// A successful streamed response without a further tool call closes the
/// continuation preference: a later identically shaped request follows the
/// ordinary rotation instead of the old candidate.
#[tokio::test]
async fn a_streamed_answer_without_a_tool_call_closes_the_preference() {
    let _home = temp_home("pool-stream-close");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let provider_c = format!("pc-{tag}");
    let alias = format!("free-stream-close-{tag}");
    let session = format!("sess-stream-close-{tag}");
    let counter_a = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_mock_upstream(move |captured| {
        if tool_result_count(captured) == 0 {
            let n = counter_a.fetch_add(1, Ordering::SeqCst) + 1;
            MockReply::Stream(chat_sse_tool_call(&format!("call_a_{n}")))
        } else {
            MockReply::Stream(chat_sse_plain("final"))
        }
    })
    .await;
    let (url_b, _log_b) = spawn_mock_upstream(|_| MockReply::Stream(chat_sse_plain("b"))).await;
    let (url_c, log_c) = spawn_mock_upstream(|captured| {
        if request_wants_stream(captured) {
            MockReply::Stream(chat_sse_plain("c"))
        } else {
            MockReply::Json(200, chat_plain_body())
        }
    })
    .await;

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
    let provider_c_value = pool_provider(
        &provider_c,
        "Provider C",
        &url_c,
        vec![pool_key("key-c", "Key C", "sk-c", true)],
        vec![provider_mapping("local-c", "remote-c", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-stream-close-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
            pool_candidate(&provider_c, "local-c", "remote-c", "chat_completions", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value, provider_c_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let path = "/v1/chat/completions";
    let (status, seed_text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": [{"role": "user", "content": "weather?"}], "tools": weather_tools(), "stream": true}),
    )
    .await;
    assert_eq!(status, 200, "the seed must be served: {seed_text}");
    let seed_call = client_chat_tool_ids(&seed_text)
        .into_iter()
        .next()
        .expect("the seed must stream a tool call");
    let continuation = json!([
        {"role": "user", "content": "weather?"},
        {"role": "assistant", "content": null, "tool_calls": [{
            "id": seed_call,
            "type": "function",
            "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
        }]},
        {"role": "tool", "tool_call_id": seed_call, "content": "sunny"},
    ]);

    // A continuated final answer with no further tool call closes the preference.
    let (status, final_text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": continuation.clone(), "tools": weather_tools(), "stream": true}),
    )
    .await;
    assert_eq!(status, 200, "the final answer must be served: {final_text}");
    assert!(
        client_chat_tool_ids(&final_text).is_empty(),
        "the final answer must not stream a tool call: {final_text}"
    );

    // The same shape sent again must use the ordinary rotation, not the closed
    // candidate.
    let (status, later_text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": continuation, "tools": weather_tools()}),
    )
    .await;
    assert_eq!(status, 200, "the later request must be served: {later_text}");
    assert!(
        find_request(&log_c, |captured| {
            tool_result_count(captured) == 1 && !request_wants_stream(captured)
        })
        .is_some(),
        "the closed preference must rotate the later request to the third candidate"
    );
    assert!(
        find_request(&log_a, |captured| {
            tool_result_count(captured) == 1 && !request_wants_stream(captured)
        })
        .is_none(),
        "a closed preference must not send the later request back to the first candidate"
    );

    stop_server().await.expect("stop the listener");
}

/// A downstream disconnect after the stream began publishes no hold and triggers
/// no replay: the next request follows the ordinary rotation.
#[tokio::test]
async fn a_mid_stream_disconnect_publishes_no_hold_and_plays_no_replay() {
    let _home = temp_home("pool-stream-disconnect");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-disconnect-{tag}");
    let session = format!("sess-stream-disconnect-{tag}");
    let aborted_call = format!("call_aborted_{tag}");
    let release = Arc::new(tokio::sync::Notify::new());
    let first = first_chat_tool_event(&aborted_call);
    let (url_a, log_a) = spawn_scripted_upstream(move |_index, _captured| {
        ScriptedReply::HoldAfterFirst {
            first: first.clone(),
            release: release.clone(),
        }
    })
    .await;
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-disconnect-{tag}"),
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

    let path = "/v1/chat/completions";
    raw_stream_then_disconnect(
        port,
        path,
        "Bearer local-key",
        &session,
        json!({"model": alias, "messages": [{"role": "user", "content": "weather?"}], "tools": weather_tools(), "stream": true}),
        &aborted_call,
    )
    .await;

    assert!(
        with_results(&log_a, 0).is_some(),
        "the aborted request must have reached the first candidate"
    );
    assert!(
        log_b.lock().expect("mock log").is_empty(),
        "a disconnect after output began must not replay to another candidate"
    );

    // The same shape as the aborted request's would-be continuation must rotate
    // normally rather than inherit a hold.
    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "weather?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": aborted_call,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": aborted_call, "content": "sunny"},
            ],
            "tools": weather_tools(),
            "stream": true,
        }),
    )
    .await;
    assert_eq!(status, 200, "the next request must be served: {text}");
    assert!(
        with_results(&log_b, 1).is_some() && with_results(&log_a, 1).is_none(),
        "an aborted stream must not hold the next request on the first candidate"
    );

    stop_server().await.expect("stop the listener");
}

/// A truncated stream without its completion marker publishes no hold, does not
/// replay, and leaves the bytes already delivered to the caller unchanged.
#[tokio::test]
async fn a_truncated_stream_publishes_no_hold_and_plays_no_replay() {
    let _home = temp_home("pool-stream-truncated");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-truncated-{tag}");
    let session = format!("sess-stream-truncated-{tag}");
    let truncated_call = format!("call_truncated_{tag}");
    let truncated = String::from_utf8(first_chat_tool_event(&truncated_call))
        .expect("ascii first event");
    let (url_a, log_a) =
        spawn_mock_upstream(move |_| MockReply::Stream(truncated.clone())).await;
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-truncated-{tag}"),
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

    let path = "/v1/chat/completions";
    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": [{"role": "user", "content": "weather?"}], "tools": weather_tools(), "stream": true}),
    )
    .await;
    assert_eq!(status, 200, "the truncated stream still answers: {text}");
    assert!(
        text.contains(&truncated_call),
        "the bytes delivered before the truncation must stay visible: {text}"
    );
    assert!(
        !client_stream_completed(path, &text),
        "a truncated stream must not fabricate the completion marker: {text}"
    );
    assert!(
        log_b.lock().expect("mock log").is_empty(),
        "a post-first-byte failure must not replay to another candidate"
    );

    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "weather?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": truncated_call,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": truncated_call, "content": "sunny"},
            ],
            "tools": weather_tools(),
            "stream": true,
        }),
    )
    .await;
    assert_eq!(status, 200, "the next request must be served: {text}");
    assert!(
        with_results(&log_b, 1).is_some() && with_results(&log_a, 1).is_none(),
        "a truncated stream must not hold the next request on the first candidate"
    );

    stop_server().await.expect("stop the listener");
}

/// Two overlapping requests in one scope: the newer admitted turn's hold survives
/// when the older request finishes last successfully.
#[tokio::test]
async fn an_older_successful_completion_cannot_replace_a_newer_turn() {
    let _home = temp_home("pool-order-older-newer");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-order-{tag}");
    let session = format!("sess-order-{tag}");
    let arrived_a = Arc::new(tokio::sync::Notify::new());
    let release_a = Arc::new(tokio::sync::Notify::new());
    let old_call = format!("call_old_{tag}");
    let new_call = format!("call_new_{tag}");
    let (url_a, log_a) = spawn_mock_upstream({
        let arrived = arrived_a.clone();
        let release = release_a.clone();
        move |_| {
            MockReply::Withhold(
                arrived.clone(),
                release.clone(),
                200,
                chat_tool_call_body(&old_call),
            )
        }
    })
    .await;
    let new_call_for_mock = new_call.clone();
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        MockReply::Json(200, chat_tool_call_body(&new_call_for_mock))
    })
    .await;

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
        "id": format!("pool-order-{tag}"),
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

    let path = "/v1/chat/completions";
    let older_body = json!({"model": alias, "messages": [{"role": "user", "content": "order?"}], "tools": weather_tools()});
    let newer_body = older_body.clone();

    let older = {
        let session = session.clone();
        tokio::spawn(async move {
            post_json(port, path, "Bearer local-key", Some(&session), None, older_body).await
        })
    };
    tokio::time::timeout(Duration::from_secs(10), arrived_a.notified())
        .await
        .expect("the older request must reach the first candidate");
    let (status, newer_text) =
        post_json(port, path, "Bearer local-key", Some(&session), None, newer_body).await;
    assert_eq!(status, 200, "the newer turn must complete: {newer_text}");
    let newer_call = chat_call(&parse(&newer_text))["id"]
        .as_str()
        .expect("newer call id")
        .to_string();

    release_a.notify_one();
    let (status, older_text) = older.await.expect("older task");
    assert_eq!(status, 200, "the older turn must complete: {older_text}");

    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "order?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": newer_call,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": newer_call, "content": "sunny"},
            ],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the continuation must be served: {text}");
    assert!(
        with_results(&log_b, 1).is_some() && with_results(&log_a, 1).is_none(),
        "the older completion must not replace the newer turn's preference"
    );

    stop_server().await.expect("stop the listener");
}

/// Two overlapping requests in one scope: an older request that finishes last
/// unsuccessfully cannot remove or resurrect the newer turn's hold.
#[tokio::test]
async fn an_older_unsuccessful_completion_cannot_resurrect_a_newer_turn() {
    let _home = temp_home("pool-order-older-failure");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-order-fail-{tag}");
    let session = format!("sess-order-fail-{tag}");
    let arrived_a = Arc::new(tokio::sync::Notify::new());
    let release_a = Arc::new(tokio::sync::Notify::new());
    let (url_a, log_a) = spawn_mock_upstream({
        let arrived = arrived_a.clone();
        let release = release_a.clone();
        move |_| {
            MockReply::Withhold(
                arrived.clone(),
                release.clone(),
                400,
                json!({"error": {"message": "invalid request"}}),
            )
        }
    })
    .await;
    let new_call = format!("call_new_{tag}");
    let new_call_for_mock = new_call.clone();
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        MockReply::Json(200, chat_tool_call_body(&new_call_for_mock))
    })
    .await;

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
        "id": format!("pool-order-fail-{tag}"),
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

    let path = "/v1/chat/completions";
    let older_body = json!({"model": alias, "messages": [{"role": "user", "content": "order?"}], "tools": weather_tools()});
    let newer_body = older_body.clone();

    let older = {
        let session = session.clone();
        tokio::spawn(async move {
            post_json(port, path, "Bearer local-key", Some(&session), None, older_body).await
        })
    };
    tokio::time::timeout(Duration::from_secs(10), arrived_a.notified())
        .await
        .expect("the older request must reach the first candidate");
    let (status, newer_text) =
        post_json(port, path, "Bearer local-key", Some(&session), None, newer_body).await;
    assert_eq!(status, 200, "the newer turn must complete: {newer_text}");
    let newer_call = chat_call(&parse(&newer_text))["id"]
        .as_str()
        .expect("newer call id")
        .to_string();

    release_a.notify_one();
    let (status, older_text) = older.await.expect("older task");
    assert_eq!(
        status, 400,
        "the older turn's terminal client error must pass through: {older_text}"
    );

    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "order?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": newer_call,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": newer_call, "content": "sunny"},
            ],
            "tools": weather_tools(),
        }),
    )
    .await;
    assert_eq!(status, 200, "the continuation must be served: {text}");
    assert!(
        with_results(&log_b, 1).is_some() && with_results(&log_a, 1).is_none(),
        "an older unsuccessful completion must not resurrect or replace the newer hold"
    );

    stop_server().await.expect("stop the listener");
}

/// Differing supplied thread identities in one session keep independent
/// histories: a continuation in a second thread does not inherit the first
/// thread's preference.
#[tokio::test]
async fn differing_thread_ids_keep_independent_streaming_histories() {
    let _home = temp_home("pool-stream-threads");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-threads-{tag}");
    let session = format!("sess-stream-threads-{tag}");
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_mock_upstream(move |_| {
        let n = counter_a.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_a_{n}")))
    })
    .await;
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-threads-{tag}"),
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

    let path = "/v1/chat/completions";
    let (status, seed_text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        Some("thread-1"),
        json!({"model": alias, "messages": [{"role": "user", "content": "weather?"}], "tools": weather_tools(), "stream": true}),
    )
    .await;
    assert_eq!(status, 200, "the thread-1 seed must be served: {seed_text}");
    let call_id = client_chat_tool_ids(&seed_text)
        .into_iter()
        .next()
        .expect("the seed must stream a tool call");
    assert!(with_results(&log_a, 0).is_some(), "the seed must reach the first candidate");

    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        Some("thread-2"),
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "weather?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": call_id, "content": "sunny"},
            ],
            "tools": weather_tools(),
            "stream": true,
        }),
    )
    .await;
    assert_eq!(status, 200, "the thread-2 continuation must be served: {text}");
    assert!(
        with_results(&log_b, 1).is_some() && with_results(&log_a, 1).is_none(),
        "a different thread identity must not inherit the first thread's preference"
    );

    stop_server().await.expect("stop the listener");
}

/// An oversized streaming request beyond the per-entry payload budget still
/// forwards intact with normal routing and no new validation error.
#[tokio::test]
async fn an_oversized_streaming_request_still_forwards_with_normal_routing() {
    let _home = temp_home("pool-stream-oversized");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-oversized-{tag}");
    let session = format!("sess-stream-oversized-{tag}");
    let (url_a, log_a) = spawn_mock_upstream(|_| MockReply::Stream(chat_sse_plain("ok"))).await;
    let (url_b, _log_b) = spawn_mock_upstream(|_| MockReply::Stream(chat_sse_plain("ok"))).await;

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
        "id": format!("pool-stream-oversized-{tag}"),
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
            "stream": true,
        }),
    )
    .await;
    assert_eq!(
        status, 200,
        "an oversized streaming request must forward without a new validation error: {text}"
    );
    let forwarded = find_request(&log_a, |captured| captured.body.len() > 1_000_000);
    assert!(
        forwarded.is_some(),
        "the oversized request body must be forwarded intact to the first candidate"
    );
    assert!(
        client_stream_completed("/v1/chat/completions", &text),
        "the oversized streaming request must still complete: {text}"
    );

    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// Isolated-store boundaries: explicit monotonic time, capacity and payload
//
// These tests drive the store seam directly with an explicit monotonic clock
// and their own store instance, so a time or budget boundary is asserted
// deterministically instead of against the process-global table or real time.
// Current eligibility, ordering and the routing effect still run through the
// same resolution helpers the request path uses.
// ---------------------------------------------------------------------------

/// Idle time after which a completed hold is treated as absent; a hold idle for
/// exactly this long stays eligible.
const HOLD_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Maximum completed entries the store retains.
const HOLD_ENTRY_CAPACITY: usize = 1024;
/// Maximum retained serialized payload of one entry.
const HOLD_ENTRY_BUDGET: usize = 1024 * 1024;
/// Maximum retained serialized payload across the whole table.
const HOLD_TOTAL_BUDGET: usize = 16 * 1024 * 1024;

/// One typed provider with a single mapping and one usable upstream key, so an
/// isolated hold store re-checks current eligibility through the real resolution
/// helpers without any process-global pool state.
fn typed_provider(
    provider_id: &str,
    base_url: &str,
    local_model: &str,
    upstream_model: &str,
    protocol: &str,
) -> GatewayUpstreamProvider {
    let mut value = pool_provider(
        provider_id,
        provider_id,
        base_url,
        vec![pool_key(
            &format!("{provider_id}-upstream-key"),
            "Upstream Key",
            "sk-upstream",
            true,
        )],
        vec![provider_mapping(local_model, upstream_model, protocol)],
    );
    value["protocol"] = json!(protocol);
    serde_json::from_value(value).expect("the typed provider fixture must deserialize")
}

/// The transport-only identity one prepared pool candidate carries.
fn candidate_identity(
    pool_id: &str,
    provider_id: &str,
    local_model: &str,
    upstream_model: &str,
    protocol: UpstreamProtocol,
) -> PoolCandidateIdentity {
    PoolCandidateIdentity {
        pool_id: pool_id.to_string(),
        provider_id: provider_id.to_string(),
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        protocol,
        reasoning: None,
    }
}

/// The routing clone of a typed provider for one ranked candidate slot.
fn prepared_candidate(
    provider: &GatewayUpstreamProvider,
    identity: &PoolCandidateIdentity,
) -> GatewayUpstreamProvider {
    let mut prepared = provider.clone();
    prepared.pool_candidate = Some(identity.clone());
    prepared
}

/// A retained Chat caller history: one user turn plus the weather tool surface.
fn chat_caller(user_content: &str) -> CallerHistory {
    CallerHistory::parse(
        "/v1/chat/completions",
        &json!({
            "messages": [{"role": "user", "content": user_content}],
            "tools": weather_tools(),
        }),
    )
    .expect("a Chat history carrying a messages array must parse")
}

/// The matching tool continuation of [`chat_caller`]: the same history plus the
/// assistant tool call and its tool result for `call_id`, with no new user
/// message.
fn chat_continuation(user_content: &str, call_id: &str) -> CallerHistory {
    CallerHistory::parse(
        "/v1/chat/completions",
        &json!({
            "messages": [
                {"role": "user", "content": user_content},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"},
                }]},
                {"role": "tool", "tool_call_id": call_id, "content": "sunny"},
            ],
            "tools": weather_tools(),
        }),
    )
    .expect("a Chat tool continuation must parse")
}

/// One isolated hold-store fixture with a unique key, pool, alias and session,
/// so no process-global scheduler, cooldown or hold state can interfere.
struct IsolatedHold {
    key_id: String,
    pool_id: String,
    alias: String,
    held: GatewayUpstreamProvider,
    other: GatewayUpstreamProvider,
    identity: PoolCandidateIdentity,
    config: GatewayConfig,
    endpoint: String,
}

impl IsolatedHold {
    /// One scope of this fixture's key, pool, alias and ingress with its own
    /// session, so many independent entries can coexist in one store.
    fn scope(&self, index: usize) -> HoldScope {
        HoldScope::new(
            self.key_id.clone(),
            format!("{}#{index}", self.pool_id),
            None,
            self.pool_id.clone(),
            self.alias.clone(),
            UpstreamProtocol::ChatCompletions.endpoint_path(),
        )
    }

    /// Two ranked slots with the held candidate second, so a preference change
    /// is observable as a reorder of `ordered`.
    fn ranked(&self) -> Vec<GatewayUpstreamProvider> {
        vec![self.other.clone(), prepared_candidate(&self.held, &self.identity)]
    }
}

fn isolated_hold(name: &str) -> IsolatedHold {
    let tag = tag();
    let pool_id = format!("pool-{name}-{tag}");
    let alias = format!("free-{name}-{tag}");
    let provider_id = format!("p-{name}-{tag}");
    let local_model = format!("local-{name}-{tag}");
    let upstream_model = format!("remote-{name}-{tag}");
    let endpoint = format!("https://{name}-{tag}.test/v1");
    let held = typed_provider(
        &provider_id,
        &endpoint,
        &local_model,
        &upstream_model,
        "chat_completions",
    );
    let identity = candidate_identity(
        &pool_id,
        &provider_id,
        &local_model,
        &upstream_model,
        UpstreamProtocol::ChatCompletions,
    );
    let other = typed_provider(
        &format!("p-{name}-other-{tag}"),
        &format!("https://{name}-other-{tag}.test/v1"),
        &format!("local-{name}-other-{tag}"),
        &format!("remote-{name}-other-{tag}"),
        "chat_completions",
    );
    let config = GatewayConfig {
        providers: vec![held.clone(), other.clone()],
        ..Default::default()
    };
    IsolatedHold {
        key_id: format!("key-{name}-{tag}"),
        pool_id,
        alias,
        held,
        other,
        identity,
        config,
        endpoint,
    }
}

/// A completed hold idle for exactly the timeout stays eligible and preferred;
/// strictly beyond it the hold expires and the request follows normal routing.
#[tokio::test]
async fn isolated_hold_stays_eligible_at_the_idle_boundary_and_expires_beyond_it() {
    let fixture = isolated_hold("idle");
    let scope = fixture.scope(0);
    let retained = chat_caller("weather?");
    let call_id = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();

    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &retained,
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(store.entries.len(), 1, "the completed hold must be retained");
    assert!(
        store.total_bytes > 0,
        "the retained hold must account for its payload"
    );

    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &call_id),
        &mut ordered,
        t0 + HOLD_IDLE_TIMEOUT,
    );
    assert_eq!(
        ordered[0].id, fixture.held.id,
        "a hold idle for exactly the timeout must stay preferred"
    );
    assert_eq!(
        store.entries.len(),
        1,
        "a boundary-eligible hold must stay retained"
    );

    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &retained,
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &call_id),
        &mut ordered,
        t0 + HOLD_IDLE_TIMEOUT + Duration::from_millis(1),
    );
    assert_eq!(
        ordered[0].id, fixture.other.id,
        "a hold idle strictly beyond the timeout must not reorder the candidates"
    );
    assert!(
        store.entries.is_empty(),
        "an expired hold must be dropped"
    );
    assert_eq!(
        store.total_bytes, 0,
        "an expired hold must release its retained payload"
    );
}

/// Past the entry cap the least recently used completed entry is evicted and the
/// count stays bounded, while the newest entry stays reusable.
#[tokio::test]
async fn isolated_store_evicts_the_least_recently_used_entry_past_capacity() {
    let fixture = isolated_hold("capacity");
    let retained = chat_caller("weather?");
    let t0 = Instant::now();
    let mut store = HoldStore::default();
    let mut scopes = Vec::new();

    for index in 0..=HOLD_ENTRY_CAPACITY {
        let scope = fixture.scope(index);
        let now = t0 + Duration::from_millis(index as u64);
        let admission = store.admit(&scope, now);
        let call_id = format!("call-capacity-{index}");
        store.record(
            &scope,
            admission,
            &retained,
            &fixture.identity,
            &fixture.endpoint,
            std::slice::from_ref(&call_id),
            now,
        );
        scopes.push(scope);
    }

    assert_eq!(
        store.entries.len(),
        HOLD_ENTRY_CAPACITY,
        "the entry count must stay at the cap after one entry beyond it"
    );
    assert!(
        !store.entries.contains_key(&scopes[0]),
        "the least recently used scope must be evicted"
    );
    assert!(
        store.entries.contains_key(&scopes[1]),
        "only the least recently used scope may be evicted"
    );
    assert!(
        store.entries.contains_key(&scopes[HOLD_ENTRY_CAPACITY]),
        "the newest scope must stay retained"
    );

    let later = t0 + Duration::from_millis(HOLD_ENTRY_CAPACITY as u64 + 1);
    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scopes[0],
        &chat_continuation("weather?", "call-capacity-0"),
        &mut ordered,
        later,
    );
    assert_eq!(
        ordered[0].id, fixture.other.id,
        "an evicted scope must not be preferred"
    );

    let mut ordered = fixture.ranked();
    let newest = HOLD_ENTRY_CAPACITY;
    store.prefer(
        &fixture.config,
        &scopes[newest],
        &chat_continuation("weather?", &format!("call-capacity-{newest}")),
        &mut ordered,
        later,
    );
    assert_eq!(
        ordered[0].id, fixture.held.id,
        "the newest retained scope must stay reusable"
    );
}

/// A completion whose retained payload exceeds the per-entry budget is not
/// retained and removes the scope's stale entry.
#[tokio::test]
async fn isolated_store_drops_an_over_budget_entry_and_its_stale_predecessor() {
    let fixture = isolated_hold("over-budget");
    let scope = fixture.scope(0);
    let retained = chat_caller("weather?");
    let call_id = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();

    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &retained,
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(
        store.entries.len(),
        1,
        "the small completed hold must be retained"
    );
    assert!(store.total_bytes > 0, "the retained hold must be accounted");

    // The retained entry's payload beyond its user content is the fixed shape
    // overhead, so the same shape exactly one byte over the budget can be built.
    let small_content_len = "weather?".len();
    assert!(store.total_bytes > small_content_len);
    let overhead = store.total_bytes - small_content_len;
    let oversized = chat_caller(&"x".repeat(HOLD_ENTRY_BUDGET + 1 - overhead));
    store.record(
        &scope,
        admission,
        &oversized,
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0 + Duration::from_millis(1),
    );
    assert!(
        !store.entries.contains_key(&scope),
        "an over-budget completion must not be retained and must remove the scope's stale entry"
    );
    assert_eq!(
        store.total_bytes, 0,
        "the removed entry must release its retained payload"
    );
}

/// A completion whose retained payload is exactly the per-entry budget stays
/// retained, so the per-entry limit is exclusive above it.
#[tokio::test]
async fn isolated_store_retains_an_entry_exactly_at_the_per_entry_budget() {
    let fixture = isolated_hold("exact-entry");
    let scope = fixture.scope(0);
    let call_id = "call-exact".to_string();
    let t0 = Instant::now();

    // Observe the fixed shape overhead of one entry through the retained total.
    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &chat_caller("weather?"),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(store.entries.len(), 1, "the probe entry must be retained");
    let probe_len = "weather?".len();
    assert!(store.total_bytes > probe_len);
    let overhead = store.total_bytes - probe_len;

    let mut exact = HoldStore::default();
    let admission = exact.admit(&scope, t0);
    exact.record(
        &scope,
        admission,
        &chat_caller(&"x".repeat(HOLD_ENTRY_BUDGET - overhead)),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(
        exact.total_bytes, HOLD_ENTRY_BUDGET,
        "an entry exactly at the per-entry budget must be retained"
    );
    assert!(
        exact.entries.contains_key(&scope),
        "the boundary-compliant entry must stay retained"
    );
}

/// A retained total exactly at the table budget is kept: entries are evicted
/// only while the total is strictly above the budget.
#[tokio::test]
async fn isolated_store_retains_a_total_exactly_at_the_payload_budget() {
    let fixture = isolated_hold("exact-total");
    let call_id = "call-uniform".to_string();
    let t0 = Instant::now();
    let mut store = HoldStore::default();

    // Derive the fixed per-entry overhead from one observed entry, then fill
    // with uniform entries until one tuned entry can land exactly on the budget.
    let probe_len = 4_000usize;
    let probe = fixture.scope(0);
    let admission = store.admit(&probe, t0);
    store.record(
        &probe,
        admission,
        &chat_caller(&"x".repeat(probe_len)),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    let overhead = store.total_bytes - probe_len;
    assert!(overhead > 0, "the entry shape must have a fixed overhead");

    let uniform = 900_000usize;
    let mut index = 1usize;
    while HOLD_TOTAL_BUDGET - store.total_bytes > HOLD_ENTRY_BUDGET {
        let scope = fixture.scope(index);
        let now = t0 + Duration::from_millis(index as u64);
        let admission = store.admit(&scope, now);
        store.record(
            &scope,
            admission,
            &chat_caller(&"x".repeat(uniform)),
            &fixture.identity,
            &fixture.endpoint,
            std::slice::from_ref(&call_id),
            now,
        );
        index += 1;
    }

    let remaining = HOLD_TOTAL_BUDGET - store.total_bytes;
    assert!(
        remaining > overhead,
        "the remaining budget must cover the tuned entry's overhead, got {remaining}"
    );
    assert!(
        remaining <= HOLD_ENTRY_BUDGET,
        "the remaining budget must fit one entry, got {remaining}"
    );
    let tuned = fixture.scope(index);
    let now = t0 + Duration::from_secs(1);
    let admission = store.admit(&tuned, now);
    store.record(
        &tuned,
        admission,
        &chat_caller(&"x".repeat(remaining - overhead)),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        now,
    );

    assert_eq!(
        store.total_bytes, HOLD_TOTAL_BUDGET,
        "a retained total exactly at the table budget must be kept"
    );
    assert!(
        store.entries.contains_key(&probe),
        "no entry may be evicted at the exact table budget"
    );
    assert!(
        store.entries.contains_key(&tuned),
        "the tuned entry at the exact table budget must be retained"
    );
    assert_eq!(
        store.entries.len(),
        index + 1,
        "every entry inside the exact table budget must be retained"
    );
}

/// Cumulative entries beyond the total payload budget evict least recently used
/// entries until the budget holds, and the newest evidence stays reusable.
#[tokio::test]
async fn isolated_store_evicts_least_recently_used_entries_until_the_total_budget_holds() {
    let fixture = isolated_hold("total-budget");
    // Each entry is below the per-entry budget while their sum exceeds the whole
    // table budget, so the store must evict.
    let large = chat_caller(&"x".repeat(900_000));
    let t0 = Instant::now();
    let mut store = HoldStore::default();
    let mut scopes = Vec::new();

    for index in 0..19usize {
        let scope = fixture.scope(index);
        let now = t0 + Duration::from_millis(index as u64);
        let admission = store.admit(&scope, now);
        store.record(
            &scope,
            admission,
            &large,
            &fixture.identity,
            &fixture.endpoint,
            &[format!("call-large-{index}")],
            now,
        );
        scopes.push(scope);
    }

    assert!(
        store.total_bytes <= HOLD_TOTAL_BUDGET,
        "the retained total must stay inside the table budget, got {}",
        store.total_bytes
    );
    assert!(
        store.entries.len() < scopes.len(),
        "entries beyond the table budget must be evicted"
    );
    assert!(
        !store.entries.contains_key(&scopes[0]),
        "the least recently used entry must be evicted first"
    );

    let newest = fixture.scope(19);
    let now = t0 + Duration::from_secs(1);
    let newest_call = format!("call-newest-{}", fixture.pool_id);
    let admission = store.admit(&newest, now);
    store.record(
        &newest,
        admission,
        &chat_caller("weather?"),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&newest_call),
        now,
    );
    assert!(
        store.total_bytes <= HOLD_TOTAL_BUDGET,
        "an insert inside the budget must keep the total bounded, got {}",
        store.total_bytes
    );
    assert!(
        store.entries.contains_key(&newest),
        "the newest evidence must be retained"
    );

    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &newest,
        &chat_continuation("weather?", &newest_call),
        &mut ordered,
        now + Duration::from_millis(1),
    );
    assert_eq!(
        ordered[0].id, fixture.held.id,
        "the newest evidence must stay reusable"
    );
}

// ---------------------------------------------------------------------------
// Streamed out-of-order settlement and native Messages upstream streams
// ---------------------------------------------------------------------------

/// A streamed request that finishes last cannot replace the newer admitted
/// turn's state: the older stream is held open until the newer turn has
/// published its own candidate, then completes successfully with its own tool
/// call, and the next continuation still prefers the newer turn's candidate.
#[tokio::test]
async fn a_streamed_completion_that_finishes_last_cannot_replace_a_newer_turn() {
    let _home = temp_home("pool-stream-order-older-success");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-order-{tag}");
    let session = format!("sess-stream-order-{tag}");
    let older_call = format!("call_older_{tag}");
    let arrived_a = Arc::new(tokio::sync::Notify::new());
    let release_a = Arc::new(tokio::sync::Notify::new());
    let older_chunks = chat_sse_tool_call_chunks(&older_call);
    let (url_a, log_a) = spawn_scripted_upstream({
        let arrived = arrived_a.clone();
        let release = release_a.clone();
        move |_index, _captured| ScriptedReply::HoldThenRelease {
            arrived: arrived.clone(),
            release: release.clone(),
            chunks: older_chunks.clone(),
        }
    })
    .await;
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-order-{tag}"),
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

    let path = "/v1/chat/completions";
    let older_body = json!({
        "model": alias,
        "messages": [{"role": "user", "content": "order?"}],
        "tools": weather_tools(),
        "stream": true,
    });
    let newer_body = older_body.clone();

    let older = {
        let session = session.clone();
        tokio::spawn(async move {
            post_json(port, path, "Bearer local-key", Some(&session), None, older_body).await
        })
    };
    tokio::time::timeout(Duration::from_secs(10), arrived_a.notified())
        .await
        .expect("the older request must reach the first candidate and be held");

    let (status, newer_text) =
        post_json(port, path, "Bearer local-key", Some(&session), None, newer_body).await;
    assert_eq!(status, 200, "the newer turn must complete: {newer_text}");
    let newer_call = client_chat_tool_ids(&newer_text)
        .into_iter()
        .next()
        .expect("the newer turn must stream a tool call");
    assert!(
        with_results(&log_b, 0).is_some(),
        "the newer turn must be served by the rotated candidate"
    );

    release_a.notify_one();
    let (status, older_text) = older.await.expect("older task");
    assert_eq!(
        status, 200,
        "the held older stream must still complete successfully: {older_text}"
    );
    assert!(
        client_chat_tool_ids(&older_text)
            .iter()
            .any(|id| id == &older_call),
        "the older stream must expose its own tool call: {older_text}"
    );

    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "order?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": newer_call,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": newer_call, "content": "sunny"},
            ],
            "tools": weather_tools(),
            "stream": true,
        }),
    )
    .await;
    assert_eq!(status, 200, "the continuation must be served: {text}");
    assert!(
        with_results(&log_b, 1).is_some() && with_results(&log_a, 1).is_none(),
        "the last-finishing older stream must not replace the newer turn's preference"
    );

    stop_server().await.expect("stop the listener");
}

/// A streamed request that finishes last as a failure cannot replace or
/// resurrect the newer admitted turn's state: the newer turn publishes its own
/// candidate and the older 400 does not remove it.
#[tokio::test]
async fn a_streamed_failure_that_finishes_last_cannot_replace_a_newer_turn() {
    let _home = temp_home("pool-stream-order-older-failure");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-order-fail-{tag}");
    let session = format!("sess-stream-order-fail-{tag}");
    let arrived_a = Arc::new(tokio::sync::Notify::new());
    let release_a = Arc::new(tokio::sync::Notify::new());
    let (url_a, log_a) = spawn_mock_upstream({
        let arrived = arrived_a.clone();
        let release = release_a.clone();
        move |_| {
            MockReply::Withhold(
                arrived.clone(),
                release.clone(),
                400,
                json!({"error": {"message": "invalid request"}}),
            )
        }
    })
    .await;
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-order-fail-{tag}"),
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

    let path = "/v1/chat/completions";
    let older_body = json!({
        "model": alias,
        "messages": [{"role": "user", "content": "order?"}],
        "tools": weather_tools(),
        "stream": true,
    });
    let newer_body = older_body.clone();

    let older = {
        let session = session.clone();
        tokio::spawn(async move {
            post_json(port, path, "Bearer local-key", Some(&session), None, older_body).await
        })
    };
    tokio::time::timeout(Duration::from_secs(10), arrived_a.notified())
        .await
        .expect("the older request must reach the first candidate and be held");

    let (status, newer_text) =
        post_json(port, path, "Bearer local-key", Some(&session), None, newer_body).await;
    assert_eq!(status, 200, "the newer turn must complete: {newer_text}");
    let newer_call = client_chat_tool_ids(&newer_text)
        .into_iter()
        .next()
        .expect("the newer turn must stream a tool call");

    release_a.notify_one();
    let (status, older_text) = older.await.expect("older task");
    assert_eq!(
        status, 400,
        "the older request's terminal client error must pass through: {older_text}"
    );

    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "order?"},
                {"role": "assistant", "content": null, "tool_calls": [{
                    "id": newer_call,
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{}"}
                }]},
                {"role": "tool", "tool_call_id": newer_call, "content": "sunny"},
            ],
            "tools": weather_tools(),
            "stream": true,
        }),
    )
    .await;
    assert_eq!(status, 200, "the continuation must be served: {text}");
    assert!(
        with_results(&log_b, 1).is_some() && with_results(&log_a, 1).is_none(),
        "an older failure that finishes last must not resurrect or replace the newer hold"
    );

    stop_server().await.expect("stop the listener");
}

/// A native Anthropic Messages SSE tool-use stream that completes, carrying the
/// given tool-use id.
fn anthropic_sse_tool_call(call_id: &str) -> String {
    let event = |kind: &str, value: Value| format!("event: {kind}\ndata: {value}\n\n");
    let mut stream = String::new();
    stream.push_str(&event(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": format!("msg-{call_id}"),
                "role": "assistant",
                "model": "upstream-anthropic",
                "content": [],
                "usage": {"input_tokens": 1, "output_tokens": 0}
            }
        }),
    ));
    stream.push_str(&event(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {
                "type": "tool_use",
                "id": call_id,
                "name": "get_weather",
                "input": {}
            }
        }),
    ));
    stream.push_str(&event(
        "content_block_delta",
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "input_json_delta", "partial_json": "{\"city\":\"SF\"}"}
        }),
    ));
    stream.push_str(&event(
        "content_block_stop",
        json!({"type": "content_block_stop", "index": 0}),
    ));
    stream.push_str(&event(
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": {"stop_reason": "tool_use"},
            "usage": {"output_tokens": 5}
        }),
    ));
    stream.push_str(&event("message_stop", json!({"type": "message_stop"})));
    stream
}

/// A Chat ingress stream served by a native Messages upstream keeps its
/// continuity: the continuation prefers the candidate that actually delivered
/// the converted tool call, with a client-visible Chat tool identity and the
/// Chat completion framing.
#[tokio::test]
async fn chat_stream_through_a_native_messages_upstream_prefers_the_delivering_candidate() {
    let _home = temp_home("pool-stream-anthropic-chain");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-anthropic-{tag}");
    let session = format!("sess-stream-anthropic-{tag}");
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_mock_upstream(move |_| {
        let n = counter_a.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(anthropic_sse_tool_call(&format!("toolu_a_{n}")))
    })
    .await;
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(anthropic_sse_tool_call(&format!("toolu_b_{n}")))
    })
    .await;

    let mut provider_a_value = pool_provider(
        &provider_a,
        "Provider A",
        &url_a,
        vec![pool_key("key-a", "Key A", "sk-a", true)],
        vec![provider_mapping("local-a", "remote-a", "anthropic_messages")],
    );
    provider_a_value["protocol"] = json!("anthropic_messages");
    let mut provider_b_value = pool_provider(
        &provider_b,
        "Provider B",
        &url_b,
        vec![pool_key("key-b", "Key B", "sk-b", true)],
        vec![provider_mapping("local-b", "remote-b", "anthropic_messages")],
    );
    provider_b_value["protocol"] = json!("anthropic_messages");
    let pool = json!({
        "id": format!("pool-stream-anthropic-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "anthropic_messages", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "anthropic_messages", Some(1)),
        ],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let path = "/v1/chat/completions";
    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    let mut call_ids: Vec<String> = Vec::new();
    for round in 0..3 {
        let body = json!({
            "model": alias,
            "messages": messages.clone(),
            "tools": weather_tools(),
            "stream": true,
        });
        let (status, text) =
            post_json(port, path, "Bearer local-key", Some(&session), None, body).await;
        assert_eq!(status, 200, "round {round} must be served: {text}");
        assert!(
            text.contains(&format!("\"model\":\"{alias}\"")),
            "round {round} must keep the public alias on the converted stream: {text}"
        );
        assert!(
            client_stream_completed(path, &text),
            "round {round} must end with the Chat completion marker: {text}"
        );
        assert!(
            text.contains("\"finish_reason\":\"tool_calls\""),
            "round {round} must convert the tool-use stop into the Chat finish reason: {text}"
        );
        let call_id = client_chat_tool_ids(&text)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("round {round} must expose a converted tool call: {text}"));
        call_ids.push(call_id.clone());
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [{
            "id": call_id,
            "type": "function",
            "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
        }]}));
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));
    }

    for (round, call_id) in call_ids.iter().take(2).enumerate() {
        let served_a =
            find_request(&log_a, |captured| {
                String::from_utf8_lossy(&captured.body).contains(call_id)
            });
        assert!(
            served_a.is_some(),
            "continuation {} must attempt the candidate that delivered {call_id}",
            round + 1
        );
        assert!(
            find_request(&log_b, |captured| {
                String::from_utf8_lossy(&captured.body).contains(call_id)
            })
            .is_none(),
            "continuation {} must not rotate to the sibling",
            round + 1
        );
    }

    let continuation = find_request(&log_a, |captured| {
        String::from_utf8_lossy(&captured.body).contains(&call_ids[0])
    })
    .expect("the first continuation must reach the delivering candidate");
    let forwarded = String::from_utf8_lossy(&continuation.body);
    assert!(
        forwarded.contains(&call_ids[0]) && forwarded.contains("\"tool_use\""),
        "the continuation must be forwarded in the native Messages shape: {forwarded}"
    );

    stop_server().await.expect("stop the listener");
}

/// A Chat SSE stream whose only tool-call-like identifiers appear in a comment,
/// a metadata event and a usage-only event, never in the caller protocol's
/// delta or completion shapes.
fn chat_sse_tool_like_metadata(comment_id: &str, metadata_id: &str, usage_id: &str) -> String {
    let metadata = json!({
        "id": "chatcmpl-tool-like",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [],
        "metadata": {
            "provider": "mock",
            "tool_calls": [{
                "id": metadata_id,
                "type": "function",
                "function": {"name": "get_weather", "arguments": "{}"}
            }]
        }
    });
    let usage = json!({
        "id": "chatcmpl-tool-like",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
        "tool_calls": [{"id": usage_id}]
    });
    let text = json!({
        "id": "chatcmpl-tool-like",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{"index": 0, "delta": {"role": "assistant", "content": "ok"}, "finish_reason": Value::Null}]
    });
    let stop = json!({
        "id": "chatcmpl-tool-like",
        "object": "chat.completion.chunk",
        "model": "upstream-stream",
        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
    });
    format!(
        ": keep-alive tool_calls {comment_id}\n\nevent: response.metadata\ndata: {metadata}\n\ndata: {usage}\n\ndata: {text}\n\ndata: {stop}\n\ndata: [DONE]\n\n"
    )
}

/// Tool-call-like identifiers that appear only in comment, metadata and
/// usage-only events create no tool evidence: the stream completes without a
/// caller tool call and a later matching continuation follows the ordinary
/// rotation instead of being held on that candidate.
#[tokio::test]
async fn tool_like_identifiers_in_metadata_and_usage_events_create_no_hold() {
    let _home = temp_home("pool-stream-tool-like-metadata");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-tool-like-{tag}");
    let session = format!("sess-stream-tool-like-{tag}");
    let comment_id = format!("call_comment_{tag}");
    let metadata_id = format!("call_metadata_{tag}");
    let usage_id = format!("call_usage_{tag}");
    let (url_a, log_a) = spawn_mock_upstream({
        let comment_id = comment_id.clone();
        let metadata_id = metadata_id.clone();
        let usage_id = usage_id.clone();
        move |_| {
            MockReply::Stream(chat_sse_tool_like_metadata(
                &comment_id,
                &metadata_id,
                &usage_id,
            ))
        }
    })
    .await;
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-tool-like-{tag}"),
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

    let path = "/v1/chat/completions";
    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "tools": weather_tools(),
            "stream": true,
        }),
    )
    .await;
    assert_eq!(status, 200, "the metadata-only stream must complete: {text}");
    assert!(
        client_stream_completed(path, &text),
        "the metadata-only stream must end with [DONE]: {text}"
    );
    assert!(
        client_chat_tool_ids(&text).is_empty(),
        "tool-like identifiers outside the caller tool shapes must not surface a tool call: {text}"
    );
    assert!(
        with_results(&log_a, 0).is_some(),
        "the metadata-only stream must be served by the first candidate"
    );

    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": null, "tool_calls": [
                    {"id": comment_id.clone(), "type": "function", "function": {"name": "get_weather", "arguments": "{}"}},
                    {"id": metadata_id.clone(), "type": "function", "function": {"name": "get_weather", "arguments": "{}"}},
                    {"id": usage_id.clone(), "type": "function", "function": {"name": "get_weather", "arguments": "{}"}},
                ]},
                {"role": "tool", "tool_call_id": comment_id, "content": "sunny"},
                {"role": "tool", "tool_call_id": metadata_id, "content": "sunny"},
                {"role": "tool", "tool_call_id": usage_id, "content": "sunny"},
            ],
            "tools": weather_tools(),
            "stream": true,
        }),
    )
    .await;
    assert_eq!(status, 200, "the would-be continuation must be served: {text}");
    assert!(
        find_request(&log_b, |captured| tool_result_count(captured) > 0).is_some()
            && find_request(&log_a, |captured| tool_result_count(captured) > 0).is_none(),
        "a metadata-only stream must not hold the next matching request on the first candidate"
    );

    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// Responses-to-Chat adapter continuity and continuity-invalidation guards
// ---------------------------------------------------------------------------

/// One Chat user message with plain text content.
fn user_message(content: &str) -> Value {
    json!({"role": "user", "content": content})
}

/// One Chat assistant message carrying a single weather function call.
fn assistant_call_message(call_id: &str) -> Value {
    json!({"role": "assistant", "content": null, "tool_calls": [{
        "id": call_id,
        "type": "function",
        "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"},
    }]})
}

/// One Chat tool result for a call identity.
fn tool_result_message(call_id: &str) -> Value {
    json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"})
}

/// A Chat caller history from explicit messages and an explicit tool surface.
fn chat_history_with_tools(messages: Vec<Value>, tools: Value) -> CallerHistory {
    CallerHistory::parse(
        "/v1/chat/completions",
        &json!({"messages": messages, "tools": tools}),
    )
    .expect("a Chat history carrying a messages array must parse")
}

/// The Responses-shaped tool surface a weather function call needs.
fn responses_tools() -> Value {
    json!([{
        "type": "function",
        "name": "get_weather",
        "description": "weather",
        "parameters": {
            "type": "object",
            "properties": {"city": {"type": "string"}},
            "required": ["city"]
        }
    }])
}

/// One Responses user input item.
fn responses_user_item(text: &str) -> Value {
    json!({"role": "user", "content": [{"type": "input_text", "text": text}]})
}

/// One Responses `function_call` input item for a call identity.
fn responses_function_call_item(call_id: &str) -> Value {
    json!({
        "type": "function_call",
        "call_id": call_id,
        "name": "get_weather",
        "arguments": "{\"city\":\"SF\"}",
    })
}

/// One Responses `function_call_output` input item for a call identity.
fn responses_function_call_output_item(call_id: &str) -> Value {
    json!({"type": "function_call_output", "call_id": call_id, "output": "sunny"})
}

/// A Responses caller history from explicit input items, instructions and tools.
fn responses_history_with_context(
    input: Vec<Value>,
    instructions: Value,
    tools: Value,
) -> CallerHistory {
    CallerHistory::parse(
        "/v1/responses",
        &json!({"input": input, "instructions": instructions, "tools": tools}),
    )
    .expect("a Responses history carrying an input array must parse")
}

/// A streamed Responses function-call chain whose candidates speak Chat
/// Completions keeps its continuity through the Responses-to-Chat adapter: each
/// continuation prefers the candidate that actually delivered the converted
/// tool call, the client-visible Responses call identity stays stable, and the
/// converted stream still ends with the Responses completion framing.
#[tokio::test]
async fn responses_stream_through_a_chat_upstream_prefers_the_delivering_candidate() {
    let _home = temp_home("pool-stream-responses-to-chat");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-stream-r2c-{tag}");
    let session = format!("sess-stream-r2c-{tag}");
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_mock_upstream(move |_| {
        let n = counter_a.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_a_{n}")))
    })
    .await;
    let (url_b, log_b) = spawn_mock_upstream(move |_| {
        let n = counter_b.fetch_add(1, Ordering::SeqCst) + 1;
        MockReply::Stream(chat_sse_tool_call(&format!("call_b_{n}")))
    })
    .await;

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
        "id": format!("pool-stream-r2c-{tag}"),
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

    let path = "/v1/responses";
    let mut input = vec![responses_user_item("weather?")];
    let mut call_ids: Vec<String> = Vec::new();
    for round in 0..3 {
        let body = json!({
            "model": alias,
            "input": input.clone(),
            "tools": responses_tools(),
            "stream": true,
        });
        let (status, text) =
            post_json(port, path, "Bearer local-key", Some(&session), None, body).await;
        assert_eq!(status, 200, "round {round} must be served: {text}");
        assert!(
            text.contains(&format!("\"model\":\"{alias}\"")),
            "round {round} must keep the public alias on the converted stream: {text}"
        );
        assert!(
            client_stream_completed(path, &text),
            "round {round} must complete with response.completed: {text}"
        );
        let ids = client_responses_call_ids(&text);
        let call_id = ids.first().cloned().unwrap_or_else(|| {
            panic!("round {round} must expose a converted Responses function call: {text}")
        });
        call_ids.push(call_id.clone());
        input.push(responses_function_call_item(&call_id));
        input.push(responses_function_call_output_item(&call_id));
    }

    for results in 1..=2usize {
        assert!(
            with_results(&log_a, results).is_some(),
            "converted continuation {results} must attempt the delivering candidate"
        );
        assert!(
            with_results(&log_b, results).is_none(),
            "converted continuation {results} must not rotate to the sibling"
        );
    }
    let continuation = with_results(&log_a, 1)
        .expect("the first converted continuation must reach the delivering candidate");
    assert!(
        String::from_utf8_lossy(&continuation.body).contains(&call_ids[0]),
        "the forwarded continuation must retain the client-visible Responses call identity"
    );

    stop_server().await.expect("stop the listener");
}

/// A would-be continuation whose history no longer preserves the retained
/// messages structurally — an earlier retained message rewritten or removed —
/// follows the ordinary weighted rotation and cannot inherit the hold.
#[tokio::test]
async fn a_changed_or_truncated_retained_prefix_does_not_inherit_the_hold() {
    let fixture = isolated_hold("prefix");
    let retained_call = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();
    let retained = chat_history_with_tools(
        vec![user_message("keep?"), user_message("weather?")],
        weather_tools(),
    );
    let rewritten = chat_history_with_tools(
        vec![
            user_message("changed?"),
            assistant_call_message(&retained_call),
            tool_result_message(&retained_call),
        ],
        weather_tools(),
    );
    let truncated = chat_history_with_tools(
        vec![
            user_message("weather?"),
            assistant_call_message(&retained_call),
            tool_result_message(&retained_call),
        ],
        weather_tools(),
    );

    for (label, caller) in [("rewritten", &rewritten), ("truncated", &truncated)] {
        let scope = fixture.scope(0);
        let mut store = HoldStore::default();
        let admission = store.admit(&scope, t0);
        store.record(
            &scope,
            admission,
            &retained,
            &fixture.identity,
            &fixture.endpoint,
            std::slice::from_ref(&retained_call),
            t0,
        );
        let mut ordered = fixture.ranked();
        store.prefer(
            &fixture.config,
            &scope,
            caller,
            &mut ordered,
            t0 + Duration::from_secs(1),
        );
        assert_eq!(
            ordered[0].id, fixture.other.id,
            "a {label} retained prefix must not inherit the hold"
        );
    }
}

/// The same retained history with a changed instruction or tool context follows
/// the ordinary weighted rotation and cannot inherit the hold, for both ingress
/// protocols that carry that context.
#[tokio::test]
async fn a_changed_instruction_or_tool_context_does_not_inherit_the_hold() {
    let fixture = isolated_hold("context");
    let retained_call = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();

    let retained_chat = chat_history_with_tools(vec![user_message("weather?")], weather_tools());
    let changed_tools = chat_history_with_tools(
        vec![
            user_message("weather?"),
            assistant_call_message(&retained_call),
            tool_result_message(&retained_call),
        ],
        json!([{"type": "function", "function": {"name": "get_time"}}]),
    );

    let retained_responses = responses_history_with_context(
        vec![responses_user_item("weather?")],
        json!("Be brief."),
        responses_tools(),
    );
    let changed_instructions = responses_history_with_context(
        vec![
            responses_user_item("weather?"),
            responses_function_call_item(&retained_call),
            responses_function_call_output_item(&retained_call),
        ],
        json!("Be verbose."),
        responses_tools(),
    );

    let cases: [(&str, &CallerHistory, &CallerHistory); 2] = [
        ("changed tool", &retained_chat, &changed_tools),
        (
            "changed instruction",
            &retained_responses,
            &changed_instructions,
        ),
    ];
    for (label, retained, caller) in cases {
        let scope = fixture.scope(0);
        let mut store = HoldStore::default();
        let admission = store.admit(&scope, t0);
        store.record(
            &scope,
            admission,
            retained,
            &fixture.identity,
            &fixture.endpoint,
            std::slice::from_ref(&retained_call),
            t0,
        );
        let mut ordered = fixture.ranked();
        store.prefer(
            &fixture.config,
            &scope,
            caller,
            &mut ordered,
            t0 + Duration::from_secs(1),
        );
        assert_eq!(
            ordered[0].id, fixture.other.id,
            "a {label} context must not inherit the hold"
        );
    }
}

/// A tool result referencing a call identity the preceding response never
/// reported follows the ordinary weighted rotation and cannot inherit the hold.
#[tokio::test]
async fn an_unknown_call_identity_does_not_inherit_the_hold() {
    let fixture = isolated_hold("unknown-call");
    let scope = fixture.scope(0);
    let retained_call = format!("call-known-{}", fixture.pool_id);
    let unknown_call = format!("call-unknown-{}", fixture.pool_id);
    let t0 = Instant::now();
    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &chat_caller("weather?"),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&retained_call),
        t0,
    );

    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &unknown_call),
        &mut ordered,
        t0 + Duration::from_secs(1),
    );
    assert_eq!(
        ordered[0].id, fixture.other.id,
        "a result for an unreported call identity must not inherit the hold"
    );
}

/// A held candidate whose mapping is disabled is not preferred; the request
/// keeps the ordinary ranked order and never escapes the pool.
#[tokio::test]
async fn a_disabled_mapping_is_not_preferred() {
    let mut fixture = isolated_hold("disabled-mapping");
    let scope = fixture.scope(0);
    let call_id = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();
    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &chat_caller("weather?"),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(store.entries.len(), 1, "the completed hold must be retained");

    // The held candidate's mapping row is no longer enabled.
    fixture.config.providers[0].mappings[0].enabled = false;

    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &call_id),
        &mut ordered,
        t0 + Duration::from_secs(1),
    );
    assert_eq!(
        ordered[0].id, fixture.other.id,
        "a held candidate with a disabled mapping must not be preferred"
    );
    assert_eq!(
        ordered[1].id, fixture.held.id,
        "the ineligible held candidate must keep its relative position"
    );
}

/// A held candidate whose provider has no usable enabled key is not preferred;
/// the remaining healthy candidate serves and the pool never escapes.
#[tokio::test]
async fn a_candidate_without_a_usable_enabled_key_is_not_preferred() {
    let mut fixture = isolated_hold("no-usable-key");
    let scope = fixture.scope(0);
    let call_id = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();
    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &chat_caller("weather?"),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(store.entries.len(), 1, "the completed hold must be retained");

    // The held candidate's key pool carries no usable enabled key.
    fixture.config.providers[0].keys.clear();

    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &call_id),
        &mut ordered,
        t0 + Duration::from_secs(1),
    );
    assert_eq!(
        ordered[0].id, fixture.other.id,
        "a held candidate without a usable enabled key must not be preferred"
    );
}

/// A held candidate that is cooling is not preferred; the remaining healthy
/// candidate serves and the pool never escapes.
#[tokio::test]
async fn a_cooling_candidate_is_not_preferred() {
    let fixture = isolated_hold("cooling");
    let scope = fixture.scope(0);
    let call_id = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();
    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &chat_caller("weather?"),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(store.entries.len(), 1, "the completed hold must be retained");

    // Cool the held candidate well beyond the assertion's real-time window.
    cool_pool_candidate(&fixture.identity, Some(Duration::from_secs(60)));

    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &call_id),
        &mut ordered,
        t0 + Duration::from_secs(1),
    );
    assert_eq!(
        ordered[0].id, fixture.other.id,
        "a cooling held candidate must not be preferred"
    );
}

/// A held candidate whose resolved source endpoint changed is not preferred:
/// the preference is invalidated by the changed source mapping.
#[tokio::test]
async fn a_changed_source_endpoint_is_not_preferred() {
    let mut fixture = isolated_hold("changed-endpoint");
    let scope = fixture.scope(0);
    let call_id = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();
    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &chat_caller("weather?"),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(store.entries.len(), 1, "the completed hold must be retained");

    // The held source now resolves to a different endpoint.
    fixture.config.providers[0].base_url = format!("https://changed-{}.test/v1", fixture.pool_id);

    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &call_id),
        &mut ordered,
        t0 + Duration::from_secs(1),
    );
    assert_eq!(
        ordered[0].id, fixture.other.id,
        "a held candidate whose source endpoint changed must not be preferred"
    );
}

/// A held candidate whose mapping resolves to a different effective protocol is
/// not preferred: the changed source mapping invalidates the preference.
#[tokio::test]
async fn a_changed_source_protocol_is_not_preferred() {
    let mut fixture = isolated_hold("changed-protocol");
    let scope = fixture.scope(0);
    let call_id = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();
    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &chat_caller("weather?"),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(store.entries.len(), 1, "the completed hold must be retained");

    // The held source mapping now resolves to a different upstream protocol.
    fixture.config.providers[0].mappings[0].protocol = Some(UpstreamProtocol::Responses);

    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &call_id),
        &mut ordered,
        t0 + Duration::from_secs(1),
    );
    assert_eq!(
        ordered[0].id, fixture.other.id,
        "a held candidate whose source protocol changed must not be preferred"
    );
}

/// A matching continuation attempt whose outcome is never delivered back to the
/// store must not extend a completed hold's idle lifetime: a hold last
/// successfully continued at `t0` is gone once more than 30 minutes have passed
/// without another successful continuation, even if a matching attempt was made
/// in between.
#[tokio::test]
async fn a_matching_attempt_without_a_delivered_outcome_does_not_extend_the_idle_lifetime() {
    let fixture = isolated_hold("failed-refresh");
    let scope = fixture.scope(0);
    let call_id = format!("call-{}", fixture.pool_id);
    let t0 = Instant::now();
    let mut store = HoldStore::default();
    let admission = store.admit(&scope, t0);
    store.record(
        &scope,
        admission,
        &chat_caller("weather?"),
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        t0,
    );
    assert_eq!(store.entries.len(), 1, "the completed hold must be retained");

    // A matching continuation is attempted 20 minutes after the last successful
    // one, but its outcome is never delivered here: no completed hold is
    // published or refreshed by that attempt.
    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &call_id),
        &mut ordered,
        t0 + Duration::from_secs(20 * 60),
    );
    assert_eq!(
        ordered[0].id, fixture.held.id,
        "the matching attempt must still be preferred while the hold is live"
    );

    // 31 minutes after the last successful continuation the hold must be gone,
    // so the same matching request observes the ordinary ranked order.
    let mut ordered = fixture.ranked();
    store.prefer(
        &fixture.config,
        &scope,
        &chat_continuation("weather?", &call_id),
        &mut ordered,
        t0 + Duration::from_secs(31 * 60),
    );
    assert_eq!(
        ordered[0].id, fixture.other.id,
        "an attempt whose outcome was never delivered must not refresh the idle lifetime"
    );
    assert!(
        store.entries.is_empty(),
        "a hold idle for more than 30 minutes without a successful continuation must be gone"
    );
}

/// AC-002: retaining one delivered continuation must not deep-copy the caller
/// history twice. Parsing the request history and then storing the hold may
/// each need the value once, but a redundant clone is unnecessary work that
/// grows with the conversation.
///
/// The test-only counter increments for each deep `Value` copy of the history
/// in `CallerHistory::parse` and in `HoldStore::record`, never for shared
/// references.
#[test]
fn recording_a_delivered_hold_copies_the_history_at_most_once() {
    let fixture = isolated_hold("history-copy");
    let scope = fixture.scope(0);
    let messages: Vec<Value> = (0..2_000)
        .map(|index| json!({"role": "user", "content": format!("filler-{index}")}))
        .collect();
    let body = json!({"messages": messages, "tools": weather_tools()});

    crate::ai_gateway::pool_context_hold::reset_history_deep_copy_count();
    let caller = CallerHistory::parse("/v1/chat/completions", &body)
        .expect("a Chat history carrying a messages array must parse");
    let now = Instant::now();
    let mut store = HoldStore::default();
    let admission = store.admit(&scope, now);
    let call_id = format!("call-{}", fixture.pool_id);
    store.record(
        &scope,
        admission,
        &caller,
        &fixture.identity,
        &fixture.endpoint,
        std::slice::from_ref(&call_id),
        now,
    );
    assert_eq!(
        store.entries.len(),
        1,
        "the delivered tool call must retain one hold"
    );

    let copies = crate::ai_gateway::pool_context_hold::history_deep_copy_count();
    assert!(
        copies <= 1,
        "retaining one delivered hold must copy the history at most once; observed {copies}"
    );
}

// ---------------------------------------------------------------------------
// Boundary B RED: the configured fallback and the same-turn continuation
// preference (AC-004 / AC-002)
// ---------------------------------------------------------------------------

/// AC-004 / AC-002: a recognized tool continuation whose regular candidates
/// are all excluded is served by the configured fallback, and a fallback
/// delivery never becomes the preferred candidate for a later continuation:
/// once a regular candidate is eligible again it has priority and the
/// fallback is not contacted.
#[tokio::test]
async fn pool_fallback_serves_a_continuation_without_becoming_its_preference() {
    let _home = temp_home("pool-context-hold-fallback-stage");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let provider_f = format!("pf-{tag}");
    let alias = format!("free-fallback-{tag}");
    let session = format!("sess-fallback-{tag}");
    let pool_id = format!("pool-fallback-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_b, log_b) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_f, log_f) = spawn_chat_tool_upstream(next.clone()).await;

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
    let provider_f_value = pool_provider(
        &provider_f,
        "Provider Fallback",
        &url_f,
        vec![pool_key("key-f", "Key F", "sk-f", true)],
        vec![provider_mapping("local-f", "remote-f", "chat_completions")],
    );
    let pool = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
        "fallback_models": ["local-f"],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value, provider_f_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    for round in 0..3usize {
        if round == 1 {
            // Both regular candidates are excluded, so only the configured
            // fallback can serve this recognized continuation.
            cool_pool_candidate(
                &PoolCandidateIdentity {
                    pool_id: pool_id.clone(),
                    provider_id: provider_a.clone(),
                    local_model: "local-a".to_string(),
                    upstream_model: "remote-a".to_string(),
                    protocol: UpstreamProtocol::ChatCompletions,
                    reasoning: None,
                },
                Some(Duration::from_secs(2)),
            );
            cool_pool_candidate(
                &PoolCandidateIdentity {
                    pool_id: pool_id.clone(),
                    provider_id: provider_b.clone(),
                    local_model: "local-b".to_string(),
                    upstream_model: "remote-b".to_string(),
                    protocol: UpstreamProtocol::ChatCompletions,
                    reasoning: None,
                },
                Some(Duration::from_secs(2)),
            );
        }
        if round == 2 {
            // The regular candidates are eligible again, so an eligible
            // regular candidate has priority over any fallback preference.
            tokio::time::sleep(Duration::from_millis(2500)).await;
        }
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
            "round {round} must be served by a pool source: {text}"
        );
        let value = parse(&text);
        assert_eq!(value["model"], alias, "the client must see the public alias");
        let call = chat_call(&value);
        let call_id = call["id"]
            .as_str()
            .expect("the response must carry a call id")
            .to_string();
        messages.push(json!({"role": "assistant", "content": null, "tool_calls": [call]}));
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));
    }

    // The excluded continuation was served by the configured fallback.
    let fallback_round_one = with_results(&log_f, 1).map(|captured| forwarded_model(&captured));
    assert_eq!(
        fallback_round_one.as_deref(),
        Some("remote-f"),
        "the excluded continuation must be served by the fallback"
    );
    assert!(
        with_results(&log_a, 1).is_none() && with_results(&log_b, 1).is_none(),
        "an excluded regular candidate must not serve the continuation"
    );
    // The fallback never becomes the preferred source of a later continuation.
    assert!(
        with_results(&log_f, 2).is_none(),
        "the fallback must not be preferred after a fallback delivery"
    );
    assert!(
        with_results(&log_a, 2).is_some() || with_results(&log_b, 2).is_some(),
        "an eligible regular candidate must have priority after the recovery"
    );

    stop_server().await.expect("stop the listener");
}

/// One Responses success body without a tool call.
fn responses_success_body() -> Value {
    json!({
        "id": "resp_1",
        "object": "response",
        "model": "remote",
        "status": "completed",
        "output": [{"id": "m1", "type": "message", "role": "assistant", "status": "completed",
            "content": [{"type": "output_text", "text": "ok", "annotations": []}]}],
        "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2},
    })
}

/// Send one request over a raw socket and hand the open connection back, so
/// the caller can drop it while the upstream attempt is still in flight.
async fn raw_request(port: u16, path: &str, session: &str, body: Value) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect gateway");
    let payload = serde_json::to_vec(&body).expect("encode body");
    let request = format!(
        "POST {path} HTTP/1.1\r\nhost: 127.0.0.1\r\ncontent-type: application/json\r\nauthorization: Bearer local-key\r\nx-opencode-session: {session}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        payload.len()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write headers");
    stream.write_all(&payload).await.expect("write body");
    stream.flush().await.expect("flush request");
    stream
}

/// AC-007 / REQ-004: a client that disconnects while the configured fallback
/// attempt is still in flight ends the request at once. The cancelled attempt
/// is never replayed, no terminal row is written, no mapping or key health is
/// published, the fallback stays eligible for the next request and no
/// continuation preference is published for the cancelled delivery.
#[tokio::test]
async fn pool_cancelled_fallback_attempt_publishes_no_row_no_health_and_no_hold() {
    let _home = temp_home("pool-fallback-cancelled-attempt");
    let port = free_port().await;
    let tag = tag();
    let provider_r = format!("pr-{tag}");
    let provider_f = format!("pf-{tag}");
    let alias = format!("free-fallback-cancel-{tag}");
    let session = format!("sess-fallback-cancel-{tag}");
    let pool_id = format!("pool-fallback-cancel-{tag}");
    let arrived = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));

    let (url_f, log_f) = {
        let arrived = arrived.clone();
        let release = release.clone();
        let calls = calls.clone();
        spawn_mock_upstream(move |_| {
            let index = calls.fetch_add(1, Ordering::SeqCst);
            if index == 0 {
                MockReply::Withhold(
                    arrived.clone(),
                    release.clone(),
                    200,
                    responses_success_body(),
                )
            } else {
                MockReply::Json(200, responses_success_body())
            }
        })
        .await
    };
    let (url_r, log_r) =
        spawn_mock_upstream(|_| MockReply::Json(200, responses_success_body())).await;

    let provider_r_value = pool_provider(
        &provider_r,
        "Regular Provider",
        &url_r,
        vec![pool_key("key-r", "Key R", "sk-r", true)],
        // The regular candidate's own mapping is added only after the
        // cancellation, so the fallback is the only eligible source while its
        // attempt is in flight.
        vec![provider_mapping("local-other", "remote-other", "responses")],
    );
    let provider_f_value = pool_provider(
        &provider_f,
        "Fallback Provider",
        &url_f,
        vec![pool_key("key-f", "Key F", "sk-f", true)],
        vec![provider_mapping("local-f", "remote-f", "responses")],
    );
    let pool = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_r, "local-r", "remote-r", "responses", Some(1)),
        ],
        "fallback_models": ["local-f"],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_r_value, provider_f_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let client = raw_request(
        port,
        "/v1/responses",
        &session,
        json!({
            "model": alias,
            "input": [responses_user_item("weather?")],
            "tools": responses_tools(),
        }),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), arrived.notified())
        .await
        .expect("the fallback attempt must begin");
    drop(client);
    // Let the held upstream answer, so the cancelled attempt cannot linger.
    release.notify_one();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(
        log_f.lock().expect("mock log").len(),
        1,
        "a cancelled fallback attempt must never be replayed: {}",
        log_f.lock().expect("mock log").len()
    );
    let records = default_usage_store().all_records().unwrap_or_default();
    assert!(
        records.is_empty(),
        "a cancelled fallback attempt must write no terminal row: {records:?}"
    );
    let stored = crate::ai_gateway::storage::read_config().expect("read the config");
    let fallback_provider = stored
        .providers
        .iter()
        .find(|provider| provider.id == provider_f)
        .expect("the fallback provider must persist");
    let fallback_mapping = fallback_provider
        .mappings
        .iter()
        .find(|mapping| mapping.local_model == "local-f")
        .expect("the fallback mapping must persist");
    assert!(
        !fallback_mapping.auto_disabled,
        "a cancellation must not disable the fallback's mapping row: {fallback_mapping:?}"
    );
    assert_eq!(
        fallback_mapping.consecutive_failures, 0,
        "a cancellation must not count a mapping failure: {fallback_mapping:?}"
    );
    assert!(
        !fallback_provider.keys[0].auto_marked,
        "a cancellation must not mark the fallback's key: {:?}",
        fallback_provider.keys[0]
    );
    let status_row = ai_gateway_model_pools_status()
        .expect("read the pool status")
        .into_iter()
        .find(|status| status.id == pool_id)
        .expect("the pool status must exist");
    let fallback_status = status_row
        .fallback_models
        .iter()
        .find(|status| status.model == "local-f")
        .expect("the pool must report its ordered fallback_models status");
    assert_eq!(
        fallback_status.model, "local-f",
        "the ordered fallback_models status must carry the trimmed model id: {fallback_status:?}"
    );
    assert!(
        fallback_status.ready,
        "a cancellation must not publish a false health state: {fallback_status:?}"
    );
    assert!(
        fallback_status.ready_sources >= 1,
        "a cancellation must not clear ready source counts: {fallback_status:?}"
    );
    assert!(
        fallback_status.total_sources >= 1,
        "a cancellation must not clear total source counts: {fallback_status:?}"
    );
    assert!(
        !fallback_status.sources.is_empty(),
        "a cancellation must not clear expanded sources: {fallback_status:?}"
    );

    // The regular candidate becomes eligible again, so the continuation is
    // served by the regular candidate. A hold published for the cancelled
    // fallback attempt would have preferred the fallback here.
    let mut recovered = read_raw_gateway_config();
    recovered["providers"]
        .as_array_mut()
        .expect("providers must persist")
        .iter_mut()
        .find(|provider| provider["id"] == provider_r)
        .expect("the regular provider must persist")["mappings"]
        .as_array_mut()
        .expect("mappings must persist")
        .push(provider_mapping("local-r", "remote-r", "responses"));
    write_raw_gateway_config(&recovered);

    let (status, text) = post_json(
        port,
        "/v1/responses",
        "Bearer local-key",
        Some(&session),
        None,
        json!({
            "model": alias,
            "input": [
                responses_user_item("weather?"),
                responses_function_call_item("call_cancelled"),
                responses_function_call_output_item("call_cancelled"),
            ],
            "tools": responses_tools(),
        }),
    )
    .await;
    stop_server().await.expect("stop the listener");

    assert_eq!(
        status, 200,
        "the continuation must be served by an eligible source: {text}"
    );
    assert_eq!(
        log_r.lock().expect("mock log").len(),
        1,
        "the eligible regular candidate must serve the continuation: {}",
        log_r.lock().expect("mock log").len()
    );
    assert_eq!(
        log_f.lock().expect("mock log").len(),
        1,
        "a cancelled fallback attempt must not publish a continuation preference: {}",
        log_f.lock().expect("mock log").len()
    );
}

// ---------------------------------------------------------------------------
// RED: stage identity must not be inferred from configuration overlap
// ---------------------------------------------------------------------------

/// A regular candidate whose exact local model is also reachable through the
/// stored `fallback_models` list must still publish a primary continuation
/// preference when it serves the request. Inferring the stage from the
/// configuration overlap would reclassify that regular success as a fallback
/// success and wrongly close the hold.
#[tokio::test]
async fn a_regular_success_overlapping_a_fallback_model_still_publishes_a_hold() {
    let _home = temp_home("pool-context-hold-overlap-regular");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let alias = format!("free-overlap-regular-{tag}");
    let session = format!("sess-overlap-regular-{tag}");
    let pool_id = format!("pool-overlap-regular-{tag}");
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
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
        // Overlap: local-a is also reachable through the fallback list.
        "fallback_models": ["local-a"],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let path = "/v1/chat/completions";
    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": messages.clone(), "tools": weather_tools()}),
    )
    .await;
    assert_eq!(status, 200, "round 0 must be served: {text}");
    let value = parse(&text);
    let call = chat_call(&value);
    let call_id = call["id"].as_str().expect("call id").to_string();
    messages.push(json!({"role": "assistant", "content": null, "tool_calls": [call]}));
    messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));

    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": messages.clone(), "tools": weather_tools()}),
    )
    .await;
    assert_eq!(status, 200, "round 1 must be served: {text}");

    // The regular candidate that served round 0 must also serve the recognized
    // continuation, because a regular success is not a fallback success even
    // when the same model is configured as a fallback.
    assert!(
        with_results(&log_a, 1).is_some(),
        "the overlapping regular candidate must remain preferred for the continuation"
    );
    assert!(
        with_results(&log_b, 1).is_none(),
        "the continuation must not rotate away from the overlapping regular candidate"
    );

    stop_server().await.expect("stop the listener");
}

/// When the regular stage is exhausted and the fallback stage serves through a
/// distinct fallback provider whose local model overlaps a regular candidate,
/// that fallback success closes any stale primary preference and publishes no
/// fallback preference. After the regular candidates recover, the next
/// continuation is served by the ordinary next regular candidate (B), not by
/// the stale primary (A) and not by the fallback (F).
#[tokio::test]
async fn a_fallback_success_overlapping_a_regular_model_closes_the_hold() {
    let _home = temp_home("pool-context-hold-overlap-fallback");
    let port = free_port().await;
    let tag = tag();
    let provider_a = format!("pa-{tag}");
    let provider_b = format!("pb-{tag}");
    let provider_f = format!("pf-{tag}");
    let alias = format!("free-overlap-fallback-{tag}");
    let session = format!("sess-overlap-fallback-{tag}");
    let pool_id = format!("pool-overlap-fallback-{tag}");
    let next = Arc::new(AtomicUsize::new(0));
    let (url_a, log_a) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_b, log_b) = spawn_chat_tool_upstream(next.clone()).await;
    let (url_f, log_f) = spawn_chat_tool_upstream(next.clone()).await;

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
    let provider_f_value = pool_provider(
        &provider_f,
        "Provider Fallback",
        &url_f,
        vec![pool_key("key-f", "Key F", "sk-f", true)],
        vec![provider_mapping("local-a", "remote-f", "chat_completions")],
    );
    let pool = json!({
        "id": pool_id,
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [
            pool_candidate(&provider_a, "local-a", "remote-a", "chat_completions", Some(1)),
            pool_candidate(&provider_b, "local-b", "remote-b", "chat_completions", Some(1)),
        ],
        "fallback_models": ["local-a"],
    });
    write_raw_gateway_config(&pool_config(
        port,
        single_key(),
        vec![provider_a_value, provider_b_value, provider_f_value],
        vec![pool],
    ));
    start_server(None).await.expect("start the listener");

    let path = "/v1/chat/completions";
    // Round 0: regular SWRR serves via A and publishes a hold on A.
    let mut messages = vec![json!({"role": "user", "content": "weather?"})];
    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": messages.clone(), "tools": weather_tools()}),
    )
    .await;
    assert_eq!(status, 200, "round 0 must be served: {text}");
    let value = parse(&text);
    assert_eq!(value["model"], alias, "the client must see the public alias");
    let call = chat_call(&value);
    let call_id = call["id"]
        .as_str()
        .expect("the response must carry a call id")
        .to_string();
    messages.push(json!({"role": "assistant", "content": null, "tool_calls": [call]}));
    messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));

    assert!(
        with_results(&log_a, 0).is_some(),
        "round 0 must be served by regular A"
    );
    assert_eq!(
        forwarded_model(&with_results(&log_a, 0).expect("checked above")),
        "remote-a"
    );

    // Cool exact identities A and B for about 2 seconds so the regular stage is
    // exhausted.
    cool_pool_candidate(
        &PoolCandidateIdentity {
            pool_id: pool_id.clone(),
            provider_id: provider_a.clone(),
            local_model: "local-a".to_string(),
            upstream_model: "remote-a".to_string(),
            protocol: UpstreamProtocol::ChatCompletions,
            reasoning: None,
        },
        Some(Duration::from_secs(2)),
    );
    cool_pool_candidate(
        &PoolCandidateIdentity {
            pool_id: pool_id.clone(),
            provider_id: provider_b.clone(),
            local_model: "local-b".to_string(),
            upstream_model: "remote-b".to_string(),
            protocol: UpstreamProtocol::ChatCompletions,
            reasoning: None,
        },
        Some(Duration::from_secs(2)),
    );

    // Round 1: fallback stage must serve through F.
    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": messages.clone(), "tools": weather_tools()}),
    )
    .await;
    assert_eq!(status, 200, "round 1 must be served by fallback: {text}");
    let value = parse(&text);
    assert_eq!(value["model"], alias, "the client must see the public alias");
    let call = chat_call(&value);
    let call_id = call["id"]
        .as_str()
        .expect("the response must carry a call id")
        .to_string();
    messages.push(json!({"role": "assistant", "content": null, "tool_calls": [call]}));
    messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "sunny"}));

    assert!(
        with_results(&log_f, 1).is_some(),
        "round 1 must be served by fallback F"
    );
    assert_eq!(
        forwarded_model(&with_results(&log_f, 1).expect("checked above")),
        "remote-f"
    );

    // Sleep about 2.5 seconds so A and B are healthy again.
    tokio::time::sleep(Duration::from_millis(2500)).await;

    // Round 2: must be served by regular B, not A and not F.
    let (status, text) = post_json(
        port,
        path,
        "Bearer local-key",
        Some(&session),
        None,
        json!({"model": alias, "messages": messages.clone(), "tools": weather_tools()}),
    )
    .await;
    assert_eq!(status, 200, "round 2 must be served: {text}");
    let value = parse(&text);
    assert_eq!(value["model"], alias, "the client must see the public alias");
    let call = chat_call(&value);
    assert!(
        call["id"].as_str().is_some(),
        "the response must carry a recognizable tool-call id"
    );

    // Explicit attempt counts.
    assert_eq!(
        log_a.lock().expect("mock log").len(),
        1,
        "regular A must be attempted only in round 0"
    );
    assert_eq!(
        log_f.lock().expect("mock log").len(),
        1,
        "fallback F must be attempted only in round 1"
    );
    assert_eq!(
        log_b.lock().expect("mock log").len(),
        1,
        "regular B must serve round 2"
    );
    assert!(
        with_results(&log_b, 2).is_some(),
        "round 2 must be served by regular B"
    );
    assert_eq!(
        forwarded_model(&with_results(&log_b, 2).expect("checked above")),
        "remote-b"
    );
    assert!(
        with_results(&log_a, 2).is_none(),
        "an open stale A hold would have served A in round 2"
    );
    assert!(
        with_results(&log_f, 2).is_none(),
        "a published fallback preference would have served F in round 2"
    );

    stop_server().await.expect("stop the listener");
}
