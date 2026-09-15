use super::commands::{default_key_for_sync, plan_terminal_sync, terminal_sync_pending};
use super::selection::{
    can_serve, candidate_providers, classify_failure, manual_reenable, pick_candidate,
    register_failure, register_success, resolve_model, set_user_enabled, FailureClass,
};
use super::storage::{config_path, resolve_default_key_id};
use super::{
    FusionConfig, FusionKey, FusionUpstreamProvider, ModelMapping, TerminalSyncRecord,
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
fn resolve_model_prefers_mapping_then_default_model() {
    let mut mapped = provider("mapped");
    mapped.default_model = Some("remote-default".to_string());
    mapped.mappings = vec![ModelMapping {
        local_model: "local-a".to_string(),
        upstream_model: "remote-a".to_string(),
    }];
    assert_eq!(
        resolve_model(&mapped, Some("local-a")).as_deref(),
        Some("remote-a")
    );
    assert_eq!(
        resolve_model(&mapped, Some("local-unknown")).as_deref(),
        Some("remote-default")
    );
    assert!(can_serve(&mapped, Some("local-unknown")));

    let mut mapping_only = provider("mapping-only");
    mapping_only.mappings = vec![ModelMapping {
        local_model: "local-a".to_string(),
        upstream_model: "remote-a".to_string(),
    }];
    assert_eq!(
        resolve_model(&mapping_only, Some("local-a")).as_deref(),
        Some("remote-a")
    );
    assert_eq!(resolve_model(&mapping_only, Some("local-unknown")), None);
    assert!(!can_serve(&mapping_only, Some("local-unknown")));

    let no_model = provider("no-model");
    assert_eq!(resolve_model(&no_model, Some("anything")), None);
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
    let candidates = candidate_providers(&providers, Some("local-unknown"));
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
async fn forwards_responses_path() {
    let home = temp_home("forward-responses");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| MockReply::Json(200, json!({"id":"resp"}))).await;

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
        },
        ModelMapping {
            local_model: "local-b".to_string(),
            upstream_model: "remote-b".to_string(),
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

