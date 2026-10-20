//! Step 1 RED behavior slice for `20261014-gateway-request-latency`
//! (REQ-002 / REQ-003, AC-002 / AC-003).
//!
//! The slice is deliberately written against stable public boundaries so it
//! fails behaviorally rather than by a compiler error before Backend lands the
//! implementation:
//!
//! * the loopback gateway HTTP entry (`POST /v1/chat/completions`,
//!   `/v1/responses`, `/v1/messages`) driven by real upstream SSE mocks;
//! * the public [`UsageLogStore`] query/inspection surface and the serialized
//!   [`UsageLogRecord`] payload (the same JSON the request-log command returns);
//! * raw SQLite reads of the `usage_logs` table for the additive migration.
//!
//! The observed first meaningful upstream output and the truthful average TPS
//! are expected to survive request-logging persistence as the nullable
//! `first_token_ms` (milliseconds) and `avg_tps` (tokens/second) fields. Before
//! Backend adds them, every measured assertion below fails because those
//! serialized numeric fields and columns are absent.
//!
//! Not tested here (kept out on purpose, see the plan Step 1 test scope):
//! private internals, collaborator mocks, the exact observer type name, and the
//! Step 2 frontend rendering. The injected-clock sub-millisecond / non-positive
//! interval cases that need explicit monotonic `Instant`s are reported to
//! Backend in the Test hand-off; the deterministic arithmetic at the persistence
//! boundary is covered here instead.

use crate::ai_gateway::output_timing::OutputTimingObserver;
use crate::ai_gateway::runtime_http::{start_server, stop_server};
use crate::ai_gateway::storage::write_config;
use crate::ai_gateway::{
    GATEWAY_CONFIG_SCHEMA_VERSION, GatewayConfig, GatewayUpstreamProvider, LogFilter, TimeRange,
    UpstreamProtocol, UsageLogRecord, UsageLogStore, UsageResult,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use super::{
    MockReply, call_gateway, config_with_key, free_port, make_temp_dir, mapping, pool_key,
    spawn_mock_upstream, temp_home, upstream_provider, usage_log_table_columns,
    wait_for_usage_logs, write_raw_gateway_config,
};

// ---------------------------------------------------------------------------
// Serialized-payload helpers: the frontend mirror and the request-log command
// both see a `UsageLogRecord` as JSON, so the numeric fields must be observable
// there without touching the (not yet existing) Rust fields directly.
// ---------------------------------------------------------------------------

fn serialized(record: &UsageLogRecord) -> Value {
    serde_json::to_value(record).expect("serialize usage log record")
}

/// Assert a measured timing field is present and numeric (never fabricated from
/// an absent value) and return it.
fn measured(record: &UsageLogRecord, key: &str) -> f64 {
    let value = serialized(record);
    value
        .get(key)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("`{key}` must serialize as a finite number for a measured row, got {value}"))
}

/// Assert a timing field is unavailable: a serialized `null` or an absent key,
/// never a numeric value.
fn assert_unavailable(record: &UsageLogRecord, key: &str) {
    let value = serialized(record);
    match value.get(key) {
        None => {}
        Some(entry) => assert!(
            entry.is_null(),
            "an unavailable `{key}` must be null or absent, never a number, got {value}"
        ),
    }
}

fn timing_column_present(path: &Path, column: &str) -> bool {
    usage_log_table_columns(path).iter().any(|name| name == column)
}

// ---------------------------------------------------------------------------
// Temporary database helpers.
// ---------------------------------------------------------------------------

fn temp_db(name: &str) -> (PathBuf, PathBuf) {
    let dir = make_temp_dir(name);
    fs::create_dir_all(&dir).expect("create temp dir");
    let db_path = dir.join("ai_gateway_usage.db");
    (dir, db_path)
}

/// Bring a database to the current schema through the store, then insert one row
/// that carries the new nullable timing columns directly.
#[allow(clippy::too_many_arguments)]
fn insert_timing_row(
    path: &Path,
    timestamp_ms: i64,
    local_model: &str,
    output_tokens: i64,
    duration_ms: i64,
    first_token_ms: Option<f64>,
    avg_tps: Option<f64>,
) {
    let connection = rusqlite::Connection::open(path).expect("open raw usage db");
    connection
        .execute(
            "INSERT INTO usage_logs (
                timestamp_ms, local_model, upstream_model, provider_id, provider_name,
                result, status, input_tokens, cache_read_tokens, cache_write_tokens,
                output_tokens, total_tokens, amount, duration_ms, first_token_ms, avg_tps
            ) VALUES (?, ?, 'remote-a', 'p1', 'Provider One', 'success', 200, 10, 0, 0, ?, ?, 0.0, ?, ?, ?)",
            rusqlite::params![
                timestamp_ms,
                local_model,
                output_tokens,
                output_tokens + 10,
                duration_ms,
                first_token_ms,
                avg_tps,
            ],
        )
        .expect("insert a row carrying the nullable `first_token_ms` / `avg_tps` columns");
}

/// The previous release's `usage_logs` table: none of the additive columns.
fn create_legacy_table(path: &Path) {
    let connection = rusqlite::Connection::open(path).expect("create legacy usage db");
    connection
        .execute_batch(
            "CREATE TABLE usage_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                local_model TEXT NOT NULL,
                upstream_model TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                result TEXT NOT NULL,
                status INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL,
                cache_read_tokens INTEGER NOT NULL,
                cache_write_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                amount REAL,
                duration_ms INTEGER NOT NULL
            );
            CREATE INDEX idx_usage_logs_timestamp ON usage_logs(timestamp_ms);
            CREATE INDEX idx_usage_logs_local_model ON usage_logs(local_model);",
        )
        .expect("create the pre-upgrade schema");
    connection
        .execute(
            "INSERT INTO usage_logs (
                timestamp_ms, local_model, upstream_model, provider_id, provider_name,
                result, status, input_tokens, cache_read_tokens, cache_write_tokens,
                output_tokens, total_tokens, amount, duration_ms
            ) VALUES (?, 'legacy-local', 'legacy-remote', 'p1', 'Provider One', 'success', 200, 10, 0, 0, 5, 15, 0.5, 42)",
            rusqlite::params![1_700_000_000_000i64],
        )
        .expect("insert the pre-upgrade row");
}

// ---------------------------------------------------------------------------
// Real chunked upstream SSE mock: the gateway reads a genuine upstream stream
// whose chunks arrive at controlled real-time offsets.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SseChunk {
    delay: Duration,
    bytes: Vec<u8>,
}

fn chunk(delay_ms: u64, body: &str) -> SseChunk {
    SseChunk {
        delay: Duration::from_millis(delay_ms),
        bytes: body.as_bytes().to_vec(),
    }
}

/// A `text/event-stream` upstream that writes each chunk after its own delay,
/// using chunked transfer encoding so the gateway observes each event at a
/// distinct time. Returns the base URL.
async fn spawn_chunked_sse_mock(chunks: Vec<SseChunk>) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind chunked mock");
    let addr = listener.local_addr().expect("chunked mock addr");
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let chunks = chunks.clone();
            tauri::async_runtime::spawn(async move {
                if crate::ai_gateway::runtime_http::read_http_request(&mut stream)
                    .await
                    .is_err()
                {
                    return;
                }
                let header = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n";
                if stream.write_all(header.as_bytes()).await.is_err() {
                    return;
                }
                for part in &chunks {
                    if !part.delay.is_zero() {
                        tokio::time::sleep(part.delay).await;
                    }
                    let prefix = format!("{:x}\r\n", part.bytes.len());
                    if stream.write_all(prefix.as_bytes()).await.is_err()
                        || stream.write_all(&part.bytes).await.is_err()
                        || stream.write_all(b"\r\n").await.is_err()
                    {
                        return;
                    }
                }
                let _ = stream.write_all(b"0\r\n\r\n").await;
                let _ = stream.flush().await;
            });
        }
    });
    format!("http://{}", addr)
}

// ---------------------------------------------------------------------------
// Chat Completions SSE builders.
// ---------------------------------------------------------------------------

fn chat_delta(delta: &str) -> String {
    format!("data: {{\"id\":\"chatcmpl\",\"choices\":[{{\"delta\":{delta}}}]}}\n\n")
}

fn chat_usage(output_literal: &str) -> String {
    format!(
        "data: {{\"id\":\"chatcmpl\",\"choices\":[],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":{output_literal}}}}}\n\n"
    )
}

const CHAT_DONE: &str = "data: [DONE]\n\n";

/// A Chat stream that emits only metadata first (heartbeat comment, role-only
/// and empty deltas), then one text delta, then usage and `[DONE]`.
///
/// `first_text_delay_ms` is the offset of the first meaningful output from the
/// attempt start; `generation_window_ms` is the delay after it until the usage
/// snapshot and stream end. The whole attempt -- the pre-output wait plus this
/// window -- is the average-TPS divisor.
fn chat_stream_with_first_text_at(
    first_text_delay_ms: u64,
    generation_window_ms: u64,
    output_literal: &str,
) -> Vec<SseChunk> {
    vec![
        chunk(0, ": keep-alive\n\n"),
        chunk(0, &chat_delta("{\"role\":\"assistant\"}")),
        chunk(0, &chat_delta("{}")),
        chunk(first_text_delay_ms, &chat_delta("{\"content\":\"hello\"}")),
        chunk(generation_window_ms, &chat_usage(output_literal)),
        chunk(0, CHAT_DONE),
    ]
}

fn configure_streaming_provider(
    port: u16,
    provider: GatewayUpstreamProvider,
    local_model: &str,
    upstream_model: &str,
) -> GatewayConfig {
    let mut config = config_with_key(port);
    let mut provider = provider;
    provider.mappings = vec![mapping(local_model, upstream_model, None)];
    config.providers.push(provider);
    config
}

async fn stream_request(
    port: u16,
    path: &str,
    local_model: &str,
) -> (u16, String, String) {
    call_gateway(
        port,
        "POST",
        path,
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": local_model, "stream": true})),
    )
    .await
}

// ---------------------------------------------------------------------------
// AC-002 primary: 1 s first output, 5 s stream end, 200 output tokens => 40 TPS.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chat_stream_persists_first_token_and_average_tps_after_reopen() {
    let home = temp_home("output-timing-chat-primary");
    let port = free_port().await;
    // First output at ~1 s, stream end at ~1 s + 4 s = ~5 s, 200 tokens.
    let upstream_url =
        spawn_chunked_sse_mock(chat_stream_with_first_text_at(1_000, 4_000, "200")).await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-a", "remote-a")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-a").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-a" && record.terminal)
        .expect("the completed stream must persist one terminal row");
    assert_eq!(record.output_tokens, 200, "the reported output count is stored");

    let first = measured(record, "first_token_ms");
    assert!(
        (600.0..=2_000.0).contains(&first),
        "first-token latency must be observed from the attempt start to the first meaningful \
         output (~1000 ms), got {first}"
    );
    let tps = measured(record, "avg_tps");
    let whole_attempt = record.output_tokens as f64 / (record.duration_ms as f64 / 1000.0);
    assert!(
        (tps - whole_attempt).abs() < 1e-6,
        "200 tokens over the measured whole attempt (~5 s) must be ~40 t/s, got {tps}"
    );
    let window_rate =
        record.output_tokens as f64 / ((record.duration_ms as f64 - first) / 1000.0);
    assert!(
        tps < window_rate - 5.0,
        "the rate must not divide by the ~1 s first-output-to-end window \
         (~{window_rate} t/s), got {tps}"
    );

    // It survives a database reopen: the value is persisted, not only cached.
    let reopened = UsageLogStore::default_store().expect("reopen default usage store");
    let restored = reopened.all_records().expect("read the reopened row");
    let restored = restored
        .iter()
        .find(|row| row.local_model == "local-a")
        .expect("the row must survive reopen");
    assert!(
        (measured(restored, "first_token_ms") - first).abs() < 1e-9,
        "the persisted first-token latency must survive reopen"
    );
    assert!(
        (measured(restored, "avg_tps") - tps).abs() < 1e-9,
        "the whole-attempt TPS must survive reopen"
    );

    stop_server().await.unwrap();
    drop(home);
}

/// The persisted TPS must divide by the whole attempt, never by the
/// first-output-to-end window that drops the pre-output wait.
#[tokio::test]
async fn chat_stream_tps_uses_whole_attempt_not_generation_window() {
    let home = temp_home("output-timing-chat-window");
    let port = free_port().await;
    // First text at ~2.0 s, stream end at ~2.0 s + 1.0 s => ~3.0 s whole
    // attempt. 400 tokens / 3.0 s = ~133 t/s. The obsolete first-output-to-end
    // divisor would give 400/1.0 = ~400 t/s.
    let upstream_url =
        spawn_chunked_sse_mock(chat_stream_with_first_text_at(2_000, 1_000, "400")).await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-w", "remote-w")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-w").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-w" && record.terminal)
        .expect("one terminal row");

    let first = measured(record, "first_token_ms");
    assert!(
        (1_500.0..=2_700.0).contains(&first),
        "first-token latency must track the ~2000 ms first output, got {first}"
    );
    let tps = measured(record, "avg_tps");
    let whole_attempt = record.output_tokens as f64 / (record.duration_ms as f64 / 1000.0);
    assert!(
        (tps - whole_attempt).abs() < 1e-6,
        "400 tokens over the measured whole attempt (~3 s) must be ~133 t/s, got {tps}"
    );
    let window_rate =
        record.output_tokens as f64 / ((record.duration_ms as f64 - first) / 1000.0);
    assert!(
        window_rate > 250.0,
        "the fixture must keep the obsolete generation window distinctly faster, got {window_rate}"
    );
    assert!(
        tps < window_rate - 100.0,
        "the rate must not divide by the ~1 s first-output-to-end window \
         (~{window_rate} t/s), got {tps}"
    );

    stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// AC-002 counterexamples: metadata prefixes, whitespace and non-text output.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metadata_heartbeat_and_empty_deltas_do_not_set_first_output() {
    let home = temp_home("output-timing-metadata-first");
    let port = free_port().await;
    let upstream_url =
        spawn_chunked_sse_mock(chat_stream_with_first_text_at(800, 900, "5")).await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-m", "remote-m")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-m").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-m" && record.terminal)
        .expect("one terminal row");
    let first = measured(record, "first_token_ms");
    assert!(
        first >= 600.0,
        "the leading heartbeat/role/empty metadata must not become the first observation; the \
         first meaningful text arrived at ~800 ms, got {first}"
    );

    stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn whitespace_only_content_is_a_meaningful_first_output() {
    let home = temp_home("output-timing-whitespace");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(vec![
        chunk(0, &chat_delta("{\"role\":\"assistant\"}")),
        chunk(300, &chat_delta("{\"content\":\" \"}")),
        chunk(50, &chat_usage("3")),
        chunk(0, CHAT_DONE),
    ])
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-sp", "remote-sp")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-sp").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-sp" && record.terminal)
        .expect("one terminal row");
    let first = measured(record, "first_token_ms");
    assert!(
        first < 2_000.0,
        "a whitespace-only delta is still output and must set a first-token observation, got {first}"
    );

    stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn reasoning_delta_can_be_the_first_meaningful_output() {
    let home = temp_home("output-timing-reasoning");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(vec![
        chunk(0, &chat_delta("{\"role\":\"assistant\"}")),
        chunk(
            500,
            &chat_delta("{\"reasoning_content\":\"thinking about it\"}"),
        ),
        chunk(200, &chat_usage("7")),
        chunk(0, CHAT_DONE),
    ])
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-r", "remote-r")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-r").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-r" && record.terminal)
        .expect("one terminal row");
    let first = measured(record, "first_token_ms");
    assert!(
        (350.0..=1_500.0).contains(&first),
        "a reasoning/thinking delta is meaningful output and must set the first observation \
         at ~500 ms, got {first}"
    );

    stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn generated_tool_call_can_be_the_first_meaningful_output() {
    let home = temp_home("output-timing-tool");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(vec![
        chunk(0, &chat_delta("{\"role\":\"assistant\"}")),
        chunk(
            450,
            &chat_delta(
                "{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\
                  \"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]}",
            ),
        ),
        chunk(
            100,
            &chat_delta("{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\\\"SF\\\"}\"}}]}"),
        ),
        chunk(100, &chat_usage("9")),
        chunk(0, CHAT_DONE),
    ])
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-t", "remote-t")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-t").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-t" && record.terminal)
        .expect("one terminal row");
    let first = measured(record, "first_token_ms");
    assert!(
        (300.0..=1_400.0).contains(&first),
        "a generated tool name/arguments is meaningful output and must set the first \
         observation, got {first}"
    );

    stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// AC-002 / AC-003: Responses and Anthropic upstream protocols, native and
// converted.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn responses_stream_persists_timing() {
    let home = temp_home("output-timing-responses");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(vec![
        chunk(0, "event: response.created\ndata: {\"type\":\"response.created\"}\n\n"),
        chunk(
            700,
            "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n",
        ),
        chunk(
            400,
            "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"output\":[],\"usage\":{\"input_tokens\":11,\"output_tokens\":64,\"input_tokens_details\":{\"cached_tokens\":0}}}}\n\n",
        ),
    ])
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    let mut config = config_with_key(port);
    let mut provider = provider;
    provider.protocol = UpstreamProtocol::Responses;
    provider.mappings = vec![mapping("local-resp", "remote-resp", None)];
    config.providers.push(provider);
    write_config(&config).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/responses", "local-resp").await;
    assert_eq!(status, 200, "the Responses stream must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-resp" && record.terminal)
        .expect("one terminal row");
    assert_eq!(record.output_tokens, 64, "the Responses output count is stored");
    let first = measured(record, "first_token_ms");
    assert!(
        (400.0..=2_000.0).contains(&first),
        "the first Responses output delta must set the first observation (~700 ms), got {first}"
    );
    let tps = measured(record, "avg_tps");
    assert!(tps > 0.0, "a measured whole attempt must yield a positive TPS, got {tps}");

    stop_server().await.unwrap();
    drop(home);
}

fn anthropic_native_stream_chunks(first_text_delay_ms: u64, total_delay_ms: u64) -> Vec<SseChunk> {
    vec![
        chunk(
            0,
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
        ),
        chunk(
            first_text_delay_ms,
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
        ),
        chunk(
            total_delay_ms,
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":50}}\n\n",
        ),
        chunk(0, "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"),
    ]
}

#[tokio::test]
async fn anthropic_native_messages_stream_persists_timing() {
    let home = temp_home("output-timing-anthropic-native");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(anthropic_native_stream_chunks(900, 1_300)).await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    let mut config = config_with_key(port);
    let mut provider = provider;
    provider.protocol = UpstreamProtocol::AnthropicMessages;
    provider.mappings = vec![mapping("local-native", "claude-native", None)];
    config.providers.push(provider);
    write_config(&config).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/messages",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-native", "max_tokens": 16, "stream": true,
                    "messages": [{"role": "user", "content": "hi"}]})),
    )
    .await;
    assert_eq!(status, 200, "the native Messages stream must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-native" && record.terminal)
        .expect("one terminal row");
    assert_eq!(record.output_tokens, 50, "the cumulative Messages output count is stored");
    let first = measured(record, "first_token_ms");
    assert!(
        (500.0..=2_000.0).contains(&first),
        "the first native content delta must set the first observation (~900 ms), got {first}"
    );
    let tps = measured(record, "avg_tps");
    assert!(tps > 0.0, "a measured native whole attempt must yield TPS, got {tps}");

    stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn converted_openai_client_from_anthropic_upstream_persists_timing() {
    let home = temp_home("output-timing-anthropic-converted");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(anthropic_native_stream_chunks(700, 1_100)).await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    let mut config = config_with_key(port);
    let mut provider = provider;
    provider.protocol = UpstreamProtocol::AnthropicMessages;
    provider.mappings = vec![mapping("local-conv", "claude-conv", None)];
    config.providers.push(provider);
    write_config(&config).unwrap();
    start_server(None).await.unwrap();

    // An OpenAI Chat client path served by an Anthropic Messages upstream is a
    // converted request; the observation must use the actual upstream protocol.
    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-conv").await;
    assert_eq!(status, 200, "the converted stream must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-conv" && record.terminal)
        .expect("one terminal row");
    let first = measured(record, "first_token_ms");
    assert!(
        (400.0..=1_800.0).contains(&first),
        "the converted stream must still observe the real upstream first output, got {first}"
    );
    let tps = measured(record, "avg_tps");
    assert!(tps > 0.0, "the converted stream must yield a truthful TPS, got {tps}");

    stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// AC-003: unavailable / zero / malformed output must stay distinct.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn non_streaming_buffered_response_has_no_invented_timing() {
    let home = temp_home("output-timing-non-streaming");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "chatcmpl",
                "choices": [{"message": {"role": "assistant", "content": "ok"}}],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5}
            }),
        )
    })
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    let mut config = config_with_key(port);
    let mut provider = provider;
    provider.mappings = vec![mapping("local-ns", "remote-ns", None)];
    config.providers.push(provider);
    write_config(&config).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-ns"})),
    )
    .await;
    assert_eq!(status, 200, "the buffered response must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-ns" && record.terminal)
        .expect("one terminal row");
    assert_unavailable(record, "first_token_ms");
    assert_unavailable(record, "avg_tps");

    stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn missing_output_count_keeps_first_token_but_leaves_tps_unavailable() {
    let home = temp_home("output-timing-missing-usage");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(vec![
        chunk(0, &chat_delta("{\"role\":\"assistant\"}")),
        chunk(300, &chat_delta("{\"content\":\"hello\"}")),
        chunk(0, CHAT_DONE),
    ])
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-mu", "remote-mu")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-mu").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-mu" && record.terminal)
        .expect("one terminal row");
    let first = measured(record, "first_token_ms");
    assert!(first > 0.0, "observed output must keep a first-token latency");
    assert_unavailable(record, "avg_tps");

    stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn malformed_output_count_cannot_become_zero_tps() {
    let home = temp_home("output-timing-malformed-usage");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(vec![
        chunk(0, &chat_delta("{\"role\":\"assistant\"}")),
        chunk(300, &chat_delta("{\"content\":\"hello\"}")),
        chunk(300, &chat_usage("\"not-a-number\"")),
        chunk(0, CHAT_DONE),
    ])
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-mal", "remote-mal")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-mal").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-mal" && record.terminal)
        .expect("one terminal row");
    assert!(
        measured(record, "first_token_ms") > 0.0,
        "observed output must keep a first-token latency"
    );
    assert_unavailable(record, "avg_tps");

    stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn explicit_zero_output_count_yields_zero_tps_and_keeps_first_token() {
    let home = temp_home("output-timing-explicit-zero");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(chat_stream_with_first_text_at(300, 500, "0")).await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-z", "remote-z")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-z").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-z" && record.terminal)
        .expect("one terminal row");
    assert!(
        measured(record, "first_token_ms") > 0.0,
        "a measured zero first-token offset is impossible here; observed output still sets it"
    );
    let tps = measured(record, "avg_tps");
    assert!(
        tps.abs() < 1e-9,
        "an explicitly reported zero output count over a positive whole attempt must be \
         0.00 t/s, got {tps}"
    );

    stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn numeric_string_output_count_is_accepted() {
    let home = temp_home("output-timing-numeric-string");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(chat_stream_with_first_text_at(300, 900, "\"200\"")).await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-str", "remote-str")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-str").await;
    assert_eq!(status, 200, "the streamed request must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-str" && record.terminal)
        .expect("one terminal row");
    assert_eq!(record.output_tokens, 200, "a numeric-string count is a valid reported count");
    let tps = measured(record, "avg_tps");
    assert!(tps > 0.0, "a valid numeric-string count must produce a TPS, got {tps}");

    stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// AC-002: retry-local attempts keep their own timing.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retry_attempts_keep_their_own_timing() {
    let home = temp_home("output-timing-retry");
    let port = free_port().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_mock = calls.clone();
    let (upstream_url, _log) = spawn_mock_upstream(move |_| {
        let attempt = calls_for_mock.fetch_add(1, Ordering::SeqCst);
        if attempt == 0 {
            MockReply::Json(500, json!({"error": {"message": "busy"}}))
        } else {
            let mut sse = String::new();
            sse.push_str(&chat_delta("{\"role\":\"assistant\"}"));
            sse.push_str(&chat_delta("{\"content\":\"recovered\"}"));
            sse.push_str(&chat_usage("30"));
            sse.push_str(CHAT_DONE);
            MockReply::Stream(sse)
        }
    })
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-retry", "remote-retry"))
        .unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-retry").await;
    assert_eq!(status, 200, "the retried stream must recover: {text}");

    let records = wait_for_usage_logs(2).await;
    let failure = records
        .iter()
        .find(|record| record.result == UsageResult::Failure)
        .expect("the failed first attempt must be logged");
    assert_unavailable(failure, "first_token_ms");
    assert_unavailable(failure, "avg_tps");

    let success = records
        .iter()
        .find(|record| record.result == UsageResult::Success)
        .expect("the successful retry must be logged");
    assert!(
        measured(success, "first_token_ms") > 0.0,
        "the successful retry must carry its own observed first token"
    );
    assert!(
        measured(success, "avg_tps") > 0.0,
        "the successful retry must carry its own TPS"
    );

    stop_server().await.unwrap();
    drop(home);
}

// NOTE: the unchanged downstream-cancellation boundary is already covered by the
// existing regression `attempt_buffer_discarded_on_downstream_disconnect` in
// `tests.rs`; it is intentionally not duplicated here.

// ---------------------------------------------------------------------------
// Deterministic persistence boundaries (no clock involved).
// ---------------------------------------------------------------------------

/// A measured read recomputes average TPS from the stored output count and the
/// whole `duration_ms`; the legacy stored `avg_tps` is only an availability
/// marker and must never be projected. A fractional millisecond duration must
/// stay fractional and never collapse to a whole-second divisor.
#[test]
fn measured_rows_recalculate_whole_attempt_tps_from_duration_ms() {
    let (dir, db_path) = temp_db("output-timing-roundtrip");
    // Opening through the store creates and migrates the schema.
    let store = UsageLogStore::at(&db_path);
    store.count().expect("initialize the usage database");

    // 200 tokens / the 5.000 s whole attempt = 40.00 t/s; the raw column keeps the
    // obsolete generation-window 50.00 that must never be projected.
    insert_timing_row(
        &db_path,
        1_700_000_000_000,
        "exact",
        200,
        5_000,
        Some(1_000.0),
        Some(50.0),
    );
    // 200 tokens / the 3.500 s whole attempt = 57.142857... t/s, unrounded.
    insert_timing_row(
        &db_path,
        1_700_000_000_001,
        "fractional",
        200,
        3_500,
        Some(1_500.0),
        Some(50.0),
    );

    let records = store.all_records().expect("read the timing rows");
    let exact = records.iter().find(|row| row.local_model == "exact").expect("exact row");
    assert_eq!(measured(exact, "first_token_ms"), 1_000.0);
    assert_eq!(
        measured(exact, "avg_tps"),
        40.0,
        "a measured read must recompute 200 tokens / the 5.000 s whole attempt = 40.00 t/s, \
         not the stored 50.00"
    );

    let fractional = records
        .iter()
        .find(|row| row.local_model == "fractional")
        .expect("fractional row");
    let tps = measured(fractional, "avg_tps");
    assert!(
        (tps - 200.0 / 3.5).abs() < 1e-9,
        "200 tokens / the 3.5 s whole attempt must round-trip unrounded, got {tps}"
    );
    assert!(
        (tps - 200.0 / 4.0).abs() > 1.0 && (tps - 200.0 / 3.0).abs() > 1.0,
        "the fractional whole attempt must not collapse to a rounded-second divisor, got {tps}"
    );
    assert!(
        (tps - 200.0 / 2.0).abs() > 1.0,
        "the read must not fall back to the 2 s first-output-to-end window, got {tps}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn zero_first_offset_and_zero_tps_stay_distinct_from_unknown() {
    let (dir, db_path) = temp_db("output-timing-zero-vs-unknown");
    let store = UsageLogStore::at(&db_path);
    store.count().expect("initialize the usage database");

    insert_timing_row(&db_path, 1_700_000_000_000, "zero", 0, 2_000, Some(0.0), Some(0.0));
    insert_timing_row(&db_path, 1_700_000_000_001, "unknown", 5, 2_000, None, None);

    let records = store.all_records().expect("read the rows");
    let zero = records.iter().find(|row| row.local_model == "zero").expect("zero row");
    assert_eq!(
        measured(zero, "first_token_ms"),
        0.0,
        "a measured zero first-token offset must be retained, not treated as unknown"
    );
    assert_eq!(
        measured(zero, "avg_tps"),
        0.0,
        "an explicit zero TPS must be retained, not treated as unknown"
    );

    let unknown = records
        .iter()
        .find(|row| row.local_model == "unknown")
        .expect("unknown row");
    assert_unavailable(unknown, "first_token_ms");
    assert_unavailable(unknown, "avg_tps");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn single_and_batch_queries_expose_timing_for_filtered_pages() {
    let (dir, db_path) = temp_db("output-timing-query");
    let store = UsageLogStore::at(&db_path);
    store.count().expect("initialize the usage database");

    insert_timing_row(&db_path, 3, "page-a", 200, 5_000, Some(1_000.0), Some(50.0));
    insert_timing_row(&db_path, 2, "page-b", 100, 2_000, Some(500.0), Some(100.0));
    insert_timing_row(&db_path, 1, "page-c", 50, 1_000, Some(250.0), Some(50.0));

    let filter = LogFilter {
        status: None,
        model: Some("page-a".to_string()),
        provider: None,
    };
    let page = store
        .query_logs(&TimeRange::default(), &filter, 1)
        .expect("query the filtered page");
    assert_eq!(page.records.len(), 1, "the model filter must select exactly one row");
    let record = &page.records[0];
    assert_eq!(record.local_model, "page-a");
    assert_eq!(measured(record, "first_token_ms"), 1_000.0);
    assert_eq!(
        measured(record, "avg_tps"),
        40.0,
        "the filtered page must project the whole-attempt 200 / 5.000 s = 40.00 t/s"
    );

    let all = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .expect("query the ungrouped page");
    assert_eq!(all.records.len(), 3, "all inserted rows are visible");
    assert!(
        all.records.iter().all(|row| serialized(row).get("avg_tps").is_some()),
        "every measured row must project the timing field"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The direct fix: a row measured under the obsolete first-output-to-end divisor
/// is re-projected from its stored whole `duration_ms` and `output_tokens`
/// without rewriting the database. The raw `avg_tps` column keeps the old value;
/// only the read projection changes. Rows without a measured TPS stay unknown.
#[test]
fn legacy_stored_tps_reprojects_whole_attempt_without_database_rewrite() {
    let (dir, db_path) = temp_db("output-timing-legacy-tps-reproject");
    let store = UsageLogStore::at(&db_path);
    store.count().expect("initialize the usage database");

    // The exact logged attempt: 1739 output tokens over a 41.830 s whole attempt
    // with a 41622.7707 ms first-token offset. The old code stored the
    // generation-window rate 8449.763198666167 t/s; the whole-attempt rate is
    // 1739 / 41.83 = 41.573... t/s (displayed 41.57).
    insert_timing_row(
        &db_path,
        1_700_000_000_000,
        "legacy-tps",
        1_739,
        41_830,
        Some(41_622.7707),
        Some(8_449.763_198_666_167),
    );
    // A measured row whose output count was never captured keeps a null TPS.
    insert_timing_row(
        &db_path,
        1_700_000_000_001,
        "legacy-unknown",
        100,
        1_000,
        None,
        None,
    );

    let expected = 1_739.0 / (41_830.0 / 1_000.0);
    let filter = LogFilter {
        status: None,
        model: Some("legacy-tps".to_string()),
        provider: None,
    };
    let page = store
        .query_logs(&TimeRange::default(), &filter, 1)
        .expect("query the legacy row");
    assert_eq!(page.records.len(), 1, "the model filter must select the legacy row");
    let record = &page.records[0];
    let first = measured(record, "first_token_ms");
    assert!(
        (first - 41_622.7707).abs() < 1e-9,
        "the observed first-token latency must be preserved, got {first}"
    );
    let tps = measured(record, "avg_tps");
    assert!(
        (tps - expected).abs() < 1e-9,
        "the read must project 1739 / 41.83 = {expected} t/s, not the stored \
         generation-window rate, got {tps}"
    );
    assert!(
        (tps - 8_449.763_198_666_167).abs() > 1.0,
        "the obsolete stored rate must not be projected, got {tps}"
    );

    // The raw columns are never rewritten: only the read projection changed.
    let raw = rusqlite::Connection::open(&db_path).expect("open raw usage db");
    let stored: (Option<f64>, Option<f64>) = raw
        .query_row(
            "SELECT avg_tps, first_token_ms FROM usage_logs WHERE local_model = 'legacy-tps'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read the raw stored timing columns");
    assert!(
        (stored.0.expect("stored avg_tps") - 8_449.763_198_666_167).abs() < 1e-9,
        "the raw stored avg_tps must stay the old value, got {:?}",
        stored.0
    );

    // A reopen projects the same recalculated value and keeps the unknown row null.
    let reopened = UsageLogStore::at(&db_path);
    let restored = reopened.all_records().expect("read after reopen");
    let restored = restored
        .iter()
        .find(|row| row.local_model == "legacy-tps")
        .expect("the legacy row survives reopen");
    assert!(
        (measured(restored, "avg_tps") - expected).abs() < 1e-9,
        "the recalculated whole-attempt TPS must survive a database reopen"
    );
    let unknown = reopened
        .all_records()
        .expect("read the unknown row")
        .into_iter()
        .find(|row| row.local_model == "legacy-unknown")
        .expect("the unknown row must exist");
    assert_unavailable(&unknown, "first_token_ms");
    assert_unavailable(&unknown, "avg_tps");

    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// AC-003: additive migration, reopen and old-projection append.
// ---------------------------------------------------------------------------

#[test]
fn legacy_schema_reopen_adds_nullable_timing_columns_and_keeps_rows() {
    let (dir, db_path) = temp_db("output-timing-legacy");
    create_legacy_table(&db_path);
    assert!(
        !timing_column_present(&db_path, "first_token_ms"),
        "the fixture must start without the new columns"
    );

    // Opening through any store operation migrates the file.
    let store = UsageLogStore::at(&db_path);
    let first_open = store.all_records().expect("read the migrated rows");
    assert_eq!(first_open.len(), 1, "the pre-upgrade row must survive");
    assert_eq!(first_open[0].output_tokens, 5, "existing row data must be preserved");
    assert_eq!(first_open[0].duration_ms, 42, "existing duration must be preserved");
    assert_unavailable(&first_open[0], "first_token_ms");
    assert_unavailable(&first_open[0], "avg_tps");

    assert!(
        timing_column_present(&db_path, "first_token_ms"),
        "the migration must add the nullable `first_token_ms` column"
    );
    assert!(
        timing_column_present(&db_path, "avg_tps"),
        "the migration must add the nullable `avg_tps` column"
    );
    let columns = usage_log_table_columns(&db_path);
    let distinct: std::collections::HashSet<&String> = columns.iter().collect();
    assert_eq!(columns.len(), distinct.len(), "no column may be duplicated: {columns:?}");

    // A second open must neither duplicate the columns nor change the rows.
    let second = UsageLogStore::at(&db_path);
    let second_open = second.all_records().expect("read through a second open");
    assert_eq!(second_open.len(), 1, "the second open must see the same single row");
    assert_eq!(
        usage_log_table_columns(&db_path),
        columns,
        "a second open must not change the schema"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn old_projection_append_keeps_timing_null_and_old_data_intact() {
    let (dir, db_path) = temp_db("output-timing-old-projection");
    let store = UsageLogStore::at(&db_path);
    store.count().expect("initialize the usage database");

    // An explicit-column insert from older code that omits the new columns must
    // still succeed; the new measurements stay unavailable.
    let connection = rusqlite::Connection::open(&db_path).expect("open raw usage db");
    connection
        .execute(
            "INSERT INTO usage_logs (
                timestamp_ms, local_model, upstream_model, provider_id, provider_name,
                result, status, input_tokens, cache_read_tokens, cache_write_tokens,
                output_tokens, total_tokens, amount, duration_ms
            ) VALUES (?, 'old-proj', 'remote-old', 'p1', 'Provider One', 'success', 200, 3, 0, 0, 7, 10, 0.1, 9)",
            rusqlite::params![1_700_000_000_000i64],
        )
        .expect("an old-projection insert must still succeed");
    drop(connection);

    let records = store.all_records().expect("read the old-projection row");
    let record = records
        .iter()
        .find(|row| row.local_model == "old-proj")
        .expect("the old-projection row must be readable");
    assert_eq!(record.output_tokens, 7, "old data must be preserved");
    assert_eq!(record.duration_ms, 9, "old duration must be preserved");
    assert_unavailable(record, "first_token_ms");
    assert_unavailable(record, "avg_tps");

    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Deterministic calculator evidence (AC-002): the whole per-attempt elapsed time
// and the reported output count, exercised through the same stable
// `OutputTimingObserver` boundary the production relay uses. These are the
// independent arithmetic proofs the persisted-SQL rows above cannot provide:
// they feed explicit monotonic `Instant`s and assert fixed expected values, so a
// rounded, clamped or formatted divisor produces a different number.
// ---------------------------------------------------------------------------

fn instant(base: std::time::Instant, offset: Duration) -> tokio::time::Instant {
    tokio::time::Instant::from_std(base + offset)
}

fn sse_event(data: &str, crlf: bool) -> Vec<u8> {
    if crlf {
        format!("data: {data}\r\n\r\n").into_bytes()
    } else {
        format!("data: {data}\n\n").into_bytes()
    }
}

fn chat_event(delta: Value) -> Vec<u8> {
    sse_event(
        &json!({"id": "chatcmpl", "choices": [{"delta": delta}]}).to_string(),
        false,
    )
}

/// Whether one complete upstream event sets the first meaningful observation.
fn observer_observes(protocol: UpstreamProtocol, event: &[u8]) -> bool {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let mut observer = OutputTimingObserver::new(protocol, started);
    observer.feed_at(event, instant(base, Duration::from_millis(5)));
    observer.finish_at(instant(base, Duration::from_millis(10)));
    observer.first_token_ms().is_some()
}

#[test]
fn observer_calculates_the_exact_whole_attempt_rate() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let first = instant(base, Duration::from_millis(1_000));
    let end = instant(base, Duration::from_millis(5_000));

    let mut observer = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    observer.feed_at(&chat_event(json!({"content": "hello"})), first);
    observer.finish_at(end);

    assert_eq!(
        observer.first_token_ms(),
        Some(1_000.0),
        "attempt start at 0 s with first output at 1 s is a 1000 ms offset"
    );
    let tps = observer
        .average_tps(Some(200))
        .expect("200 tokens over the positive 5 s whole attempt is a valid rate");
    assert!(
        (tps - 40.0).abs() < 1e-9,
        "200 tokens / the whole 5 s attempt = 40.00 t/s, not the 4 s \
         first-output-to-end window, got {tps}"
    );
    assert_eq!(
        observer.timing(Some(200)),
        (Some(1_000.0), Some(40.0)),
        "timing() must return the same first-token and whole-attempt TPS pair"
    );
}

#[test]
fn observer_fractional_whole_attempt_is_never_rounded_to_whole_seconds() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let first = instant(base, Duration::from_millis(1_500));
    let end = instant(base, Duration::from_millis(3_500));

    let mut observer = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    observer.feed_at(&chat_event(json!({"content": "x"})), first);
    observer.finish_at(end);

    assert_eq!(observer.first_token_ms(), Some(1_500.0));
    let tps = observer
        .average_tps(Some(200))
        .expect("a positive 3.5 s whole attempt is a valid denominator");
    assert!(
        (tps - 200.0 / 3.5).abs() < 1e-9,
        "200 tokens / the whole 3.5 s attempt = {:.9}, got {tps}",
        200.0 / 3.5
    );
    assert!(
        (tps - 200.0 / 4.0).abs() > 1.0,
        "the fractional whole attempt must not collapse to a rounded 4 s divisor, got {tps}"
    );
    assert!(
        (tps - 200.0 / 3.0).abs() > 1.0,
        "the fractional whole attempt must not collapse to a rounded 3 s divisor, got {tps}"
    );
    assert!(
        (tps - 200.0 / 2.0).abs() > 1.0,
        "the divisor must be the 3.5 s whole attempt, not the 2 s first-output-to-end \
         window, got {tps}"
    );
}

#[test]
fn observer_sub_millisecond_whole_attempt_is_neither_rounded_nor_clamped() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let first = instant(base, Duration::from_micros(100));
    let end = instant(base, Duration::from_micros(600));

    let mut observer = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    observer.feed_at(&chat_event(json!({"content": "x"})), first);
    observer.finish_at(end);

    let first_ms = observer
        .first_token_ms()
        .expect("a 100 microsecond first-token offset is observed");
    assert!(
        (first_ms - 0.1).abs() < 1e-9,
        "a 100 microsecond first-token offset is a valid unrounded observation, got {first_ms}"
    );
    let tps = observer
        .average_tps(Some(200))
        .expect("a 0.6 ms whole attempt is a positive interval");
    assert!(
        (tps - 200.0 / 0.0006).abs() < 1.0,
        "200 tokens / the 0.0006 s whole attempt = {:.3} t/s, got {tps}",
        200.0 / 0.0006
    );
    assert!(
        (tps - 200_000.0).abs() > 1_000.0,
        "the divisor must not be clamped to the 1 ms floor, got {tps}"
    );
    assert!(
        (tps - 400_000.0).abs() > 1_000.0,
        "the divisor must be the 0.6 ms whole attempt, not the 0.5 ms \
         first-output-to-end window, got {tps}"
    );
}

#[test]
fn observer_zero_or_non_positive_whole_attempt_has_no_tps() {
    let base = std::time::Instant::now();

    // Zero-length whole attempt: the attempt starts and ends with its first output.
    let started = tokio::time::Instant::from_std(base);
    let mut zero = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    zero.feed_at(&chat_event(json!({"content": "x"})), started);
    zero.finish_at(started);
    assert_eq!(zero.first_token_ms(), Some(0.0));
    assert_eq!(
        zero.average_tps(Some(200)),
        None,
        "a zero-length whole attempt yields no TPS"
    );

    // Non-positive whole attempt: the attempted end precedes the attempt start.
    let late_start = instant(base, Duration::from_millis(1_000));
    let mut negative = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, late_start);
    negative.feed_at(&chat_event(json!({"content": "x"})), late_start);
    negative.finish_at(instant(base, Duration::from_millis(500)));
    assert_eq!(
        negative.average_tps(Some(200)),
        None,
        "a non-positive whole attempt yields no TPS"
    );

    // No first output at all.
    let started = tokio::time::Instant::from_std(base);
    let mut none = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    none.finish_at(instant(base, Duration::from_millis(3_000)));
    assert_eq!(none.first_token_ms(), None);
    assert_eq!(none.average_tps(Some(200)), None);
}

/// An all-at-once stream can have its first meaningful output arrive together
/// with the attempt end. That is a zero generation window, but the whole attempt
/// from its start to that end is positive, so the full-attempt rate remains
/// measurable.
#[test]
fn observer_uses_whole_attempt_when_first_output_matches_attempt_end() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let all_at_once = instant(base, Duration::from_millis(2_000));

    let mut observer = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    observer.feed_at(&chat_event(json!({"content": "x"})), all_at_once);
    observer.finish_at(all_at_once);

    assert_eq!(observer.first_token_ms(), Some(2_000.0));
    let tps = observer
        .average_tps(Some(200))
        .expect("a positive 2 s whole attempt stays measurable with first == end");
    assert!(
        (tps - 100.0).abs() < 1e-9,
        "200 tokens / the whole 2 s attempt = 100.00 t/s with first == end, got {tps}"
    );
}

#[test]
fn observer_missing_count_is_unknown_but_an_explicit_zero_is_measured() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let mut observer = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    observer.feed_at(
        &chat_event(json!({"content": "x"})),
        instant(base, Duration::from_millis(1_000)),
    );
    observer.finish_at(instant(base, Duration::from_millis(5_000)));

    assert_eq!(
        observer.average_tps(None),
        None,
        "an absent reported count is unknown, never zero"
    );
    assert_eq!(
        observer.average_tps(Some(0)),
        Some(0.0),
        "an explicit zero count over a positive whole attempt is 0.00 t/s"
    );
}

#[test]
fn observer_buffers_fragmented_crlf_and_multiline_events() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);

    // A fragmented event is observed only when its terminating blank line
    // completes.
    let mut fragmented = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    fragmented.feed_at(
        b"data: {\"choices\":[{\"delta\":{\"content\":\"he",
        instant(base, Duration::from_millis(100)),
    );
    assert_eq!(
        fragmented.first_token_ms(),
        None,
        "an incomplete event must not become the first observation"
    );
    fragmented.feed_at(
        b"llo\"}}]}\n\n",
        instant(base, Duration::from_millis(250)),
    );
    assert_eq!(
        fragmented.first_token_ms(),
        Some(250.0),
        "the chunk that completes the event is the observation instant"
    );

    // CRLF framing.
    let mut crlf = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    crlf.feed_at(
        &sse_event("{\"choices\":[{\"delta\":{\"content\":\"x\"}}]}", true),
        instant(base, Duration::from_millis(300)),
    );
    assert_eq!(crlf.first_token_ms(), Some(300.0));

    // Multiline `data:` lines joined with a newline before parsing.
    let mut multiline = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    multiline.feed_at(
        b"data: {\"choices\":[{\"delta\":\ndata: {\"content\":\"x\"}}]}\n\n",
        instant(base, Duration::from_millis(400)),
    );
    assert_eq!(multiline.first_token_ms(), Some(400.0));
}

#[test]
fn observer_recognizes_a_final_unterminated_event_at_finish() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let received = instant(base, Duration::from_millis(2_000));

    let mut observer = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    observer.feed_at(
        b"data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}",
        received,
    );
    assert_eq!(
        observer.first_token_ms(),
        None,
        "an event without its blank-line terminator stays buffered"
    );
    observer.finish_at(received);
    assert_eq!(
        observer.first_token_ms(),
        Some(2_000.0),
        "finish must recognize a final valid event that arrived without the usual terminator"
    );
}

/// AC-003: once the first meaningful output is known, later upstream events --
/// whether they remain in the same received feed or arrive in later fragments --
/// must not be framed merely to observe timing again. `framed_event_count` is a
/// test-only seam: it counts every complete event the observer extracts for
/// observation, including events that carry no meaningful output and whether or
/// not they are eventually parsed. The event that establishes the first
/// observation still counts; every later one must not.
#[test]
fn observer_stops_framing_after_first_meaningful_output() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);

    let mut observer = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    observer.feed_at(
        &chat_event(json!({"role": "assistant"})),
        instant(base, Duration::from_millis(50)),
    );
    let framed_before_first = observer.framed_event_count();
    assert_eq!(
        framed_before_first, 1,
        "the role-only event is framed before any meaningful output is known"
    );

    // The first meaningful event and one later event share a single feed.
    let same_feed = [
        chat_event(json!({"content": "first"})),
        chat_event(json!({"content": "later in the same feed"})),
    ]
    .concat();
    observer.feed_at(&same_feed, instant(base, Duration::from_millis(120)));
    assert_eq!(observer.first_token_ms(), Some(120.0));
    assert_eq!(
        observer.framed_event_count(),
        framed_before_first + 1,
        "the event that establishes first output must be framed, but the event \
         remaining in the same feed must not be"
    );

    // Later fragments must not resume framing either.
    for index in 0..4 {
        observer.feed_at(
            &chat_event(json!({"content": format!("later fragment {index}")})),
            instant(base, Duration::from_millis(200 + index * 100)),
        );
    }
    assert_eq!(
        observer.framed_event_count(),
        framed_before_first + 1,
        "no later fragment may be framed after the first meaningful output"
    );

    observer.finish_at(instant(base, Duration::from_millis(1_000)));
    assert_eq!(
        observer.timing(Some(50)),
        (Some(120.0), Some(50.0)),
        "the first-output instant and the whole-attempt rate must keep their \
         public meaning"
    );
}

/// AC-003: preserving the observable timing boundary while raw bytes arrive.
/// The earliest meaningful instant wins even when more events share the feed and
/// later fragments continue; a multibyte UTF-8 character split across chunks is
/// only meaningful once the completing chunk produces a valid event.
#[test]
fn observer_keeps_earliest_output_across_shared_feed_and_split_utf8() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);

    let mut shared = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    let feed = [
        chat_event(json!({"role": "assistant"})),
        chat_event(json!({"content": "first"})),
        chat_event(json!({"content": "second"})),
    ]
    .concat();
    shared.feed_at(&feed, instant(base, Duration::from_millis(120)));
    shared.feed_at(
        &chat_event(json!({"content": "later fragment"})),
        instant(base, Duration::from_millis(300)),
    );
    shared.finish_at(instant(base, Duration::from_millis(1_000)));
    assert_eq!(
        shared.timing(Some(50)),
        (Some(120.0), Some(50.0)),
        "events after the first in one feed and later fragments must not move \
         the first-output instant"
    );

    let mut split = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    let event = chat_event(json!({"content": "é"}));
    let byte = event
        .iter()
        .position(|value| *value == 0xC3)
        .expect("the UTF-8 event carries a multibyte character");
    split.feed_at(&event[..=byte], instant(base, Duration::from_millis(100)));
    assert_eq!(
        split.first_token_ms(),
        None,
        "a chunk ending mid-UTF-8 must not produce an event or an observation"
    );
    split.feed_at(
        &event[byte + 1..],
        instant(base, Duration::from_millis(250)),
    );
    split.finish_at(instant(base, Duration::from_millis(500)));
    assert_eq!(
        split.timing(Some(50)),
        (Some(250.0), Some(100.0)),
        "the chunk that completes the UTF-8 event is the observation instant"
    );
}

#[test]
fn observer_excludes_metadata_error_completion_and_identifier_only_events() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let end = instant(base, Duration::from_millis(1_000));

    let mut chat = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    chat.feed_at(b": keep-alive\n\n", end);
    chat.feed_at(&sse_event("[DONE]", false), end);
    chat.feed_at(&chat_event(json!({"role": "assistant"})), end);
    chat.feed_at(&chat_event(json!({})), end);
    chat.feed_at(&chat_event(json!({"content": ""})), end);
    chat.feed_at(
        &chat_event(json!({"tool_calls": [{"index": 0, "id": "call_1", "type": "function"}]})),
        end,
    );
    chat.feed_at(&sse_event("{\"error\": {\"message\": \"boom\"}}", false), end);
    chat.finish_at(end);
    assert_eq!(
        chat.first_token_ms(),
        None,
        "chat metadata, an error, a completion marker and an identifier-only tool call are not output"
    );
    assert_eq!(chat.average_tps(Some(100)), None);

    let mut responses = OutputTimingObserver::new(UpstreamProtocol::Responses, started);
    responses.feed_at(&sse_event("{\"type\": \"response.created\"}", false), end);
    responses.feed_at(&sse_event("{\"type\": \"response.in_progress\"}", false), end);
    responses.feed_at(
        &sse_event(
            "{\"type\": \"response.output_item.added\", \"item\": {\"type\": \"function_call\", \"id\": \"fc_1\", \"call_id\": \"call_1\", \"name\": \"\", \"arguments\": \"\"}}",
            false,
        ),
        end,
    );
    responses.feed_at(
        &sse_event("{\"type\": \"response.output_text.delta\", \"delta\": \"\"}", false),
        end,
    );
    responses.finish_at(end);
    assert_eq!(
        responses.first_token_ms(),
        None,
        "Responses lifecycle, an empty delta and an id/call-id-only function item are not output"
    );

    let mut anthropic = OutputTimingObserver::new(UpstreamProtocol::AnthropicMessages, started);
    anthropic.feed_at(
        &sse_event("{\"type\": \"message_start\", \"message\": {\"usage\": {\"input_tokens\": 1, \"output_tokens\": 0}}}", false),
        end,
    );
    anthropic.feed_at(
        &sse_event(
            "{\"type\": \"content_block_start\", \"content_block\": {\"type\": \"tool_use\", \"id\": \"toolu_1\", \"name\": \"\"}}",
            false,
        ),
        end,
    );
    anthropic.feed_at(
        &sse_event(
            "{\"type\": \"content_block_delta\", \"delta\": {\"type\": \"signature_delta\", \"signature\": \"abc\"}}",
            false,
        ),
        end,
    );
    anthropic.feed_at(
        &sse_event("{\"type\": \"message_delta\", \"delta\": {\"stop_reason\": \"end_turn\"}, \"usage\": {\"output_tokens\": 5}}", false),
        end,
    );
    anthropic.feed_at(&sse_event("{\"type\": \"message_stop\"}", false), end);
    anthropic.finish_at(end);
    assert_eq!(
        anthropic.first_token_ms(),
        None,
        "Anthropic lifecycle, a signature delta and an id-only tool_use are not output"
    );
}

#[test]
fn observer_recognizes_non_text_reasoning_tool_and_whitespace_output() {
    // Chat Completions.
    assert!(
        observer_observes(
            UpstreamProtocol::ChatCompletions,
            &chat_event(json!({"content": " "}))
        ),
        "whitespace inside a non-empty content delta is output"
    );
    assert!(
        observer_observes(
            UpstreamProtocol::ChatCompletions,
            &chat_event(json!({"reasoning_content": "think"}))
        ),
        "a reasoning_content delta is output"
    );
    assert!(
        observer_observes(
            UpstreamProtocol::ChatCompletions,
            &chat_event(json!({"reasoning": "think"}))
        ),
        "a reasoning delta is output"
    );
    assert!(
        observer_observes(
            UpstreamProtocol::ChatCompletions,
            &chat_event(json!({"tool_calls": [{"function": {"name": "get_weather"}}]}))
        ),
        "a generated tool name is output"
    );
    assert!(
        observer_observes(
            UpstreamProtocol::ChatCompletions,
            &chat_event(json!({"tool_calls": [{"function": {"arguments": "{\"city\":\"SF\"}"}}]}))
        ),
        "generated tool arguments are output"
    );

    // Responses.
    assert!(observer_observes(
        UpstreamProtocol::Responses,
        &sse_event("{\"type\": \"response.output_text.delta\", \"delta\": \"x\"}", false)
    ));
    assert!(observer_observes(
        UpstreamProtocol::Responses,
        &sse_event("{\"type\": \"response.reasoning_summary_text.delta\", \"delta\": \"x\"}", false)
    ));
    assert!(observer_observes(
        UpstreamProtocol::Responses,
        &sse_event("{\"type\": \"response.reasoning_text.delta\", \"delta\": \"x\"}", false)
    ));
    assert!(observer_observes(
        UpstreamProtocol::Responses,
        &sse_event("{\"type\": \"response.function_call_arguments.delta\", \"delta\": \"{}\"}", false)
    ));
    assert!(
        observer_observes(
            UpstreamProtocol::Responses,
            &sse_event(
                "{\"type\": \"response.output_item.done\", \"item\": {\"type\": \"function_call\", \"id\": \"fc_1\", \"call_id\": \"call_1\", \"name\": \"get_weather\", \"arguments\": \"\"}}",
                false,
            )
        ),
        "a completed function_call item with a non-empty name is output"
    );

    // Anthropic Messages.
    assert!(observer_observes(
        UpstreamProtocol::AnthropicMessages,
        &sse_event("{\"type\": \"content_block_start\", \"content_block\": {\"type\": \"text\", \"text\": \"x\"}}", false)
    ));
    assert!(observer_observes(
        UpstreamProtocol::AnthropicMessages,
        &sse_event("{\"type\": \"content_block_delta\", \"delta\": {\"type\": \"text_delta\", \"text\": \"x\"}}", false)
    ));
    assert!(observer_observes(
        UpstreamProtocol::AnthropicMessages,
        &sse_event("{\"type\": \"content_block_delta\", \"delta\": {\"type\": \"thinking_delta\", \"thinking\": \"x\"}}", false)
    ));
    assert!(observer_observes(
        UpstreamProtocol::AnthropicMessages,
        &sse_event("{\"type\": \"content_block_delta\", \"delta\": {\"type\": \"input_json_delta\", \"partial_json\": \"{}\"}}", false)
    ));
    assert!(
        observer_observes(
            UpstreamProtocol::AnthropicMessages,
            &sse_event("{\"type\": \"content_block_start\", \"content_block\": {\"type\": \"tool_use\", \"id\": \"toolu_1\", \"name\": \"get_weather\"}}", false)
        ),
        "a tool_use content block with a non-empty generated name is output"
    );
}

/// REQ-002 / AC-002: the deprecated Chat Completions `delta.function_call` shape
/// (still raw-forwarded by same-protocol Chat upstreams) carries a generated
/// function name/arguments and must set the first meaningful observation. First
/// output at 1 s, stream end at 5 s and 200 reported tokens yield a 1000 ms first
/// token and 40.00 t/s over the whole attempt.
#[test]
fn observer_recognizes_deprecated_chat_function_call_name_and_arguments() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let first = instant(base, Duration::from_millis(1_000));
    let end = instant(base, Duration::from_millis(5_000));

    let mut name = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    name.feed_at(
        &chat_event(json!({"function_call": {"name": "get_weather"}})),
        first,
    );
    name.finish_at(end);
    assert_eq!(
        name.first_token_ms(),
        Some(1_000.0),
        "a deprecated function_call.name is generated output at ~1000 ms"
    );
    assert_eq!(
        name.average_tps(Some(200)),
        Some(40.0),
        "200 tokens over the whole 5 s attempt is 40.00 t/s"
    );
    assert_eq!(
        name.timing(Some(200)),
        (Some(1_000.0), Some(40.0)),
        "timing() returns the same measured pair for the deprecated shape"
    );

    let mut arguments = OutputTimingObserver::new(UpstreamProtocol::ChatCompletions, started);
    arguments.feed_at(
        &chat_event(json!({"function_call": {"arguments": "{\"city\":\"SF\"}"}})),
        first,
    );
    arguments.finish_at(end);
    assert_eq!(
        arguments.first_token_ms(),
        Some(1_000.0),
        "a deprecated function_call.arguments is generated output at ~1000 ms"
    );
    assert_eq!(arguments.average_tps(Some(200)), Some(40.0));
}

/// Counterexample guard for the deprecated shape: an empty or identifier-only
/// `function_call` carries no generated output.
#[test]
fn observer_ignores_empty_and_identifier_only_deprecated_chat_function_call() {
    assert!(
        !observer_observes(
            UpstreamProtocol::ChatCompletions,
            &chat_event(json!({"function_call": {"name": "", "arguments": ""}}))
        ),
        "an empty deprecated function_call is not output"
    );
    assert!(
        !observer_observes(
            UpstreamProtocol::ChatCompletions,
            &chat_event(json!({"function_call": {"id": "call_1"}}))
        ),
        "an identifier-only deprecated function_call is not output"
    );
}

/// REQ-002 / AC-002: an initial Anthropic `content_block_start` thinking block
/// that already carries reasoning text is meaningful output, while a
/// signature-only or empty thinking block carries no content.
#[test]
fn observer_recognizes_initial_anthropic_thinking_block_but_not_signature_only() {
    let base = std::time::Instant::now();
    let started = tokio::time::Instant::from_std(base);
    let first = instant(base, Duration::from_millis(1_000));
    let end = instant(base, Duration::from_millis(5_000));

    let mut thinking = OutputTimingObserver::new(UpstreamProtocol::AnthropicMessages, started);
    thinking.feed_at(
        &sse_event(
            "{\"type\": \"content_block_start\", \"index\": 0, \"content_block\": {\"type\": \"thinking\", \"thinking\": \"let me reason\"}}",
            false,
        ),
        first,
    );
    thinking.finish_at(end);
    assert_eq!(
        thinking.first_token_ms(),
        Some(1_000.0),
        "an initial thinking content block with actual reasoning text is output"
    );
    assert_eq!(thinking.average_tps(Some(200)), Some(40.0));

    assert!(
        !observer_observes(
            UpstreamProtocol::AnthropicMessages,
            &sse_event(
                "{\"type\": \"content_block_start\", \"index\": 0, \"content_block\": {\"type\": \"thinking\", \"signature\": \"opaque\"}}",
                false,
            )
        ),
        "a signature-only thinking block carries no reasoning text"
    );
    assert!(
        !observer_observes(
            UpstreamProtocol::AnthropicMessages,
            &sse_event(
                "{\"type\": \"content_block_start\", \"index\": 0, \"content_block\": {\"type\": \"thinking\", \"thinking\": \"\"}}",
                false,
            )
        ),
        "an empty thinking string is not output"
    );
}

#[test]
fn observer_interprets_events_with_the_actual_upstream_protocol() {
    let responses_event =
        sse_event("{\"type\": \"response.output_text.delta\", \"delta\": \"x\"}", false);
    assert!(
        !observer_observes(UpstreamProtocol::ChatCompletions, &responses_event),
        "a Responses event must not be read as Chat Completions output"
    );
    let chat = chat_event(json!({"content": "x"}));
    assert!(
        !observer_observes(UpstreamProtocol::Responses, &chat),
        "a Chat event must not be read as Responses output"
    );
    assert!(
        !observer_observes(UpstreamProtocol::AnthropicMessages, &chat),
        "a Chat event must not be read as Anthropic Messages output"
    );
}

// ---------------------------------------------------------------------------
// AC-003 / AC-002: real forwarding evidence for the remaining supported output
// shapes, the model-pool path and a partial stream failure, plus the raw
// reported-output-count selection variants.
// ---------------------------------------------------------------------------

/// A Chat stream: role-only metadata, then ordinary text, then the given raw
/// `usage` object, then `[DONE]`, each on its own chunk so the whole attempt
/// is measurable.
fn chat_stream_with_usage_literal(usage: &str) -> Vec<SseChunk> {
    vec![
        chunk(0, &chat_delta("{\"role\":\"assistant\"}")),
        chunk(40, &chat_delta("{\"content\":\"hello\"}")),
        chunk(
            60,
            &format!("data: {{\"id\":\"chatcmpl\",\"choices\":[],\"usage\":{usage}}}\n\n"),
        ),
        chunk(0, CHAT_DONE),
    ]
}

/// A `text/event-stream` upstream whose per-request chunks are chosen from the
/// real request body, so one configured mock can serve several model mappings.
async fn spawn_chunked_sse_mock_with<F>(build: F) -> String
where
    F: Fn(&[u8]) -> Vec<SseChunk> + Send + Sync + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind varying chunked mock");
    let addr = listener.local_addr().expect("varying chunked mock addr");
    let build = Arc::new(build);
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let build = build.clone();
            tauri::async_runtime::spawn(async move {
                let Ok(request) = crate::ai_gateway::runtime_http::read_http_request(&mut stream)
                    .await
                else {
                    return;
                };
                let chunks = build(&request.body);
                let header = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n";
                if stream.write_all(header.as_bytes()).await.is_err() {
                    return;
                }
                for part in &chunks {
                    if !part.delay.is_zero() {
                        tokio::time::sleep(part.delay).await;
                    }
                    let prefix = format!("{:x}\r\n", part.bytes.len());
                    if stream.write_all(prefix.as_bytes()).await.is_err()
                        || stream.write_all(&part.bytes).await.is_err()
                        || stream.write_all(b"\r\n").await.is_err()
                    {
                        return;
                    }
                }
                let _ = stream.write_all(b"0\r\n\r\n").await;
                let _ = stream.flush().await;
            });
        }
    });
    format!("http://{}", addr)
}

/// AC-003: the raw selected output count (missing, null, negative, fractional,
/// absent, present-but-invalid, a valid numeric string and a valid count behind
/// an invalid input/cache decomposition) decides TPS availability independently
/// of the billing output token and of input/cache validity.
#[tokio::test]
async fn reported_output_count_variants_stay_truthful() {
    let home = temp_home("output-timing-count-variants");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock_with(|body| {
        let upstream = serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|value| value.get("model").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        let usage = match upstream.as_str() {
            "remote-null" => "{\"prompt_tokens\":10,\"completion_tokens\":null}",
            "remote-negative" => "{\"prompt_tokens\":10,\"completion_tokens\":-5}",
            "remote-fractional" => "{\"prompt_tokens\":10,\"completion_tokens\":5.5}",
            "remote-absent" => "{\"prompt_tokens\":10}",
            "remote-precedence" => {
                "{\"prompt_tokens\":10,\"output_tokens\":\"200\",\"completion_tokens\":\"junk\"}"
            }
            "remote-no-fallback" => {
                "{\"prompt_tokens\":10,\"output_tokens\":-1,\"completion_tokens\":42}"
            }
            "remote-bad-cache" => {
                "{\"prompt_tokens\":10,\"cache_read_input_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":4},\"output_tokens\":40}"
            }
            other => panic!("unexpected upstream model {other}"),
        };
        chat_stream_with_usage_literal(usage)
    })
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    let mut config = config_with_key(port);
    let mut provider = provider;
    provider.mappings = vec![
        mapping("local-null", "remote-null", None),
        mapping("local-negative", "remote-negative", None),
        mapping("local-fractional", "remote-fractional", None),
        mapping("local-absent", "remote-absent", None),
        mapping("local-precedence", "remote-precedence", None),
        mapping("local-no-fallback", "remote-no-fallback", None),
        mapping("local-bad-cache", "remote-bad-cache", None),
    ];
    config.providers.push(provider);
    write_config(&config).unwrap();
    start_server(None).await.unwrap();

    let locals = [
        "local-null",
        "local-negative",
        "local-fractional",
        "local-absent",
        "local-precedence",
        "local-no-fallback",
        "local-bad-cache",
    ];
    for local in locals {
        let (status, _content_type, text) =
            stream_request(port, "/v1/chat/completions", local).await;
        assert_eq!(status, 200, "variant {local} must be served: {text}");
    }

    let records = wait_for_usage_logs(locals.len() as u32).await;
    let row = |name: &str| {
        records
            .iter()
            .find(|record| record.local_model == name && record.terminal)
            .unwrap_or_else(|| panic!("row for {name} must exist"))
    };

    for name in [
        "local-null",
        "local-negative",
        "local-fractional",
        "local-absent",
        "local-no-fallback",
    ] {
        let record = row(name);
        let first = measured(record, "first_token_ms");
        assert!(
            first > 0.0,
            "{name} observed output must keep its first-token latency, got {first}"
        );
        assert_unavailable(record, "avg_tps");
    }

    assert!(
        measured(row("local-precedence"), "avg_tps") > 0.0,
        "a valid `output_tokens` string must win over an invalid `completion_tokens`"
    );
    assert_eq!(
        row("local-precedence").output_tokens,
        200,
        "the billing output token must select output_tokens over completion_tokens"
    );
    assert!(
        measured(row("local-bad-cache"), "avg_tps") > 0.0,
        "a valid output count must produce TPS even when the input/cache decomposition is invalid"
    );
    assert_eq!(row("local-bad-cache").output_tokens, 40);

    stop_server().await.unwrap();
    drop(home);
}

/// AC-002: a request served through an operator-defined model pool observes and
/// persists the same timing, using the pool candidate's actual upstream
/// protocol.
#[tokio::test]
async fn model_pool_stream_persists_observed_timing() {
    let home = temp_home("output-timing-model-pool");
    let port = free_port().await;
    let upstream_url =
        spawn_chunked_sse_mock(chat_stream_with_first_text_at(700, 1_300, "60")).await;

    let tag = uuid::Uuid::new_v4().simple().to_string();
    let provider_id = format!("p-{tag}");
    let alias = format!("pool-alias-{tag}");
    let provider = json!({
        "id": provider_id,
        "name": "Pool Provider",
        "base_url": upstream_url,
        "protocol": "chat_completions",
        "enabled": true,
        "keys": [pool_key("key-default", "Default", "sk", true)],
        "mappings": [{
            "local_model": "local-pool",
            "upstream_model": "remote-pool",
            "protocol": "chat_completions",
            "enabled": true,
        }],
    });
    let pool = json!({
        "id": format!("pool-{tag}"),
        "name": "Pool",
        "model": alias,
        "enabled": true,
        "candidates": [{
            "provider_id": provider_id,
            "local_model": "local-pool",
            "upstream_model": "remote-pool",
            "protocol": "chat_completions",
            "weight": 1,
        }],
    });
    write_raw_gateway_config(&json!({
        "schema_version": GATEWAY_CONFIG_SCHEMA_VERSION,
        "port": port,
        "keys": [{
            "id": "k-local",
            "label": "k-local",
            "value": "local-key",
            "enabled": true,
            "created_at": 1,
        }],
        "providers": [provider],
        "model_pools": [pool],
        "model_pool_reserved_models": [],
    }));
    start_server(None).await.unwrap();

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
    assert_eq!(status, 200, "the pool stream must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == alias && record.terminal)
        .expect("the pool attempt must persist one terminal row");
    assert_eq!(record.upstream_model, "remote-pool");
    let first = measured(record, "first_token_ms");
    assert!(
        (350.0..=2_000.0).contains(&first),
        "the pool attempt's first upstream output (~700 ms) must be observed, got {first}"
    );
    let tps = measured(record, "avg_tps");
    assert!(tps > 0.0, "the pool attempt must yield a truthful TPS, got {tps}");

    stop_server().await.unwrap();
    drop(home);
}

/// AC-002 / error boundary: a truncated upstream stream keeps only the output
/// usage it actually observed before the failure, together with its first-token
/// measurement and the TPS derived from that cumulative count.
#[tokio::test]
async fn partial_stream_failure_keeps_observed_cumulative_output_tps() {
    let home = temp_home("output-timing-partial-failure");
    let port = free_port().await;
    let body = format!(
        "{}{}{}",
        chat_delta("{\"role\":\"assistant\"}"),
        chat_delta("{\"content\":\"partial\"}"),
        chat_usage("30"),
    );
    let declared = body.len() + 512;
    let (upstream_url, _log) = spawn_mock_upstream(move |_| {
        MockReply::PartialStream(body.clone(), declared)
    })
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    write_config(&configure_streaming_provider(port, provider, "local-pf", "remote-pf")).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/chat/completions", "local-pf").await;
    assert_eq!(status, 200, "the truncated stream still answers: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-pf")
        .expect("the failed attempt must persist one row");
    assert_eq!(
        record.result,
        UsageResult::Failure,
        "a stream that ends before its declared length is a failed attempt"
    );
    assert_eq!(
        record.output_tokens, 30,
        "the failed attempt keeps only the cumulative output observed before the failure"
    );
    let first = measured(record, "first_token_ms");
    assert!(
        first > 0.0,
        "the failed attempt keeps its observed first-token latency, got {first}"
    );
    let tps = measured(record, "avg_tps");
    assert!(
        tps > 0.0,
        "the failed attempt keeps the TPS derived from its observed cumulative count, got {tps}"
    );

    stop_server().await.unwrap();
    drop(home);
}

/// AC-002: a Responses `function_call` item with a generated name is meaningful
/// first output on a native Responses upstream.
#[tokio::test]
async fn responses_function_call_item_sets_first_output() {
    let home = temp_home("output-timing-responses-function-call");
    let port = free_port().await;
    let upstream_url = spawn_chunked_sse_mock(vec![
        chunk(0, "event: response.created\ndata: {\"type\":\"response.created\"}\n\n"),
        chunk(
            0,
            "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"\",\"arguments\":\"\"}}\n\n",
        ),
        chunk(
            600,
            "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"get_weather\",\"arguments\":\"{}\"}}\n\n",
        ),
        chunk(
            700,
            "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r\",\"output\":[],\"usage\":{\"input_tokens\":10,\"output_tokens\":25}}}\n\n",
        ),
    ])
    .await;
    let provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    let mut config = config_with_key(port);
    let mut provider = provider;
    provider.protocol = UpstreamProtocol::Responses;
    provider.mappings = vec![mapping("local-rfc", "remote-rfc", None)];
    config.providers.push(provider);
    write_config(&config).unwrap();
    start_server(None).await.unwrap();

    let (status, _content_type, text) =
        stream_request(port, "/v1/responses", "local-rfc").await;
    assert_eq!(status, 200, "the Responses function_call stream must be served: {text}");

    let records = wait_for_usage_logs(1).await;
    let record = records
        .iter()
        .find(|record| record.local_model == "local-rfc" && record.terminal)
        .expect("one terminal row");
    assert_eq!(record.output_tokens, 25, "the Responses output count is stored");
    let first = measured(record, "first_token_ms");
    assert!(
        (400.0..=1_600.0).contains(&first),
        "an id-only function item is not output; the generated name at ~600 ms must set the \
         first observation, got {first}"
    );

    stop_server().await.unwrap();
    drop(home);
}
