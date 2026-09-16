use super::commands::{default_key_for_sync, plan_terminal_sync, terminal_sync_pending};
use super::selection::{
    candidate_providers, classify_failure, manual_reenable, pick_candidate, register_failure,
    register_success, resolve_model_for_protocol, set_user_enabled, FailureClass, ModelResolution,
};
use super::storage::{config_path, resolve_default_key_id};
use super::{
    FusionConfig, FusionKey, FusionUpstreamProvider, ModelMapping, TerminalSyncRecord,
    UpstreamProtocol,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn make_temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "onespace-api-fusion-{}-{}",
        name,
        uuid::Uuid::new_v4()
    ))
}

fn with_temp_home<T>(name: &str, f: impl FnOnce(&Path) -> T) -> T {
    let _guard = crate::lock_test_home_env();
    let temp_home = make_temp_dir(name);
    fs::create_dir_all(&temp_home).expect("create temp home");
    let original_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &temp_home);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&temp_home)));
    if let Some(home) = original_home {
        std::env::set_var("HOME", home);
    } else {
        std::env::remove_var("HOME");
    }
    let _ = fs::remove_dir_all(&temp_home);
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

fn key(id: &str, enabled: bool) -> FusionKey {
    FusionKey {
        id: id.to_string(),
        label: id.to_string(),
        value: format!("value-{id}"),
        enabled,
        created_at: 1,
    }
}

fn provider(id: &str) -> FusionUpstreamProvider {
    FusionUpstreamProvider {
        id: id.to_string(),
        name: format!("Provider {id}"),
        base_url: "https://api.example.com/v1".to_string(),
        api_key: "sk-test".to_string(),
        default_model: None,
        protocol: UpstreamProtocol::ChatCompletions,
        mappings: Vec::new(),
        enabled: true,
        auto_disabled: false,
        disabled_reason: None,
        disabled_at: None,
        consecutive_failures: 0,
        last_error_at: None,
    }
}

#[test]
fn fusion_config_round_trips_and_encrypts_secrets_on_disk() {
    with_temp_home("roundtrip", |_home| {
        let mut config = FusionConfig::default();
        let mut first = provider("p1");
        first.api_key = "sk-super-secret-123".to_string();
        first.default_model = Some("remote-default".to_string());
        first.mappings = vec![ModelMapping {
            local_model: "local-a".to_string(),
            upstream_model: "remote-a".to_string(),
            protocol: None,
        }];
        config.providers.push(first);
        config.keys.push(FusionKey {
            id: "k1".to_string(),
            label: "Key 1".to_string(),
            value: "local-key-abc".to_string(),
            enabled: true,
            created_at: 1,
        });

        super::storage::write_config(&config).expect("write config");

        let loaded = super::storage::read_config().expect("read config");
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].api_key, "sk-super-secret-123");
        assert_eq!(
            loaded.providers[0].mappings[0].upstream_model,
            "remote-a"
        );
        assert_eq!(loaded.providers[0].default_model.as_deref(), Some("remote-default"));
        assert_eq!(loaded.keys.len(), 1);
        assert_eq!(loaded.keys[0].value, "local-key-abc");
        assert_eq!(loaded.default_key_id.as_deref(), Some("k1"));

        let raw = fs::read_to_string(config_path().unwrap()).unwrap();
        assert!(
            !raw.contains("sk-super-secret-123"),
            "provider api key must not be stored in plaintext"
        );
        assert!(
            !raw.contains("local-key-abc"),
            "local api key must not be stored in plaintext"
        );
        assert!(raw.trim_start().starts_with("v2:"));
    });
}

#[test]
fn default_key_prefers_first_enabled_and_advances_on_disable() {
    let keys = vec![key("k1", false), key("k2", true), key("k3", true)];
    assert_eq!(resolve_default_key_id(&keys, None).as_deref(), Some("k2"));

    // Manual switch wins while the chosen key is enabled.
    assert_eq!(
        resolve_default_key_id(&keys, Some("k3")).as_deref(),
        Some("k3")
    );

    // Disabling the current default advances to the next enabled entry in order.
    let advanced = vec![key("k1", true), key("k2", false), key("k3", true)];
    assert_eq!(
        resolve_default_key_id(&advanced, Some("k2")).as_deref(),
        Some("k3")
    );

    // Wrap-around when the tail is disabled too.
    let wrapped = vec![key("k1", true), key("k2", false), key("k3", false)];
    assert_eq!(
        resolve_default_key_id(&wrapped, Some("k2")).as_deref(),
        Some("k1")
    );

    // No enabled entry means an empty default key.
    let none_enabled = vec![key("k1", false), key("k2", false)];
    assert_eq!(resolve_default_key_id(&none_enabled, Some("k1")), None);
}

#[test]
fn default_key_choice_persists_across_reload() {
    with_temp_home("default-key-persist", |_home| {
        let mut config = FusionConfig::default();
        config.keys = vec![key("k1", true), key("k2", true), key("k3", true)];
        super::storage::write_config(&config).expect("write config");
        let mut loaded = super::storage::read_config().expect("read config");
        assert_eq!(loaded.default_key_id.as_deref(), Some("k1"));

        loaded.default_key_id = Some("k3".to_string());
        super::storage::write_config(&loaded).expect("write config");
        let reloaded = super::storage::read_config().expect("read config");
        assert_eq!(reloaded.default_key_id.as_deref(), Some("k3"));
    });
}

#[test]
fn resolve_model_for_protocol_prefers_matching_rows_and_rejects_other_protocols() {
    // A row without its own protocol inherits the provider protocol; the default
    // model only serves unmapped models under the same inbound protocol.
    let mut mapped = provider("mapped");
    mapped.default_model = Some("remote-default".to_string());
    mapped.mappings = vec![ModelMapping {
        local_model: "local-a".to_string(),
        upstream_model: "remote-a".to_string(),
        protocol: None,
    }];

    // 1. A matching row whose effective protocol equals the inbound protocol is served,
    //    and it wins over the default model.
    assert!(matches!(
        resolve_model_for_protocol(&mapped, Some("local-a"), UpstreamProtocol::ChatCompletions),
        ModelResolution::Serve(ref model) if model.as_str() == "remote-a"
    ));

    // 2. The same row under another inbound protocol is a mismatch; the default model
    //    must not be used as a fallback.
    assert!(matches!(
        resolve_model_for_protocol(&mapped, Some("local-a"), UpstreamProtocol::Responses),
        ModelResolution::ProtocolMismatch(UpstreamProtocol::ChatCompletions)
    ));

    // 3. An unmapped model falls back to the default model when the provider protocol matches.
    assert!(matches!(
        resolve_model_for_protocol(&mapped, Some("local-unknown"), UpstreamProtocol::ChatCompletions),
        ModelResolution::Serve(ref model) if model.as_str() == "remote-default"
    ));

    // 4. An unmapped model is a miss when the provider protocol does not match.
    assert!(matches!(
        resolve_model_for_protocol(&mapped, Some("local-unknown"), UpstreamProtocol::Responses),
        ModelResolution::NoMatch
    ));

    // A row may pin the inbound protocol even when the provider exposes another family.
    let mut per_model = provider("per-model");
    per_model.default_model = Some("remote-default".to_string());
    per_model.mappings = vec![ModelMapping {
        local_model: "local-r".to_string(),
        upstream_model: "remote-r".to_string(),
        protocol: Some(UpstreamProtocol::Responses),
    }];
    assert!(matches!(
        resolve_model_for_protocol(&per_model, Some("local-r"), UpstreamProtocol::Responses),
        ModelResolution::Serve(ref model) if model.as_str() == "remote-r"
    ));
    assert!(matches!(
        resolve_model_for_protocol(&per_model, Some("local-r"), UpstreamProtocol::ChatCompletions),
        ModelResolution::ProtocolMismatch(UpstreamProtocol::Responses)
    ));

    // 5. A matching row with an empty upstream model is discarded, so it neither serves
    //    nor triggers a mismatch: the default-model rules above still decide.
    let mut blank_row = provider("blank-row");
    blank_row.default_model = Some("remote-default".to_string());
    blank_row.mappings = vec![ModelMapping {
        local_model: "local-blank".to_string(),
        upstream_model: "   ".to_string(),
        protocol: Some(UpstreamProtocol::Responses),
    }];
    assert!(matches!(
        resolve_model_for_protocol(&blank_row, Some("local-blank"), UpstreamProtocol::ChatCompletions),
        ModelResolution::Serve(ref model) if model.as_str() == "remote-default"
    ));
    assert!(matches!(
        resolve_model_for_protocol(&blank_row, Some("local-blank"), UpstreamProtocol::Responses),
        ModelResolution::NoMatch
    ));

    // Neither a matching row nor an eligible default model is always a miss.
    let no_model = provider("no-model");
    assert!(matches!(
        resolve_model_for_protocol(&no_model, Some("anything"), UpstreamProtocol::ChatCompletions),
        ModelResolution::NoMatch
    ));
}

#[test]
fn candidate_providers_requires_enabled_active_and_resolvable() {
    let mut serving = provider("serving");
    serving.default_model = Some("remote-default".to_string());
    let mut disabled = provider("disabled");
    disabled.default_model = Some("remote-default".to_string());
    disabled.enabled = false;
    let mut auto_disabled = provider("auto-disabled");
    auto_disabled.default_model = Some("remote-default".to_string());
    auto_disabled.auto_disabled = true;
    let unresolvable = provider("unresolvable");

    let providers = vec![serving, disabled, auto_disabled, unresolvable];
    let candidates = candidate_providers(
        &providers,
        Some("local-unknown"),
        UpstreamProtocol::ChatCompletions,
    );
    let ids: Vec<&str> = candidates.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, vec!["serving"]);
}

#[test]
fn pick_candidate_covers_all_candidates() {
    let mut a = provider("a");
    a.default_model = Some("remote-default".to_string());
    let mut b = provider("b");
    b.default_model = Some("remote-default".to_string());
    let candidates = vec![a, b];

    let mut seen = HashSet::new();
    for _ in 0..200 {
        if let Some(picked) = pick_candidate(&candidates) {
            seen.insert(picked.id);
        }
    }
    assert_eq!(seen.len(), 2, "both candidates must be selected over time");
}

#[test]
fn classify_failure_matrix_matches_spec() {
    assert_eq!(classify_failure(401, false, true), FailureClass::DisableImmediately);
    assert_eq!(classify_failure(403, false, true), FailureClass::DisableImmediately);
    assert_eq!(classify_failure(429, false, true), FailureClass::Transient);
    assert_eq!(classify_failure(404, false, true), FailureClass::Transient);
    assert_eq!(classify_failure(500, false, true), FailureClass::Retryable);
    assert_eq!(classify_failure(503, false, true), FailureClass::Retryable);
    assert_eq!(classify_failure(400, false, true), FailureClass::ReturnToClient);
    assert_eq!(classify_failure(422, false, true), FailureClass::ReturnToClient);
    assert_eq!(classify_failure(418, false, true), FailureClass::ReturnToClient);
    // Non-JSON body (even with a 2xx status) is retryable.
    assert_eq!(classify_failure(200, false, false), FailureClass::Retryable);
    assert_eq!(classify_failure(500, false, false), FailureClass::Retryable);
    // Network errors are retryable regardless of status.
    assert_eq!(classify_failure(0, true, true), FailureClass::Retryable);
}

#[test]
fn auto_disable_threshold_immediate_disable_and_success_reset() {
    let mut threshold = provider("threshold");
    assert!(!register_failure(&mut threshold, FailureClass::Retryable, "boom", 10));
    assert!(!register_failure(&mut threshold, FailureClass::Retryable, "boom", 11));
    assert_eq!(threshold.consecutive_failures, 2);
    assert!(!threshold.auto_disabled);
    assert!(register_failure(&mut threshold, FailureClass::Retryable, "boom", 12));
    assert!(threshold.auto_disabled);
    assert_eq!(threshold.consecutive_failures, 3);
    assert_eq!(threshold.disabled_reason.as_deref(), Some("boom"));
    assert_eq!(threshold.disabled_at, Some(12));

    let mut auth = provider("auth");
    assert!(register_failure(&mut auth, FailureClass::DisableImmediately, "unauthorized", 5));
    assert!(auth.auto_disabled);
    assert_eq!(auth.disabled_reason.as_deref(), Some("unauthorized"));

    let mut transient = provider("transient");
    assert!(!register_failure(&mut transient, FailureClass::Transient, "rate limited", 5));
    assert!(!register_failure(&mut transient, FailureClass::Transient, "not found", 6));
    assert!(!register_failure(&mut transient, FailureClass::Transient, "rate limited", 7));
    assert!(!transient.auto_disabled);
    assert_eq!(transient.consecutive_failures, 0);

    let mut returned = provider("returned");
    assert!(!register_failure(&mut returned, FailureClass::ReturnToClient, "bad request", 5));
    assert!(!returned.auto_disabled);

    let mut recovered = provider("recovered");
    register_failure(&mut recovered, FailureClass::Retryable, "x", 1);
    register_success(&mut recovered);
    assert_eq!(recovered.consecutive_failures, 0);
    assert_eq!(recovered.last_error_at, None);
}

#[test]
fn auto_disabled_state_persists_and_separates_from_user_enabled() {
    with_temp_home("auto-disable-persist", |_home| {
        let mut config = FusionConfig::default();
        let mut p = provider("p1");
        p.enabled = true;
        register_failure(&mut p, FailureClass::DisableImmediately, "auth failed", 123);
        config.providers.push(p);
        super::storage::write_config(&config).expect("write config");

        let mut loaded = super::storage::read_config().expect("read config");
        assert!(loaded.providers[0].enabled);
        assert!(loaded.providers[0].auto_disabled);
        assert_eq!(
            loaded.providers[0].disabled_reason.as_deref(),
            Some("auth failed")
        );
        assert_eq!(loaded.providers[0].disabled_at, Some(123));

        manual_reenable(&mut loaded.providers[0]);
        super::storage::write_config(&loaded).expect("write config");
        let reloaded = super::storage::read_config().expect("read config");
        assert!(reloaded.providers[0].enabled, "user intent must be preserved");
        assert!(!reloaded.providers[0].auto_disabled);
        assert_eq!(reloaded.providers[0].disabled_reason, None);
        assert_eq!(reloaded.providers[0].disabled_at, None);
    });
}

#[test]
fn user_toggle_does_not_mask_auto_disabled_state() {
    let mut p = provider("p1");
    set_user_enabled(&mut p, true);
    register_failure(&mut p, FailureClass::DisableImmediately, "auth", 1);
    set_user_enabled(&mut p, true);
    assert!(p.enabled);
    assert!(p.auto_disabled);
}

// ---------------------------------------------------------------------------
// Step 3: HTTP runtime and pass-through forwarding (mock upstream)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Captured {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

type CapturedLog = Arc<Mutex<Vec<Captured>>>;

enum MockReply {
    Json(u16, Value),
    Stream(String),
    /// Declare a larger content-length than the bytes sent, then close early.
    PartialStream(String, usize),
    /// Arbitrary status and content type with a raw (possibly non-JSON) body.
    Raw(u16, &'static str, Vec<u8>),
    /// Close the connection without answering.
    Drop,
}

struct TempHome {
    path: PathBuf,
    original: Option<String>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Drop for TempHome {
    fn drop(&mut self) {
        match self.original.take() {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn temp_home(name: &str) -> TempHome {
    let lock = crate::lock_test_home_env();
    let path = make_temp_dir(name);
    fs::create_dir_all(&path).expect("create temp home");
    let original = std::env::var("HOME").ok();
    std::env::set_var("HOME", &path);
    TempHome {
        path,
        original,
        _lock: lock,
    }
}

async fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind free");
    let port = listener.local_addr().expect("free addr").port();
    drop(listener);
    port
}

async fn spawn_mock_upstream<F>(behavior: F) -> (String, CapturedLog)
where
    F: Fn(&Captured) -> MockReply + Send + Sync + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    let log: CapturedLog = Arc::new(Mutex::new(Vec::new()));
    let log_for_server = log.clone();
    let behavior = Arc::new(behavior);
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let log = log_for_server.clone();
            let behavior = behavior.clone();
            tauri::async_runtime::spawn(async move {
                let Ok(request) = super::runtime_http::read_http_request(&mut stream).await else {
                    return;
                };
                let captured = Captured {
                    method: request.method,
                    path: request.path,
                    headers: request.headers,
                    body: request.body,
                };
                log.lock().expect("mock log").push(captured.clone());
                match behavior(&captured) {
                    MockReply::Json(status, value) => {
                        let body = serde_json::to_vec(&value).unwrap_or_default();
                        let header = format!(
                            "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(&body).await;
                    }
                    MockReply::Stream(body) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(body.as_bytes()).await;
                    }
                    MockReply::PartialStream(body, declared) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {declared}\r\nconnection: close\r\n\r\n"
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(body.as_bytes()).await;
                        let _ = stream.flush().await;
                    }
                    MockReply::Raw(status, content_type, body) => {
                        let header = format!(
                            "HTTP/1.1 {status} OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(&body).await;
                    }
                    MockReply::Drop => {}
                }
            });
        }
    });
    (format!("http://{}", addr), log)
}

async fn call_fusion(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> (u16, String, String) {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}{path}");
    let mut request = if method == "GET" {
        client.get(url)
    } else {
        client.post(url)
    };
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    if let Some(body) = body {
        request = request
            .header("content-type", "application/json")
            .body(serde_json::to_vec(&body).expect("encode body"));
    }
    let response = request.send().await.expect("fusion request");
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let text = response.text().await.expect("fusion body");
    (status, content_type, text)
}

fn key_named(id: &str, value: &str) -> FusionKey {
    FusionKey {
        id: id.to_string(),
        label: id.to_string(),
        value: value.to_string(),
        enabled: true,
        created_at: 1,
    }
}

fn upstream_provider(
    id: &str,
    name: &str,
    base_url: &str,
    api_key: &str,
    default_model: Option<&str>,
) -> FusionUpstreamProvider {
    FusionUpstreamProvider {
        id: id.to_string(),
        name: name.to_string(),
        base_url: base_url.to_string(),
        api_key: api_key.to_string(),
        default_model: default_model.map(str::to_string),
        protocol: UpstreamProtocol::ChatCompletions,
        mappings: Vec::new(),
        enabled: true,
        auto_disabled: false,
        disabled_reason: None,
        disabled_at: None,
        consecutive_failures: 0,
        last_error_at: None,
    }
}

#[tokio::test]
async fn forwards_chat_completions_path_body_and_provider_auth() {
    let home = temp_home("forward-chat");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({"id":"chatcmpl","choices":[{"message":{"role":"assistant","content":"ok"}}]}),
        )
    })
    .await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "upstream-secret", None);
    provider.mappings = vec![ModelMapping {
        local_model: "local-a".to_string(),
        upstream_model: "remote-a".to_string(),
        protocol: None,
    }];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let request_body = json!({
        "model": "local-a",
        "messages": [{"role": "user", "content": "hello"}],
        "temperature": 0.25,
        "max_tokens": 16
    });
    let (status, _content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(request_body),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let captured = log.lock().unwrap().clone();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].path, "/v1/chat/completions");
    assert_eq!(
        captured[0].headers.get("authorization").map(String::as_str),
        Some("Bearer upstream-secret")
    );
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(sent["model"], "remote-a");
    assert_eq!(sent["messages"], json!([{"role": "user", "content": "hello"}]));
    assert_eq!(sent["temperature"], 0.25);
    assert_eq!(sent["max_tokens"], 16);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn unversioned_openai_paths_are_normalized_before_forwarding() {
    let home = temp_home("unversioned-paths");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id":"chatcmpl","choices":[]}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "p1",
        "Provider One",
        &upstream_url,
        "sk",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, text) = call_fusion(
        port,
        "POST",
        "/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "messages": []})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let captured = log.lock().unwrap().clone();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].path, "/v1/chat/completions");

    let (models_status, _, _) = call_fusion(
        port,
        "GET",
        "/models",
        &[("authorization", "Bearer local-key")],
        None,
    )
    .await;
    assert_eq!(models_status, 200);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn forwards_responses_path() {
    let home = temp_home("forward-responses");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| MockReply::Json(200, json!({"id":"resp"}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider(
        "p1",
        "Provider One",
        &upstream_url,
        "sk",
        Some("remote-default"),
    );
    provider.protocol = UpstreamProtocol::Responses;
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("x-api-key", "local-key")],
        Some(json!({"model": "anything"})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let captured = log.lock().unwrap().clone();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].path, "/v1/responses");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// Defect 1: a provider base URL that already ends in `/v1` must not be joined
/// with a canonical path that also starts with `v1/`.
#[test]
fn join_url_collapses_duplicate_v1_between_base_and_path() {
    assert_eq!(
        super::forwarding::join_url("https://opencode.ai/zen/go/v1", "/v1/responses"),
        "https://opencode.ai/zen/go/v1/responses"
    );
    assert_eq!(
        super::forwarding::join_url("https://opencode.ai/zen/go", "/v1/responses"),
        "https://opencode.ai/zen/go/v1/responses"
    );
    assert_eq!(
        super::forwarding::join_url("http://127.0.0.1:9999", "/v1/chat/completions"),
        "http://127.0.0.1:9999/v1/chat/completions"
    );
}

/// Defect 1 end-to-end: the canonical inbound path must reach a `/v1` base URL
/// exactly once, never as `/v1/v1/...`.
#[tokio::test]
async fn provider_base_url_with_v1_does_not_double_the_version_segment() {
    let home = temp_home("no-doubled-v1");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut config = config_with_key(port);
    let mut provider = upstream_provider(
        "p1",
        "Provider One",
        &format!("{upstream_url}/v1"),
        "sk",
        None,
    );
    provider.mappings = vec![ModelMapping {
        local_model: "local-a".to_string(),
        upstream_model: "remote-a".to_string(),
        protocol: None,
    }];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "messages": []})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let captured = log.lock().unwrap().clone();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].path, "/v1/chat/completions",
        "a base URL ending in /v1 must not be doubled by the canonical path"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// Defect 2: client headers such as `x-opencode-session` are forwarded upstream,
/// while the local relay credential (`authorization`, `x-api-key`) is replaced
/// by the provider credential and never leaks.
#[tokio::test]
async fn client_headers_are_forwarded_except_relay_credentials() {
    let home = temp_home("forward-client-headers");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({"id":"chatcmpl","choices":[{"message":{"role":"assistant","content":"ok"}}]}),
        )
    })
    .await;

    let mut config = config_with_key(port);
    config.providers.push(upstream_provider(
        "p1",
        "Provider One",
        &upstream_url,
        "provider-key",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-api-key", "local-key"),
            ("x-opencode-session", "sess-1"),
        ],
        Some(json!({"model": "local-model", "messages": []})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let captured = log.lock().unwrap().clone();
    assert_eq!(captured.len(), 1);
    let headers = &captured[0].headers;
    assert_eq!(
        headers.get("x-opencode-session").map(String::as_str),
        Some("sess-1"),
        "vendor session header must reach upstream: {headers:?}"
    );
    assert_eq!(
        headers.get("authorization").map(String::as_str),
        Some("Bearer provider-key"),
        "upstream must receive exactly the provider credential: {headers:?}"
    );
    assert_ne!(
        headers.get("authorization").map(String::as_str),
        Some("Bearer local-key"),
        "the local relay key must never leak upstream: {headers:?}"
    );
    assert!(
        !headers.contains_key("x-api-key"),
        "the local x-api-key must never leak upstream: {headers:?}"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn providers_are_only_offered_the_protocol_they_are_configured_for() {
    let home = temp_home("protocol-routing");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut responses_only = upstream_provider(
        "p1",
        "Responses Only",
        &upstream_url,
        "sk",
        Some("remote-default"),
    );
    responses_only.protocol = UpstreamProtocol::Responses;
    config.providers.push(responses_only);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 502, "unexpected response: {text}");
    assert!(
        text.contains("configured for /responses"),
        "mismatch reason must be actionable: {text}"
    );
    assert!(
        log.lock().unwrap().is_empty(),
        "a protocol mismatch must not contact upstream"
    );

    let (status, _, text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn models_endpoint_returns_local_union_without_upstream() {
    let home = temp_home("models-union");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"data": []}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![
        ModelMapping {
            local_model: "local-a".to_string(),
            upstream_model: "remote-a".to_string(),
            protocol: None,
        },
        ModelMapping {
            local_model: "local-b".to_string(),
            upstream_model: "remote-b".to_string(),
            protocol: None,
        },
    ];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "GET",
        "/v1/models",
        &[("authorization", "Bearer local-key")],
        None,
    )
    .await;
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&text).unwrap();
    let mut ids: Vec<String> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["local-a".to_string(), "local-b".to_string()]);
    assert!(
        log.lock().unwrap().is_empty(),
        "GET /v1/models must not contact upstream"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn auth_accepts_bearer_and_x_api_key_and_rejects_invalid_credentials() {
    let home = temp_home("auth");
    let port = free_port().await;
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut disabled = key_named("k2", "disabled-key");
    disabled.enabled = false;
    config.keys.push(disabled);
    config.providers.push(upstream_provider(
        "p1",
        "Provider One",
        &upstream_url,
        "sk",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let body = Some(json!({"model": "local"}));
    let (bearer, _, _) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        body.clone(),
    )
    .await;
    assert_eq!(bearer, 200);
    let (x_api_key, _, _) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("x-api-key", "local-key")],
        body.clone(),
    )
    .await;
    assert_eq!(x_api_key, 200);
    let (wrong, _, _) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer nope")],
        body.clone(),
    )
    .await;
    assert_eq!(wrong, 401);
    let (disabled_key, _, _) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("x-api-key", "disabled-key")],
        body.clone(),
    )
    .await;
    assert_eq!(disabled_key, 401);
    let (missing, _, _) = call_fusion(port, "POST", "/v1/chat/completions", &[], body).await;
    assert_eq!(missing, 401);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn unauthorized_when_no_enabled_keys() {
    let home = temp_home("no-enabled-keys");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| MockReply::Json(200, json!({}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    let mut disabled = key_named("k1", "disabled-key");
    disabled.enabled = false;
    config.keys.push(disabled);
    config.providers.push(upstream_provider(
        "p1",
        "Provider One",
        &upstream_url,
        "sk",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, _) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer disabled-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 401);
    assert!(log.lock().unwrap().is_empty());

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn unknown_path_and_method_return_404() {
    let home = temp_home("unknown-path");
    let port = free_port().await;
    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "p1",
        "Provider One",
        "http://127.0.0.1:1",
        "sk",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (unknown, _, _) = call_fusion(
        port,
        "POST",
        "/v1/embeddings",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(unknown, 404);
    let (wrong_method, _, _) = call_fusion(
        port,
        "GET",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        None,
    )
    .await;
    assert_eq!(wrong_method, 404);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn retryable_failure_switches_to_next_candidate() {
    let home = temp_home("retryable-switch");
    let port = free_port().await;
    let (failing_url, failing_log) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;
    let (working_url, working_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-working"}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "failing",
        "Failing Provider",
        &failing_url,
        "sk",
        Some("remote-default"),
    ));
    config.providers.push(upstream_provider(
        "working",
        "Working Provider",
        &working_url,
        "sk",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");
    assert!(text.contains("from-working"));
    assert!(failing_log.lock().unwrap().len() <= 1, "each candidate at most once");
    assert_eq!(working_log.lock().unwrap().len(), 1);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn all_candidates_fail_returns_502_all_providers_unavailable() {
    let home = temp_home("all-fail");
    let port = free_port().await;
    let (url_a, _) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;
    let (url_b, _) =
        spawn_mock_upstream(|_| MockReply::Json(503, json!({"error": {"message": "down"}}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &url_a,
        "sk",
        Some("remote-default"),
    ));
    config.providers.push(upstream_provider(
        "b",
        "Provider B",
        &url_b,
        "sk",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 502, "unexpected response: {text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("Provider A"), "message: {message}");
    assert!(message.contains("Provider B"), "message: {message}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn streaming_all_fail_returns_200_sse_error_then_done() {
    let home = temp_home("stream-all-fail");
    let port = free_port().await;
    let (url_a, _) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;
    let (url_b, _) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &url_a,
        "sk",
        Some("remote-default"),
    ));
    config.providers.push(upstream_provider(
        "b",
        "Provider B",
        &url_b,
        "sk",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local", "stream": true})),
    )
    .await;
    assert_eq!(status, 200, "streaming stop response uses HTTP 200");
    assert!(content_type.contains("text/event-stream"), "content-type: {content_type}");
    assert!(text.contains("all_providers_unavailable"), "body: {text}");
    assert!(text.contains("data: [DONE]"), "body: {text}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn streaming_switches_when_first_provider_fails_before_first_byte() {
    let _home = temp_home("stream-switch");
    let (drop_url, _drop_log) = spawn_mock_upstream(|_| MockReply::Drop).await;
    let stream_body =
        "data: {\"choices\":[{\"delta\":{\"content\":\"from-b\"}}]}\n\ndata: [DONE]\n\n";
    let (stream_url, stream_log) =
        spawn_mock_upstream(move |_| MockReply::Stream(stream_body.to_string())).await;

    let mut config = FusionConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &drop_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &stream_url, "sk", Some("remote-default"));
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();

    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    super::runtime_http::attempt_streaming(
        &mut server,
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
    )
    .await
    .unwrap();
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("from-b"), "expected second provider stream: {text}");
    assert_eq!(stream_log.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn streaming_terminates_after_first_byte_without_switching() {
    let _home = temp_home("stream-terminate");
    let partial = "data: {\"choices\":[{\"delta\":{\"content\":\"partial-a\"}}]}\n\n";
    let (partial_url, partial_log) = spawn_mock_upstream(move |_| {
        MockReply::PartialStream(partial.to_string(), partial.len() + 500)
    })
    .await;
    let (stream_url, stream_log) = spawn_mock_upstream(|_| {
        MockReply::Stream(
            "data: {\"choices\":[{\"delta\":{\"content\":\"from-b\"}}]}\n\ndata: [DONE]\n\n"
                .to_string(),
        )
    })
    .await;

    let mut config = FusionConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &partial_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &stream_url, "sk", Some("remote-default"));
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();

    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    super::runtime_http::attempt_streaming(
        &mut server,
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
    )
    .await
    .unwrap();
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("partial-a"), "expected first provider bytes: {text}");
    assert!(
        !text.contains("from-b"),
        "must not retry after bytes were written: {text}"
    );
    assert_eq!(partial_log.lock().unwrap().len(), 1);
    assert!(
        stream_log.lock().unwrap().is_empty(),
        "second provider must not be contacted after first byte"
    );
}

#[tokio::test]
async fn bind_failure_returns_actionable_error_and_keeps_configured_port() {
    let _home = temp_home("bind-failure");
    let occupied = TcpListener::bind(("127.0.0.1", 0)).await.expect("occupy port");
    let port = occupied.local_addr().unwrap().port();

    let mut config = FusionConfig::default();
    config.port = port;
    super::storage::write_config(&config).unwrap();

    let error = super::runtime_http::start_server().await.unwrap_err();
    assert!(error.contains(&port.to_string()), "error must name the port: {error}");
    assert!(
        error.contains("bind") || error.contains("failed to bind"),
        "error must be actionable: {error}"
    );
    let reloaded = super::storage::read_config().unwrap();
    assert_eq!(reloaded.port, port, "configured port must not be rewritten");

    drop(occupied);
}

#[tokio::test]
async fn server_starts_listens_and_stops() {
    let home = temp_home("start-stop");
    let port = free_port().await;
    let (upstream_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "p1",
        "Provider One",
        &upstream_url,
        "sk",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();

    let status = super::runtime_http::start_server().await.unwrap();
    assert!(status.running);
    assert_eq!(status.port, port);
    assert_eq!(status.local_base_url, format!("http://127.0.0.1:{port}"));

    let (code, _, _) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(code, 200);

    let stopped = super::runtime_http::stop_server().await.unwrap();
    assert!(!stopped.running);

    let mut closed = false;
    for _ in 0..40 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err() {
            closed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(closed, "port must stop accepting connections after stop");
    drop(home);
}

// ---------------------------------------------------------------------------
// Step 4: commands and terminal sync
// ---------------------------------------------------------------------------

#[test]
fn config_defaults_to_port_17688() {
    with_temp_home("defaults", |_home| {
        let config = super::commands::api_fusion_get_config().unwrap();
        assert_eq!(config.port, 17688);
        assert!(!config.enabled);
    });
}

#[test]
fn terminal_sync_plan_preserves_other_fields() {
    let payload = json!({
        "providers": [
            {
                "id": "oc-1",
                "tool": "opencode",
                "name": "My OpenCode",
                "model": "gpt-x",
                "icon": "star",
                "is_enabled": false,
                "base_url": "https://old.example.com",
                "api_key": "********",
                "tool_config": { "keep": true }
            }
        ]
    });
    let plans = plan_terminal_sync(
        &payload,
        &["oc-1".to_string()],
        "http://127.0.0.1:17688",
        "local-key",
    )
    .unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].provider_id, "oc-1");
    assert_eq!(plans[0].tool, "opencode");
    let merged = &plans[0].merged;
    assert_eq!(merged["base_url"], "http://127.0.0.1:17688");
    assert_eq!(merged["api_key"], "local-key");
    assert_eq!(merged["name"], "My OpenCode");
    assert_eq!(merged["model"], "gpt-x");
    assert_eq!(merged["icon"], "star");
    assert_eq!(merged["is_enabled"], false);
    assert_eq!(merged["tool_config"]["keep"], true);
}

#[test]
fn terminal_sync_plan_rejects_unsupported_tools() {
    for tool in ["claude", "antigravity"] {
        let payload = json!({ "providers": [{ "id": "t-1", "tool": tool }] });
        let error = plan_terminal_sync(
            &payload,
            &["t-1".to_string()],
            "http://127.0.0.1:17688",
            "local-key",
        )
        .unwrap_err();
        assert!(error.contains("unsupported"), "error: {error}");
    }
}

#[test]
fn terminal_sync_plan_rejects_missing_target() {
    let payload = json!({ "providers": [] });
    let error = plan_terminal_sync(
        &payload,
        &["ghost".to_string()],
        "http://127.0.0.1:17688",
        "local-key",
    )
    .unwrap_err();
    assert!(error.contains("not found"), "error: {error}");
}

#[test]
fn terminal_sync_requires_an_enabled_default_key() {
    with_temp_home("default-key-required", |_home| {
        let mut config = FusionConfig::default();
        config.keys.push(key_named("k1", "local-key"));
        super::storage::write_config(&config).unwrap();
        let loaded = super::storage::read_config().unwrap();
        assert!(default_key_for_sync(&loaded).is_ok());

        let mut disabled = loaded.clone();
        disabled.keys[0].enabled = false;
        super::storage::write_config(&disabled).unwrap();
        let reloaded = super::storage::read_config().unwrap();
        let error = default_key_for_sync(&reloaded).unwrap_err();
        assert!(error.contains("local API key"), "error: {error}");
    });
}

#[test]
fn terminal_sync_pending_uses_ledger_not_plaintext_key() {
    let record = TerminalSyncRecord {
        provider_id: "p1".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: "http://127.0.0.1:17688".to_string(),
        synced_at: 10,
    };
    assert!(!terminal_sync_pending(
        &record,
        Some("k1"),
        "http://127.0.0.1:17688"
    ));
    assert!(terminal_sync_pending(
        &record,
        Some("k2"),
        "http://127.0.0.1:17688"
    ));
    assert!(terminal_sync_pending(
        &record,
        Some("k1"),
        "http://127.0.0.1:17777"
    ));
    assert!(terminal_sync_pending(
        &record,
        None,
        "http://127.0.0.1:17688"
    ));
}

#[test]
fn reenable_clears_auto_disabled_and_preserves_user_enabled() {
    with_temp_home("reenable-command", |_home| {
        let mut config = FusionConfig::default();
        let mut p = provider("p1");
        p.enabled = true;
        register_failure(&mut p, FailureClass::DisableImmediately, "auth", 1);
        config.providers.push(p);
        super::storage::write_config(&config).unwrap();

        let after = super::commands::api_fusion_reenable_provider("p1".to_string()).unwrap();
        assert!(after.providers[0].enabled);
        assert!(!after.providers[0].auto_disabled);
        assert_eq!(after.providers[0].disabled_reason, None);

        // Turning user intent off must not be undone by a later manual re-enable.
        let mut reloaded = super::storage::read_config().unwrap();
        reloaded.providers[0].enabled = false;
        reloaded.providers[0].auto_disabled = true;
        reloaded.providers[0].disabled_reason = Some("boom".to_string());
        super::storage::write_config(&reloaded).unwrap();

        let after = super::commands::api_fusion_reenable_provider("p1".to_string()).unwrap();
        assert!(!after.providers[0].enabled, "user intent must be preserved");
        assert!(!after.providers[0].auto_disabled);
    });
}

#[test]
fn provider_enable_command_only_changes_user_intent() {
    with_temp_home("provider-enable", |_home| {
        let mut config = FusionConfig::default();
        let mut p = provider("p1");
        p.auto_disabled = true;
        p.disabled_reason = Some("auth".to_string());
        config.providers.push(p);
        super::storage::write_config(&config).unwrap();

        let after =
            super::commands::api_fusion_set_provider_enabled("p1".to_string(), false).unwrap();
        assert!(!after.providers[0].enabled);
        assert!(after.providers[0].auto_disabled, "auto state independent");
        assert!(super::commands::api_fusion_set_provider_enabled("ghost".to_string(), true).is_err());
    });
}

#[test]
fn key_commands_persist_and_advance_default_key() {
    with_temp_home("key-commands", |_home| {
        let config = super::commands::api_fusion_upsert_key(FusionKey {
            id: "k1".to_string(),
            label: "K1".to_string(),
            value: "v1".to_string(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        assert_eq!(config.default_key_id.as_deref(), Some("k1"));

        super::commands::api_fusion_upsert_key(FusionKey {
            id: "k2".to_string(),
            label: "K2".to_string(),
            value: "v2".to_string(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        let switched = super::commands::api_fusion_set_default_key("k2".to_string()).unwrap();
        assert_eq!(switched.default_key_id.as_deref(), Some("k2"));

        // Disabling the current default advances to the next enabled key.
        let advanced = super::commands::api_fusion_upsert_key(FusionKey {
            id: "k2".to_string(),
            label: "K2".to_string(),
            value: String::new(),
            enabled: false,
            created_at: 0,
        })
        .unwrap();
        assert_eq!(advanced.default_key_id.as_deref(), Some("k1"));
        assert_eq!(advanced.keys[1].value, "v2", "empty value preserves the stored secret");

        // No enabled key remains -> default clears.
        let cleared = super::commands::api_fusion_delete_key("k1".to_string()).unwrap();
        assert_eq!(cleared.default_key_id, None);
    });
}

#[test]
fn new_keys_without_a_value_get_a_random_secret() {
    with_temp_home("key-autogen", |_home| {
        let first = super::commands::api_fusion_upsert_key(FusionKey {
            id: String::new(),
            label: "CI".to_string(),
            value: String::new(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        let first_value = first.keys[0].value.clone();
        assert!(first_value.starts_with("sk-fusion-"), "unexpected key: {first_value}");
        assert!(first_value.len() > "sk-fusion-".len());

        let second = super::commands::api_fusion_upsert_key(FusionKey {
            id: String::new(),
            label: "CI 2".to_string(),
            value: String::new(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        assert_ne!(first_value, second.keys[1].value, "generated keys must differ");
    });
}

#[test]
fn provider_delete_removes_ledger_entry() {
    with_temp_home("provider-delete", |_home| {
        let mut config = FusionConfig::default();
        config.providers.push(provider("p1"));
        config.terminal_syncs.push(TerminalSyncRecord {
            provider_id: "p1".to_string(),
            tool: "opencode".to_string(),
            synced_key_id: "k1".to_string(),
            synced_base_url: "http://127.0.0.1:17688".to_string(),
            synced_at: 1,
        });
        super::storage::write_config(&config).unwrap();

        let after = super::commands::api_fusion_delete_provider("p1".to_string()).unwrap();
        assert!(after.providers.is_empty());
        assert!(after.terminal_syncs.is_empty());
    });
}

#[test]
fn every_command_is_registered_in_the_invoke_handler() {
    const RUN_APP_SOURCE: &str = include_str!("../app_runtime/run_app.rs");
    const LIB_SOURCE: &str = include_str!("../lib.rs");

    assert!(LIB_SOURCE.contains("mod api_fusion;"));
    let commands = [
        "api_fusion_get_config",
        "api_fusion_save_config",
        "api_fusion_upsert_provider",
        "api_fusion_delete_provider",
        "api_fusion_set_provider_enabled",
        "api_fusion_reenable_provider",
        "api_fusion_upsert_key",
        "api_fusion_delete_key",
        "api_fusion_set_default_key",
        "api_fusion_start",
        "api_fusion_stop",
        "api_fusion_status",
        "api_fusion_terminal_targets",
        "api_fusion_configure_terminal",
        "api_fusion_sync_terminal",
    ];
    for command in commands {
        let registration = format!("api_fusion::{command},");
        assert_eq!(
            RUN_APP_SOURCE.matches(&registration).count(),
            1,
            "command {command} must be registered exactly once in generate_handler!"
        );
    }
}

#[tokio::test]
async fn no_candidate_model_returns_all_unavailable_without_upstream_request() {
    let home = temp_home("no-candidate");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "should-not-run"}))).await;

    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut p = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    p.mappings = vec![ModelMapping {
        local_model: "known-local".to_string(),
        upstream_model: "remote-a".to_string(),
        protocol: None,
    }];
    config.providers.push(p);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "unknown-local"})),
    )
    .await;
    assert_eq!(status, 502, "unexpected response: {text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    assert!(
        log.lock().unwrap().is_empty(),
        "no upstream request may be issued when there is no candidate"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn non_json_upstream_response_is_retryable_and_switches() {
    let _home = temp_home("non-json-retry");
    let (non_json_url, non_json_log) =
        spawn_mock_upstream(|_| MockReply::Stream("this is not json".to_string())).await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = FusionConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &non_json_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk", Some("remote-default"));
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
    )
    .await;
    assert_eq!(response.status, 200);
    assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
    assert_eq!(non_json_log.lock().unwrap().len(), 1);
    assert_eq!(ok_log.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn return_to_client_error_is_passed_through_without_switching_or_disabling() {
    let _home = temp_home("return-to-client");
    let (bad_request_url, bad_request_log) =
        spawn_mock_upstream(|_| MockReply::Json(400, json!({"error": {"message": "bad request"}})))
            .await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = FusionConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &bad_request_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk", Some("remote-default"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
    )
    .await;
    assert_eq!(response.status, 400, "upstream client error must pass through");
    assert_eq!(bad_request_log.lock().unwrap().len(), 1);
    assert!(ok_log.lock().unwrap().is_empty(), "must not switch on 4xx");
    let stored = config
        .providers
        .iter()
        .find(|provider| provider.id == "a")
        .unwrap();
    assert!(!stored.auto_disabled);
    assert_eq!(stored.consecutive_failures, 0);
}

// ---------------------------------------------------------------------------
// Step 5: cross-module end-to-end behavior (task-003)
//
// Each test below drives the real local listener or the real forwarding path
// against an in-process mock upstream; no real network or user configuration.
// ---------------------------------------------------------------------------

async fn closed_port_base_url() -> String {
    let port = free_port().await;
    format!("http://127.0.0.1:{port}")
}

fn config_with_key(port: u16) -> FusionConfig {
    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config
}

#[tokio::test]
async fn end_to_end_random_pool_selects_every_resolvable_candidate() {
    // AC-009: two providers resolve the same model; the exact runtime selection
    // (`candidate_providers` + `shuffled_candidates`) must pick both over time,
    // and the chosen provider is reached over real HTTP.
    let (url_a, _log_a) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-a"}))).await;
    let (url_b, _log_b) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let providers = vec![
        upstream_provider("a", "Provider A", &url_a, "sk-a", Some("remote-model")),
        upstream_provider("b", "Provider B", &url_b, "sk-b", Some("remote-model")),
    ];
    let candidates: Vec<FusionUpstreamProvider> =
        candidate_providers(&providers, Some("local-model"), UpstreamProtocol::ChatCompletions)
            .into_iter()
            .cloned()
            .collect();
    assert_eq!(candidates.len(), 2, "both providers can serve the request model");

    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();
    let mut seen_a = 0usize;
    let mut seen_b = 0usize;
    for _ in 0..64 {
        let ordered = super::selection::shuffled_candidates(&candidates);
        let chosen = ordered.first().expect("at least one candidate");
        let response = super::forwarding::forward_non_streaming(
            chosen,
            "/v1/chat/completions",
            &body,
            "remote-model",
            &HashMap::new(),
        )
        .await
        .expect("forward to mock upstream");
        assert_eq!(response.status, 200);
        match chosen.id.as_str() {
            "a" => seen_a += 1,
            "b" => seen_b += 1,
            other => panic!("unexpected provider selected: {other}"),
        }
    }
    assert!(
        seen_a > 0 && seen_b > 0,
        "both candidates must be selected over time: a={seen_a}, b={seen_b}"
    );
}

#[tokio::test]
async fn end_to_end_network_failure_falls_back_and_tries_first_candidate_once() {
    // AC-010: first candidate network error -> caller gets the second provider's
    // success response and the first candidate is attempted only once.
    let _home = temp_home("e2e-failover-network");
    let (drop_url, drop_log) = spawn_mock_upstream(|_| MockReply::Drop).await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &drop_url, "sk-a", Some("remote-model"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local-model"),
        &mut config,
        &HashMap::new(),
    )
    .await;

    assert_eq!(response.status, 200, "network error must fall back");
    assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
    assert_eq!(
        drop_log.lock().unwrap().len(),
        1,
        "first candidate must be attempted exactly once"
    );
    assert_eq!(ok_log.lock().unwrap().len(), 1, "fallback must use the second candidate");
}

#[tokio::test]
async fn end_to_end_5xx_falls_back_and_tries_first_candidate_once() {
    // AC-010: first candidate 5xx -> second provider succeeds; first is not retried.
    let _home = temp_home("e2e-failover-5xx");
    let (fail_url, fail_log) =
        spawn_mock_upstream(|_| MockReply::Json(503, json!({"error": {"message": "down"}}))).await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &fail_url, "sk-a", Some("remote-model"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local-model"),
        &mut config,
        &HashMap::new(),
    )
    .await;

    assert_eq!(response.status, 200, "5xx must fall back");
    assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
    assert_eq!(fail_log.lock().unwrap().len(), 1);
    assert_eq!(ok_log.lock().unwrap().len(), 1);

    let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
    assert_eq!(a_stored.consecutive_failures, 1);
    assert!(!a_stored.auto_disabled, "a single 5xx must not disable the provider");
}

#[tokio::test]
async fn end_to_end_auth_failures_disable_immediately_and_switch() {
    // AC-011: 401/403 disable the provider right away, record the reason, and the
    // request continues on the next candidate.
    for status in [401u16, 403u16] {
        let home = temp_home(&format!("e2e-auth-{status}"));
        let (auth_url, auth_log) = spawn_mock_upstream(move |_| {
            MockReply::Json(status, json!({"error": {"message": "denied"}}))
        })
        .await;
        let (ok_url, ok_log) =
            spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

        let mut config = config_with_key(0);
        let a = upstream_provider("a", "Provider A", &auth_url, "sk-a", Some("remote-model"));
        let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
        config.providers.push(a.clone());
        config.providers.push(b.clone());
        let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

        let response = super::runtime_http::attempt_non_streaming(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
        )
        .await;

        assert_eq!(response.status, 200, "status {status} must fall back");
        assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
        assert_eq!(auth_log.lock().unwrap().len(), 1);
        assert_eq!(ok_log.lock().unwrap().len(), 1);

        let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
        assert!(a_stored.auto_disabled, "status {status} must auto-disable immediately");
        assert!(
            a_stored
                .disabled_reason
                .as_deref()
                .unwrap_or("")
                .contains(&status.to_string()),
            "reason must record the status: {:?}",
            a_stored.disabled_reason
        );
        assert!(a_stored.disabled_at.is_some());
        let b_stored = config.providers.iter().find(|p| p.id == "b").unwrap();
        assert!(!b_stored.auto_disabled);
        drop(home);
    }
}

#[tokio::test]
async fn end_to_end_retryable_failures_auto_disable_at_threshold_and_stop_calling() {
    // AC-011: three consecutive 500s auto-disable the provider; once disabled it
    // is no longer contacted and the caller keeps receiving all-unavailable.
    let home = temp_home("e2e-threshold");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;

    let mut config = config_with_key(port);
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &upstream_url,
        "sk",
        Some("remote-model"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    for attempt in 1..=3 {
        let (status, _, text) = call_fusion(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(json!({"model": "local-model"})),
        )
        .await;
        assert_eq!(status, 502, "attempt {attempt} unexpected: {text}");
    }

    let stored = super::storage::read_config().unwrap();
    let a_stored = stored.providers.iter().find(|p| p.id == "a").unwrap();
    assert!(a_stored.auto_disabled, "third consecutive failure must auto-disable");
    assert_eq!(a_stored.consecutive_failures, 3);

    let (status, _, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-model"})),
    )
    .await;
    assert_eq!(status, 502);
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    assert_eq!(
        log.lock().unwrap().len(),
        3,
        "auto-disabled provider must not be contacted again"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn end_to_end_below_threshold_provider_stays_enabled() {
    // AC-011 negative: two consecutive failures are below the threshold of three.
    let home = temp_home("e2e-below-threshold");
    let port = free_port().await;
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;

    let mut config = config_with_key(port);
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &upstream_url,
        "sk",
        Some("remote-model"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    for _ in 0..2 {
        let (status, _, _) = call_fusion(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(json!({"model": "local-model"})),
        )
        .await;
        assert_eq!(status, 502);
    }

    let stored = super::storage::read_config().unwrap();
    let a_stored = stored.providers.iter().find(|p| p.id == "a").unwrap();
    assert!(
        !a_stored.auto_disabled,
        "below the threshold the provider must stay enabled"
    );
    assert_eq!(a_stored.consecutive_failures, 2);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn end_to_end_network_errors_accumulate_and_disable() {
    // AC-011: network errors count as retryable failures and reach the threshold.
    let home = temp_home("e2e-network-threshold");
    let port = free_port().await;
    let dead_url = closed_port_base_url().await;

    let mut config = config_with_key(port);
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &dead_url,
        "sk",
        Some("remote-model"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    for _ in 0..3 {
        let (status, _, _) = call_fusion(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(json!({"model": "local-model"})),
        )
        .await;
        assert_eq!(status, 502);
    }

    let stored = super::storage::read_config().unwrap();
    let a_stored = stored.providers.iter().find(|p| p.id == "a").unwrap();
    assert!(a_stored.auto_disabled);
    assert_eq!(a_stored.consecutive_failures, 3);
    assert!(
        a_stored
            .disabled_reason
            .as_deref()
            .unwrap_or("")
            .contains("network error"),
        "reason must describe the network failure: {:?}",
        a_stored.disabled_reason
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn end_to_end_transient_429_and_404_switch_without_disabling() {
    // AC-011: 429/404 switch to the next candidate but never count as failures.
    for status in [429u16, 404u16] {
        let home = temp_home(&format!("e2e-transient-{status}"));
        let (transient_url, transient_log) = spawn_mock_upstream(move |_| {
            MockReply::Json(status, json!({"error": {"message": "transient"}}))
        })
        .await;
        let (ok_url, ok_log) =
            spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

        let mut config = config_with_key(0);
        let a = upstream_provider("a", "Provider A", &transient_url, "sk-a", Some("remote-model"));
        let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
        config.providers.push(a.clone());
        config.providers.push(b.clone());
        let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

        let response = super::runtime_http::attempt_non_streaming(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
        )
        .await;

        assert_eq!(response.status, 200, "status {status} must switch");
        assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
        assert_eq!(transient_log.lock().unwrap().len(), 1);
        assert_eq!(ok_log.lock().unwrap().len(), 1);

        let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
        assert!(!a_stored.auto_disabled, "status {status} must not disable");
        assert_eq!(a_stored.consecutive_failures, 0, "status {status} must not count");
        drop(home);
    }
}

#[tokio::test]
async fn end_to_end_client_4xx_returns_to_caller_without_switching_or_disabling() {
    // AC-011 negative: 400/422 and other unlisted 4xx are the caller's problem,
    // so they are returned directly and no provider is disabled.
    for status in [400u16, 422u16, 418u16] {
        let home = temp_home(&format!("e2e-client-{status}"));
        let (bad_url, bad_log) = spawn_mock_upstream(move |_| {
            MockReply::Json(status, json!({"error": {"message": "bad request"}}))
        })
        .await;
        let (ok_url, ok_log) =
            spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

        let mut config = config_with_key(0);
        let a = upstream_provider("a", "Provider A", &bad_url, "sk-a", Some("remote-model"));
        let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
        config.providers.push(a.clone());
        config.providers.push(b.clone());
        let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

        let response = super::runtime_http::attempt_non_streaming(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
        )
        .await;

        assert_eq!(response.status, status, "status {status} must pass through");
        assert_eq!(bad_log.lock().unwrap().len(), 1);
        assert!(ok_log.lock().unwrap().is_empty(), "status {status} must not switch");

        let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
        assert!(!a_stored.auto_disabled, "status {status} must not disable");
        assert_eq!(a_stored.consecutive_failures, 0, "status {status} must not count");
        drop(home);
    }
}

#[tokio::test]
async fn end_to_end_non_json_response_is_a_counted_failure_not_success() {
    // AC-012: a non-JSON body (including a 2xx status) is not a success; it
    // switches and counts toward the consecutive-failure threshold.
    let home = temp_home("e2e-non-json-2xx");
    let (bad_url, bad_log) = spawn_mock_upstream(|_| {
        MockReply::Raw(200, "text/plain", b"this is not json".to_vec())
    })
    .await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &bad_url, "sk-a", Some("remote-model"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    for attempt in 1..=3u32 {
        let response = super::runtime_http::attempt_non_streaming(
            &[a.clone(), b.clone()],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
        )
        .await;
        assert_eq!(response.status, 200, "attempt {attempt}");
        assert!(
            String::from_utf8_lossy(&response.body).contains("from-b"),
            "a non-JSON 2xx must not reach the caller as success"
        );
        let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
        assert_eq!(
            a_stored.consecutive_failures, attempt,
            "non-JSON must count as a failure"
        );
        assert_eq!(a_stored.auto_disabled, attempt >= 3);
    }
    assert_eq!(bad_log.lock().unwrap().len(), 3);
    assert_eq!(ok_log.lock().unwrap().len(), 3);
    drop(home);

    // A non-JSON body on a 5xx status is likewise a counted failure.
    let home = temp_home("e2e-non-json-500");
    let (bad_url, _bad_log) = spawn_mock_upstream(|_| {
        MockReply::Raw(500, "text/html", b"<html>upstream error</html>".to_vec())
    })
    .await;
    let (ok_url, _ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &bad_url, "sk-a", Some("remote-model"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());

    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local-model"),
        &mut config,
        &HashMap::new(),
    )
    .await;
    assert_eq!(response.status, 200);
    assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
    let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
    assert_eq!(a_stored.consecutive_failures, 1);
    assert!(!a_stored.auto_disabled);
    drop(home);
}

#[tokio::test]
async fn end_to_end_all_unavailable_non_streaming_lists_each_provider_failure() {
    // AC-014: non-streaming all-unavailable is HTTP 502 with code
    // all_providers_unavailable and a per-provider failure summary.
    let home = temp_home("e2e-all-unavailable-summary");
    let port = free_port().await;
    let (url_a, _) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "a-down"}}))).await;
    let (url_b, _) =
        spawn_mock_upstream(|_| MockReply::Json(503, json!({"error": {"message": "b-down"}}))).await;

    let mut config = config_with_key(port);
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &url_a,
        "sk-a",
        Some("remote-model"),
    ));
    config.providers.push(upstream_provider(
        "b",
        "Provider B",
        &url_b,
        "sk-b",
        Some("remote-model"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-model"})),
    )
    .await;
    assert_eq!(status, 502, "unexpected response: {text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("Provider A"), "message: {message}");
    assert!(message.contains("Provider B"), "message: {message}");
    assert!(message.contains("HTTP 500"), "message: {message}");
    assert!(message.contains("HTTP 503"), "message: {message}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn end_to_end_all_unavailable_streaming_error_event_precedes_done() {
    // AC-015: streaming all-unavailable is HTTP 200 SSE whose error object event
    // comes before the terminating `data: [DONE]`.
    let home = temp_home("e2e-all-unavailable-stream");
    let port = free_port().await;
    let (url_a, _) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "a-down"}}))).await;
    let (url_b, _) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "b-down"}}))).await;

    let mut config = config_with_key(port);
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &url_a,
        "sk-a",
        Some("remote-model"),
    ));
    config.providers.push(upstream_provider(
        "b",
        "Provider B",
        &url_b,
        "sk-b",
        Some("remote-model"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-model", "stream": true})),
    )
    .await;
    assert_eq!(status, 200, "streaming stop response uses HTTP 200");
    assert!(
        content_type.contains("text/event-stream"),
        "content-type: {content_type}"
    );
    let error_index = text
        .find("all_providers_unavailable")
        .unwrap_or_else(|| panic!("missing error payload: {text}"));
    let done_index = text
        .find("data: [DONE]")
        .unwrap_or_else(|| panic!("missing [DONE]: {text}"));
    assert!(
        error_index < done_index,
        "error event must precede [DONE]: {text}"
    );

    let first_event = text.split("\n\n").next().unwrap();
    let payload = first_event
        .strip_prefix("data: ")
        .unwrap_or_else(|| panic!("first event must be a data event: {text}"));
    let value: Value = serde_json::from_str(payload).expect("error event must be JSON");
    assert_eq!(value["error"]["code"], "all_providers_unavailable");
    let message = value["error"]["message"].as_str().unwrap();
    assert!(message.contains("Provider A"), "message: {message}");
    assert!(message.contains("Provider B"), "message: {message}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn end_to_end_path_prefix_and_body_equivalence_for_chat_and_responses() {
    // AC-007/AC-008: upstream path is the provider base URL (including its own
    // path prefix) joined verbatim with the request path; auth is the provider
    // key and only `model` changes in the body. Each protocol has its own
    // provider because routing is configured per provider.
    let home = temp_home("e2e-path-prefix");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;
    let base_url = format!("{upstream_url}/custom/prefix");

    let mut config = config_with_key(port);
    let mapping = ModelMapping {
        local_model: "local-a".to_string(),
        upstream_model: "remote-a".to_string(),
        protocol: None,
    };
    let mut chat_provider =
        upstream_provider("p1", "Provider One", &base_url, "upstream-secret", None);
    chat_provider.mappings = vec![mapping.clone()];
    config.providers.push(chat_provider);
    let mut responses_provider =
        upstream_provider("p2", "Provider Two", &base_url, "upstream-secret", None);
    responses_provider.protocol = UpstreamProtocol::Responses;
    responses_provider.mappings = vec![mapping];
    config.providers.push(responses_provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let chat_body = json!({
        "model": "local-a",
        "messages": [{"role": "user", "content": "hello"}],
        "temperature": 0.5,
        "stream": false
    });
    let (status, _, text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(chat_body.clone()),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let (status, _, text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "input": "hi"})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let captured = log.lock().unwrap().clone();
    assert_eq!(captured.len(), 2);
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].path, "/custom/prefix/v1/chat/completions");
    assert_eq!(
        captured[0].headers.get("authorization").map(String::as_str),
        Some("Bearer upstream-secret")
    );
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(sent["model"], "remote-a");
    assert_eq!(sent["messages"], chat_body["messages"]);
    assert_eq!(sent["temperature"], chat_body["temperature"]);
    assert_eq!(sent["stream"], chat_body["stream"]);

    assert_eq!(captured[1].method, "POST");
    assert_eq!(captured[1].path, "/custom/prefix/v1/responses");
    assert_eq!(
        captured[1].headers.get("authorization").map(String::as_str),
        Some("Bearer upstream-secret")
    );
    let sent: Value = serde_json::from_slice(&captured[1].body).unwrap();
    assert_eq!(sent["model"], "remote-a");
    assert_eq!(sent["input"], "hi");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn end_to_end_models_union_and_unknown_route_error_shape() {
    // AC-008: GET /v1/models returns only the union of models from enabled,
    // non-auto-disabled providers and never contacts upstream; unknown routes
    // return a standard OpenAI 404 error body.
    let home = temp_home("e2e-models-404");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "should-not-be-called"}))).await;

    let mut config = config_with_key(port);
    let mut active = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    active.mappings = vec![
        ModelMapping {
            local_model: "local-b".to_string(),
            upstream_model: "remote-b".to_string(),
            protocol: None,
        },
        ModelMapping {
            local_model: "local-a".to_string(),
            upstream_model: "remote-a".to_string(),
            protocol: None,
        },
    ];
    let mut disabled = upstream_provider("p2", "Provider Two", &upstream_url, "sk", None);
    disabled.enabled = false;
    disabled.mappings = vec![ModelMapping {
        local_model: "local-disabled".to_string(),
        upstream_model: "x".to_string(),
        protocol: None,
    }];
    let mut auto_disabled = upstream_provider("p3", "Provider Three", &upstream_url, "sk", None);
    auto_disabled.auto_disabled = true;
    auto_disabled.mappings = vec![ModelMapping {
        local_model: "local-auto".to_string(),
        upstream_model: "x".to_string(),
        protocol: None,
    }];
    let no_model = upstream_provider("p4", "Provider Four", &upstream_url, "sk", None);
    config
        .providers
        .extend([active, disabled, auto_disabled, no_model]);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, text) = call_fusion(
        port,
        "GET",
        "/v1/models",
        &[("authorization", "Bearer local-key")],
        None,
    )
    .await;
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&text).unwrap();
    let mut ids: Vec<String> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["local-a".to_string(), "local-b".to_string()]);
    assert!(
        log.lock().unwrap().is_empty(),
        "GET /v1/models must not contact upstream"
    );

    let (status, _, text) = call_fusion(
        port,
        "POST",
        "/v1/embeddings",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 404);
    let body: Value = serde_json::from_str(&text).unwrap();
    assert!(
        body["error"]["message"].is_string() && body["error"]["type"].is_string(),
        "404 must carry a standard OpenAI error body: {text}"
    );

    let (status, _, _) = call_fusion(
        port,
        "GET",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        None,
    )
    .await;
    assert_eq!(status, 404);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// Step 6: regression tests for dual-axis review findings (A/C/D/E)
//
// These drive the Tauri command functions and the real forwarding / streaming
// path against in-process mock upstreams, using the shared temp-home isolation
// and free-port helpers defined above.
// ---------------------------------------------------------------------------

/// Finding A (error): `api_fusion_start` / `api_fusion_stop` must persist
/// `FusionConfig.enabled` so the enable intent survives a reload (AC-003,
/// AC-019). The test reads the flag back from disk after each command.
#[tokio::test]
async fn api_fusion_start_and_stop_persist_enabled_flag() {
    let _home = temp_home("enabled-persist");
    let port = free_port().await;
    let mut config = config_with_key(port);
    config.enabled = false;
    super::storage::write_config(&config).unwrap();

    let started = super::commands::api_fusion_start().await.unwrap();
    assert!(started.running, "start must report a running server");
    let enabled_after_start = super::storage::read_config().unwrap().enabled;

    let stopped = super::commands::api_fusion_stop().await.unwrap();
    assert!(!stopped.running, "stop must report a stopped server");
    let enabled_after_stop = super::storage::read_config().unwrap().enabled;

    assert!(
        enabled_after_start,
        "api_fusion_start must persist enabled=true (reloaded {enabled_after_start})"
    );
    assert!(
        !enabled_after_stop,
        "api_fusion_stop must persist enabled=false (reloaded {enabled_after_stop})"
    );
}

/// Finding C (low): a blackholed upstream connect must be classified as a
/// retryable failure and switch to the next candidate instead of hanging or
/// being treated as success (AC-010, AC-011).
///
/// TEST-NET-1 (`192.0.2.0/24`) is reserved and non-routable, so the SYN is
/// dropped and the connect blackholes until a connect/request timeout is
/// enforced. The 15s bound below encodes "must not hang"; it is generous enough
/// for a conventional connect timeout (the repo's `proxy.rs` uses 10s).
#[tokio::test]
async fn blackhole_connection_timeout_is_retryable_and_switches_within_bound() {
    let _home = temp_home("connect-timeout-retry");
    let blackhole_url = "http://192.0.2.1:81";
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", blackhole_url, "sk-a", Some("remote-model"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        super::runtime_http::attempt_non_streaming(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
        ),
    )
    .await
    .expect("a blackhole connect must not hang; it must time out and be retryable");

    assert_eq!(response.status, 200, "a timed-out candidate must switch");
    assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
    assert_eq!(
        ok_log.lock().unwrap().len(),
        1,
        "fallback must use the second candidate"
    );
    let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
    assert_eq!(
        a_stored.consecutive_failures, 1,
        "a connection timeout must count as a failure"
    );
    assert!(!a_stored.auto_disabled, "one timeout must not auto-disable");
}

/// Bind a loopback listener that accepts connections, reads the request and
/// then holds the socket open without ever sending a response.
async fn spawn_unresponsive_upstream() -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind unresponsive upstream");
    let addr = listener.local_addr().expect("unresponsive addr");
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tauri::async_runtime::spawn(async move {
                let _ = super::runtime_http::read_http_request(&mut stream).await;
                tokio::time::sleep(std::time::Duration::from_secs(120)).await;
            });
        }
    });
    format!("http://{}", addr)
}

/// Finding C (low), deterministic path: an upstream that accepts the TCP
/// connection but never answers must not hang the forwarding request. The
/// enforced timeout is a retryable failure that switches to the next candidate
/// and counts toward the failure threshold (AC-010, AC-011).
#[tokio::test]
async fn unresponsive_upstream_is_a_retryable_timeout_not_a_hang() {
    let _home = temp_home("unresponsive-timeout");
    let hang_url = spawn_unresponsive_upstream().await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &hang_url, "sk-a", Some("remote-model"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        super::runtime_http::attempt_non_streaming(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
        ),
    )
    .await
    .expect("an unresponsive upstream must not hang; a timeout must be enforced");

    assert_eq!(response.status, 200, "a timed-out candidate must switch");
    assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
    assert_eq!(ok_log.lock().unwrap().len(), 1);
    let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
    assert_eq!(
        a_stored.consecutive_failures, 1,
        "a response timeout must count as a failure"
    );
    assert!(!a_stored.auto_disabled, "one timeout must not auto-disable");
}

/// Finding D (low): for a streaming request, a 2xx upstream body that is neither
/// JSON nor an SSE fragment must not be passed through as success. It must be a
/// counted retryable failure and the next candidate must serve the complete
/// stream (AC-012, AC-016).
#[tokio::test]
async fn streaming_2xx_non_json_is_retryable_and_switches_before_first_byte() {
    let _home = temp_home("stream-non-json-2xx");
    let (bad_url, bad_log) = spawn_mock_upstream(|_| {
        MockReply::Raw(200, "text/plain", b"this is not json".to_vec())
    })
    .await;
    let stream_body =
        "data: {\"choices\":[{\"delta\":{\"content\":\"from-b\"}}]}\n\ndata: [DONE]\n\n";
    let (stream_url, stream_log) =
        spawn_mock_upstream(move |_| MockReply::Stream(stream_body.to_string())).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &bad_url, "sk-a", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &stream_url, "sk-b", Some("remote-default"));
    // The runtime only records failures for candidates registered in the config,
    // so the fixture must mirror the listeners it is about to drive.
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();

    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    super::runtime_http::attempt_streaming(
        &mut server,
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
    )
    .await
    .unwrap();
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out);

    assert!(
        !text.contains("this is not json"),
        "a 2xx non-JSON body must not be written to the caller: {text}"
    );
    assert!(
        text.contains("from-b"),
        "the second candidate must serve the complete stream: {text}"
    );
    assert!(
        text.contains("data: [DONE]"),
        "the stream must terminate: {text}"
    );
    assert_eq!(bad_log.lock().unwrap().len(), 1);
    assert_eq!(stream_log.lock().unwrap().len(), 1);
    let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
    assert_eq!(
        a_stored.consecutive_failures, 1,
        "a 2xx non-JSON stream must count as a failure"
    );
    assert!(!a_stored.auto_disabled);
}

/// Finding E (low): the local service listens on `127.0.0.1` only, so the same
/// port on the machine's primary non-loopback IPv4 must refuse connections
/// (AC-003). The primary address is discovered with a UDP "connect" that only
/// selects the outbound route and sends no packets.
#[tokio::test]
async fn loopback_listener_rejects_non_loopback_address_on_same_port() {
    let _home = temp_home("non-loopback-refused");
    let port = free_port().await;
    let config = config_with_key(port);
    super::storage::write_config(&config).unwrap();
    super::commands::api_fusion_start().await.unwrap();

    let loopback_ok = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .is_ok();

    let primary_ip = std::net::UdpSocket::bind("0.0.0.0:0")
        .ok()
        .and_then(|socket| {
            socket.connect("8.8.8.8:80").ok()?;
            socket.local_addr().ok()
        })
        .map(|addr| addr.ip())
        .filter(|ip| !ip.is_loopback());

    let non_loopback_refused = match primary_ip {
        Some(ip) => tokio::net::TcpStream::connect((ip, port)).await.is_err(),
        None => true,
    };

    super::commands::api_fusion_stop().await.unwrap();

    assert!(loopback_ok, "loopback listener must accept connections");
    if let Some(ip) = primary_ip {
        assert!(
            non_loopback_refused,
            "loopback-only listener must refuse {ip}:{port}, but the connection succeeded"
        );
    } else {
        eprintln!(
            "environment limitation: no non-loopback IPv4 available; \
             loopback-only listener assertion was not exercised"
        );
    }
}

// ---------------------------------------------------------------------------
// Step 7: terminal one-click configure / sync through the injectable upsert
// seam (Finding B)
//
// `apply_terminal_sync_with` accepts an injected upsert so the external
// app_store boundary is a fake here; everything else (planning, merge, ledger)
// runs as production code against an isolated config directory.
// ---------------------------------------------------------------------------

fn terminal_providers_payload() -> Value {
    json!({
        "providers": [
            {
                "id": "oc-1",
                "tool": "opencode",
                "name": "My OpenCode",
                "model": "gpt-x",
                "icon": "star",
                "is_enabled": false,
                "base_url": "https://old-opencode.example.com",
                "api_key": "old-key-1",
                "tool_config": { "keep": true }
            },
            {
                "id": "cx-1",
                "tool": "codex",
                "name": "My Codex",
                "model": "gpt-y",
                "icon": "bolt",
                "is_enabled": true,
                "base_url": "https://old-codex.example.com",
                "api_key": "old-key-2",
                "tool_config": { "nested": { "keep": 42 } }
            },
            {
                "id": "oc-2",
                "tool": "opencode",
                "name": "Unselected",
                "model": "gpt-z",
                "base_url": "https://old-unselected.example.com",
                "api_key": "old-key-3"
            }
        ]
    })
}

/// Finding B: one-click configure/sync merges the local base URL and default key
/// into every selected target through the injectable upsert, preserves all other
/// fields exactly, and never touches unselected targets (AC-017).
#[tokio::test]
async fn terminal_sync_with_seam_merges_selected_targets_and_preserves_fields() {
    let _home = temp_home("terminal-sync-seam-merge");
    let mut config = FusionConfig::default();
    config.port = 17688;
    config.keys.push(key_named("k1", "local-key-123"));
    super::storage::write_config(&config).unwrap();
    let local_base_url = super::storage::local_base_url(config.port);

    let captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    let records = super::commands::apply_terminal_sync_with(
        &terminal_providers_payload(),
        move |value| -> super::commands::UpsertFuture {
            let sink = sink.clone();
            Box::pin(async move {
                sink.lock().expect("capture lock").push(value);
                Ok(())
            })
        },
        vec!["oc-1".to_string(), "cx-1".to_string()],
    )
    .await
    .expect("terminal sync must succeed");

    let submitted = captured.lock().unwrap().clone();
    assert_eq!(
        submitted.len(),
        2,
        "upsert calls must match target_ids: {submitted:?}"
    );
    assert!(
        submitted.iter().all(|value| value["id"] != "oc-2"),
        "an unselected target must not be upserted: {submitted:?}"
    );

    let by_id = |id: &str| {
        submitted
            .iter()
            .find(|value| value["id"] == id)
            .unwrap_or_else(|| panic!("missing submitted record for {id}"))
    };

    let opencode = by_id("oc-1");
    assert_eq!(opencode["base_url"], local_base_url.as_str());
    assert_eq!(opencode["api_key"], "local-key-123");
    assert_eq!(opencode["name"], "My OpenCode");
    assert_eq!(opencode["model"], "gpt-x");
    assert_eq!(opencode["icon"], "star");
    assert_eq!(opencode["is_enabled"], false);
    assert_eq!(opencode["tool_config"]["keep"], true);

    let codex = by_id("cx-1");
    assert_eq!(codex["base_url"], local_base_url.as_str());
    assert_eq!(codex["api_key"], "local-key-123");
    assert_eq!(codex["name"], "My Codex");
    assert_eq!(codex["model"], "gpt-y");
    assert_eq!(codex["icon"], "bolt");
    assert_eq!(codex["is_enabled"], true);
    assert_eq!(codex["tool_config"]["nested"]["keep"], 42);

    assert_eq!(records.len(), 2);
    for record in &records {
        assert_eq!(record.synced_key_id, "k1");
        assert_eq!(record.synced_base_url, local_base_url);
    }
    let mut tools: Vec<&str> = records.iter().map(|record| record.tool.as_str()).collect();
    tools.sort();
    assert_eq!(tools, vec!["codex", "opencode"]);
}

/// Finding B: a successful sync refreshes the persisted ledger to this run's
/// default key id and local base URL, and re-running it for the same providers
/// replaces the entries instead of accumulating duplicates (AC-018).
#[tokio::test]
async fn terminal_sync_with_seam_refreshes_ledger_without_duplicates() {
    let _home = temp_home("terminal-sync-seam-ledger");
    let mut config = FusionConfig::default();
    config.port = 17688;
    config.keys.push(key_named("k1", "local-key-123"));
    super::storage::write_config(&config).unwrap();
    let local_base_url = super::storage::local_base_url(config.port);

    let ids = vec!["oc-1".to_string(), "cx-1".to_string()];
    let payload = terminal_providers_payload();
    let first = super::commands::apply_terminal_sync_with(
        &payload,
        |_value| -> super::commands::UpsertFuture { Box::pin(async move { Ok(()) }) },
        ids.clone(),
    )
    .await
    .expect("first sync must succeed");
    assert_eq!(first.len(), 2);

    let reloaded = super::storage::read_config().unwrap();
    assert_eq!(
        reloaded.terminal_syncs.len(),
        2,
        "one ledger entry per synced provider: {:?}",
        reloaded.terminal_syncs
    );
    let mut ledger_ids: Vec<String> = reloaded
        .terminal_syncs
        .iter()
        .map(|record| record.provider_id.clone())
        .collect();
    ledger_ids.sort();
    assert_eq!(ledger_ids, vec!["cx-1".to_string(), "oc-1".to_string()]);
    for record in &reloaded.terminal_syncs {
        assert_eq!(record.synced_key_id, "k1");
        assert_eq!(record.synced_base_url, local_base_url);
    }
    let mut expected: Vec<(&str, &str, &str)> = first
        .iter()
        .map(|record| {
            (
                record.provider_id.as_str(),
                record.synced_key_id.as_str(),
                record.synced_base_url.as_str(),
            )
        })
        .collect();
    expected.sort();
    let mut persisted: Vec<(&str, &str, &str)> = reloaded
        .terminal_syncs
        .iter()
        .map(|record| {
            (
                record.provider_id.as_str(),
                record.synced_key_id.as_str(),
                record.synced_base_url.as_str(),
            )
        })
        .collect();
    persisted.sort();
    assert_eq!(persisted, expected, "returned records must match the ledger");

    let second = super::commands::apply_terminal_sync_with(
        &payload,
        |_value| -> super::commands::UpsertFuture { Box::pin(async move { Ok(()) }) },
        ids,
    )
    .await
    .expect("second sync must succeed");
    assert_eq!(second.len(), 2);

    let reloaded = super::storage::read_config().unwrap();
    assert_eq!(
        reloaded.terminal_syncs.len(),
        2,
        "re-syncing the same provider must replace, not duplicate, ledger entries: {:?}",
        reloaded.terminal_syncs
    );
}

/// Finding B atomicity: when the injected upsert fails, the pipeline returns the
/// error and the persisted ledger keeps its previous value, so the ledger never
/// claims a sync that did not happen (AC-017/AC-018).
#[tokio::test]
async fn terminal_sync_with_seam_aborts_without_writing_ledger_on_upsert_error() {
    let _home = temp_home("terminal-sync-seam-error");
    let mut config = FusionConfig::default();
    config.port = 17688;
    config.keys.push(key_named("k1", "local-key-123"));
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "existing".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k-old".to_string(),
        synced_base_url: "http://127.0.0.1:1".to_string(),
        synced_at: 111,
    });
    super::storage::write_config(&config).unwrap();
    let before = super::storage::read_config().unwrap().terminal_syncs;

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_upsert = calls.clone();
    let error = super::commands::apply_terminal_sync_with(
        &terminal_providers_payload(),
        move |_value| -> super::commands::UpsertFuture {
            let calls = calls_for_upsert.clone();
            Box::pin(async move {
                let should_fail = {
                    let mut count = calls.lock().expect("call counter");
                    *count += 1;
                    *count >= 2
                };
                if should_fail {
                    return Err("injected upsert failure".to_string());
                }
                Ok(())
            })
        },
        vec!["oc-1".to_string(), "cx-1".to_string()],
    )
    .await
    .unwrap_err();

    assert!(
        error.contains("injected upsert failure"),
        "error must surface the upsert failure: {error}"
    );
    assert_eq!(
        *calls.lock().unwrap(),
        2,
        "every selected target up to the failure is attempted"
    );

    let after = super::storage::read_config().unwrap().terminal_syncs;
    assert_eq!(
        after, before,
        "ledger must be unchanged when an upsert fails"
    );
}

// ---------------------------------------------------------------------------
// Plan 20260916-api-fusion-per-model-endpoint, Step 1 (RED)
//
// A mapping row may declare the endpoint protocol it belongs to
// (`chat_completions` / `responses`); an absent or null declaration inherits the
// provider protocol. Configurations that declare a mapping protocol are built
// from JSON on purpose: today the field is unknown to serde and silently
// dropped, so the assertions fail on behavior (the relay routes/loses the
// declaration) instead of failing the whole crate at compile time. The persisted
// file is encrypted (`v2:`), so "persisted" is observed through the serialized
// view of `read_config`.
// ---------------------------------------------------------------------------

/// One mapping row as submitted/persisted by the UI. `None` means the field is
/// absent, i.e. "inherit the provider protocol".
fn json_mapping(local: &str, upstream: &str, protocol: Option<&str>) -> Value {
    match protocol {
        Some(protocol) => json!({
            "local_model": local,
            "upstream_model": upstream,
            "protocol": protocol,
        }),
        None => json!({
            "local_model": local,
            "upstream_model": upstream,
        }),
    }
}

fn json_provider(
    id: &str,
    name: &str,
    base_url: &str,
    protocol: &str,
    default_model: Option<&str>,
    mappings: Vec<Value>,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "base_url": base_url,
        "api_key": "sk-upstream",
        "default_model": default_model,
        "protocol": protocol,
        "mappings": mappings,
    })
}

fn json_config_with_key(port: u16, providers: Vec<Value>) -> Value {
    json!({
        "port": port,
        "keys": [{
            "id": "k1",
            "label": "k1",
            "value": "local-key",
            "enabled": true,
            "created_at": 1,
        }],
        "providers": providers,
    })
}

/// Decode the first `data:` event of a relay SSE error stream as JSON.
fn first_sse_event(text: &str) -> Value {
    let line = text
        .lines()
        .find(|line| line.starts_with("data: ") && !line.contains("[DONE]"))
        .unwrap_or_else(|| panic!("no SSE data event in: {text}"));
    serde_json::from_str(line.trim_start_matches("data: "))
        .unwrap_or_else(|error| panic!("SSE event is not JSON ({error}): {text}"))
}

/// `Debug`-free rendering of the captured upstream calls for failure messages.
fn captured_summary(captured: &[Captured]) -> String {
    captured
        .iter()
        .map(|item| {
            format!(
                "{} {} body={}",
                item.method,
                item.path,
                String::from_utf8_lossy(&item.body)
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// AC-001: a mapping-level protocol declaration is persisted and survives a
/// write/read round trip.
#[test]
fn mapping_protocol_survives_config_write_and_read() {
    with_temp_home("per-model-protocol-persist", |_home| {
        let config: FusionConfig = serde_json::from_value(json_config_with_key(
            17688,
            vec![json_provider(
                "p1",
                "OpenCode Go",
                "https://api.example.com/v1",
                "responses",
                None,
                vec![json_mapping("mimo-v2.5", "mimo-v2.5", Some("chat_completions"))],
            )],
        ))
        .expect("decode config with a mapping-level protocol");

        super::storage::write_config(&config).expect("write config");

        let raw = fs::read_to_string(config_path().unwrap()).expect("read config file");
        assert!(
            raw.trim_start().starts_with("v2:"),
            "the config file is encrypted, so persistence is asserted on the reloaded view"
        );

        let reloaded = super::storage::read_config().expect("read config");
        let serialized = serde_json::to_value(&reloaded).expect("encode reloaded config");
        let mapping = &serialized["providers"][0]["mappings"][0];
        assert_eq!(mapping["local_model"], "mimo-v2.5", "{serialized}");
        assert_eq!(
            mapping["protocol"], "chat_completions",
            "a mapping-level protocol declaration must survive write+read: {serialized}"
        );
    });
}

/// AC-002: within one provider record, each model is forwarded to the endpoint
/// named by the mapping row that matched it.
#[tokio::test]
async fn mapping_protocol_routes_each_model_to_its_declared_endpoint() {
    let home = temp_home("per-model-protocol-forward");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let config: FusionConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![json_provider(
            "p1",
            "OpenCode Go",
            &upstream_url,
            "responses",
            Some("deepseek-v4.1-flash"),
            vec![
                json_mapping("deepseek-v4.1-flash", "deepseek-v4.1-flash", Some("responses")),
                json_mapping("mimo-v2.5", "mimo-v2.5", Some("chat_completions")),
            ],
        )],
    ))
    .expect("decode config with mapping-level protocols");
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (responses_status, _, responses_text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "deepseek-v4.1-flash", "input": "hi"})),
    )
    .await;
    let (chat_status, _, chat_text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "mimo-v2.5", "messages": [{"role": "user", "content": "hi"}]})),
    )
    .await;

    let captured = log.lock().unwrap().clone();
    let summary = format!(
        "responses=(status={responses_status}, body={responses_text}) chat=(status={chat_status}, body={chat_text}) upstream=[{}]",
        captured_summary(&captured)
    );
    assert_eq!(
        responses_status, 200,
        "the responses mapping must be served: {summary}"
    );
    assert_eq!(
        chat_status, 200,
        "a chat_completions mapping inside a responses provider must be served via the chat endpoint: {summary}"
    );
    assert_eq!(captured.len(), 2, "each request reaches upstream once: {summary}");
    assert_eq!(captured[0].path, "/v1/responses", "{summary}");
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(sent["model"], "deepseek-v4.1-flash", "{summary}");
    assert_eq!(captured[1].path, "/v1/chat/completions", "{summary}");
    let sent: Value = serde_json::from_slice(&captured[1].body).unwrap();
    assert_eq!(sent["model"], "mimo-v2.5", "{summary}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-003: a matched mapping whose protocol differs from the inbound protocol
/// makes the provider ineligible for the whole request: no upstream call, no
/// fallback to `default_model`, 502 for non-streaming and a 200 SSE carrying the
/// same error object for streaming.
#[tokio::test]
async fn mapping_protocol_mismatch_is_never_served_and_never_falls_back_to_default() {
    let home = temp_home("per-model-protocol-mismatch");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "should-not-run"}))).await;

    let config: FusionConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![json_provider(
            "p1",
            "OpenCode Go",
            &upstream_url,
            "responses",
            Some("deepseek-v4.1-flash"),
            vec![json_mapping("mimo-v2.5", "mimo-v2.5", Some("chat_completions"))],
        )],
    ))
    .expect("decode config with a mapping-level protocol");
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "mimo-v2.5", "input": "hi"})),
    )
    .await;
    let (stream_status, stream_content_type, stream_text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "mimo-v2.5", "input": "hi", "stream": true})),
    )
    .await;

    let upstream_calls = log.lock().unwrap().len();
    assert_eq!(
        upstream_calls, 0,
        "a protocol-mismatched mapping must not contact upstream and must not fall back to default_model: non_stream=(status={status}, body={text}) stream=(status={stream_status}, type={stream_content_type}, body={stream_text})"
    );

    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(status, 502, "non-streaming mismatch must be 502: {text}");
    assert_eq!(
        body["error"]["code"], "all_providers_unavailable",
        "body={text}"
    );

    assert_eq!(stream_status, 200, "streaming mismatch keeps HTTP 200: {stream_text}");
    assert!(
        stream_content_type.contains("text/event-stream"),
        "content-type: {stream_content_type}"
    );
    assert!(stream_text.contains("data: [DONE]"), "body: {stream_text}");
    assert_eq!(
        first_sse_event(&stream_text)["error"],
        body["error"],
        "the streaming error object must equal the non-streaming one: {stream_text}"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-004: an unmapped request model falls back to `default_model` only when the
/// provider protocol matches the inbound protocol.
#[tokio::test]
async fn default_model_fallback_requires_a_matching_provider_protocol() {
    let home = temp_home("per-model-default-fallback");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut config = config_with_key(port);
    let mut provider = upstream_provider(
        "p1",
        "OpenCode Go",
        &upstream_url,
        "sk-upstream",
        Some("deepseek-v4.1-flash"),
    );
    provider.protocol = UpstreamProtocol::Responses;
    provider.mappings = vec![ModelMapping {
        local_model: "other-local".to_string(),
        upstream_model: "other-remote".to_string(),
        protocol: None,
    }];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (responses_status, _, responses_text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "unknown-local", "input": "hi"})),
    )
    .await;
    let (chat_status, _, chat_text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "unknown-local", "messages": []})),
    )
    .await;

    let captured = log.lock().unwrap().clone();
    let summary = format!(
        "responses=(status={responses_status}, body={responses_text}) chat=(status={chat_status}, body={chat_text}) upstream=[{}]",
        captured_summary(&captured)
    );
    assert_eq!(responses_status, 200, "the default model must serve the matching protocol: {summary}");
    assert_eq!(
        captured.len(), 1,
        "the protocol-mismatched chat request must not reach upstream: {summary}"
    );
    assert_eq!(captured[0].path, "/v1/responses", "{summary}");
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(
        sent["model"], "deepseek-v4.1-flash",
        "an unmapped model falls back to default_model: {summary}"
    );
    assert_eq!(chat_status, 502, "the provider cannot serve the chat protocol: {summary}");
    let body: Value = serde_json::from_str(&chat_text).unwrap();
    assert_eq!(body["error"]["code"], "all_providers_unavailable", "{summary}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-005: two mapping rows for one local model declare different protocols and
/// carry their own remote model names; `GET /v1/models` still lists it once.
#[tokio::test]
async fn same_local_model_serves_both_protocols_from_its_own_row() {
    let home = temp_home("per-model-dual-protocol");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let config: FusionConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![json_provider(
            "p1",
            "OpenCode Go",
            &upstream_url,
            "chat_completions",
            None,
            vec![
                json_mapping("shared-model", "remote-chat", Some("chat_completions")),
                json_mapping("shared-model", "remote-responses", Some("responses")),
            ],
        )],
    ))
    .expect("decode config with dual-protocol mapping rows");
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (chat_status, _, chat_text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "shared-model", "messages": []})),
    )
    .await;
    let (responses_status, _, responses_text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "shared-model", "input": "hi"})),
    )
    .await;
    let (models_status, _, models_text) = call_fusion(
        port,
        "GET",
        "/v1/models",
        &[("authorization", "Bearer local-key")],
        None,
    )
    .await;

    let captured = log.lock().unwrap().clone();
    let summary = format!(
        "chat=(status={chat_status}, body={chat_text}) responses=(status={responses_status}, body={responses_text}) upstream=[{}]",
        captured_summary(&captured)
    );
    assert_eq!(chat_status, 200, "{summary}");
    assert_eq!(responses_status, 200, "{summary}");
    assert_eq!(
        captured.len(), 2,
        "each protocol must reach upstream with its own row: {summary}"
    );
    assert_eq!(captured[0].path, "/v1/chat/completions", "{summary}");
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(
        sent["model"], "remote-chat",
        "the chat row must use its own remote model: {summary}"
    );
    assert_eq!(captured[1].path, "/v1/responses", "{summary}");
    let sent: Value = serde_json::from_slice(&captured[1].body).unwrap();
    assert_eq!(
        sent["model"], "remote-responses",
        "the responses row must use its own remote model: {summary}"
    );

    assert_eq!(models_status, 200, "{models_text}");
    let models: Value = serde_json::from_str(&models_text).unwrap();
    let occurrences = models["data"]
        .as_array()
        .expect("models data array")
        .iter()
        .filter(|item| item["id"] == "shared-model")
        .count();
    assert_eq!(
        occurrences, 1,
        "a model served by two mapping rows must still be listed once: {models_text}"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-006: when the only provider is ineligible because of a mapping-level
/// protocol mismatch, the 502 message (and the streaming equivalent) names the
/// endpoint the model is configured for.
#[tokio::test]
async fn protocol_mismatch_error_names_the_required_endpoint() {
    let home = temp_home("per-model-endpoint-hint");
    let port = free_port().await;
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "should-not-run"}))).await;

    let config: FusionConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![json_provider(
            "p1",
            "OpenCode Go",
            &upstream_url,
            "responses",
            None,
            vec![json_mapping("mimo-v2.5", "mimo-v2.5", Some("chat_completions"))],
        )],
    ))
    .expect("decode config with a mapping-level protocol");
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "mimo-v2.5", "input": "hi"})),
    )
    .await;
    let (stream_status, stream_content_type, stream_text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "mimo-v2.5", "input": "hi", "stream": true})),
    )
    .await;

    let summary = format!(
        "non_stream=(status={status}, body={text}) stream=(status={stream_status}, type={stream_content_type}, body={stream_text})"
    );
    assert_eq!(status, 502, "{summary}");
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        body["error"]["code"], "all_providers_unavailable",
        "{summary}"
    );
    let message = body["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("/chat/completions"),
        "the 502 must name the endpoint the requested model is configured for: {summary}"
    );

    assert_eq!(stream_status, 200, "{summary}");
    assert!(
        stream_content_type.contains("text/event-stream"),
        "{summary}"
    );
    assert!(
        stream_text.contains("/chat/completions"),
        "the streaming error must carry the same endpoint hint: {summary}"
    );
    assert!(stream_text.contains("data: [DONE]"), "{summary}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-007: a mapping without a protocol field (the shape of existing encrypted
/// configs) inherits the provider protocol instead of defaulting to
/// `chat_completions`.
#[tokio::test]
async fn mapping_without_protocol_inherits_the_provider_protocol() {
    let home = temp_home("per-model-legacy-inherit");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut config = config_with_key(port);
    let mut provider = upstream_provider("p1", "OpenCode Go", &upstream_url, "sk-upstream", None);
    provider.protocol = UpstreamProtocol::Responses;
    provider.mappings = vec![ModelMapping {
        local_model: "legacy-local".to_string(),
        upstream_model: "legacy-remote".to_string(),
        protocol: None,
    }];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "legacy-local", "input": "hi"})),
    )
    .await;

    let captured = log.lock().unwrap().clone();
    let summary = format!(
        "status={status}, body={text}, upstream=[{}]",
        captured_summary(&captured)
    );
    assert_eq!(
        status, 200,
        "a mapping without a protocol field must inherit the responses provider protocol: {summary}"
    );
    assert_eq!(captured.len(), 1, "{summary}");
    assert_eq!(
        captured[0].path, "/v1/responses",
        "inheritance must not fall back to the chat_completions default: {summary}"
    );
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(
        sent["model"], "legacy-remote",
        "the matched mapping's own remote model must be used: {summary}"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// REQ-001 / REQ-007: a mapping whose protocol is submitted as an explicit JSON
/// `null` — the shape the UI sends for "follow the provider" — must be treated
/// exactly like a missing field. It is persisted as an inheritance marker (key
/// absent or `null`, never a concrete value), forwards through the provider
/// protocol, and cannot serve the opposite endpoint.
#[tokio::test]
async fn mapping_with_explicit_null_protocol_inherits_the_provider_protocol() {
    let home = temp_home("per-model-null-inherit");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    // Explicitly present `"protocol": null`, unlike `json_mapping(.., None)`.
    let explicit_null_mapping = json!({
        "local_model": "inherit-local",
        "upstream_model": "inherit-remote",
        "protocol": null,
    });
    let config: FusionConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![json_provider(
            "p1",
            "OpenCode Go",
            &upstream_url,
            "responses",
            None,
            vec![explicit_null_mapping],
        )],
    ))
    .expect("decode config with an explicit null mapping protocol");

    // Persistence round trip: the declaration must survive as inheritance, never
    // as a concrete protocol value.
    super::storage::write_config(&config).unwrap();
    let reloaded = super::storage::read_config().expect("read config");
    let serialized = serde_json::to_value(&reloaded).expect("encode reloaded config");
    let persisted = &serialized["providers"][0]["mappings"][0];
    assert_eq!(persisted["local_model"], "inherit-local", "{serialized}");
    assert!(
        persisted["protocol"].is_null(),
        "an explicit null protocol must stay null/absent when persisted, not become a value: {serialized}"
    );

    super::runtime_http::start_server().await.unwrap();

    let (responses_status, _, responses_text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "inherit-local", "input": "hi"})),
    )
    .await;
    let (chat_status, _, chat_text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "inherit-local", "messages": [{"role": "user", "content": "hi"}]})),
    )
    .await;

    let captured = log.lock().unwrap().clone();
    let summary = format!(
        "responses=(status={responses_status}, body={responses_text}) chat=(status={chat_status}, body={chat_text}) upstream=[{}]",
        captured_summary(&captured)
    );

    assert_eq!(
        responses_status, 200,
        "a null-protocol mapping must inherit the responses provider protocol: {summary}"
    );
    assert_eq!(
        captured.len(), 1,
        "only the responses request may reach upstream: {summary}"
    );
    assert_eq!(
        captured[0].path, "/v1/responses",
        "inheritance must not fall back to the chat_completions default: {summary}"
    );
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(
        sent["model"], "inherit-remote",
        "the matched mapping's own remote model must be used: {summary}"
    );

    assert_eq!(
        chat_status, 502,
        "the inherited responses protocol cannot serve a chat request: {summary}"
    );
    let body: Value = serde_json::from_str(&chat_text).unwrap();
    assert_eq!(
        body["error"]["code"], "all_providers_unavailable",
        "body={chat_text}"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// Plan 20260916-api-fusion-per-model-endpoint, Step 3 (cross-module E2E)
//
// Each case drives the real local relay listener (`call_fusion` -> loopback
// HTTP) against an in-process mock upstream and asserts on the upstream
// capture log. Only the relay's observable HTTP surface and the persisted
// config round trip are asserted; no private collaborator is touched.
// ---------------------------------------------------------------------------

/// AC-002 streaming path: inside one provider record, each model streams
/// successfully through the endpoint its own mapping row declares. The client
/// receives HTTP 200, `text/event-stream`, and the exact SSE bytes the mock
/// upstream produced, while upstream sees the declared path and request model.
#[tokio::test]
async fn streaming_forwarding_reaches_each_models_own_endpoint_in_one_record() {
    let home = temp_home("per-model-stream-both-protocols");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|captured| {
        let marker = if captured.path == "/v1/responses" {
            "responses-stream-ok"
        } else {
            "chat-stream-ok"
        };
        MockReply::Stream(format!(
            "data: {{\"marker\":\"{marker}\"}}\n\ndata: [DONE]\n\n"
        ))
    })
    .await;

    let config: FusionConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![json_provider(
            "p1",
            "OpenCode Go",
            &upstream_url,
            "responses",
            Some("deepseek-v4.1-flash"),
            vec![
                json_mapping("deepseek-v4.1-flash", "deepseek-v4.1-flash", Some("responses")),
                json_mapping("mimo-v2.5", "mimo-v2.5", Some("chat_completions")),
            ],
        )],
    ))
    .expect("decode config with mapping-level protocols");
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (responses_status, responses_type, responses_text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "deepseek-v4.1-flash", "input": "hi", "stream": true})),
    )
    .await;
    let (chat_status, chat_type, chat_text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({
            "model": "mimo-v2.5",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true
        })),
    )
    .await;

    let captured = log.lock().unwrap().clone();
    let summary = format!(
        "responses=(status={responses_status}, type={responses_type}, body={responses_text}) chat=(status={chat_status}, type={chat_type}, body={chat_text}) upstream=[{}]",
        captured_summary(&captured)
    );

    assert_eq!(
        responses_status, 200,
        "the responses model must stream successfully: {summary}"
    );
    assert!(
        responses_type.contains("text/event-stream"),
        "responses content-type must be SSE: {summary}"
    );
    assert!(
        responses_text.contains("responses-stream-ok"),
        "the responses client must receive the mock upstream's SSE bytes: {summary}"
    );
    assert!(responses_text.contains("data: [DONE]"), "{summary}");

    assert_eq!(
        chat_status, 200,
        "the chat model must stream successfully from the same record: {summary}"
    );
    assert!(
        chat_type.contains("text/event-stream"),
        "chat content-type must be SSE: {summary}"
    );
    assert!(
        chat_text.contains("chat-stream-ok"),
        "the chat client must receive the mock upstream's SSE bytes: {summary}"
    );
    assert!(chat_text.contains("data: [DONE]"), "{summary}");

    assert_eq!(captured.len(), 2, "each stream must reach upstream once: {summary}");
    assert_eq!(captured[0].method, "POST", "{summary}");
    assert_eq!(captured[0].path, "/v1/responses", "{summary}");
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(
        sent["model"], "deepseek-v4.1-flash",
        "the responses stream must carry the deepseek model: {summary}"
    );
    assert_eq!(captured[1].method, "POST", "{summary}");
    assert_eq!(captured[1].path, "/v1/chat/completions", "{summary}");
    let sent: Value = serde_json::from_slice(&captured[1].body).unwrap();
    assert_eq!(
        sent["model"], "mimo-v2.5",
        "the chat stream must carry the mimo model: {summary}"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// Spec counterexample "协议不一致的服务商不得被选中、不得发起上游请求、不得计入
/// 连续失败": a provider whose matching row declares another protocol is not a
/// candidate, so repeated mismatched requests stay HTTP 502 with zero upstream
/// calls and leave `consecutive_failures == 0` / `auto_disabled == false` on
/// disk. The mismatch direction is `/v1/responses` requesting a chat-only row,
/// which is the direction the spec's mismatch scenario defines; a
/// `chat_completions` request for the same row would match and be served.
#[tokio::test]
async fn protocol_mismatch_never_counts_as_failure_or_auto_disables() {
    let home = temp_home("per-model-mismatch-no-failure-count");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "should-not-run"}))).await;

    let config: FusionConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![json_provider(
            "p1",
            "OpenCode Go",
            &upstream_url,
            "responses",
            Some("deepseek-v4.1-flash"),
            vec![json_mapping("mimo-v2.5", "mimo-v2.5", Some("chat_completions"))],
        )],
    ))
    .expect("decode config with a mapping-level protocol");
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    for attempt in 1..=4u32 {
        let (status, _content_type, text) = call_fusion(
            port,
            "POST",
            "/v1/responses",
            &[("authorization", "Bearer local-key")],
            Some(json!({"model": "mimo-v2.5", "input": "hi"})),
        )
        .await;
        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            status, 502,
            "mismatched attempt {attempt} must stay all-unavailable: {text}"
        );
        assert_eq!(
            body["error"]["code"], "all_providers_unavailable",
            "attempt {attempt}: {text}"
        );
    }

    assert!(
        log.lock().unwrap().is_empty(),
        "an ineligible, protocol-mismatched provider must never contact upstream"
    );

    let stored = super::storage::read_config().unwrap();
    let p1 = stored.providers.iter().find(|p| p.id == "p1").unwrap();
    assert_eq!(
        p1.consecutive_failures, 0,
        "a protocol mismatch must not count toward the failure threshold"
    );
    assert!(
        !p1.auto_disabled,
        "a protocol mismatch must never auto-disable the provider"
    );
    assert_eq!(p1.disabled_reason, None);
    assert_eq!(p1.disabled_at, None);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-002/REQ-002 across records: two records declare the same local model but
/// different provider protocols, so each inbound protocol selects its own
/// record (proved by the distinct upstream credential) and its own remote model.
#[tokio::test]
async fn cross_record_candidates_are_selected_by_each_records_protocol() {
    let home = temp_home("per-model-cross-record");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut config = config_with_key(port);
    let mut chat_record =
        upstream_provider("chat-record", "Chat Record", &upstream_url, "sk-chat", None);
    chat_record.protocol = UpstreamProtocol::ChatCompletions;
    chat_record.mappings = vec![ModelMapping {
        local_model: "shared-model".to_string(),
        upstream_model: "remote-chat".to_string(),
        protocol: None,
    }];
    let mut responses_record = upstream_provider(
        "responses-record",
        "Responses Record",
        &upstream_url,
        "sk-responses",
        None,
    );
    responses_record.protocol = UpstreamProtocol::Responses;
    responses_record.mappings = vec![ModelMapping {
        local_model: "shared-model".to_string(),
        upstream_model: "remote-responses".to_string(),
        protocol: None,
    }];
    config.providers.push(chat_record);
    config.providers.push(responses_record);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (chat_status, _, chat_text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "shared-model", "messages": []})),
    )
    .await;
    let (responses_status, _, responses_text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "shared-model", "input": "hi"})),
    )
    .await;

    let captured = log.lock().unwrap().clone();
    let summary = format!(
        "chat=(status={chat_status}, body={chat_text}) responses=(status={responses_status}, body={responses_text}) upstream=[{}]",
        captured_summary(&captured)
    );
    assert_eq!(chat_status, 200, "the chat record must serve the chat request: {summary}");
    assert_eq!(
        responses_status, 200,
        "the responses record must serve the responses request: {summary}"
    );
    assert_eq!(captured.len(), 2, "each request reaches its own record once: {summary}");

    assert_eq!(captured[0].path, "/v1/chat/completions", "{summary}");
    assert_eq!(
        captured[0].headers.get("authorization").map(String::as_str),
        Some("Bearer sk-chat"),
        "the chat-completions record must be the one contacted: {summary}"
    );
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(sent["model"], "remote-chat", "{summary}");

    assert_eq!(captured[1].path, "/v1/responses", "{summary}");
    assert_eq!(
        captured[1].headers.get("authorization").map(String::as_str),
        Some("Bearer sk-responses"),
        "the responses record must be the one contacted: {summary}"
    );
    let sent: Value = serde_json::from_slice(&captured[1].body).unwrap();
    assert_eq!(sent["model"], "remote-responses", "{summary}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// Spec counterexample "请求体除 `model` 外必须保持等价，不得插入任何协议转换
/// 字段": for both protocols the upstream request body must be field-for-field
/// identical to the client body once `model` is removed (no dropped fields, no
/// injected conversion fields).
#[tokio::test]
async fn request_body_is_equivalent_except_model_for_chat_and_responses() {
    let home = temp_home("per-model-body-equivalence");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let config: FusionConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![json_provider(
            "p1",
            "OpenCode Go",
            &upstream_url,
            "responses",
            None,
            vec![
                json_mapping("deepseek-v4.1-flash", "remote-responses", Some("responses")),
                json_mapping("mimo-v2.5", "remote-chat", Some("chat_completions")),
            ],
        )],
    ))
    .expect("decode config with mapping-level protocols");
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let responses_body = json!({
        "model": "deepseek-v4.1-flash",
        "input": [{"role": "user", "content": "hi"}],
        "temperature": 0.25,
        "max_output_tokens": 128,
        "metadata": {"trace": "abc", "nested": {"keep": [1, 2, 3]}},
        "relay_custom_flag": true
    });
    let chat_body = json!({
        "model": "mimo-v2.5",
        "messages": [{"role": "user", "content": "hi"}],
        "temperature": 0.5,
        "max_tokens": 64,
        "top_p": 0.9,
        "custom_object": {"a": 1, "b": "two"}
    });

    let (responses_status, _, responses_text) = call_fusion(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(responses_body.clone()),
    )
    .await;
    let (chat_status, _, chat_text) = call_fusion(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(chat_body.clone()),
    )
    .await;

    let captured = log.lock().unwrap().clone();
    let summary = format!(
        "responses=(status={responses_status}, body={responses_text}) chat=(status={chat_status}, body={chat_text}) upstream=[{}]",
        captured_summary(&captured)
    );
    assert_eq!(responses_status, 200, "{summary}");
    assert_eq!(chat_status, 200, "{summary}");
    assert_eq!(captured.len(), 2, "{summary}");

    let sent_responses: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(sent_responses["model"], "remote-responses", "{summary}");
    let mut sent_rest = sent_responses.clone();
    sent_rest.as_object_mut().unwrap().remove("model");
    let mut expected_rest = responses_body.clone();
    expected_rest.as_object_mut().unwrap().remove("model");
    assert_eq!(
        sent_rest, expected_rest,
        "only `model` may change on /v1/responses; no field may be added or dropped: {summary}"
    );

    let sent_chat: Value = serde_json::from_slice(&captured[1].body).unwrap();
    assert_eq!(sent_chat["model"], "remote-chat", "{summary}");
    let mut sent_rest = sent_chat.clone();
    sent_rest.as_object_mut().unwrap().remove("model");
    let mut expected_rest = chat_body.clone();
    expected_rest.as_object_mut().unwrap().remove("model");
    assert_eq!(
        sent_rest, expected_rest,
        "only `model` may change on /v1/chat/completions; no field may be added or dropped: {summary}"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-005 across records: the same local model declared by two records under
/// different protocols is listed once, and `GET /v1/models` is the deduplicated
/// union of both records' models without contacting upstream.
#[tokio::test]
async fn models_endpoint_deduplicates_the_same_local_model_across_records() {
    let home = temp_home("per-model-models-dedup-cross-record");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"data": []}))).await;

    let config: FusionConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![
            json_provider(
                "p-chat",
                "Chat Record",
                &upstream_url,
                "chat_completions",
                None,
                vec![
                    json_mapping("shared-model", "remote-chat", Some("chat_completions")),
                    json_mapping("chat-only", "remote-chat-only", Some("chat_completions")),
                ],
            ),
            json_provider(
                "p-responses",
                "Responses Record",
                &upstream_url,
                "responses",
                None,
                vec![
                    json_mapping("shared-model", "remote-responses", Some("responses")),
                    json_mapping("responses-only", "remote-responses-only", Some("responses")),
                ],
            ),
        ],
    ))
    .expect("decode two per-protocol records");
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_fusion(
        port,
        "GET",
        "/v1/models",
        &[("authorization", "Bearer local-key")],
        None,
    )
    .await;
    assert_eq!(status, 200, "{text}");

    let body: Value = serde_json::from_str(&text).unwrap();
    let ids: Vec<String> = body["data"]
        .as_array()
        .expect("models data array")
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    let shared_occurrences = ids.iter().filter(|id| id.as_str() == "shared-model").count();
    assert_eq!(
        shared_occurrences, 1,
        "a local model served by two records must be listed once: {text}"
    );
    assert_eq!(
        ids.len(),
        {
            let mut unique = ids.clone();
            unique.sort();
            unique.dedup();
            unique.len()
        },
        "the models payload must not contain duplicate ids: {text}"
    );
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique,
        vec![
            "chat-only".to_string(),
            "responses-only".to_string(),
            "shared-model".to_string()
        ],
        "GET /v1/models must return the sorted deduplicated union of both records: {text}"
    );
    assert!(
        log.lock().unwrap().is_empty(),
        "GET /v1/models must not contact upstream"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}


