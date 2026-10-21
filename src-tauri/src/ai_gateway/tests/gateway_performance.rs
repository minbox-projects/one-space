//! Ignored whole-gateway measurements for the fixed AC-001 inventory.
//!
//! Each case is its own ignored test, so the runner executes one exact case per
//! fresh process. Every pass prints one `AGW_METRIC {json}` line containing one
//! finite latency sample per measured operation, the whole-pass business wall
//! time, dimensions and measured counters. Process-cold means fresh
//! process-owned state; fixture preparation and compilation stay outside the
//! measured business samples.
//!
//! Assertions here are structural/behavioral only: operation counts, exact
//! query results, calendar classes and successful transport. There is no
//! absolute wall-time threshold.
//!
//! No request body, header or credential value is printed or saved.

use crate::ai_gateway::{
    model_pools::{plan_pool_request, PoolPlan},
    reasoning::{PoolOutputCap, PoolReasoningRequest},
    runtime_http::{
        attempt_non_streaming, attempt_streaming, models_payload, server_status, start_server,
        stop_server,
    },
    selection::candidate_providers,
    storage::{read_config, write_config},
    templates::provider_template_views,
    usage_log::{
        reset_usage_log_write_stats, resolve_range_selector, usage_log_write_stats, LogFilter,
        UsageLogRecord, UsageLogStore, UsageResult,
    },
    usage_store::{query_work_stats, reset_query_work_stats},
    GatewayConfig, GatewayKey, GatewayUpstreamProvider, ModelMapping, ModelPool,
    ModelPoolCandidate, ProviderDayCoverage, UpstreamProtocol, GATEWAY_CONFIG_SCHEMA_VERSION,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const OPS: usize = 200;
const WARM_PASSES: usize = 5;
const APPEND_REQUESTS: usize = 10_000;
const APPEND_ATTEMPTS: usize = 3;
const LOCAL_KEY: &str = "SAFE_FIXTURE_local_key";
const UPSTREAM_KEY: &str = "SAFE_FIXTURE_upstream_key";

/// Removes a temporary fixture directory when the case ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "onespace-agw-perf-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn emit_case(
    case: &str,
    phase: &str,
    pass: usize,
    samples_ms: Vec<f64>,
    wall_ms: f64,
    operations: usize,
    dimensions: Value,
    counters: Value,
) {
    assert_eq!(
        samples_ms.len(),
        operations,
        "{case} pass {pass} must carry one sample per operation"
    );
    assert!(
        samples_ms.iter().all(|value| value.is_finite() && *value >= 0.0),
        "{case} pass {pass} samples must be finite and non-negative"
    );
    assert!(
        wall_ms.is_finite() && wall_ms >= 0.0,
        "{case} pass {pass} business wall must be finite and non-negative"
    );
    let payload = json!({
        "case": case,
        "phase": phase,
        "pass": pass,
        "business_samples_ms": samples_ms,
        "business_wall_ms": wall_ms,
        "operations": operations,
        "dimensions": dimensions,
        "counters": counters,
    });
    println!(
        "\nAGW_METRIC {}",
        serde_json::to_string(&payload).expect("serialize measurement")
    );
}

fn emit_pass(
    case: &str,
    pass: usize,
    samples_ms: Vec<f64>,
    wall_ms: f64,
    operations: usize,
    dimensions: &Value,
    counters: Value,
) {
    let phase = if pass == 0 { "cold" } else { "warm" };
    emit_case(
        case,
        phase,
        pass,
        samples_ms,
        wall_ms,
        operations,
        dimensions.clone(),
        counters,
    );
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn local_key() -> GatewayKey {
    GatewayKey {
        id: "local-key".to_string(),
        label: "Local key".to_string(),
        value: LOCAL_KEY.to_string(),
        enabled: true,
        created_at: 1,
    }
}

fn upstream_key(id: &str) -> crate::ai_gateway::UpstreamKey {
    serde_json::from_value(super::pool_key(id, "Default", UPSTREAM_KEY, true))
        .expect("upstream key fixture")
}

fn config_fixture(providers: usize, mappings: usize, keys: usize) -> GatewayConfig {
    let mut config = GatewayConfig::default();
    config.enabled = true;
    config.keys.push(local_key());
    config.default_key_id = Some("local-key".to_string());
    for provider_index in 0..providers {
        let mut provider = super::provider(&format!("p{provider_index}"));
        provider.keys = (0..keys)
            .map(|key_index| upstream_key(&format!("k{provider_index}-{key_index}")))
            .collect();
        provider.mappings = (0..mappings)
            .map(|mapping_index| {
                let local = format!("local-{provider_index}-{mapping_index}");
                let upstream = format!("remote-{provider_index}-{mapping_index}");
                super::mapping(&local, &upstream, None)
            })
            .collect();
        config.providers.push(provider);
    }
    config
}

/// Seeds the encrypted configuration without touching the in-process read
/// cache, so the first measured read is genuinely process-cold.
fn seed_config_without_cache(config: &GatewayConfig) {
    let path = crate::ai_gateway::storage::config_path().expect("config path");
    std::fs::create_dir_all(path.parent().expect("config parent")).expect("create config dir");
    let mut next = config.clone();
    next.schema_version = GATEWAY_CONFIG_SCHEMA_VERSION;
    let json = serde_json::to_string(&next).expect("serialize configuration");
    let password = crate::crypto::get_or_init_master_password().expect("master password");
    let encrypted = crate::crypto::encrypt(&json, &password).expect("encrypt configuration");
    std::fs::write(&path, encrypted).expect("write configuration");
}

fn emit_config_case(case: &str, providers: usize, mappings: usize, keys: usize) {
    super::with_temp_home(case, |_home| {
        let config = config_fixture(providers, mappings, keys);
        seed_config_without_cache(&config);
        let dimensions = json!({
            "providers": providers,
            "mappings_per_provider": mappings,
            "keys_per_provider": keys,
            "operations": "status+models",
            "cold_scope": "fresh_process_empty_read_cache",
        });
        for pass in 0..=WARM_PASSES {
            let mut samples = Vec::with_capacity(OPS);
            let mut status_reads = 0usize;
            let mut model_reads = 0usize;
            let mut model_count = 0usize;
            let started = Instant::now();
            for index in 0..OPS {
                let operation = Instant::now();
                if index % 2 == 0 {
                    let status = server_status().expect("status snapshot");
                    assert_eq!(status.provider_count, providers);
                    status_reads += 1;
                } else {
                    let loaded = read_config().expect("read the hot configuration");
                    let payload = models_payload(&loaded);
                    model_count = payload["data"].as_array().map(Vec::len).unwrap_or(0);
                    assert!(model_count >= mappings);
                    model_reads += 1;
                }
                samples.push(elapsed_ms(operation));
            }
            let wall_ms = elapsed_ms(started);
            emit_pass(
                case,
                pass,
                samples,
                wall_ms,
                OPS,
                &dimensions,
                json!({
                    "status_reads": status_reads,
                    "model_payload_builds": model_reads,
                    "models_advertised": model_count,
                }),
            );
        }
    });
}

/// Routing fixture with exactly one candidate for `local-shared`.
fn routing_config(providers: usize, mappings: usize, keys: usize, base_url: &str) -> GatewayConfig {
    let mut config = config_fixture(providers, mappings, keys);
    for (provider_index, provider) in config.providers.iter_mut().enumerate() {
        provider.base_url = base_url.to_string();
        for (mapping_index, mapping) in provider.mappings.iter_mut().enumerate() {
            mapping.local_model = if provider_index == 0 && mapping_index == 0 {
                "local-shared".to_string()
            } else {
                format!("local-{provider_index}-{mapping_index}")
            };
            mapping.upstream_model = format!("remote-{provider_index}-{mapping_index}");
        }
    }
    config
}

fn provider_for_protocol(
    id: &str,
    protocol: UpstreamProtocol,
    base_url: &str,
) -> GatewayUpstreamProvider {
    let protocol_value = protocol_json(protocol);
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "base_url": base_url,
        "protocol": protocol_value,
        "enabled": true,
        "keys": [super::pool_key("key-default", "Default", UPSTREAM_KEY, true)],
        "default_model": null,
        "mappings": [{
            "local_model": "local-shared",
            "upstream_model": "remote-shared",
            "protocol": protocol_value,
            "enabled": true
        }]
    }))
    .expect("provider fixture")
}

fn protocol_json(protocol: UpstreamProtocol) -> &'static str {
    match protocol {
        UpstreamProtocol::ChatCompletions => "chat_completions",
        UpstreamProtocol::Responses => "responses",
        UpstreamProtocol::AnthropicMessages => "anthropic_messages",
    }
}

fn path_for(protocol: UpstreamProtocol) -> &'static str {
    match protocol {
        UpstreamProtocol::ChatCompletions => "/v1/chat/completions",
        UpstreamProtocol::Responses => "/v1/responses",
        UpstreamProtocol::AnthropicMessages => "/v1/messages",
    }
}

fn request_body(path: &str, bytes: usize, stream: bool, model: &str) -> Vec<u8> {
    let text = "x".repeat(bytes);
    let value = if path == "/v1/messages" {
        json!({
            "model": model,
            "max_tokens": 16,
            "stream": stream,
            "messages": [{"role": "user", "content": text}]
        })
    } else if path == "/v1/responses" {
        json!({"model": model, "stream": stream, "input": text})
    } else {
        json!({
            "model": model,
            "stream": stream,
            "messages": [{"role": "user", "content": text}]
        })
    };
    serde_json::to_vec(&value).expect("request body")
}

fn success_reply() -> super::MockReply {
    super::MockReply::Json(
        200,
        json!({
            "id": "perf",
            "type": "message",
            "role": "assistant",
            "choices": [{"message": {"role": "assistant", "content": "ok"}}],
            "output": [{"type": "message", "content": [{"type": "output_text", "text": "ok"}]}],
            "content": [{"type": "text", "text": "ok"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2,
                      "input_tokens": 1, "output_tokens": 1}
        }),
    )
}

fn chat_sse(events: usize) -> String {
    let mut text = String::new();
    for index in 0..events {
        let value = json!({
            "id": "chatcmpl-perf",
            "object": "chat.completion.chunk",
            "choices": [{
                "index": 0,
                "delta": {"content": (index % 10).to_string()},
                "finish_reason": null
            }]
        });
        text.push_str("data: ");
        text.push_str(&value.to_string());
        text.push_str("\n\n");
    }
    text.push_str("data: [DONE]\n\n");
    text
}

fn responses_sse(events: usize) -> String {
    let mut text = String::new();
    for index in 0..events {
        let value = json!({
            "type": "response.output_text.delta",
            "item_id": "msg_perf",
            "output_index": 0,
            "content_index": 0,
            "delta": (index % 10).to_string()
        });
        text.push_str("event: response.output_text.delta\ndata: ");
        text.push_str(&value.to_string());
        text.push_str("\n\n");
    }
    let completed = json!({
        "type": "response.completed",
        "response": {"id": "resp-perf", "status": "completed", "output": [],
                     "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}}
    });
    text.push_str("event: response.completed\ndata: ");
    text.push_str(&completed.to_string());
    text.push_str("\n\n");
    text
}

fn messages_sse(events: usize) -> String {
    let mut text = String::from(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_perf\",\"role\":\"assistant\",\"model\":\"remote-shared\",\"content\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n",
    );
    for index in 0..events {
        let value = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": (index % 10).to_string()}
        });
        text.push_str("event: content_block_delta\ndata: ");
        text.push_str(&value.to_string());
        text.push_str("\n\n");
    }
    text.push_str("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
    text
}

fn stream_body(protocol: UpstreamProtocol, events: usize) -> String {
    match protocol {
        UpstreamProtocol::ChatCompletions => chat_sse(events),
        UpstreamProtocol::Responses => responses_sse(events),
        UpstreamProtocol::AnthropicMessages => messages_sse(events),
    }
}


/// A local mock upstream that writes the response in exact fragment sizes, so a
/// measured stream case controls framing instead of only converting in memory.
async fn spawn_chunked_mock(body: String, fragment_bytes: usize) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind chunked mock");
    let addr = listener.local_addr().expect("chunked mock addr");
    let body = Arc::new(body.into_bytes());
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let body = body.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::ai_gateway::runtime_http::read_http_request(&mut stream).await;
                let header = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                if stream.write_all(header.as_bytes()).await.is_err() {
                    return;
                }
                for chunk in body.chunks(fragment_bytes.max(1)) {
                    if stream.write_all(chunk).await.is_err() {
                        return;
                    }
                    if stream.flush().await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    format!("http://{}", addr)
}

fn fragment_event_count(_case: &str) -> usize {
    1_000
}

fn mock_reply(protocol: UpstreamProtocol, events: Option<usize>) -> super::MockReply {
    match events {
        Some(count) => super::MockReply::Stream(stream_body(protocol, count)),
        None => success_reply(),
    }
}

async fn direct_non_streaming_pass(
    provider: &GatewayUpstreamProvider,
    path: &'static str,
    body: &[u8],
) -> (Vec<f64>, usize, f64) {
    let mut samples = Vec::with_capacity(OPS);
    let mut attempts_total = 0usize;
    let started = Instant::now();
    for _ in 0..OPS {
        let operation = Instant::now();
        let mut attempts = Vec::new();
        let response = attempt_non_streaming(
            std::slice::from_ref(provider),
            path,
            body,
            Some("local-shared"),
            &HashMap::new(),
            false,
            None,
            &mut attempts,
        ).await;
        assert_eq!(
            response.status, 200,
            "direct-mock operation must succeed without printing the body"
        );
        attempts_total += attempts.len();
        samples.push(elapsed_ms(operation));
    }
    (samples, attempts_total, elapsed_ms(started))
}

async fn direct_streaming_pass(
    provider: &GatewayUpstreamProvider,
    path: &'static str,
    body: &[u8],
) -> (Vec<f64>, usize, f64) {
    let mut samples = Vec::with_capacity(OPS);
    let mut output_bytes = 0usize;
    let started = Instant::now();
    for _ in 0..OPS {
        let operation = Instant::now();
        let (mut client, mut server) = tokio::io::duplex(16 * 1024 * 1024);
        let mut attempts = Vec::new();
        let capture = attempt_streaming(
            &mut server,
            std::slice::from_ref(provider),
            path,
            body,
            Some("local-shared"),
            &HashMap::new(),
            false,
            None,
            &mut attempts,
        ).await
        .expect("direct-mock stream must complete");
        drop(server);
        let mut output = Vec::new();
        client.read_to_end(&mut output).await
            .expect("read direct-mock stream");
        assert_eq!(capture.status, 200);
        assert!(!output.is_empty(), "a measured stream must deliver bytes");
        output_bytes += output.len();
        samples.push(elapsed_ms(operation));
    }
    (samples, output_bytes, elapsed_ms(started))
}

async fn run_direct_case(
    case: &str,
    ingress: UpstreamProtocol,
    upstream: UpstreamProtocol,
    stream: bool,
    fragment_bytes: Option<usize>,
) {
    let _home = super::temp_home(case);
    let upstream_url = if let Some(fragment) = fragment_bytes {
        spawn_chunked_mock(stream_body(upstream, 1_000), fragment).await
    } else {
        super::spawn_mock_upstream(move |_| mock_reply(upstream, None))
            .await
            .0
    };
    let provider = provider_for_protocol("perf-provider", upstream, &upstream_url);
    let path = path_for(ingress);
    let body = request_body(path, 32 * 1024, stream, "local-shared");
    let dimensions = json!({
        "ingress": protocol_json(ingress),
        "upstream": protocol_json(upstream),
        "stream": stream,
        "events": fragment_bytes.map(|_| 1_000usize),
        "fragment_bytes": fragment_bytes,
        "request_bytes": body.len(),
        "case_kind": "direct_mock",
        "cold_scope": "fresh_process",
    });
    for pass in 0..=WARM_PASSES {
        let (samples, counter, wall_ms) = if stream {
            direct_streaming_pass(&provider, path, &body).await
        } else {
            direct_non_streaming_pass(&provider, path, &body).await
        };
        emit_pass(
            case,
            pass,
            samples,
            wall_ms,
            OPS,
            &dimensions,
            json!({
                "requests": OPS,
                "stream": stream,
                "attempts": if stream { OPS } else { counter },
                "output_bytes": if stream { counter } else { 0 },
            }),
        );
    }
}

async fn loopback_pass(
    client: &reqwest::Client,
    url: &str,
    body: Arc<Vec<u8>>,
    concurrency: usize,
    stream: bool,
) -> (Vec<f64>, f64) {
    let started = Instant::now();
    if concurrency == 1 {
        let mut samples = Vec::with_capacity(OPS);
        for _ in 0..OPS {
            let operation = Instant::now();
            let response = client
                .post(url)
                .header("authorization", format!("Bearer {LOCAL_KEY}"))
                .header("content-type", "application/json")
                .body(body.as_ref().clone())
                .send()
                .await
                .expect("loopback request");
            assert_eq!(response.status().as_u16(), 200);
            let bytes = response.bytes().await.expect("loopback body");
            assert!(!bytes.is_empty(), "a measured response must carry bytes");
            if stream {
                assert!(bytes.len() > 0);
            }
            samples.push(elapsed_ms(operation));
        }
        return (samples, elapsed_ms(started));
    }

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut tasks = JoinSet::new();
    for _ in 0..OPS {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("acquire concurrency permit");
        let client = client.clone();
        let url = url.to_string();
        let body = body.clone();
        tasks.spawn(async move {
            let _permit = permit;
            let operation = Instant::now();
            let response = client
                .post(url)
                .header("authorization", format!("Bearer {LOCAL_KEY}"))
                .header("content-type", "application/json")
                .body(body.as_ref().clone())
                .send()
                .await
                .expect("loopback request");
            assert_eq!(response.status().as_u16(), 200);
            let bytes = response.bytes().await.expect("loopback body");
            assert!(!bytes.is_empty(), "a measured response must carry bytes");
            elapsed_ms(operation)
        });
    }
    let mut samples = Vec::with_capacity(OPS);
    while let Some(joined) = tasks.join_next().await {
        samples.push(joined.expect("loopback task"));
    }
    (samples, elapsed_ms(started))
}

async fn run_loopback_case(
    case: &str,
    ingress: UpstreamProtocol,
    upstream: UpstreamProtocol,
    concurrency: usize,
    request_bytes: usize,
    stream: bool,
    events: Option<usize>,
    fragment_bytes: Option<usize>,
) {
    let _home = super::temp_home(case);
    let port = super::free_port().await;
    let upstream_url = if let Some(fragment) = fragment_bytes {
        spawn_chunked_mock(
            stream_body(upstream, fragment_event_count(case)),
            fragment,
        )
        .await
    } else {
        super::spawn_mock_upstream(move |_| mock_reply(upstream, None))
            .await
            .0
    };
    let provider = provider_for_protocol("perf-provider", upstream, &upstream_url);
    let mut config = GatewayConfig::default();
    config.enabled = true;
    config.port = port;
    config.keys.push(local_key());
    config.default_key_id = Some("local-key".to_string());
    config.providers.push(provider);
    write_config(&config).expect("seed forwarding configuration");
    start_server(None).await.expect("start loopback gateway");

    let path = path_for(ingress);
    let body = Arc::new(request_body(path, request_bytes, stream, "local-shared"));
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}{path}");
    let dimensions = json!({
        "ingress": protocol_json(ingress),
        "upstream": protocol_json(upstream),
        "stream": stream,
        "events": events,
        "fragment_bytes": fragment_bytes,
        "request_bytes": body.len(),
        "concurrency": concurrency,
        "case_kind": "loopback_gateway",
        "cold_scope": "fresh_process_preparation_inclusive",
    });
    for pass in 0..=WARM_PASSES {
        let (samples, wall_ms) =
            loopback_pass(&client, &url, body.clone(), concurrency, stream).await;
        emit_pass(
            case,
            pass,
            samples,
            wall_ms,
            OPS,
            &dimensions,
            json!({
                "requests": OPS,
                "concurrency": concurrency,
                "request_bytes": body.len(),
                "stream": stream,
            }),
        );
    }
    stop_server().await.expect("stop loopback gateway");
}

async fn run_routing_case(case: &str, providers: usize, mappings: usize, keys: usize) {
    let _home = super::temp_home(case);
    let port = super::free_port().await;
    let (_upstream_url, _captured) = super::spawn_mock_upstream(|_| success_reply()).await;
    let mut config = routing_config(providers, mappings, keys, &_upstream_url);
    config.port = port;
    write_config(&config).expect("seed routing configuration");
    start_server(None).await.expect("start routing gateway");
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let body = Arc::new(request_body("/v1/chat/completions", 32 * 1024, false, "local-shared"));
    let dimensions = json!({
        "providers": providers,
        "mappings_per_provider": mappings,
        "keys_per_provider": keys,
        "candidate": "local-shared",
        "request_bytes": body.len(),
        "case_kind": "loopback_routing",
        "cold_scope": "fresh_process_preparation_inclusive",
    });
    for pass in 0..=WARM_PASSES {
        let (samples, wall_ms) = loopback_pass(&client, &url, body.clone(), 1, false).await;
        emit_pass(
            case,
            pass,
            samples,
            wall_ms,
            OPS,
            &dimensions,
            json!({
                "requests": OPS,
                "candidate_calls": candidate_providers(
                    &config.providers,
                    Some("local-shared"),
                    UpstreamProtocol::ChatCompletions,
                ).len(),
            }),
        );
    }
    stop_server().await.expect("stop routing gateway");
}

fn pool_fixture(pools: usize, candidates: usize) -> GatewayConfig {
    let mut providers = Vec::new();
    let mut raw_pools = Vec::new();
    for pool_index in 0..pools {
        let mut raw_mappings = Vec::new();
        let mut raw_candidates = Vec::new();
        for candidate_index in 0..candidates {
            let local = format!("local-{pool_index}-{candidate_index}");
            let upstream = format!("remote-{pool_index}-{candidate_index}");
            raw_mappings.push(json!({
                "local_model": local,
                "upstream_model": upstream,
                "protocol": "chat_completions",
                "enabled": true
            }));
            raw_candidates.push(json!({
                "provider_id": format!("p{pool_index}"),
                "local_model": local,
                "upstream_model": upstream,
                "protocol": "chat_completions",
                "weight": 1
            }));
        }
        providers.push(json!({
            "id": format!("p{pool_index}"),
            "name": format!("Provider {pool_index}"),
            "base_url": "https://api.example.com/v1",
            "protocol": "chat_completions",
            "enabled": true,
            "keys": [super::pool_key("key-default", "Default", UPSTREAM_KEY, true)],
            "mappings": raw_mappings
        }));
        raw_pools.push(json!({
            "id": format!("pool-{pool_index}"),
            "name": format!("Pool {pool_index}"),
            "model": format!("pool-model-{pool_index}"),
            "enabled": true,
            "candidates": raw_candidates
        }));
    }
    serde_json::from_value(json!({
        "schema_version": 3,
        "enabled": true,
        "port": 17688,
        "providers": providers,
        "keys": [local_key()],
        "model_pools": raw_pools
    }))
    .expect("pool fixture")
}

fn run_pool_decision_case() {
    let config = pool_fixture(10, 20);
    let request = PoolReasoningRequest {
        requested: None,
        cap: PoolOutputCap {
            field: "max_tokens",
            tokens: 4096,
        },
    };
    let dimensions = json!({
        "pools": 10,
        "candidates_per_pool": 20,
        "operations": "plan_pool_request",
    });
    for pass in 0..=WARM_PASSES {
        let mut samples = Vec::with_capacity(OPS);
        let started = Instant::now();
        let mut ready_candidates = 0usize;
        for _ in 0..OPS {
            let operation = Instant::now();
            let plan = plan_pool_request(
                &config,
                "pool-model-0",
                UpstreamProtocol::ChatCompletions,
                &request,
                1,
            );
            match plan {
                Some(PoolPlan::Ready { ordered, .. }) => ready_candidates += ordered.len(),
                _ => panic!("pool decision must stay ready"),
            }
            samples.push(elapsed_ms(operation));
        }
        let wall_ms = elapsed_ms(started);
        emit_pass(
            "pool_decision_10x20",
            pass,
            samples,
            wall_ms,
            OPS,
            &dimensions,
            json!({
                "pool_plans": OPS,
                "ready_candidates": ready_candidates,
            }),
        );
    }
}

fn pool_config(base_url: &str, target: UpstreamProtocol) -> GatewayConfig {
    let mut provider = provider_for_protocol("p0", target, base_url);
    provider.mappings = vec![ModelMapping {
        local_model: "local-shared".to_string(),
        upstream_model: "remote-shared".to_string(),
        protocol: Some(target),
        display_name: None,
        enabled: true,
        reasoning_efforts: Vec::new(),
        auto_disabled: false,
        disabled_reason: None,
        disabled_at: None,
        consecutive_failures: 0,
        last_error_at: None,
    }];
    let mut config = GatewayConfig::default();
    config.enabled = true;
    config.keys.push(local_key());
    config.default_key_id = Some("local-key".to_string());
    config.providers.push(provider);
    config.model_pools.push(ModelPool {
        id: "pool-0".to_string(),
        name: "Pool 0".to_string(),
        model: "pool-model-0".to_string(),
        enabled: true,
        candidates: vec![ModelPoolCandidate {
            provider_id: "p0".to_string(),
            local_model: "local-shared".to_string(),
            upstream_model: "remote-shared".to_string(),
            protocol: target,
            weight: 1,
            reasoning: None,
        }],
        default_reasoning_effort: Default::default(),
        fallback_models: Vec::new(),
        fallback: None,
    });
    config
}

async fn run_pool_loopback_case(
    case: &str,
    ingress: UpstreamProtocol,
    target: UpstreamProtocol,
) {
    let _home = super::temp_home(case);
    let port = super::free_port().await;
    let (_upstream_url, _captured) = super::spawn_mock_upstream(|_| success_reply()).await;
    let mut config = pool_config(&_upstream_url, target);
    config.port = port;
    write_config(&config).expect("seed pool configuration");
    start_server(None).await.expect("start pool gateway");
    let path = path_for(ingress);
    let body = Arc::new(request_body(path, 32 * 1024, false, "pool-model-0"));
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}{path}");
    let dimensions = json!({
        "ingress": protocol_json(ingress),
        "upstream": protocol_json(target),
        "pool_candidates": 1,
        "request_bytes": body.len(),
        "case_kind": "loopback_pool",
        "cold_scope": "fresh_process_preparation_inclusive",
    });
    for pass in 0..=WARM_PASSES {
        let (samples, wall_ms) = loopback_pass(&client, &url, body.clone(), 1, false).await;
        emit_pass(
            case,
            pass,
            samples,
            wall_ms,
            OPS,
            &dimensions,
            json!({"requests": OPS, "pool": "pool-model-0"}),
        );
    }
    stop_server().await.expect("stop pool gateway");
}

fn append_record(request: usize, attempt: usize, base_ms: i64) -> UsageLogRecord {
    UsageLogRecord {
        timestamp_ms: base_ms + request as i64,
        local_model: "local-perf".to_string(),
        upstream_model: "upstream-perf".to_string(),
        provider_id: "provider-perf".to_string(),
        provider_name: "Provider Perf".to_string(),
        result: UsageResult::Success,
        status: 200,
        input_tokens: 10,
        cache_read_tokens: 1,
        cache_write_tokens: 0,
        output_tokens: 5,
        total_tokens: 15,
        amount: Some(0.001),
        cost_breakdown: None,
        duration_ms: 5,
        error_message: None,
        terminal: attempt + 1 == APPEND_ATTEMPTS,
        reasoning_effort: None,
        first_token_ms: None,
        avg_tps: None,
        resolved_reasoning_effort: None,
        native_reasoning: None,
    }
}

fn run_append_case() {
    let dir = TempDir::new("append");
    let store = UsageLogStore::at(dir.0.join("ai_gateway_usage.db"));
    let base_ms = chrono::Utc::now().timestamp_millis();
    reset_usage_log_write_stats();
    let dimensions = json!({
        "requests": APPEND_REQUESTS,
        "attempts_per_request": APPEND_ATTEMPTS,
    });
    let mut previous = crate::ai_gateway::UsageLogWriteStats::default();
    for pass in 0..=WARM_PASSES {
        let mut samples = Vec::with_capacity(APPEND_REQUESTS);
        let started = Instant::now();
        for request in 0..APPEND_REQUESTS {
            let records = (0..APPEND_ATTEMPTS)
                .map(|attempt| append_record(request, attempt, base_ms))
                .collect::<Vec<_>>();
            let operation = Instant::now();
            store
                .append_batch(&records, 365)
                .expect("append one request batch");
            samples.push(elapsed_ms(operation));
        }
        let wall_ms = elapsed_ms(started);
        let stats = usage_log_write_stats();
        let delta = crate::ai_gateway::UsageLogWriteStats {
            db_opens: stats.db_opens - previous.db_opens,
            transactions: stats.transactions - previous.transactions,
            rows_written: stats.rows_written - previous.rows_written,
            batch_writes: stats.batch_writes - previous.batch_writes,
        };
        assert!(
            delta.rows_written >= (APPEND_REQUESTS * APPEND_ATTEMPTS) as u64,
            "every measured attempt row must be written"
        );
        assert!(
            delta.batch_writes >= APPEND_REQUESTS as u64,
            "every measured request must issue one batch append"
        );
        emit_pass(
            "usage_log_append_10000x3",
            pass,
            samples,
            wall_ms,
            APPEND_REQUESTS,
            &dimensions,
            json!({
                "db_opens": delta.db_opens,
                "transactions": delta.transactions,
                "rows_written": delta.rows_written,
                "batch_writes": delta.batch_writes,
            }),
        );
        previous = stats;
    }
}

fn usage_record(index: usize, providers: usize, models: usize, days: usize, now_ms: i64) -> UsageLogRecord {
    let provider = index % providers;
    let model = index % models;
    let day = index % days;
    let failure = index % 10 == 0;
    UsageLogRecord {
        timestamp_ms: now_ms - (day as i64) * 86_400_000 - (index as i64 % 1_000),
        local_model: format!("model-{model}"),
        upstream_model: format!("upstream-{model}"),
        provider_id: format!("provider-{provider}"),
        provider_name: format!("Provider {provider}"),
        result: if failure { UsageResult::Failure } else { UsageResult::Success },
        status: if failure { 500 } else { 200 },
        input_tokens: 10,
        cache_read_tokens: 2,
        cache_write_tokens: 0,
        output_tokens: 5,
        total_tokens: 17,
        amount: if index % 7 == 0 { None } else { Some(0.001) },
        cost_breakdown: None,
        duration_ms: 5,
        error_message: if failure { Some("SAFE_FIXTURE_error".to_string()) } else { None },
        terminal: true,
        reasoning_effort: None,
        first_token_ms: None,
        avg_tps: None,
        resolved_reasoning_effort: None,
        native_reasoning: None,
    }
}

fn seed_usage_fixture(
    store: &UsageLogStore,
    rows: usize,
    providers: usize,
    models: usize,
    days: usize,
    now_ms: i64,
) {
    let mut chunk = Vec::with_capacity(100);
    for index in 0..rows {
        chunk.push(usage_record(index, providers, models, days, now_ms));
        if chunk.len() == 100 || index + 1 == rows {
            store
                .append_batch(&chunk, 365)
                .expect("seed usage fixture");
            chunk.clear();
        }
    }
}

fn range_selector(index: usize) -> &'static str {
    ["today", "yesterday", "7d", "15d", "30d", "all"][index % 6]
}

fn query_mixed_pass(store: &UsageLogStore, now_ms: i64, expected_rows: usize) -> (Vec<f64>, Value) {
    let mut samples = Vec::with_capacity(OPS);
    let mut range_queries = 0usize;
    let mut first_last_queries = 0usize;
    let mut grouped_queries = 0usize;
    let mut filter_queries = 0usize;
    let mut facet_queries = 0usize;
    let mut returned_rows = 0usize;
    let started = Instant::now();
    for index in 0..OPS {
        let operation = Instant::now();
        match index % 8 {
            0 => {
                let resolved = resolve_range_selector(Some(range_selector(index)), now_ms)
                    .expect("supported range");
                let page = store
                    .query_logs(&resolved.range, &LogFilter::default(), 1)
                    .expect("first page");
                assert_eq!(page.page, 1);
                assert_eq!(page.page_size, 50);
                returned_rows += page.records.len();
                range_queries += 1;
            }
            1 => {
                let resolved = resolve_range_selector(Some("all"), now_ms).expect("all range");
                let first = store
                    .query_logs(&resolved.range, &LogFilter::default(), 1)
                    .expect("first page");
                assert_eq!(first.total as usize, expected_rows);
                let last = store
                    .query_logs(&resolved.range, &LogFilter::default(), first.total_pages)
                    .expect("last page");
                assert_eq!(last.page, first.total_pages);
                assert_eq!(last.page_size, 50);
                assert!(
                    first.records.first().map(|row| row.timestamp_ms)
                        >= last.records.first().map(|row| row.timestamp_ms),
                    "first page must not be older than the last page"
                );
                returned_rows += first.records.len() + last.records.len();
                first_last_queries += 1;
            }
            2 => {
                let resolved = resolve_range_selector(Some("all"), now_ms).expect("all range");
                let groups = store
                    .group_logs(&resolved.range, &LogFilter::default(), "model")
                    .expect("model groups");
                assert_eq!(groups.len(), 200);
                assert_eq!(
                    groups.iter().map(|group| group.request_count as usize).sum::<usize>(),
                    expected_rows
                );
                grouped_queries += 1;
            }
            3 => {
                let resolved = resolve_range_selector(Some("all"), now_ms).expect("all range");
                let groups = store
                    .group_logs(&resolved.range, &LogFilter::default(), "provider")
                    .expect("provider groups");
                assert_eq!(groups.len(), 50);
                assert_eq!(
                    groups.iter().map(|group| group.request_count as usize).sum::<usize>(),
                    expected_rows
                );
                grouped_queries += 1;
            }
            4 => {
                let resolved = resolve_range_selector(Some("all"), now_ms).expect("all range");
                let groups = store
                    .group_logs(&resolved.range, &LogFilter::default(), "day")
                    .expect("day groups");
                assert_eq!(groups.len(), 365);
                assert_eq!(
                    groups.iter().map(|group| group.request_count as usize).sum::<usize>(),
                    expected_rows
                );
                grouped_queries += 1;
            }
            5 => {
                let resolved = resolve_range_selector(Some("all"), now_ms).expect("all range");
                let filter = LogFilter {
                    status: Some(UsageResult::Failure),
                    model: Some("model-0".to_string()),
                    provider: Some("Provider 0".to_string()),
                };
                let page = store
                    .query_logs(&resolved.range, &filter, 1)
                    .expect("combined filter page");
                assert!(!page.records.is_empty());
                assert!(page.records.iter().all(|row| {
                    row.result == UsageResult::Failure
                        && row.local_model == "model-0"
                        && row.provider_name == "Provider 0"
                }));
                returned_rows += page.records.len();
                filter_queries += 1;
            }
            6 => {
                let resolved = resolve_range_selector(Some("all"), now_ms).expect("all range");
                let page = store
                    .query_logs(&resolved.range, &LogFilter::default(), 1)
                    .expect("facet page");
                assert_eq!(page.models.len(), 200);
                assert_eq!(page.providers.len(), 50);
                facet_queries += 1;
            }
            _ => {
                for selector in ["today", "yesterday", "7d", "15d", "30d", "all"] {
                    let resolved = resolve_range_selector(Some(selector), now_ms)
                        .expect("supported range");
                    let page = store
                        .query_logs(&resolved.range, &LogFilter::default(), 1)
                        .expect("six-range page");
                    assert_eq!(page.page, 1);
                    range_queries += 1;
                }
            }
        }
        samples.push(elapsed_ms(operation));
    }
    let wall_ms = elapsed_ms(started);
    (
        samples,
        json!({
            "business_wall_ms": wall_ms,
            "range_queries": range_queries,
            "first_last_queries": first_last_queries,
            "grouped_queries": grouped_queries,
            "filter_queries": filter_queries,
            "facet_queries": facet_queries,
            "result_rows": returned_rows,
        }),
    )
}

fn stats_mixed_pass(store: &UsageLogStore, now_ms: i64) -> (Vec<f64>, Value) {
    let mut samples = Vec::with_capacity(OPS);
    let mut range_queries = 0usize;
    let mut returned_models = 0usize;
    let started = Instant::now();
    for index in 0..OPS {
        let operation = Instant::now();
        let resolved = resolve_range_selector(Some(range_selector(index)), now_ms)
            .expect("supported range");
        let stats = store
            .usage_stats(&resolved.range, resolved.hourly)
            .expect("usage stats");
        assert!(stats.models.len() <= 200);
        returned_models += stats.models.len();
        range_queries += 1;
        samples.push(elapsed_ms(operation));
    }
    let wall_ms = elapsed_ms(started);
    (
        samples,
        json!({
            "business_wall_ms": wall_ms,
            "range_queries": range_queries,
            "models_returned": returned_models,
        }),
    )
}

fn run_usage_query_case(case: &str, rows: usize, stats: bool) {
    let dir = TempDir::new(&format!("usage-{rows}-{stats}"));
    let store = UsageLogStore::at(dir.0.join("ai_gateway_usage.db"));
    let now_ms = chrono::Utc::now().timestamp_millis();
    seed_usage_fixture(&store, rows, 50, 200, 365, now_ms);
    let dimensions = json!({
        "detail_rows": rows,
        "providers": 50,
        "models": 200,
        "days": 365,
        "selectors": ["today", "yesterday", "7d", "15d", "30d", "all"],
        "operations": if stats { "stats" } else { "first+last+groups+filters+facets" },
    });
    for pass in 0..=WARM_PASSES {
        let (samples, counters) = if stats {
            stats_mixed_pass(&store, now_ms)
        } else {
            query_mixed_pass(&store, now_ms, rows)
        };
        let wall_ms = counters["business_wall_ms"].as_f64().expect("measured wall");
        emit_pass(case, pass, samples, wall_ms, OPS, &dimensions, counters);
    }
}

fn history_record(day_index: usize, now_ms: i64) -> UsageLogRecord {
    UsageLogRecord {
        timestamp_ms: now_ms - (day_index as i64) * 86_400_000,
        local_model: "local-history".to_string(),
        upstream_model: "upstream-history".to_string(),
        provider_id: "provider-history".to_string(),
        provider_name: "Provider History".to_string(),
        result: UsageResult::Success,
        status: 200,
        input_tokens: 10,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        output_tokens: 5,
        total_tokens: 15,
        amount: Some(0.001),
        cost_breakdown: None,
        duration_ms: 5,
        error_message: None,
        terminal: true,
        reasoning_effort: None,
        first_token_ms: None,
        avg_tps: None,
        resolved_reasoning_effort: None,
        native_reasoning: None,
    }
}

fn seed_history(
    store: &UsageLogStore,
    days: usize,
    now_ms: i64,
    continuity_start_day: Option<usize>,
) {
    if let Some(offset) = continuity_start_day {
        let continuity_ms = now_ms - (offset as i64) * 86_400_000;
        crate::ai_gateway::usage_log::set_usage_clock_for_test(continuity_ms);
        let connection = store.open().expect("open history store");
        crate::ai_gateway::provider_usage::create_summary_tables(&connection)
            .expect("create summary tables");
        crate::ai_gateway::provider_usage::stamp_continuity_boundary(&connection)
            .expect("stamp continuity");
        drop(connection);
    }
    crate::ai_gateway::usage_log::set_usage_clock_for_test(now_ms);
    let mut chunk = Vec::with_capacity(100);
    for day in 0..days {
        // Leave genuine zero days (continuous coverage, no request) and skip
        // one retained-day gap so the 366-day fixture has unavailable dates.
        if day % 97 == 0 && day != 0 && day + 1 != days {
            continue;
        }
        if days == 366 && (day == 3 || day == 10 || day == 11) {
            continue;
        }
        chunk.push(history_record(day, now_ms));
        if chunk.len() == 100 || day + 1 == days {
            store
                .append_batch(&chunk, 365)
                .expect("seed provider history");
            chunk.clear();
        }
    }
    crate::ai_gateway::usage_log::clear_usage_clock_for_test();
}

fn utc8_day(now_ms: i64, offset_days: i64) -> String {
    let shifted = now_ms + 8 * 60 * 60 * 1000 - offset_days * 86_400_000;
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(shifted)
        .expect("valid timestamp")
        .format("%Y-%m-%d")
        .to_string()
}

fn run_history_case(case: &str, days: usize, continuity_offset: Option<usize>) {
    let dir = TempDir::new(case);
    let store = UsageLogStore::at(dir.0.join("ai_gateway_usage.db"));
    let now_ms = chrono::Utc::now().timestamp_millis();
    seed_history(&store, days, now_ms, continuity_offset);
    let history = store
        .provider_usage_history("provider-history", now_ms)
        .expect("provider history");
    assert_eq!(history.days.len(), days, "every calendar date stays present");
    assert_eq!(
        history.days.first().map(|day| day.day.as_str()),
        Some(utc8_day(now_ms, days as i64 - 1).as_str()),
        "the first calendar date is exact"
    );
    assert_eq!(
        history.days.last().map(|day| day.day.as_str()),
        Some(utc8_day(now_ms, 0).as_str()),
        "the last calendar date is exact"
    );
    let zero_days = history
        .days
        .iter()
        .filter(|day| {
            day.coverage == ProviderDayCoverage::Continuous
                && day.metrics.as_ref().map(|metrics| metrics.request_count) == Some(0)
        })
        .count();
    let unavailable_days = history
        .days
        .iter()
        .filter(|day| day.coverage == ProviderDayCoverage::Unavailable)
        .count();
    let ongoing_days = history.days.iter().filter(|day| day.ongoing).count();
    assert_eq!(ongoing_days, 1, "only the current UTC+8 day is ongoing");
    assert_eq!(history.days.last().map(|day| day.ongoing), Some(true));
    if days == 366 {
        assert!(zero_days > 0, "continuous no-request days stay genuine zero");
        assert!(unavailable_days > 0, "the retained gap stays unavailable");
    }
    assert!(zero_days > 0, "the fixture keeps at least one genuine zero day");
    let deleted = store
        .provider_usage_identities(&[])
        .expect("provider identities");
    assert_eq!(deleted.len(), 1);
    assert!(deleted[0].deleted && deleted[0].has_usage);

    let dimensions = json!({
        "days": days,
        "coverage": "zero|unavailable|ongoing",
        "continuity_start_offset_days": continuity_offset,
    });
    for pass in 0..=WARM_PASSES {
        let mut samples = Vec::with_capacity(OPS);
        let started = Instant::now();
        let mut calendar_days = 0usize;
        for _ in 0..OPS {
            let operation = Instant::now();
            let next = store
                .provider_usage_history("provider-history", now_ms)
                .expect("provider history");
            calendar_days += next.days.len();
            samples.push(elapsed_ms(operation));
        }
        let wall_ms = elapsed_ms(started);
        emit_pass(
            case,
            pass,
            samples,
            wall_ms,
            OPS,
            &dimensions,
            json!({
                "history_queries": OPS,
                "calendar_days": calendar_days,
                "zero_days": zero_days,
                "unavailable_days": unavailable_days,
                "ongoing_days": ongoing_days,
                "deleted_identities": deleted.len(),
            }),
        );
    }
}

fn run_template_case() {
    let config = GatewayConfig::default();
    let dimensions = json!({"builtin_templates": true, "persisted_templates": 0});
    for pass in 0..=WARM_PASSES {
        let mut samples = Vec::with_capacity(OPS);
        let started = Instant::now();
        let mut views = 0usize;
        for _ in 0..OPS {
            let operation = Instant::now();
            views += provider_template_views(&config)
                .expect("provider template views")
                .len();
            samples.push(elapsed_ms(operation));
        }
        let wall_ms = elapsed_ms(started);
        assert!(views > 0);
        emit_pass(
            "template_views",
            pass,
            samples,
            wall_ms,
            OPS,
            &dimensions,
            json!({"view_builds": OPS, "views": views}),
        );
    }
}

async fn dispatch(case: &str) {
    match case {
        "config_status_models_small" => emit_config_case(case, 4, 20, 2),
        "config_status_models_large" => emit_config_case(case, 100, 100, 4),
        "routing_small" => run_routing_case(case, 4, 20, 2).await,
        "routing_large" => run_routing_case(case, 100, 100, 4).await,
        "forwarding_chat_sync_32k_c1" => {
            run_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions, 1, 32 * 1024, false, None, None).await
        }
        "forwarding_chat_sync_32k_c8" => {
            run_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions, 8, 32 * 1024, false, None, None).await
        }
        "forwarding_chat_sync_32k_c32" => {
            run_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions, 32, 32 * 1024, false, None, None).await
        }
        "forwarding_chat_sync_256k_c1" => {
            run_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions, 1, 256 * 1024, false, None, None).await
        }
        "forwarding_responses_sync_32k_c1" => {
            run_loopback_case(case, UpstreamProtocol::Responses, UpstreamProtocol::Responses, 1, 32 * 1024, false, None, None).await
        }
        "forwarding_messages_sync_32k_c1" => {
            run_loopback_case(case, UpstreamProtocol::AnthropicMessages, UpstreamProtocol::AnthropicMessages, 1, 32 * 1024, false, None, None).await
        }
        "forwarding_chat_stream_64_c1" => {
            run_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions, 1, 32 * 1024, true, Some(1_000), Some(64)).await
        }
        "forwarding_chat_stream_4096_c1" => {
            run_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions, 1, 256 * 1024, true, Some(1_000), Some(4096)).await
        }
        "forwarding_responses_stream_64_c1" => {
            run_loopback_case(case, UpstreamProtocol::Responses, UpstreamProtocol::Responses, 1, 32 * 1024, true, Some(1_000), Some(64)).await
        }
        "forwarding_messages_stream_64_c1" => {
            run_loopback_case(case, UpstreamProtocol::AnthropicMessages, UpstreamProtocol::AnthropicMessages, 1, 32 * 1024, true, Some(1_000), Some(64)).await
        }
        "direct_mock_chat_sync_32k" => {
            run_direct_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions, false, None).await
        }
        "direct_mock_responses_sync_32k" => {
            run_direct_case(case, UpstreamProtocol::Responses, UpstreamProtocol::Responses, false, None).await
        }
        "direct_mock_messages_sync_32k" => {
            run_direct_case(case, UpstreamProtocol::AnthropicMessages, UpstreamProtocol::AnthropicMessages, false, None).await
        }
        "direct_mock_chat_stream_64" => {
            run_direct_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions, true, Some(64)).await
        }
        "direct_mock_chat_stream_4096" => {
            run_direct_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions, true, Some(4096)).await
        }
        "direct_mock_responses_stream_64" => {
            run_direct_case(case, UpstreamProtocol::Responses, UpstreamProtocol::Responses, true, Some(64)).await
        }
        "direct_mock_messages_stream_64" => {
            run_direct_case(case, UpstreamProtocol::AnthropicMessages, UpstreamProtocol::AnthropicMessages, true, Some(64)).await
        }
        "conversion_chat_to_messages_sync" => {
            run_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::AnthropicMessages, 1, 32 * 1024, false, None, None).await
        }
        "conversion_responses_to_messages_sync" => {
            run_loopback_case(case, UpstreamProtocol::Responses, UpstreamProtocol::AnthropicMessages, 1, 32 * 1024, false, None, None).await
        }
        "conversion_chat_to_messages_stream" => {
            run_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::AnthropicMessages, 1, 32 * 1024, true, Some(1_000), Some(64)).await
        }
        "conversion_responses_to_messages_stream" => {
            run_loopback_case(case, UpstreamProtocol::Responses, UpstreamProtocol::AnthropicMessages, 1, 32 * 1024, true, Some(1_000), Some(64)).await
        }
        "direct_mock_conversion_chat_to_messages_sync" => {
            run_direct_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::AnthropicMessages, false, None).await
        }
        "direct_mock_conversion_responses_to_messages_sync" => {
            run_direct_case(case, UpstreamProtocol::Responses, UpstreamProtocol::AnthropicMessages, false, None).await
        }
        "direct_mock_conversion_chat_to_messages_stream" => {
            run_direct_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::AnthropicMessages, true, Some(64)).await
        }
        "direct_mock_conversion_responses_to_messages_stream" => {
            run_direct_case(case, UpstreamProtocol::Responses, UpstreamProtocol::AnthropicMessages, true, Some(64)).await
        }
        "pool_decision_10x20" => run_pool_decision_case(),
        "pool_chat_to_chat" => {
            run_pool_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::ChatCompletions).await
        }
        "pool_chat_to_responses" => {
            run_pool_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::Responses).await
        }
        "pool_chat_to_messages" => {
            run_pool_loopback_case(case, UpstreamProtocol::ChatCompletions, UpstreamProtocol::AnthropicMessages).await
        }
        "pool_responses_to_chat" => {
            run_pool_loopback_case(case, UpstreamProtocol::Responses, UpstreamProtocol::ChatCompletions).await
        }
        "pool_responses_to_responses" => {
            run_pool_loopback_case(case, UpstreamProtocol::Responses, UpstreamProtocol::Responses).await
        }
        "pool_responses_to_messages" => {
            run_pool_loopback_case(case, UpstreamProtocol::Responses, UpstreamProtocol::AnthropicMessages).await
        }
        "usage_log_append_10000x3" => run_append_case(),
        "usage_stats_1k" => run_usage_query_case(case, 1_000, true),
        "usage_stats_100k" => run_usage_query_case(case, 100_000, true),
        "usage_query_1k" => run_usage_query_case(case, 1_000, false),
        "usage_query_100k" => run_usage_query_case(case, 100_000, false),
        "provider_history_366" => run_history_case(case, 366, Some(6)),
        "provider_history_3650" => run_history_case(case, 3_650, Some(3_649)),
        "template_views" => run_template_case(),
        other => panic!("unknown measurement case: {other}"),
    }
}

macro_rules! perf_case {
    ($name:ident, $case:literal) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
        #[ignore = "fixed whole-gateway measurement; run via tools/measure-ai-gateway.mjs"]
        async fn $name() {
            dispatch($case).await;
        }
    };
}

perf_case!(gateway_performance_config_status_models_small, "config_status_models_small");
perf_case!(gateway_performance_config_status_models_large, "config_status_models_large");
perf_case!(gateway_performance_routing_small, "routing_small");
perf_case!(gateway_performance_routing_large, "routing_large");
perf_case!(gateway_performance_forwarding_chat_sync_32k_c1, "forwarding_chat_sync_32k_c1");
perf_case!(gateway_performance_forwarding_chat_sync_32k_c8, "forwarding_chat_sync_32k_c8");
perf_case!(gateway_performance_forwarding_chat_sync_32k_c32, "forwarding_chat_sync_32k_c32");
perf_case!(gateway_performance_forwarding_chat_sync_256k_c1, "forwarding_chat_sync_256k_c1");
perf_case!(gateway_performance_forwarding_responses_sync_32k_c1, "forwarding_responses_sync_32k_c1");
perf_case!(gateway_performance_forwarding_messages_sync_32k_c1, "forwarding_messages_sync_32k_c1");
perf_case!(gateway_performance_forwarding_chat_stream_64_c1, "forwarding_chat_stream_64_c1");
perf_case!(gateway_performance_forwarding_chat_stream_4096_c1, "forwarding_chat_stream_4096_c1");
perf_case!(gateway_performance_forwarding_responses_stream_64_c1, "forwarding_responses_stream_64_c1");
perf_case!(gateway_performance_forwarding_messages_stream_64_c1, "forwarding_messages_stream_64_c1");
perf_case!(gateway_performance_direct_mock_chat_sync_32k, "direct_mock_chat_sync_32k");
perf_case!(gateway_performance_direct_mock_responses_sync_32k, "direct_mock_responses_sync_32k");
perf_case!(gateway_performance_direct_mock_messages_sync_32k, "direct_mock_messages_sync_32k");
perf_case!(gateway_performance_direct_mock_chat_stream_64, "direct_mock_chat_stream_64");
perf_case!(gateway_performance_direct_mock_chat_stream_4096, "direct_mock_chat_stream_4096");
perf_case!(gateway_performance_direct_mock_responses_stream_64, "direct_mock_responses_stream_64");
perf_case!(gateway_performance_direct_mock_messages_stream_64, "direct_mock_messages_stream_64");
perf_case!(gateway_performance_conversion_chat_to_messages_sync, "conversion_chat_to_messages_sync");
perf_case!(gateway_performance_conversion_responses_to_messages_sync, "conversion_responses_to_messages_sync");
perf_case!(gateway_performance_conversion_chat_to_messages_stream, "conversion_chat_to_messages_stream");
perf_case!(gateway_performance_conversion_responses_to_messages_stream, "conversion_responses_to_messages_stream");
perf_case!(gateway_performance_direct_mock_conversion_chat_to_messages_sync, "direct_mock_conversion_chat_to_messages_sync");
perf_case!(gateway_performance_direct_mock_conversion_responses_to_messages_sync, "direct_mock_conversion_responses_to_messages_sync");
perf_case!(gateway_performance_direct_mock_conversion_chat_to_messages_stream, "direct_mock_conversion_chat_to_messages_stream");
perf_case!(gateway_performance_direct_mock_conversion_responses_to_messages_stream, "direct_mock_conversion_responses_to_messages_stream");
perf_case!(gateway_performance_pool_decision_10x20, "pool_decision_10x20");
perf_case!(gateway_performance_pool_chat_to_chat, "pool_chat_to_chat");
perf_case!(gateway_performance_pool_chat_to_responses, "pool_chat_to_responses");
perf_case!(gateway_performance_pool_chat_to_messages, "pool_chat_to_messages");
perf_case!(gateway_performance_pool_responses_to_chat, "pool_responses_to_chat");
perf_case!(gateway_performance_pool_responses_to_responses, "pool_responses_to_responses");
perf_case!(gateway_performance_pool_responses_to_messages, "pool_responses_to_messages");
perf_case!(gateway_performance_usage_log_append_10000x3, "usage_log_append_10000x3");
perf_case!(gateway_performance_usage_stats_1k, "usage_stats_1k");
perf_case!(gateway_performance_usage_stats_100k, "usage_stats_100k");
perf_case!(gateway_performance_usage_query_1k, "usage_query_1k");
perf_case!(gateway_performance_usage_query_100k, "usage_query_100k");
perf_case!(gateway_performance_provider_history_366, "provider_history_366");
perf_case!(gateway_performance_provider_history_3650, "provider_history_3650");
perf_case!(gateway_performance_template_views, "template_views");

/// REQ-004/AC-005 non-ignored regression: two identical public reads must reuse
/// the same initialized query connection and their prepared statements. The
/// test-only counters count real `Connection::open`, schema/migration
/// initialization and statement preparation caused by the public call; they are
/// thread-local so this test stays independent of parallel store tests.
#[test]
fn repeated_query_reuses_initialized_connection_and_prepared_statements() {
    let dir = TempDir::new("query-reuse");
    let store = UsageLogStore::at(dir.0.join("ai_gateway_usage.db"));
    let now_ms = chrono::Utc::now().timestamp_millis();
    seed_usage_fixture(&store, 60, 5, 5, 7, now_ms);
    let range = resolve_range_selector(Some("all"), now_ms)
        .expect("all range")
        .range;

    reset_query_work_stats();
    let first = store
        .query_logs(&range, &LogFilter::default(), 1)
        .expect("first public read");
    let after_first = query_work_stats();
    assert_eq!(
        after_first.connection_opens, 1,
        "the first read must open the actual database once"
    );
    assert_eq!(
        after_first.initializations, 1,
        "the first read must initialize the actual database once"
    );
    assert!(
        after_first.statement_prepares > 0,
        "the first read must prepare its real statements: {after_first:?}"
    );

    let second = store
        .query_logs(&range, &LogFilter::default(), 1)
        .expect("second public read");
    let after_second = query_work_stats();
    assert_eq!(second.total, first.total);
    assert_eq!(
        after_second.connection_opens, after_first.connection_opens,
        "a repeated read must reuse the open query connection: {after_second:?}"
    );
    assert_eq!(
        after_second.initializations, after_first.initializations,
        "a repeated read must not rerun schema or migration initialization: {after_second:?}"
    );
    assert_eq!(
        after_second.statement_prepares, after_first.statement_prepares,
        "identical public reads must reuse their prepared statements: {after_second:?}"
    );
}
