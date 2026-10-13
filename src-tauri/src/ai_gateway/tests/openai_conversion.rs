//! Step 2 behavior tests for pool-only Chat <-> Responses conversion and the
//! portable request contract (20261013-free-model-pool: REQ-004/005/007/009;
//! AC-005/006/007/017).
//!
//! Every test drives only already-compiled entry points at the real wire
//! boundary: the encrypted configuration writer (`write_raw_gateway_config`),
//! the real loopback listener (`start_server` plus `call_gateway` / a raw
//! `TcpStream`) and in-process mock upstreams. Pool records are supplied as raw
//! JSON so a RED is a *compiled observable* failure — a fenced `502` where a
//! converted `200` is required, a silently forwarded nonportable field, or a
//! stream that never converges — rather than a missing Rust symbol. No private
//! converter, SWRR helper or scheduler state is touched.
//!
//! Wire fixtures are raw JSON/SSE in the canonical OpenAI shapes the repo
//! already produces (`anthropic_sse.rs` establishes the Responses event names),
//! so the assertions stay independent of the implementation shape. Step 1's
//! same-protocol transport baseline cannot serve an opposite-protocol pool
//! candidate, cannot rewrite the public alias into a same-protocol response and
//! does not run the portable field matrix, so these tests fail behaviorally
//! before Step 2 lands.

use super::{
    call_gateway, free_port, pool_key, pool_provider, spawn_mock_upstream, temp_home,
    wait_for_usage_logs, write_raw_gateway_config, Captured, CapturedLog, MockReply,
};
use crate::ai_gateway::runtime_http::{start_server, stop_server};
use crate::ai_gateway::{GATEWAY_CONFIG_SCHEMA_VERSION, UsageResult};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

const LOCAL_KEY: &str = "local-key";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn tag() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// A schema-3 raw configuration carrying the frozen plan's pool fields plus one
/// local relay key.
fn pool_config_v3(port: u16, providers: Vec<Value>, pools: Vec<Value>) -> Value {
    json!({
        "schema_version": GATEWAY_CONFIG_SCHEMA_VERSION,
        "port": port,
        "keys": [{
            "id": "k-local",
            "label": "k-local",
            "value": LOCAL_KEY,
            "enabled": true,
            "created_at": 1,
        }],
        "providers": providers,
        "model_pools": pools,
        "model_pool_reserved_models": [],
    })
}

fn mapping(local_model: &str, upstream_model: &str, protocol: &str) -> Value {
    json!({
        "local_model": local_model,
        "upstream_model": upstream_model,
        "protocol": protocol,
        "enabled": true,
    })
}

fn candidate(provider_id: &str, local_model: &str, upstream_model: &str, protocol: &str) -> Value {
    json!({
        "provider_id": provider_id,
        "local_model": local_model,
        "upstream_model": upstream_model,
        "protocol": protocol,
        "weight": 1,
    })
}

/// One provider with an ordered key pool and one or more mappings at `protocol`.
fn provider_value(id: &str, url: &str, protocol: &str, mappings: Vec<Value>) -> Value {
    let mut provider = pool_provider(
        id,
        "Provider",
        url,
        vec![pool_key("key-default", "Default", "sk-upstream", true)],
        mappings,
    );
    provider["protocol"] = json!(protocol);
    provider
}

fn captured(log: &CapturedLog) -> Vec<Captured> {
    log.lock().expect("mock log").clone()
}

fn captured_body(captured: &Captured) -> Value {
    serde_json::from_slice(&captured.body).unwrap_or(Value::Null)
}

async fn post(port: u16, path: &str, body: Value) -> (u16, String, String) {
    let authorization = format!("Bearer {LOCAL_KEY}");
    call_gateway(
        port,
        "POST",
        path,
        &[("authorization", authorization.as_str())],
        Some(body),
    )
    .await
}

fn error_message(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// Whether the error message mentions `token` as a whole identifier token (so
/// `n`, `stop` and `include` are checked without substring false positives).
fn message_names_token(text: &str, token: &str) -> bool {
    message_of(text)
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|part| part.eq_ignore_ascii_case(token))
}

fn message_of(text: &str) -> String {
    error_message(text).to_ascii_lowercase()
}

/// A 400 that names the offending field. `tokens` lists acceptable spellings.
fn assert_field_rejected(status: u16, text: &str, tokens: &[&str]) {
    assert_eq!(
        status, 400,
        "a nonportable pool field must be rejected with 400 before any attempt, got {status}: {text}"
    );
    let message = message_of(text);
    assert!(
        !message.trim().is_empty(),
        "the 400 must carry a field-specific message: {text}"
    );
    assert!(
        tokens.iter().any(|token| message.contains(token.to_ascii_lowercase().as_str())),
        "the 400 message must name one of {tokens:?}: {text}"
    );
}

/// A Chat ingress body with a two-turn stateless history, one system and one
/// developer instruction and a function call/result, plus both sampling
/// controls and the tool surface.
fn chat_two_turn_body(alias: &str) -> Value {
    json!({
        "model": alias,
        "messages": [
            {"role": "system", "content": "be terse"},
            {"role": "developer", "content": "dev note"},
            {"role": "user", "content": "weather?"},
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
        "parallel_tool_calls": false,
        "temperature": 0.25,
        "top_p": 0.9
    })
}

/// A Responses ingress body with a two-turn stateless history and a function
/// call/result.
fn responses_two_turn_body(alias: &str) -> Value {
    json!({
        "model": alias,
        "instructions": "be terse",
        "input": [
            {"role": "user", "content": [{"type": "input_text", "text": "weather?"}]},
            {"type": "function_call", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"SF\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "sunny"}
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
        "temperature": 0.1,
        "max_output_tokens": 7
    })
}

fn responses_json(text: &str) -> Value {
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
            "content": [{"type": "output_text", "text": text, "annotations": []}]
        }],
        "usage": {"input_tokens": 11, "output_tokens": 4, "total_tokens": 15}
    })
}

fn chat_json(text: &str) -> Value {
    json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "model": "remote-c",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 9, "completion_tokens": 3, "total_tokens": 12}
    })
}

/// A successful Responses SSE body with a multibyte text delta, so a byte-level
/// split can fall inside a UTF-8 sequence.
fn responses_sse_success() -> String {
    let created = json!({"type":"response.created","response":{"id":"resp_1","object":"response","model":"remote-r","status":"in_progress","output":[]}});
    let added = json!({"type":"response.output_item.added","output_index":0,"item":{"id":"msg_1","type":"message","role":"assistant","status":"in_progress","content":[]}});
    let delta = json!({"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"héllo"});
    let done = json!({"type":"response.output_item.done","output_index":0,"item":{"id":"msg_1","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"héllo","annotations":[]}]}});
    let completed = json!({
        "type":"response.completed",
        "response":{"id":"resp_1","object":"response","model":"remote-r","status":"completed",
            "output":[{"id":"msg_1","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"héllo","annotations":[]}]}],
            "usage":{"input_tokens":5,"output_tokens":2,"total_tokens":7}}
    });
    format!(
        "event: response.created\ndata: {created}\n\nevent: response.output_item.added\ndata: {added}\n\nevent: response.output_text.delta\ndata: {delta}\n\nevent: response.output_item.done\ndata: {done}\n\nevent: response.completed\ndata: {completed}\n\n"
    )
}

/// The first converted delta of a successful Responses stream, used by the
/// gated incremental test.
fn responses_sse_prefix() -> String {
    let created = json!({"type":"response.created","response":{"id":"resp_1","object":"response","model":"remote-r","status":"in_progress","output":[]}});
    let added = json!({"type":"response.output_item.added","output_index":0,"item":{"id":"msg_1","type":"message","role":"assistant","status":"in_progress","content":[]}});
    let delta = json!({"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"Hi"});
    format!(
        "event: response.created\ndata: {created}\n\nevent: response.output_item.added\ndata: {added}\n\nevent: response.output_text.delta\ndata: {delta}\n\n"
    )
}

fn responses_sse_rest() -> String {
    let done = json!({"type":"response.output_item.done","output_index":0,"item":{"id":"msg_1","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"Hi there","annotations":[]}]}});
    let completed = json!({
        "type":"response.completed",
        "response":{"id":"resp_1","object":"response","model":"remote-r","status":"completed",
            "output":[{"id":"msg_1","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"Hi there","annotations":[]}]}],
            "usage":{"input_tokens":5,"output_tokens":2,"total_tokens":7}}
    });
    format!("event: response.output_item.done\ndata: {done}\n\nevent: response.completed\ndata: {completed}\n\n")
}

/// A Responses SSE prefix that yields a text delta then ends without a
/// completion event (premature EOF).
fn responses_sse_partial() -> String {
    let created = json!({"type":"response.created","response":{"id":"resp_1","object":"response","model":"remote-r","status":"in_progress","output":[]}});
    let delta = json!({"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"partial"});
    format!("event: response.created\ndata: {created}\n\nevent: response.output_text.delta\ndata: {delta}\n\n")
}

/// A successful Chat SSE body with text deltas and a usage chunk.
fn chat_sse_success() -> String {
    let d1 = json!({"id":"chatcmpl-1","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"role":"assistant","content":"Hel"},"finish_reason":null}]});
    let d2 = json!({"id":"chatcmpl-1","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"content":"lo"},"finish_reason":null}]});
    let stop = json!({"id":"chatcmpl-1","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]});
    let usage = json!({"id":"chatcmpl-1","object":"chat.completion.chunk","model":"remote-c","choices":[],"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}});
    format!("data: {d1}\n\ndata: {d2}\n\ndata: {stop}\n\ndata: {usage}\n\ndata: [DONE]\n\n")
}

/// A Chat SSE body with a fragmented function-call argument stream.
fn chat_sse_tools() -> String {
    let start = json!({"id":"chatcmpl-1","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":""}}]},"finish_reason":null}]});
    let arg1 = json!({"id":"chatcmpl-1","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"city\""}}]},"finish_reason":null}]});
    let arg2 = json!({"id":"chatcmpl-1","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"SF\"}"}}]},"finish_reason":null}]});
    let stop = json!({"id":"chatcmpl-1","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]});
    format!("data: {start}\n\ndata: {arg1}\n\ndata: {arg2}\n\ndata: {stop}\n\ndata: [DONE]\n\n")
}

// ---------------------------------------------------------------------------
// Custom upstream mocks (chunked / gated SSE)
// ---------------------------------------------------------------------------

/// Write an SSE body in fixed-size byte chunks, forcing fragmentation across
/// UTF-8, event and tool-argument boundaries.
async fn spawn_chunked_sse_mock(chunks: Vec<Vec<u8>>) -> (String, CapturedLog) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind chunked mock");
    let addr = listener.local_addr().expect("chunked mock addr");
    let log: CapturedLog = Arc::new(Mutex::new(Vec::new()));
    let log_for_server = log.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let log = log_for_server.clone();
            let chunks = chunks.clone();
            tauri::async_runtime::spawn(async move {
                let Ok(request) = crate::ai_gateway::runtime_http::read_http_request(&mut stream).await
                else {
                    return;
                };
                log.lock().expect("mock log").push(Captured {
                    method: request.method,
                    path: request.path,
                    headers: request.headers,
                    body: request.body,
                });
                let header =
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                if stream.write_all(header.as_bytes()).await.is_err() {
                    return;
                }
                for chunk in chunks {
                    if stream.write_all(&chunk).await.is_err() {
                        return;
                    }
                    let _ = stream.flush().await;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            });
        }
    });
    (format!("http://{addr}"), log)
}

/// A gated SSE upstream: send `first`, then wait for the test to release it (or
/// a bounded fallback) before sending `rest`. `attempts` counts accepted
/// requests so a test can wait for arrival without racing the reader.
struct GatedSse {
    url: String,
    release: Arc<Notify>,
    rest_sent: Arc<AtomicBool>,
    attempts: Arc<AtomicUsize>,
}

async fn spawn_gated_sse_mock(first: String, rest: String) -> GatedSse {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind gated mock");
    let addr = listener.local_addr().expect("gated mock addr");
    let release = Arc::new(Notify::new());
    let rest_sent = Arc::new(AtomicBool::new(false));
    let attempts = Arc::new(AtomicUsize::new(0));
    let release_srv = release.clone();
    let rest_sent_srv = rest_sent.clone();
    let attempts_srv = attempts.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let release = release_srv.clone();
            let rest_sent = rest_sent_srv.clone();
            let attempts = attempts_srv.clone();
            let first = first.clone();
            let rest = rest.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::ai_gateway::runtime_http::read_http_request(&mut stream).await;
                attempts.fetch_add(1, Ordering::SeqCst);
                let header =
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(first.as_bytes()).await;
                let _ = stream.flush().await;
                tokio::select! {
                    _ = release.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
                rest_sent.store(true, Ordering::SeqCst);
                let _ = stream.write_all(rest.as_bytes()).await;
                let _ = stream.flush().await;
            });
        }
    });
    GatedSse {
        url: format!("http://{addr}"),
        release,
        rest_sent,
        attempts,
    }
}

/// Write a raw HTTP/1.1 request to the shared gateway port and leave it open.
async fn open_raw_request(port: u16, path: &str, body: &Value) -> TcpStream {
    let bytes = serde_json::to_vec(body).expect("encode raw request");
    let request = format!(
        "POST {path} HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: Bearer {LOCAL_KEY}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        bytes.len()
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect gateway");
    let mut frame = request.into_bytes();
    frame.extend_from_slice(&bytes);
    stream.write_all(&frame).await.expect("write raw request");
    stream.flush().await.expect("flush raw request");
    stream
}

async fn wait_for_arrival(attempts: &AtomicUsize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while attempts.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the upstream request must arrive");
}

// ---------------------------------------------------------------------------
// REQ-004 / AC-005: JSON text and tools in both directions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pool_chat_ingress_converts_to_responses_upstream_and_back() {
    let _home = temp_home("pool-chat-to-responses-json");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Json(200, responses_json("Sunny"))).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(port, "/v1/chat/completions", chat_two_turn_body(&alias)).await;
    assert_eq!(status, 200, "a converted Chat->Responses JSON request must succeed: {text}");
    let body: Value = serde_json::from_str(&text).expect("Chat JSON body");
    assert_eq!(body["object"], "chat.completion", "downstream must be a Chat body: {text}");
    assert_eq!(body["model"], alias, "downstream must carry the public pool alias: {text}");
    assert_eq!(body["choices"][0]["message"]["content"], "Sunny", "converted text: {text}");

    let calls = captured(&log);
    assert_eq!(calls.len(), 1, "exactly one upstream attempt");
    assert_eq!(
        calls[0].path, "/v1/responses",
        "the Responses candidate must be reached at /v1/responses"
    );
    let sent = captured_body(&calls[0]);
    assert_eq!(sent["model"], "remote-r", "the real upstream model must be forwarded");
    assert!(
        sent.get("messages").is_none(),
        "a Responses upstream must never receive Chat `messages`: {sent}"
    );
    let serialized = sent.to_string();
    assert!(serialized.contains("be terse"), "the system instruction must survive: {sent}");
    assert!(serialized.contains("dev note"), "the developer instruction must survive: {sent}");
    assert!(serialized.contains("weather?"), "the user turn must survive: {sent}");
    assert!(serialized.contains("sunny"), "the tool result must survive: {sent}");
    assert!(serialized.contains("get_weather"), "the function schema must survive: {sent}");
    assert_eq!(sent["temperature"], 0.25, "temperature must survive: {sent}");
    assert_eq!(sent["top_p"], 0.9, "top_p must survive: {sent}");
    assert_eq!(sent["parallel_tool_calls"], false, "the parallel control must survive: {sent}");
    assert!(sent.get("tool_choice").is_some(), "the tool choice must survive: {sent}");

    stop_server().await.expect("stop listener");
}

#[tokio::test]
async fn pool_responses_ingress_converts_to_chat_upstream_and_back() {
    let _home = temp_home("pool-responses-to-chat-json");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Json(200, chat_json("Hi there"))).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "chat_completions",
        vec![mapping("local-c", "remote-c", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-c", "remote-c", "chat_completions")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(port, "/v1/responses", responses_two_turn_body(&alias)).await;
    assert_eq!(status, 200, "a converted Responses->Chat JSON request must succeed: {text}");
    let body: Value = serde_json::from_str(&text).expect("Responses JSON body");
    assert_eq!(body["object"], "response", "downstream must be a Responses body: {text}");
    assert_eq!(body["model"], alias, "downstream must carry the public pool alias: {text}");
    assert!(text.contains("Hi there"), "converted text: {text}");

    let calls = captured(&log);
    assert_eq!(calls.len(), 1, "exactly one upstream attempt");
    assert_eq!(calls[0].path, "/v1/chat/completions");
    let sent = captured_body(&calls[0]);
    assert_eq!(sent["model"], "remote-c");
    assert!(
        sent.get("input").is_none(),
        "a Chat upstream must never receive Responses `input`: {sent}"
    );
    assert!(sent["messages"].is_array(), "a Chat upstream needs `messages`: {sent}");
    let serialized = sent.to_string();
    assert!(serialized.contains("weather?"), "the user turn must survive: {sent}");
    assert!(serialized.contains("sunny"), "the tool result must survive: {sent}");
    assert!(serialized.contains("get_weather"), "the function schema must survive: {sent}");
    assert_eq!(sent["max_tokens"], 7, "max_output_tokens maps to the Chat output limit: {sent}");
    assert_eq!(sent["temperature"], 0.1);

    stop_server().await.expect("stop listener");
}

/// REQ-004: a same-protocol pool JSON response still carries the public alias
/// identity, not the real upstream model.
#[tokio::test]
async fn same_protocol_pool_json_returns_public_alias_identity() {
    let _home = temp_home("pool-same-protocol-alias");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, _log) = spawn_mock_upstream(|_| MockReply::Json(200, chat_json("ok"))).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "chat_completions",
        vec![mapping("local-c", "remote-c", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-c", "remote-c", "chat_completions")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({"model": alias, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    assert_eq!(status, 200, "the same-protocol pool request must succeed: {text}");
    let body: Value = serde_json::from_str(&text).expect("Chat JSON body");
    assert_eq!(
        body["model"], alias,
        "even a same-protocol pool response must carry the public alias, not the upstream model: {text}"
    );

    stop_server().await.expect("stop listener");
}

/// REQ-004: `max_completion_tokens` takes precedence over `max_tokens`, a
/// supplied zero stays zero and an omitted limit stays omitted for the
/// Responses translation.
#[tokio::test]
async fn chat_limits_precedence_and_omission_map_to_responses_output_limit() {
    let _home = temp_home("pool-chat-limits");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Json(200, responses_json("ok"))).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let base = json!({"model": alias, "messages": [{"role": "user", "content": "hi"}]});
    let mut zero = base.clone();
    zero["max_completion_tokens"] = json!(0);
    let mut precedence = base.clone();
    precedence["max_completion_tokens"] = json!(5);
    precedence["max_tokens"] = json!(9);

    for (case, body) in [("zero", zero), ("precedence", precedence), ("omitted", base)] {
        let (status, _ct, text) = post(port, "/v1/chat/completions", body).await;
        assert_eq!(status, 200, "limit case {case} must be served: {text}");
    }

    let calls = captured(&log);
    assert_eq!(calls.len(), 3);
    assert_eq!(
        captured_body(&calls[0])["max_output_tokens"], 0,
        "a supplied zero limit must stay zero: {}",
        captured_body(&calls[0])
    );
    assert_eq!(
        captured_body(&calls[1])["max_output_tokens"], 5,
        "max_completion_tokens must win over max_tokens: {}",
        captured_body(&calls[1])
    );
    assert!(
        captured_body(&calls[2]).get("max_output_tokens").is_none(),
        "an omitted limit must stay omitted: {}",
        captured_body(&calls[2])
    );

    stop_server().await.expect("stop listener");
}

/// REQ-004 / REQ-005: an omitted or explicitly `text` output format is accepted
/// in both directions.
#[tokio::test]
async fn explicit_text_output_format_is_accepted_in_both_directions() {
    let _home = temp_home("pool-text-format");
    let port = free_port().await;
    let tag = tag();
    let r_id = format!("p-r-{tag}");
    let c_id = format!("p-c-{tag}");
    let r_alias = format!("free-r-{tag}");
    let c_alias = format!("free-c-{tag}");
    let (r_url, _r_log) = spawn_mock_upstream(|_| MockReply::Json(200, responses_json("ok"))).await;
    let (c_url, _c_log) = spawn_mock_upstream(|_| MockReply::Json(200, chat_json("ok"))).await;

    let r_provider = provider_value(&r_id, &r_url, "responses", vec![mapping("local-r", "remote-r", "responses")]);
    let c_provider = provider_value(&c_id, &c_url, "chat_completions", vec![mapping("local-c", "remote-c", "chat_completions")]);
    let r_pool = json!({"id": format!("pool-r-{tag}"), "name": "R", "model": r_alias, "enabled": true,
        "candidates": [candidate(&r_id, "local-r", "remote-r", "responses")]});
    let c_pool = json!({"id": format!("pool-c-{tag}"), "name": "C", "model": c_alias, "enabled": true,
        "candidates": [candidate(&c_id, "local-c", "remote-c", "chat_completions")]});
    write_raw_gateway_config(&pool_config_v3(
        port,
        vec![r_provider, c_provider],
        vec![r_pool, c_pool],
    ));
    start_server(None).await.expect("start listener");

    let (chat_status, _ct, chat_text) = post(
        port,
        "/v1/chat/completions",
        json!({
            "model": r_alias,
            "messages": [{"role": "user", "content": "hi"}],
            "response_format": {"type": "text"}
        }),
    )
    .await;
    assert_eq!(chat_status, 200, "Chat text response_format must be accepted: {chat_text}");

    let (resp_status, _ct, resp_text) = post(
        port,
        "/v1/responses",
        json!({
            "model": c_alias,
            "input": "hi",
            "text": {"format": {"type": "text"}}
        }),
    )
    .await;
    assert_eq!(resp_status, 200, "Responses explicit text format must be accepted: {resp_text}");

    stop_server().await.expect("stop listener");
}

// ---------------------------------------------------------------------------
// REQ-005 / AC-007: nonportable fields fail before an attempt
// ---------------------------------------------------------------------------

/// A same-protocol Chat pool candidate, so a forwarded request would reach the
/// mock and any accepted nonportable field is an observable leak.
async fn chat_candidate_fixture(
    port: u16,
    tag: &str,
) -> (String, CapturedLog) {
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Json(200, chat_json("ok"))).await;
    let provider = provider_value(
        &provider_id,
        &url,
        "chat_completions",
        vec![mapping("local-c", "remote-c", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-c", "remote-c", "chat_completions")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");
    (alias, log)
}

async fn responses_candidate_fixture(port: u16, tag: &str) -> (String, CapturedLog) {
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Json(200, responses_json("ok"))).await;
    let provider = provider_value(
        &provider_id,
        &url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");
    (alias, log)
}

#[tokio::test]
async fn chat_nonportable_fields_are_rejected_before_any_attempt() {
    let _home = temp_home("pool-chat-nonportable");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = chat_candidate_fixture(port, &tag).await;

    for (field, value) in [
        ("stop", json!("STOP")),
        ("frequency_penalty", json!(0.5)),
        ("presence_penalty", json!(0.5)),
        ("seed", json!(7)),
        ("logit_bias", json!({"50256": -100})),
    ] {
        let mut body = json!({"model": alias, "messages": [{"role": "user", "content": "hi"}]});
        body[field] = value;
        let (status, _ct, text) = post(port, "/v1/chat/completions", body).await;
        assert_field_rejected(status, &text, &[field]);
        assert!(
            captured(&log).is_empty(),
            "field {field} must be rejected before any upstream attempt"
        );
    }

    stop_server().await.expect("stop listener");
}

#[tokio::test]
async fn unknown_stream_options_member_and_nonboolean_include_usage_are_rejected() {
    let _home = temp_home("pool-stream-options");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = chat_candidate_fixture(port, &tag).await;

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({
            "model": alias,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
            "stream_options": {"include_usage": true, "bogus": 1}
        }),
    )
    .await;
    assert_field_rejected(status, &text, &["bogus", "stream_options"]);

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({
            "model": alias,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
            "stream_options": {"include_usage": "yes"}
        }),
    )
    .await;
    assert_field_rejected(status, &text, &["include_usage"]);

    assert!(captured(&log).is_empty(), "no stream_options failure may reach upstream");
    stop_server().await.expect("stop listener");
}

#[tokio::test]
async fn responses_truncation_and_include_are_rejected() {
    let _home = temp_home("pool-responses-truncation");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = responses_candidate_fixture(port, &tag).await;

    for (field, value) in [
        ("truncation", json!("auto")),
        ("include", json!(["reasoning.encrypted_content"])),
    ] {
        let mut body = json!({"model": alias, "input": "hi"});
        body[field] = value;
        let (status, _ct, text) = post(port, "/v1/responses", body).await;
        assert_field_rejected(status, &text, &[field]);
        assert!(captured(&log).is_empty(), "field {field} must not reach upstream");
    }

    stop_server().await.expect("stop listener");
}

#[tokio::test]
async fn responses_nonportable_state_items_and_content_are_rejected() {
    let _home = temp_home("pool-responses-state");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = responses_candidate_fixture(port, &tag).await;

    for (name, tokens, extra) in [
        ("previous_response_id", vec!["previous_response_id"], json!({"previous_response_id": "resp_0"})),
        ("conversation", vec!["conversation"], json!({"conversation": "conv_1"})),
        ("item_reference", vec!["item_reference", "item"], json!({"input": [{"type": "item_reference", "id": "item_1"}]})),
        ("background", vec!["background"], json!({"background": true})),
        ("store", vec!["store"], json!({"store": true})),
        ("n", vec!["n"], json!({"n": 2})),
        ("hosted_tools", vec!["web_search", "tool"], json!({"tools": [{"type": "web_search"}]})),
        ("image", vec!["image"], json!({"input": [{"role": "user", "content": [{"type": "input_image", "image_url": "https://example.com/x.png"}]}]})),
        ("logprobs", vec!["logprobs"], json!({"logprobs": true})),
        ("opaque_reasoning", vec!["reasoning", "item", "encrypted"], json!({"input": [{"type": "reasoning", "encrypted_content": "opaque"}]})),
    ] {
        let mut body = json!({"model": alias, "input": "hi"});
        if let Some(object) = extra.as_object() {
            for (key, value) in object {
                body[key.as_str()] = value.clone();
            }
        }
        let (status, _ct, text) = post(port, "/v1/responses", body).await;
        assert_eq!(status, 400, "case {name} must be rejected with 400: {text}");
        let names = tokens
            .iter()
            .any(|token| message_names_token(&text, token) || message_of(&text).contains(*token));
        assert!(names, "case {name} must name the offending field: {text}");
        assert!(captured(&log).is_empty(), "case {name} must not reach upstream");
    }

    stop_server().await.expect("stop listener");
}

#[tokio::test]
async fn json_and_schema_output_formats_are_rejected_naming_the_format_field() {
    let _home = temp_home("pool-json-format");
    let port = free_port().await;

    let chat_tag = tag();
    let (alias, log) = chat_candidate_fixture(port, &chat_tag).await;
    for (name, format) in [
        ("json_object", json!({"type": "json_object"})),
        (
            "json_schema",
            json!({"type": "json_schema", "json_schema": {"name": "x", "schema": {"type": "object"}}}),
        ),
    ] {
        let (status, _ct, text) = post(
            port,
            "/v1/chat/completions",
            json!({
                "model": alias,
                "messages": [{"role": "user", "content": "hi"}],
                "response_format": format
            }),
        )
        .await;
        assert_field_rejected(status, &text, &["response_format", "format"]);
        assert!(captured(&log).is_empty(), "Chat format {name} must not reach upstream");
    }
    stop_server().await.expect("stop listener");

    let responses_tag = tag();
    let (alias, log) = responses_candidate_fixture(port, &responses_tag).await;
    let (status, _ct, text) = post(
        port,
        "/v1/responses",
        json!({
            "model": alias,
            "input": "hi",
            "text": {"format": {"type": "json_schema", "name": "x", "schema": {"type": "object"}}}
        }),
    )
    .await;
    assert_field_rejected(status, &text, &["format"]);
    assert!(captured(&log).is_empty(), "a Responses JSON format must not reach upstream");
    stop_server().await.expect("stop listener");
}

/// AC-007: field validation runs before any candidate selection, so a
/// probe-only source must still reject an invalid field and settle no probe or
/// health state.
#[tokio::test]
async fn probe_only_source_still_rejects_nonportable_fields_before_probe_or_health() {
    let _home = temp_home("pool-probe-only-validation");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Json(200, chat_json("ok"))).await;

    let mut disabled = mapping("local-c", "remote-c", "chat_completions");
    disabled["auto_disabled"] = json!(true);
    disabled["disabled_reason"] = json!("HTTP 500 upstream error");
    disabled["disabled_at"] = json!(1);
    disabled["consecutive_failures"] = json!(3);
    let provider = provider_value(&provider_id, &url, "chat_completions", vec![disabled]);
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-c", "remote-c", "chat_completions")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "seed": 7
        }),
    )
    .await;
    assert_field_rejected(status, &text, &["seed"]);
    assert!(
        captured(&log).is_empty(),
        "validation must run before the probe candidate"
    );

    let stored = crate::ai_gateway::storage::read_config().expect("read config");
    let provider = stored
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .expect("provider persists");
    assert!(
        provider.mappings[0].auto_disabled,
        "a rejected field must not settle the probe health"
    );
    assert_eq!(provider.mappings[0].consecutive_failures, 3);

    stop_server().await.expect("stop listener");
}

/// REQ-005: absent/false `store` and `background` are valid and preserved; the
/// same-protocol response still carries the public alias.
#[tokio::test]
async fn absent_or_false_store_and_background_are_preserved() {
    let _home = temp_home("pool-store-background-false");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Json(200, responses_json("ok"))).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/responses",
        json!({"model": alias, "input": "hi", "store": false, "background": false}),
    )
    .await;
    assert_eq!(status, 200, "false store/background must be accepted: {text}");
    let body: Value = serde_json::from_str(&text).expect("Responses JSON body");
    assert_eq!(body["model"], alias, "the alias identity must be returned: {text}");

    let sent = captured_body(&captured(&log)[0]);
    assert_eq!(sent["store"], false, "an explicit false store must not be dropped: {sent}");
    assert_eq!(
        sent["background"], false,
        "an explicit false background must not be dropped: {sent}"
    );

    stop_server().await.expect("stop listener");
}

/// REQ-005: ordinary (non-pool) requests keep their current passthrough, so a
/// nonportable-for-pools field like `stop` still reaches an ordinary upstream.
#[tokio::test]
async fn ordinary_request_with_stop_still_passes_through_unchanged() {
    let _home = temp_home("pool-ordinary-passthrough");
    let port = free_port().await;
    let tag = tag();
    let (url, log) = spawn_mock_upstream(|_| MockReply::Json(200, chat_json("ok"))).await;
    let provider = provider_value(
        &format!("p-{tag}"),
        &url,
        "chat_completions",
        vec![mapping("local-ordinary", "remote-c", "chat_completions")],
    );
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({
            "model": "local-ordinary",
            "messages": [{"role": "user", "content": "hi"}],
            "stop": "STOP"
        }),
    )
    .await;
    assert_eq!(status, 200, "an ordinary request must keep its passthrough: {text}");
    let sent = captured_body(&captured(&log)[0]);
    assert_eq!(sent["stop"], "STOP", "an ordinary `stop` must still reach upstream: {sent}");

    stop_server().await.expect("stop listener");
}

// ---------------------------------------------------------------------------
// REQ-004 / AC-006 / AC-017: incremental converted streams
// ---------------------------------------------------------------------------

#[tokio::test]
async fn converted_responses_sse_is_fragmented_tolerant_and_ends_with_one_done() {
    let _home = temp_home("pool-fragmented-responses-sse");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let body = responses_sse_success();
    let chunks: Vec<Vec<u8>> = body.into_bytes().chunks(4).map(<[u8]>::to_vec).collect();
    let (url, log) = spawn_chunked_sse_mock(chunks).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, content_type, text) = post(
        port,
        "/v1/chat/completions",
        json!({"model": alias, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    assert_eq!(status, 200, "the converted stream must succeed: {text}");
    assert!(
        content_type.contains("text/event-stream"),
        "the caller must receive SSE: {content_type}"
    );
    assert!(
        text.contains("héllo") || text.contains("h\\u00e9llo"),
        "the multibyte text delta must survive fragmented UTF-8: {text}"
    );
    assert_eq!(
        text.matches("data: [DONE]").count(),
        1,
        "a converted Chat stream must end with exactly one [DONE]: {text}"
    );

    let calls = captured(&log);
    assert_eq!(calls.len(), 1, "exactly one upstream attempt");
    assert_eq!(calls[0].path, "/v1/responses");
    assert!(
        captured_body(&calls[0]).get("stream_options").is_none(),
        "the gateway must not inject an upstream include_usage flag"
    );

    stop_server().await.expect("stop listener");
}

#[tokio::test]
async fn converted_chat_sse_to_responses_emits_incremental_text_and_completion() {
    let _home = temp_home("pool-chat-sse-to-responses");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Stream(chat_sse_success())).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "chat_completions",
        vec![mapping("local-c", "remote-c", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-c", "remote-c", "chat_completions")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, content_type, text) = post(
        port,
        "/v1/responses",
        json!({"model": alias, "stream": true, "input": "hi"}),
    )
    .await;
    assert_eq!(status, 200, "the converted Responses stream must succeed: {text}");
    assert!(
        content_type.contains("text/event-stream"),
        "the caller must receive SSE: {content_type}"
    );
    assert!(
        text.contains("response.output_text.delta") || text.contains("output_text.delta"),
        "incremental text events must reach the caller: {text}"
    );
    assert!(text.contains("Hel") && text.contains("lo"), "both deltas: {text}");
    let completed = text
        .lines()
        .filter(|line| line.trim() == "event: response.completed")
        .count();
    assert_eq!(completed, 1, "exactly one response.completed event: {text}");
    assert!(
        !text.contains("[DONE]"),
        "a Responses stream must not inject the Chat [DONE]: {text}"
    );

    let calls = captured(&log);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1/chat/completions");

    stop_server().await.expect("stop listener");
}

#[tokio::test]
async fn converted_chat_sse_tool_arguments_are_fragmented_and_identity_is_stable() {
    let _home = temp_home("pool-chat-sse-tools");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let body = chat_sse_tools();
    let chunks: Vec<Vec<u8>> = body.into_bytes().chunks(5).map(<[u8]>::to_vec).collect();
    let (url, log) = spawn_chunked_sse_mock(chunks).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "chat_completions",
        vec![mapping("local-c", "remote-c", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-c", "remote-c", "chat_completions")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/responses",
        json!({"model": alias, "stream": true, "input": "hi",
            "tools": [{"type": "function", "name": "get_weather", "parameters": {"type": "object"}}]}),
    )
    .await;
    assert_eq!(status, 200, "the converted tool stream must succeed: {text}");
    assert!(
        text.contains("call_abc"),
        "the stable tool call id must survive conversion: {text}"
    );
    assert!(
        text.contains("function_call_arguments.delta"),
        "the tool arguments must stream as Responses events: {text}"
    );
    assert!(text.contains("city") && text.contains("SF"), "the full arguments must arrive: {text}");
    let completed = text
        .lines()
        .filter(|line| line.trim() == "event: response.completed")
        .count();
    assert_eq!(completed, 1, "exactly one response.completed event: {text}");
    assert!(!text.contains("[DONE]"), "a Responses stream must not carry [DONE]: {text}");

    assert_eq!(captured(&log).len(), 1);
    stop_server().await.expect("stop listener");
}

/// AC-006 / AC-017: the first converted delta reaches the caller while the
/// upstream is still producing output (sequencing, not a timing threshold).
#[tokio::test]
async fn converted_stream_is_incremental_before_upstream_eof() {
    let _home = temp_home("pool-incremental-before-eof");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let gated = spawn_gated_sse_mock(responses_sse_prefix(), responses_sse_rest()).await;

    let provider = provider_value(
        &provider_id,
        &gated.url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let mut stream = open_raw_request(
        port,
        "/v1/chat/completions",
        &json!({"model": alias, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    wait_for_arrival(&gated.attempts).await;

    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut incremental = false;
    loop {
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await {
            Ok(Ok(0)) | Ok(Err(_)) => break,
            Ok(Ok(n)) => {
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if !incremental && text.contains("\"Hi\"") {
                    incremental = !gated.rest_sent.load(Ordering::SeqCst);
                    gated.release.notify_one();
                }
                if text.contains("[DONE]") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let output = String::from_utf8_lossy(&buf).into_owned();
    assert!(incremental, "the first converted delta must precede upstream completion: {output}");
    assert_eq!(
        output.matches("data: [DONE]").count(),
        1,
        "exactly one [DONE] after the upstream completes: {output}"
    );

    stop_server().await.expect("stop listener");
}

/// REQ-004: a requested Chat `stream_options.include_usage` is honored in the
/// converted output without injecting an upstream flag.
#[tokio::test]
async fn chat_stream_include_usage_true_adds_terminal_usage_without_upstream_flag() {
    let _home = temp_home("pool-include-usage-true");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Stream(responses_sse_success())).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({
            "model": alias,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
            "stream_options": {"include_usage": true}
        }),
    )
    .await;
    assert_eq!(status, 200, "include_usage stream must succeed: {text}");
    assert!(
        text.contains("prompt_tokens") && text.contains("completion_tokens"),
        "a requested include_usage must add a terminal usage chunk: {text}"
    );
    assert_eq!(text.matches("data: [DONE]").count(), 1, "exactly one [DONE]: {text}");

    let sent = captured_body(&captured(&log)[0]);
    assert!(
        sent.get("stream_options").is_none(),
        "the gateway must never inject an unrequested upstream include_usage: {sent}"
    );

    stop_server().await.expect("stop listener");
}

#[tokio::test]
async fn chat_stream_without_include_usage_omits_terminal_usage_and_upstream_flag() {
    let _home = temp_home("pool-include-usage-absent");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, log) = spawn_mock_upstream(|_| MockReply::Stream(responses_sse_success())).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({"model": alias, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    assert_eq!(status, 200, "the stream must succeed: {text}");
    assert!(
        !text.contains("prompt_tokens"),
        "an unrequested usage chunk must not appear: {text}"
    );
    assert_eq!(text.matches("data: [DONE]").count(), 1, "exactly one [DONE]: {text}");

    let sent = captured_body(&captured(&log)[0]);
    assert!(sent.get("stream_options").is_none());

    stop_server().await.expect("stop listener");
}

/// AC-017: a post-byte upstream failure emits a caller-format error with no
/// success marker and no replay, and the actual source is settled once.
#[tokio::test]
async fn post_byte_upstream_failure_has_no_success_marker_or_fallback() {
    let _home = temp_home("pool-post-byte-failure");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let partial = responses_sse_partial();
    let declared = partial.len() + 256;
    let (url, log) = spawn_mock_upstream(move |_| MockReply::PartialStream(partial.clone(), declared)).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (_status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({"model": alias, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    assert!(text.contains("partial"), "the first delta must reach the caller: {text}");
    assert!(
        !text.contains("data: [DONE]"),
        "a failed converted stream must never gain a success marker: {text}"
    );
    assert!(text.contains("error"), "a post-byte failure must surface an error: {text}");
    assert_eq!(captured(&log).len(), 1, "a post-byte failure must not fall back or replay");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1, "the actual source must be settled exactly once: {records:?}");
    assert_eq!(records[0].provider_id, provider_id);
    assert_eq!(records[0].local_model, alias);
    assert_eq!(records[0].upstream_model, "remote-r");
    assert_eq!(records[0].result, UsageResult::Failure);

    stop_server().await.expect("stop listener");
}

/// AC-017: cancelling a converted pool stream mid-flight persists no usage row
/// and does not settle a health failure on the real source.
#[tokio::test]
async fn cancellation_mid_stream_stops_upstream_and_settles_nothing() {
    let _home = temp_home("pool-converted-cancel");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let gated = spawn_gated_sse_mock(responses_sse_prefix(), responses_sse_rest()).await;

    let provider = provider_value(
        &provider_id,
        &gated.url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let mut stream = open_raw_request(
        port,
        "/v1/chat/completions",
        &json!({"model": alias, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    wait_for_arrival(&gated.attempts).await;

    let mut buf = [0u8; 4096];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf))
        .await
        .expect("the first bytes must arrive");
    assert!(read.unwrap_or(0) > 0, "the converted stream must have started");

    // Cancel downstream while the upstream is still producing.
    drop(stream);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let store = super::default_usage_store();
    assert!(
        store.all_records().unwrap_or_default().is_empty(),
        "a cancelled converted stream must persist no usage row"
    );
    assert_eq!(gated.attempts.load(Ordering::SeqCst), 1, "no later call may run");

    let stored = crate::ai_gateway::storage::read_config().expect("read config");
    let provider = stored
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .expect("provider persists");
    assert!(!provider.mappings[0].auto_disabled, "cancellation must not disable the mapping");
    assert_eq!(provider.mappings[0].consecutive_failures, 0);
    assert_eq!(provider.mappings[0].last_error_at, None);

    stop_server().await.expect("stop listener");
}

// ===========================================================================
// Review-repair (2026-10-13): supported-path defects found on the Step 2
// implementation. These tests drive the public listener with canonical OpenAI
// JSON/SSE fixtures and assert only wire shape, source health and usage rows.
// ===========================================================================

fn response_event_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("event:"))
        .map(|name| name.trim().to_string())
        .collect()
}

fn response_data_values(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .filter(|data| *data != "[DONE]")
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .collect()
}

fn chat_data_values(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .filter(|data| *data != "[DONE]")
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .collect()
}

fn response_added_item_types(text: &str) -> Vec<String> {
    response_data_values(text)
        .into_iter()
        .filter(|value| value.get("type").and_then(Value::as_str) == Some("response.output_item.added"))
        .filter_map(|value| {
            value
                .get("item")
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

fn response_function_argument_deltas(text: &str) -> Vec<(String, String)> {
    response_data_values(text)
        .into_iter()
        .filter(|value| {
            value.get("type").and_then(Value::as_str) == Some("response.function_call_arguments.delta")
        })
        .map(|value| {
            (
                value.get("item_id").and_then(Value::as_str).unwrap_or_default().to_string(),
                value.get("delta").and_then(Value::as_str).unwrap_or_default().to_string(),
            )
        })
        .collect()
}

/// (non-empty tool-call ids in wire order, per-index (index, id, name, args)).
fn chat_tool_call_summary(text: &str) -> (Vec<String>, Vec<(usize, String, String, String)>) {
    let mut ids: Vec<String> = Vec::new();
    let mut by_index: BTreeMap<usize, (String, String, String)> = BTreeMap::new();
    for value in chat_data_values(text) {
        for choice in value
            .get("choices")
            .and_then(Value::as_array)
            .map_or(&[][..], |choices| choices.as_slice())
        {
            for call in choice
                .get("delta")
                .and_then(|delta| delta.get("tool_calls"))
                .and_then(Value::as_array)
                .map_or(&[][..], |calls| calls.as_slice())
            {
                let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let entry = by_index.entry(index).or_default();
                if let Some(id) = call.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()) {
                    ids.push(id.to_string());
                    entry.0 = id.to_string();
                }
                if let Some(name) = call
                    .get("function")
                    .and_then(|function| function.get("name"))
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                {
                    entry.1 = name.to_string();
                }
                if let Some(args) = call
                    .get("function")
                    .and_then(|function| function.get("arguments"))
                    .and_then(Value::as_str)
                {
                    entry.2.push_str(args);
                }
            }
        }
    }
    let entries = by_index
        .into_iter()
        .map(|(index, (id, name, args))| (index, id, name, args))
        .collect();
    (ids, entries)
}

/// A same-protocol Chat pool whose only candidate streams `sse`.
async fn chat_stream_fixture(port: u16, sse: String) -> (String, CapturedLog) {
    let provider_id = format!("p-{}", tag());
    let alias = format!("free-model-{}", tag());
    let (url, log) = spawn_mock_upstream(move |_| MockReply::Stream(sse.clone())).await;
    let provider = provider_value(
        &provider_id,
        &url,
        "chat_completions",
        vec![mapping("local-c", "remote-c", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{}", tag()),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-c", "remote-c", "chat_completions")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");
    (alias, log)
}

/// A Responses upstream function-call stream where the delta frames key by the
/// function-call item id (`fc_123`) while the client-facing id is the call id
/// (`call_456`), with the arguments fragmented across UTF-8 boundaries.
fn responses_tool_sse_distinct_ids() -> String {
    let created = json!({"type":"response.created","response":{"id":"resp_tool","object":"response","model":"remote-r","status":"in_progress","output":[]}});
    let added = json!({"type":"response.output_item.added","output_index":0,"item":{"id":"fc_123","call_id":"call_456","type":"function_call","name":"weather","arguments":"","status":"in_progress"}});
    let d1 = json!({"type":"response.function_call_arguments.delta","item_id":"fc_123","output_index":0,"delta":"{\"city\":"});
    let d2 = json!({"type":"response.function_call_arguments.delta","item_id":"fc_123","output_index":0,"delta":"\"济南\"}"});
    let done = json!({"type":"response.output_item.done","output_index":0,"item":{"id":"fc_123","call_id":"call_456","type":"function_call","name":"weather","arguments":"{\"city\":\"济南\"}","status":"completed"}});
    let completed = json!({"type":"response.completed","response":{"id":"resp_tool","object":"response","model":"remote-r","status":"completed","output":[{"id":"fc_123","call_id":"call_456","type":"function_call","name":"weather","arguments":"{\"city\":\"济南\"}","status":"completed"}],"usage":{"input_tokens":4,"output_tokens":6,"total_tokens":10}}});
    format!("event: response.created\ndata: {created}\n\nevent: response.output_item.added\ndata: {added}\n\nevent: response.function_call_arguments.delta\ndata: {d1}\n\nevent: response.function_call_arguments.delta\ndata: {d2}\n\nevent: response.output_item.done\ndata: {done}\n\nevent: response.completed\ndata: {completed}\n\n")
}

/// A Chat upstream SSE where the tool call arrives first and text follows.
fn chat_sse_tool_first_text_later() -> String {
    let start = json!({"id":"chatcmpl-t","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_t1","type":"function","function":{"name":"weather","arguments":""}}]},"finish_reason":null}]});
    let arg1 = json!({"id":"chatcmpl-t","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"city\":\""}}]},"finish_reason":null}]});
    let arg2 = json!({"id":"chatcmpl-t","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"济南\"}"}}]},"finish_reason":null}]});
    let text = json!({"id":"chatcmpl-t","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"content":"final text"},"finish_reason":null}]});
    let stop = json!({"id":"chatcmpl-t","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]});
    format!("data: {start}\n\ndata: {arg1}\n\ndata: {arg2}\n\ndata: {text}\n\ndata: {stop}\n\ndata: [DONE]\n\n")
}

/// A Chat upstream SSE that carries valid content and usage, then a supported
/// in-band error object, then the Chat `[DONE]` sentinel.
fn chat_sse_error_then_done() -> String {
    let d1 = json!({"id":"chatcmpl-e","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"role":"assistant","content":"partial"},"finish_reason":null}]});
    let stop = json!({"id":"chatcmpl-e","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]});
    let error = json!({"error":{"message":"upstream exploded","type":"server_error"}});
    format!("data: {d1}\n\ndata: {stop}\n\ndata: {error}\n\ndata: [DONE]\n\n")
}

/// A Chat upstream SSE with a valid delta followed by a malformed data frame and
/// the Chat `[DONE]` sentinel.
fn chat_sse_malformed_then_done() -> String {
    let d1 = json!({"id":"chatcmpl-m","object":"chat.completion.chunk","model":"remote-c","choices":[{"index":0,"delta":{"content":"partial"},"finish_reason":null}]});
    format!("data: {d1}\n\ndata: {{not json}}\n\ndata: [DONE]\n\n")
}

/// Defect 1 (REQ-004/AC-006): a Responses function-call stream keys its argument
/// deltas by the function-call item id while the client tool_call id is the call
/// id. The converted Chat stream must keep the call id stable and concatenate
/// the fully fragmented UTF-8 arguments.
#[tokio::test]
async fn responses_function_call_sse_preserves_call_id_and_arguments() {
    let _home = temp_home("pool-responses-fn-sse");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let body = responses_tool_sse_distinct_ids();
    let chunks: Vec<Vec<u8>> = body.into_bytes().chunks(7).map(<[u8]>::to_vec).collect();
    let (url, log) = spawn_chunked_sse_mock(chunks).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "responses",
        vec![mapping("local-r", "remote-r", "responses")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-r", "remote-r", "responses")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({"model": alias, "stream": true, "messages": [{"role": "user", "content": "weather?"}]}),
    )
    .await;
    assert_eq!(status, 200, "the Responses function stream must convert: {text}");
    assert_eq!(
        text.matches("data: [DONE]").count(),
        1,
        "exactly one [DONE]: {text}"
    );

    let (ids, entries) = chat_tool_call_summary(&text);
    assert!(!ids.is_empty(), "the client tool call must carry an id: {text}");
    assert!(
        ids.iter().all(|id| id == "call_456"),
        "every client tool_call id must be the stable call_id, got {ids:?}: {text}"
    );
    assert_eq!(entries.len(), 1, "the function call must map to one tool index: {text}");
    let (index, _id, name, args) = &entries[0];
    assert_eq!(*index, 0, "the tool index must stay stable: {text}");
    assert_eq!(name, "weather", "the tool name must survive independently of its id: {text}");
    assert_eq!(
        args, "{\"city\":\"济南\"}",
        "the fragmented UTF-8 arguments must be complete: {text}"
    );

    assert_eq!(captured(&log).len(), 1, "exactly one upstream attempt");
    stop_server().await.expect("stop listener");
}

/// Defect 2a (REQ-004/AC-006): a converted Chat -> Responses stream must emit
/// the canonical text lifecycle events, not only the item and response
/// completion.
#[tokio::test]
async fn chat_to_responses_stream_emits_full_text_lifecycle() {
    let _home = temp_home("pool-chat-responses-text-lifecycle");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, _log) = spawn_mock_upstream(|_| MockReply::Stream(chat_sse_success())).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "chat_completions",
        vec![mapping("local-c", "remote-c", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-c", "remote-c", "chat_completions")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/responses",
        json!({"model": alias, "stream": true, "input": "hi"}),
    )
    .await;
    assert_eq!(status, 200, "the converted Responses stream must succeed: {text}");

    let names = response_event_names(&text);
    for required in [
        "response.created",
        "response.output_item.added",
        "response.content_part.added",
        "response.output_text.delta",
        "response.output_text.done",
        "response.content_part.done",
        "response.output_item.done",
        "response.completed",
    ] {
        assert!(
            names.iter().any(|name| name == required),
            "the converted Responses stream must emit {required}: {text}"
        );
    }
    assert_eq!(
        names.iter().filter(|name| *name == "response.completed").count(),
        1,
        "exactly one response.completed: {text}"
    );
    assert_eq!(
        names.last().map(String::as_str),
        Some("response.completed"),
        "response.completed must be the terminal event: {text}"
    );
    assert!(!text.contains("[DONE]"), "a Responses stream must not carry [DONE]: {text}");

    stop_server().await.expect("stop listener");
}

/// Defect 2b (REQ-004/AC-006): a tool-first Chat stream must not fabricate a
/// leading empty message item, must preserve the wire output order/indices and
/// must signal function-call argument completion.
#[tokio::test]
async fn chat_to_responses_tool_first_keeps_wire_order_without_fabricated_message() {
    let _home = temp_home("pool-chat-responses-tool-first");
    let port = free_port().await;
    let tag = tag();
    let provider_id = format!("p-{tag}");
    let alias = format!("free-model-{tag}");
    let (url, _log) = spawn_mock_upstream(|_| MockReply::Stream(chat_sse_tool_first_text_later())).await;

    let provider = provider_value(
        &provider_id,
        &url,
        "chat_completions",
        vec![mapping("local-c", "remote-c", "chat_completions")],
    );
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Free Modle",
        "model": alias,
        "enabled": true,
        "candidates": [candidate(&provider_id, "local-c", "remote-c", "chat_completions")],
    });
    write_raw_gateway_config(&pool_config_v3(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/responses",
        json!({"model": alias, "stream": true, "input": "hi"}),
    )
    .await;
    assert_eq!(status, 200, "the converted tool-first stream must succeed: {text}");

    let added = response_added_item_types(&text);
    assert_eq!(
        added.first().map(String::as_str),
        Some("function_call"),
        "a tool-first stream must not fabricate a leading message item: {text}"
    );
    assert_eq!(
        added,
        vec!["function_call".to_string(), "message".to_string()],
        "the wire output order must follow the upstream arrival order: {text}"
    );
    assert!(
        response_event_names(&text)
            .iter()
            .any(|name| name == "response.function_call_arguments.done"),
        "the converted tool stream must signal argument completion: {text}"
    );

    let deltas = response_function_argument_deltas(&text);
    assert!(!deltas.is_empty(), "the tool arguments must stream: {text}");
    assert!(
        deltas.iter().all(|(id, _)| id == "call_t1"),
        "the function_call item id must stay stable: {deltas:?}"
    );
    let args: String = deltas.iter().map(|(_, delta)| delta.as_str()).collect();
    assert_eq!(args, "{\"city\":\"济南\"}", "the arguments must concatenate: {text}");
    assert!(text.contains("final text"), "the later text must reach the caller: {text}");
    assert_eq!(
        response_event_names(&text)
            .iter()
            .filter(|name| *name == "response.completed")
            .count(),
        1,
        "exactly one response.completed: {text}"
    );
    assert!(!text.contains("[DONE]"), "a Responses stream must not carry [DONE]: {text}");

    stop_server().await.expect("stop listener");
}

/// Defect 3a (REQ-007/AC-017): a same-protocol Chat pool stream that carries an
/// in-band error object before `[DONE]` must surface an error and must not end
/// with a success `[DONE]`; the source failure is logged once with no fallback.
#[tokio::test]
async fn same_protocol_chat_stream_in_band_error_has_no_success_done() {
    let _home = temp_home("pool-same-protocol-error-stream");
    let port = free_port().await;
    let (alias, log) = chat_stream_fixture(port, chat_sse_error_then_done()).await;

    let (_status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({"model": alias, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    assert!(text.contains("partial"), "the first delta must reach the caller: {text}");
    assert!(
        text.contains("upstream_stream_error"),
        "an in-band upstream error must reach the caller as a gateway error: {text}"
    );
    assert!(
        !text.contains("[DONE]"),
        "an errored stream must never end with a success [DONE]: {text}"
    );
    assert_eq!(captured(&log).len(), 1, "no fallback after the first byte: {text}");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1, "exactly one terminal row: {records:?}");
    assert_eq!(records[0].local_model, alias, "the alias is the local model: {records:?}");
    assert!(
        !records[0].provider_id.is_empty(),
        "the real source must be settled: {records:?}"
    );
    assert_eq!(
        records[0].result,
        UsageResult::Failure,
        "an in-band error must settle as a failure: {records:?}"
    );

    stop_server().await.expect("stop listener");
}

/// Defect 3c (REQ-007/AC-017): a same-protocol Chat pool stream with a malformed
/// data frame before `[DONE]` is a post-byte failure, not a forwarded success.
#[tokio::test]
async fn same_protocol_chat_stream_malformed_event_is_post_byte_failure() {
    let _home = temp_home("pool-same-protocol-malformed-stream");
    let port = free_port().await;
    let (alias, log) = chat_stream_fixture(port, chat_sse_malformed_then_done()).await;

    let (_status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({"model": alias, "stream": true, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await;
    assert!(text.contains("partial"), "the first delta must reach the caller: {text}");
    assert!(
        !text.contains("{not json}"),
        "a malformed upstream frame must never be forwarded to the caller: {text}"
    );
    assert!(
        text.contains("upstream_stream_error"),
        "a malformed frame must surface as a gateway error: {text}"
    );
    assert!(!text.contains("[DONE]"), "a malformed stream must not gain a [DONE]: {text}");
    assert_eq!(captured(&log).len(), 1, "no fallback after the first byte: {text}");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].result, UsageResult::Failure, "{records:?}");

    stop_server().await.expect("stop listener");
}

/// Defect 3b (REQ-007/AC-017): the Chat -> Responses parser must treat an
/// in-band error object as a failure and must not end with response.completed.
#[tokio::test]
async fn chat_to_responses_stream_in_band_error_does_not_complete() {
    let _home = temp_home("pool-chat-responses-error-stream");
    let port = free_port().await;
    let (alias, log) = chat_stream_fixture(port, chat_sse_error_then_done()).await;

    let (_status, _ct, text) = post(
        port,
        "/v1/responses",
        json!({"model": alias, "stream": true, "input": "hi"}),
    )
    .await;
    assert!(
        text.contains("partial") || text.contains("response.output_text.delta"),
        "the first delta must reach the caller: {text}"
    );
    assert!(
        !response_event_names(&text)
            .iter()
            .any(|name| name == "response.completed"),
        "an in-band upstream error must not end with response.completed: {text}"
    );
    assert!(text.contains("error"), "the caller must receive an error signal: {text}");
    assert!(!text.contains("[DONE]"), "a Responses stream must not carry [DONE]: {text}");
    assert_eq!(captured(&log).len(), 1);

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].result, UsageResult::Failure, "{records:?}");

    stop_server().await.expect("stop listener");
}

/// Defect 4 (REQ-005/AC-007): nested unknown chat members must be rejected by
/// name before any attempt; they may not be silently dropped during conversion.
#[tokio::test]
async fn nested_unknown_chat_members_are_rejected() {
    let _home = temp_home("pool-nested-unknown-chat");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = chat_candidate_fixture(port, &tag).await;

    let mut tool = json!({"type": "function", "function": {"name": "weather", "parameters": {"type": "object"}}});
    tool["function"]["unknown_control"] = json!(1);

    let cases: Vec<(&str, Value, Vec<&str>)> = vec![
        (
            "tools.function.unknown_control",
            json!({"model": alias, "messages": [{"role": "user", "content": "hi"}], "tools": [tool]}),
            vec!["unknown_control", "function"],
        ),
        (
            "tool_choice.function.unknown_control",
            json!({"model": alias, "messages": [{"role": "user", "content": "hi"}],
                "tool_choice": {"type": "function", "function": {"name": "weather", "unknown_control": 1}}}),
            vec!["unknown_control", "tool_choice"],
        ),
        (
            "messages.unknown_control",
            json!({"model": alias, "messages": [{"role": "user", "content": "hi", "unknown_control": 1}]}),
            vec!["unknown_control", "messages"],
        ),
        (
            "response_format.unknown_control",
            json!({"model": alias, "messages": [{"role": "user", "content": "hi"}],
                "response_format": {"type": "text", "unknown_control": 1}}),
            vec!["unknown_control", "response_format"],
        ),
    ];

    for (label, body, tokens) in cases {
        let (status, _ct, text) = post(port, "/v1/chat/completions", body).await;
        assert_eq!(status, 400, "case {label}: a nested unknown member must be rejected: {text}");
        assert!(
            tokens.iter().any(|token| message_of(&text).contains(token)),
            "case {label}: the 400 must name the nested member: {text}"
        );
        assert!(captured(&log).is_empty(), "case {label}: no upstream attempt before rejection");
    }

    stop_server().await.expect("stop listener");
}

/// Defect 4 (REQ-005/AC-007): nested unknown Responses members must be rejected
/// by name before any attempt.
#[tokio::test]
async fn nested_unknown_responses_members_are_rejected() {
    let _home = temp_home("pool-nested-unknown-responses");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = responses_candidate_fixture(port, &tag).await;

    let cases: Vec<(&str, Value, Vec<&str>)> = vec![
        (
            "input.unknown_control",
            json!({"model": alias, "input": [{"role": "user", "content": "hi", "unknown_control": 1}]}),
            vec!["unknown_control", "input"],
        ),
        (
            "input.content.unknown_control",
            json!({"model": alias, "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi", "unknown_control": 1}]}]}),
            vec!["unknown_control", "content"],
        ),
        (
            "text.verbosity",
            json!({"model": alias, "input": "hi", "text": {"format": {"type": "text"}, "verbosity": "high"}}),
            vec!["verbosity", "text"],
        ),
    ];

    for (label, body, tokens) in cases {
        let (status, _ct, text) = post(port, "/v1/responses", body).await;
        assert_eq!(status, 400, "case {label}: a nested unknown member must be rejected: {text}");
        assert!(
            tokens.iter().any(|token| message_of(&text).contains(token)),
            "case {label}: the 400 must name the nested member: {text}"
        );
        assert!(captured(&log).is_empty(), "case {label}: no upstream attempt before rejection");
    }

    stop_server().await.expect("stop listener");
}

/// Defect 4 guard (REQ-005): a function's `parameters` is an arbitrary JSON
/// Schema and must survive untouched; only its sibling control members are
/// constrained.
#[tokio::test]
async fn function_parameter_schema_contents_stay_arbitrary() {
    let _home = temp_home("pool-params-arbitrary");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = responses_candidate_fixture(port, &tag).await;

    let parameters = json!({
        "type": "object",
        "properties": {
            "city": {"type": "string", "x-annotated": true},
            "x-nested": {"type": "object", "properties": {"deep": {"type": "integer"}}}
        },
        "additionalProperties": false,
        "x-arbitrary": "kept"
    });
    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{"type": "function", "function": {"name": "weather", "parameters": parameters.clone()}}]
        }),
    )
    .await;
    assert_eq!(status, 200, "a legitimate function JSON schema must be accepted: {text}");
    let sent = captured_body(&captured(&log)[0]);
    assert_eq!(
        sent["tools"][0]["parameters"], parameters,
        "the arbitrary JSON schema must be preserved: {sent}"
    );

    stop_server().await.expect("stop listener");
}

/// Defect 4 (REQ-004/AC-007): every supplied output limit is validated, even
/// when the precedence field is present; two non-negative integers stay
/// accepted and keep the Chat precedence.
#[tokio::test]
async fn chat_limit_validation_checks_every_supplied_limit() {
    let _home = temp_home("pool-limit-validation");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = responses_candidate_fixture(port, &tag).await;

    for (label, value) in [
        ("fractional", json!(1.5)),
        ("negative", json!(-1)),
        ("string", json!("10")),
    ] {
        let body = json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "max_completion_tokens": 0,
            "max_tokens": value
        });
        let (status, _ct, text) = post(port, "/v1/chat/completions", body).await;
        assert_eq!(
            status, 400,
            "limit case {label}: every supplied limit must be validated: {text}"
        );
        assert!(
            message_of(&text).contains("max_tokens"),
            "limit case {label}: the 400 must name max_tokens: {text}"
        );
        assert!(
            captured(&log).is_empty(),
            "limit case {label}: no upstream attempt before rejection"
        );
    }

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({
            "model": alias,
            "messages": [{"role": "user", "content": "hi"}],
            "max_completion_tokens": 5,
            "max_tokens": 9
        }),
    )
    .await;
    assert_eq!(status, 200, "two non-negative integer limits must be accepted: {text}");
    assert_eq!(
        captured_body(&captured(&log)[0])["max_output_tokens"], 5,
        "max_completion_tokens must keep precedence: {text}"
    );

    stop_server().await.expect("stop listener");
}

/// Defect 4 (REQ-005/AC-007): a non-boolean `stream` or `parallel_tool_calls`
/// must be rejected by name, not silently coerced or dropped.
#[tokio::test]
async fn chat_stream_and_parallel_boolean_typing_is_enforced() {
    let _home = temp_home("pool-boolean-typing");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = chat_candidate_fixture(port, &tag).await;

    for field in ["stream", "parallel_tool_calls"] {
        let mut body = json!({"model": alias, "messages": [{"role": "user", "content": "hi"}]});
        body[field] = json!("yes");
        let (status, _ct, text) = post(port, "/v1/chat/completions", body).await;
        assert_eq!(status, 400, "{field} must be type-checked: {text}");
        assert!(
            message_of(&text).contains(field),
            "the 400 must name {field}: {text}"
        );
        assert!(captured(&log).is_empty(), "{field} must not reach upstream: {text}");
    }

    stop_server().await.expect("stop listener");
}

/// Defect 4 (REQ-005): Chat `store`/`background` accept a boolean; false is
/// valid, true and a wrong type are field-named 400s.
#[tokio::test]
async fn chat_store_and_background_boolean_values_are_validated() {
    let _home = temp_home("pool-store-background-chat");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = chat_candidate_fixture(port, &tag).await;

    for (field, value, expect) in [
        ("store", json!(false), 200u16),
        ("background", json!(false), 200),
        ("store", json!(true), 400),
        ("background", json!(true), 400),
        ("store", json!("x"), 400),
    ] {
        let mut body = json!({"model": alias, "messages": [{"role": "user", "content": "hi"}]});
        body[field] = value.clone();
        let (status, _ct, text) = post(port, "/v1/chat/completions", body).await;
        assert_eq!(
            status, expect,
            "chat {field}={value} must be {expect}: {text}"
        );
        if expect == 400 {
            assert!(
                message_of(&text).contains(field),
                "the 400 must name {field}: {text}"
            );
        }
    }

    // The accepted false value stays on the forwarded body rather than being
    // silently dropped.
    let sent = captured_body(&captured(&log)[0]);
    assert_eq!(sent["store"], false, "a false store must be preserved: {sent}");

    stop_server().await.expect("stop listener");
}

/// Defect 5 (REQ-005/AC-007): a non-text Chat tool result must be rejected
/// before any upstream attempt; a text tool result stays accepted.
#[tokio::test]
async fn nontext_chat_tool_result_is_rejected_before_upstream() {
    let _home = temp_home("pool-nontext-chat-tool-result");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = chat_candidate_fixture(port, &tag).await;

    let image_result = json!({
        "model": alias,
        "messages": [
            {"role": "user", "content": "call the tool"},
            {"role": "assistant", "content": null, "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "weather", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "call_1", "content": [{"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}]}
        ]
    });
    let (status, _ct, text) = post(port, "/v1/chat/completions", image_result).await;
    assert_eq!(
        status, 400,
        "a non-text tool result must be rejected before any attempt: {text}"
    );
    assert!(
        message_of(&text).contains("image") || message_of(&text).contains("content"),
        "the 400 must name the non-text tool result: {text}"
    );
    assert!(
        captured(&log).is_empty(),
        "no upstream attempt may see the non-text tool result"
    );

    let text_result = json!({
        "model": alias,
        "messages": [
            {"role": "user", "content": "call the tool"},
            {"role": "assistant", "content": null, "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "weather", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "call_1", "content": "sunny"}
        ]
    });
    let (status, _ct, text) = post(port, "/v1/chat/completions", text_result).await;
    assert_eq!(status, 200, "a text tool result must stay accepted: {text}");

    stop_server().await.expect("stop listener");
}

/// Defect 5 (REQ-005/AC-007): a non-text Responses function_call_output must be
/// rejected before any upstream attempt; a text output stays accepted.
#[tokio::test]
async fn nontext_responses_tool_result_is_rejected_before_upstream() {
    let _home = temp_home("pool-nontext-responses-tool-result");
    let port = free_port().await;
    let tag = tag();
    let (alias, log) = responses_candidate_fixture(port, &tag).await;

    let image_result = json!({
        "model": alias,
        "input": [
            {"role": "user", "content": [{"type": "input_text", "text": "call the tool"}]},
            {"type": "function_call", "call_id": "call_1", "name": "weather", "arguments": "{}"},
            {"type": "function_call_output", "call_id": "call_1", "output": [{"type": "input_image", "image_url": "data:image/png;base64,AAAA"}]}
        ]
    });
    let (status, _ct, text) = post(port, "/v1/responses", image_result).await;
    assert_eq!(
        status, 400,
        "a non-text function output must be rejected before any attempt: {text}"
    );
    assert!(
        message_of(&text).contains("image") || message_of(&text).contains("output"),
        "the 400 must name the non-text tool output: {text}"
    );
    assert!(captured(&log).is_empty(), "no upstream attempt may see the non-text output");

    let text_result = json!({
        "model": alias,
        "input": [
            {"role": "user", "content": [{"type": "input_text", "text": "call the tool"}]},
            {"type": "function_call", "call_id": "call_1", "name": "weather", "arguments": "{}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "sunny"}
        ]
    });
    let (status, _ct, text) = post(port, "/v1/responses", text_result).await;
    assert_eq!(status, 200, "a text function output must stay accepted: {text}");

    stop_server().await.expect("stop listener");
}

/// Defect 7 guard (REQ-004): `strict` is representable between the two OpenAI
/// protocols and must be preserved in both directions.
#[tokio::test]
async fn strict_tool_control_passes_between_openai_protocols() {
    let _home = temp_home("pool-strict-openai");
    let port = free_port().await;
    let tag = tag();
    let r_id = format!("p-r-{tag}");
    let c_id = format!("p-c-{tag}");
    let r_alias = format!("free-r-{tag}");
    let c_alias = format!("free-c-{tag}");
    let (r_url, r_log) = spawn_mock_upstream(|_| MockReply::Json(200, responses_json("ok"))).await;
    let (c_url, c_log) = spawn_mock_upstream(|_| MockReply::Json(200, chat_json("ok"))).await;

    let r_provider = provider_value(&r_id, &r_url, "responses", vec![mapping("local-r", "remote-r", "responses")]);
    let c_provider = provider_value(&c_id, &c_url, "chat_completions", vec![mapping("local-c", "remote-c", "chat_completions")]);
    let r_pool = json!({"id": format!("pool-r-{tag}"), "name": "R", "model": r_alias, "enabled": true,
        "candidates": [candidate(&r_id, "local-r", "remote-r", "responses")]});
    let c_pool = json!({"id": format!("pool-c-{tag}"), "name": "C", "model": c_alias, "enabled": true,
        "candidates": [candidate(&c_id, "local-c", "remote-c", "chat_completions")]});
    write_raw_gateway_config(&pool_config_v3(port, vec![r_provider, c_provider], vec![r_pool, c_pool]));
    start_server(None).await.expect("start listener");

    let (status, _ct, text) = post(
        port,
        "/v1/chat/completions",
        json!({
            "model": r_alias,
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{"type": "function", "function": {"name": "weather", "parameters": {"type": "object"}, "strict": true}}]
        }),
    )
    .await;
    assert_eq!(status, 200, "a Chat strict tool must be accepted toward Responses: {text}");
    assert_eq!(
        captured_body(&captured(&r_log)[0])["tools"][0]["strict"], true,
        "Chat `strict` must survive the Responses conversion: {text}"
    );

    let (status, _ct, text) = post(
        port,
        "/v1/responses",
        json!({
            "model": c_alias,
            "input": "hi",
            "tools": [{"type": "function", "name": "weather", "parameters": {"type": "object"}, "strict": true}]
        }),
    )
    .await;
    assert_eq!(status, 200, "a Responses strict tool must be accepted toward Chat: {text}");
    assert_eq!(
        captured_body(&captured(&c_log)[0])["tools"][0]["function"]["strict"], true,
        "Responses `strict` must survive the Chat conversion: {text}"
    );

    stop_server().await.expect("stop listener");
}
