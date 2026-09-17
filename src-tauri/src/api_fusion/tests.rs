use super::commands::{build_gateway_provider, default_key_for_sync, terminal_sync_pending};
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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
            display_name: None,
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

/// The gateway model mapping may carry a display name; it survives a config
/// write/read and stays optional for legacy rows.
#[test]
fn model_mapping_display_name_round_trips_and_stays_optional() {
    with_temp_home("mapping-display-name", |_home| {
        let mut config = FusionConfig::default();
        let mut p = provider("p1");
        p.mappings = vec![
            mapping("local-named", "remote-named", Some("GPT-4o")),
            mapping("local-plain", "remote-plain", None),
        ];
        config.providers.push(p);
        super::storage::write_config(&config).expect("write config");

        let loaded = super::storage::read_config().expect("read config");
        assert_eq!(
            loaded.providers[0].mappings[0].display_name.as_deref(),
            Some("GPT-4o")
        );
        assert_eq!(loaded.providers[0].mappings[1].display_name, None);
    });
}

#[test]
fn model_mapping_display_name_is_skipped_when_absent() {
    let legacy: ModelMapping =
        serde_json::from_value(json!({"local_model": "local-a", "upstream_model": "remote-a"}))
            .expect("a mapping without display_name must deserialize");
    assert_eq!(legacy.display_name, None);

    let named: ModelMapping = serde_json::from_value(json!({
        "local_model": "local-a",
        "upstream_model": "remote-a",
        "display_name": "GPT-4o",
    }))
    .expect("a mapping with display_name must deserialize");
    assert_eq!(named.display_name.as_deref(), Some("GPT-4o"));

    let encoded = serde_json::to_value(&legacy).expect("serialize mapping");
    assert!(
        encoded.get("display_name").is_none(),
        "an absent display_name must not be serialized: {encoded}"
    );
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
        display_name: None,
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
        display_name: None,
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
        display_name: None,
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
    /// Return an arbitrary status with a larger content-length than the bytes sent.
    PartialRaw(u16, &'static str, Vec<u8>, usize),
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
                        let retry_after = if status >= 500 {
                            "retry-after-ms: 0\r\n"
                        } else {
                            ""
                        };
                        let header = format!(
                            "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{retry_after}\r\n",
                            body.len(),
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
                    MockReply::PartialRaw(status, content_type, body, declared) => {
                        let header = format!(
                            "HTTP/1.1 {status} Unauthorized\r\ncontent-type: {content_type}\r\ncontent-length: {declared}\r\nconnection: close\r\nretry-after-ms: 0\r\n\r\n"
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(&body).await;
                        let _ = stream.flush().await;
                    }
                    MockReply::Raw(status, content_type, body) => {
                        let retry_after = if status >= 500 {
                            "retry-after-ms: 0\r\n"
                        } else {
                            ""
                        };
                        let header = format!(
                            "HTTP/1.1 {status} OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n{retry_after}\r\n",
                            body.len(),
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

fn mapping(local_model: &str, upstream_model: &str, display_name: Option<&str>) -> ModelMapping {
    ModelMapping {
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        protocol: None,
        display_name: display_name.map(str::to_string),
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
        display_name: None,
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
        display_name: None,
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
    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// Step 1 (20260916-api-fusion-upstream-retry): bounded retry / recovery
// ---------------------------------------------------------------------------

/// Scripted JSON mock upstream running on the CURRENT tokio runtime (via
/// `tokio::spawn`, never the tauri global runtime). The returned counter is
/// incremented once per accepted request so a test can assert the exact number
/// of upstream attempts. The last scripted reply repeats once the sequence is
/// exhausted.
async fn spawn_json_sequence_mock(responses: Vec<(u16, Value)>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    let count = Arc::new(AtomicUsize::new(0));
    let count_for_server = count.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let count = count_for_server.clone();
            let responses = responses.clone();
            tokio::spawn(async move {
                let Ok(_request) = super::runtime_http::read_http_request(&mut stream).await else {
                    return;
                };
                let index = count.fetch_add(1, Ordering::SeqCst);
                let fallback = responses.last().cloned().unwrap_or((502, json!({})));
                let (status, value) = responses.get(index).cloned().unwrap_or(fallback);
                let body = serde_json::to_vec(&value).unwrap_or_default();
                let header = format!(
                    "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(&body).await;
            });
        }
    });
    (format!("http://{}", addr), count)
}

/// RED slice for REQ-002 / AC-002 (recovery): a single upstream that fails the
/// first attempt with a retryable 500 and succeeds on the second must be
/// retried within the same request. Observable boundary: the full HTTP response
/// returned by `attempt_non_streaming` plus the exact number of upstream
/// requests. Current behavior makes only one attempt, so this fails.
#[tokio::test]
async fn attempt_non_streaming_retries_single_provider_after_500_then_succeeds() {
    let _home = temp_home("retry-recovery-single-provider");
    let (upstream_url, upstream_requests) = spawn_json_sequence_mock(vec![
        (500, json!({"error": {"message": "temporarily unavailable"}})),
        (200, json!({"id": "recovered", "choices": []})),
    ])
    .await;

    let mut config = FusionConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    config.providers.push(a.clone());
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let response = super::runtime_http::attempt_non_streaming(
        std::slice::from_ref(&a),
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
    )
    .await;

    // Assert the attempt count first: the RED gap is that the current code stops
    // after the first 500 instead of issuing a bounded retry.
    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        2,
        "one initial attempt plus exactly one bounded retry"
    );

    let body_text = String::from_utf8_lossy(&response.body);
    assert_eq!(
        response.status, 200,
        "a transient 500 must be retried until the provider recovers; body: {body_text}"
    );
    assert!(
        body_text.contains("recovered"),
        "response must carry the recovered attempt body: {body_text}"
    );
}

async fn assert_truncated_auth_non_streaming(status: u16) {
    let _home = temp_home(&format!("truncated-auth-non-streaming-{status}"));
    let auth_body = br#"{"error":{"message":"upstream auth failed"}}"#.to_vec();
    let auth_declared = auth_body.len() + 32;
    let (auth_url, auth_requests) = spawn_mock_upstream(move |_| {
        MockReply::PartialRaw(
            status,
            "application/json",
            auth_body.clone(),
            auth_declared,
        )
    })
    .await;
    let (fallback_url, fallback_requests) = spawn_mock_upstream(|_| {
        MockReply::Json(200, json!({"id": "healthy-fallback"}))
    })
    .await;

    let mut config = FusionConfig::default();
    let auth = upstream_provider(
        "auth",
        "Auth Provider",
        &auth_url,
        "auth-key",
        Some("remote-default"),
    );
    let fallback = upstream_provider(
        "fallback",
        "Healthy Fallback",
        &fallback_url,
        "fallback-key",
        Some("remote-default"),
    );
    config.providers.extend([auth.clone(), fallback.clone()]);
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let response = super::runtime_http::attempt_non_streaming(
        &[auth, fallback],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
    )
    .await;

    assert_eq!(response.status, 200);
    assert!(
        String::from_utf8_lossy(&response.body).contains("healthy-fallback"),
        "healthy fallback must answer the request"
    );
    assert_eq!(auth_requests.lock().unwrap().len(), 1);
    assert_eq!(fallback_requests.lock().unwrap().len(), 1);

    let persisted = super::storage::read_config().expect("read persisted provider state");
    let auth_provider = persisted
        .providers
        .iter()
        .find(|provider| provider.id == "auth")
        .expect("auth provider state");
    assert!(
        auth_provider.auto_disabled,
        "HTTP {status} must immediately persist auto_disabled despite the truncated body"
    );
}

#[tokio::test]
async fn truncated_auth_401_non_streaming_disables_once_and_falls_back() {
    assert_truncated_auth_non_streaming(401).await;
}

#[tokio::test]
async fn truncated_auth_403_non_streaming_disables_once_and_falls_back() {
    assert_truncated_auth_non_streaming(403).await;
}

async fn assert_truncated_auth_streaming(status: u16) {
    let _home = temp_home(&format!("truncated-auth-streaming-{status}"));
    let auth_body = br#"{"error":{"message":"upstream auth failed"}}"#.to_vec();
    let auth_declared = auth_body.len() + 32;
    let (auth_url, auth_requests) = spawn_mock_upstream(move |_| {
        MockReply::PartialRaw(
            status,
            "application/json",
            auth_body.clone(),
            auth_declared,
        )
    })
    .await;
    let (fallback_url, fallback_requests) = spawn_mock_upstream(|_| {
        MockReply::Stream(
            "data: {\"choices\":[{\"delta\":{\"content\":\"healthy-fallback\"}}]}\n\ndata: [DONE]\n\n"
                .to_string(),
        )
    })
    .await;

    let mut config = FusionConfig::default();
    let auth = upstream_provider(
        "auth",
        "Auth Provider",
        &auth_url,
        "auth-key",
        Some("remote-default"),
    );
    let fallback = upstream_provider(
        "fallback",
        "Healthy Fallback",
        &fallback_url,
        "fallback-key",
        Some("remote-default"),
    );
    config.providers.extend([auth.clone(), fallback.clone()]);
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();

    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    super::runtime_http::attempt_streaming(
        &mut server,
        &[auth, fallback],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
    )
    .await
    .expect("streaming fallback must complete");
    drop(server);

    let mut output = Vec::new();
    client.read_to_end(&mut output).await.unwrap();
    let output = String::from_utf8_lossy(&output);
    assert!(
        output.contains("healthy-fallback"),
        "healthy fallback stream must answer the request: {output}"
    );
    assert_eq!(auth_requests.lock().unwrap().len(), 1);
    assert_eq!(fallback_requests.lock().unwrap().len(), 1);

    let persisted = super::storage::read_config().expect("read persisted provider state");
    let auth_provider = persisted
        .providers
        .iter()
        .find(|provider| provider.id == "auth")
        .expect("auth provider state");
    assert!(
        auth_provider.auto_disabled,
        "HTTP {status} must immediately persist auto_disabled despite the truncated body"
    );
}

#[tokio::test]
async fn truncated_auth_401_streaming_disables_once_and_falls_back() {
    assert_truncated_auth_streaming(401).await;
}

#[tokio::test]
async fn truncated_auth_403_streaming_disables_once_and_falls_back() {
    assert_truncated_auth_streaming(403).await;
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
            display_name: None,
        },
        ModelMapping {
            local_model: "local-b".to_string(),
            upstream_model: "remote-b".to_string(),
            protocol: None,
            display_name: None,
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
fn build_gateway_provider_rejects_unsupported_tools() {
    for tool in ["claude", "antigravity", ""] {
        let error = build_gateway_provider(
            "g-1",
            tool,
            "http://127.0.0.1:17688",
            "local-key",
            &[],
        )
        .unwrap_err();
        assert!(error.contains("unsupported"), "tool {tool:?} error: {error}");
    }
}

/// The tool input is case-insensitive but the emitted `tool` value is always
/// lowercase, and the tool-specific branch is selected case-insensitively.
#[test]
fn build_gateway_provider_normalizes_tool_case_to_lowercase() {
    let opencode = build_gateway_provider(
        "fus-oc",
        "OpenCode",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[],
    )
    .expect("mixed-case opencode must build");
    assert_eq!(opencode["tool"], "opencode", "emitted tool must be lowercase: {opencode}");
    assert_eq!(opencode["provider_key"], "api_gateway");
    assert_eq!(opencode["tool_config"]["npm"], "@ai-sdk/openai-compatible");

    let codex = build_gateway_provider(
        "fus-cx",
        "Codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[],
    )
    .expect("mixed-case codex must build");
    assert_eq!(codex["tool"], "codex", "emitted tool must be lowercase: {codex}");
    assert_eq!(codex["tool_config"]["wire_api"], "chat");
    assert!(codex.get("provider_key").is_none(), "codex has no provider_key");
}

/// Only enabled gateways contribute: a gateway with `enabled == false` or
/// `auto_disabled == true` is excluded from the opencode model map and from the
/// codex model selection, even when it is listed first.
#[test]
fn build_gateway_provider_ignores_disabled_and_auto_disabled_gateways() {
    let mut disabled = upstream_provider(
        "g1",
        "Disabled",
        "https://disabled.example/v1",
        "sk",
        Some("disabled-default"),
    );
    disabled.enabled = false;
    disabled.mappings = vec![mapping("disabled-local", "disabled-remote", Some("Disabled"))];

    let mut auto_disabled = upstream_provider(
        "g2",
        "Auto Disabled",
        "https://auto.example/v1",
        "sk",
        Some("auto-default"),
    );
    auto_disabled.auto_disabled = true;
    auto_disabled.mappings = vec![mapping("auto-local", "auto-remote", Some("Auto"))];

    let mut enabled = upstream_provider(
        "g3",
        "Enabled",
        "https://enabled.example/v1",
        "sk",
        Some("enabled-default"),
    );
    enabled.mappings = vec![mapping("enabled-local", "enabled-remote", Some("Enabled"))];

    let gateways = [disabled, auto_disabled, enabled];

    let opencode = build_gateway_provider(
        "fus-oc",
        "opencode",
        "http://127.0.0.1:17688",
        "local-key-123",
        &gateways,
    )
    .expect("opencode provider must build");
    assert_eq!(
        opencode["tool_config"]["models"],
        json!({ "enabled-local": { "name": "Enabled" } }),
        "only the enabled gateway mappings may be emitted: {opencode}"
    );

    let codex = build_gateway_provider(
        "fus-cx",
        "codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &gateways,
    )
    .expect("codex provider must build");
    assert_eq!(
        codex["model"], "enabled-local",
        "a disabled gateway must not win the model selection: {codex}"
    );
}

#[test]
fn build_gateway_provider_opencode_carries_gateway_models_and_marker() {
    let mut gateway = upstream_provider("g1", "Gateway A", "https://upstream.example/v1", "sk", None);
    gateway.mappings = vec![
        mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o")),
        mapping("ds", "deepseek-chat", None),
        mapping("", "ignored", Some("Ignored")),
    ];

    let value = build_gateway_provider(
        "fus-oc",
        "opencode",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway],
    )
    .expect("opencode provider must build");

    assert_eq!(value["id"], "fus-oc");
    assert_eq!(value["name"], "API Gateway");
    assert_eq!(value["tool"], "opencode");
    assert_eq!(value["base_url"], "http://127.0.0.1:17688");
    assert_eq!(value["api_key"], "local-key-123");
    assert_eq!(value["tool_config"]["api_fusion_gateway"], true);
    assert!(value.get("active").is_none(), "must never auto-activate: {value}");
    assert!(value.get("is_active").is_none(), "must never auto-activate: {value}");

    assert_eq!(value["provider_key"], "api_gateway");
    assert_eq!(value["tool_config"]["npm"], "@ai-sdk/openai-compatible");
    assert_eq!(
        value["tool_config"]["options"]["baseURL"],
        "http://127.0.0.1:17688"
    );
    assert_eq!(value["tool_config"]["options"]["apiKey"], "local-key-123");
    assert_eq!(
        value["tool_config"]["models"],
        json!({
            "gpt-4o": { "name": "GPT-4o" },
            "ds": { "name": "deepseek-chat" },
        }),
        "models are keyed by non-empty local_model: {value}"
    );
}

#[test]
fn build_gateway_provider_opencode_keeps_first_duplicate_and_falls_back_names() {
    let mut first = upstream_provider("g1", "Gateway A", "https://a.example/v1", "sk", None);
    first.mappings = vec![mapping("dup", "first-upstream", Some("First Name"))];
    let mut second = upstream_provider("g2", "Gateway B", "https://b.example/v1", "sk", None);
    second.mappings = vec![
        mapping("dup", "second-upstream", Some("Second Name")),
        mapping("plain", "plain-upstream", Some("")),
    ];

    let value = build_gateway_provider(
        "fus-oc",
        "opencode",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[first, second],
    )
    .expect("opencode provider must build");

    assert_eq!(
        value["tool_config"]["models"],
        json!({
            "dup": { "name": "First Name" },
            "plain": { "name": "plain-upstream" },
        }),
        "the first duplicate wins and an empty display name falls back to the upstream model"
    );
}

#[test]
fn build_gateway_provider_codex_shape_is_wire_api_chat_without_options() {
    let mut gateway = upstream_provider(
        "g1",
        "Gateway A",
        "https://upstream.example/v1",
        "sk",
        Some("gateway-default"),
    );
    gateway.mappings = vec![mapping("local-a", "remote-a", None)];

    let value = build_gateway_provider(
        "fus-cx",
        "codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway],
    )
    .expect("codex provider must build");

    assert_eq!(value["id"], "fus-cx");
    assert_eq!(value["name"], "API Gateway");
    assert_eq!(value["tool"], "codex");
    assert_eq!(value["base_url"], "http://127.0.0.1:17688");
    assert_eq!(value["api_key"], "local-key-123");
    assert_eq!(value["tool_config"]["api_fusion_gateway"], true);
    assert_eq!(value["tool_config"]["wire_api"], "chat");
    assert_eq!(
        value["model"], "local-a",
        "a mapping local_model must win over default_model: {value}"
    );
    assert!(value.get("provider_key").is_none(), "codex has no provider_key: {value}");
    assert!(
        value["tool_config"].get("options").is_none(),
        "codex has no options block: {value}"
    );
    assert!(value.get("active").is_none(), "must never auto-activate: {value}");
    assert!(value.get("is_active").is_none(), "must never auto-activate: {value}");
}

#[test]
fn build_gateway_provider_codex_skips_empty_default_models() {
    let empty = upstream_provider("g1", "Gateway A", "https://a.example/v1", "sk", Some(""));
    let named = upstream_provider(
        "g2",
        "Gateway B",
        "https://b.example/v1",
        "sk",
        Some("gateway-default"),
    );

    let value = build_gateway_provider(
        "fus-cx",
        "codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[empty, named],
    )
    .expect("codex provider must build");

    assert_eq!(value["model"], "gateway-default");
}

/// A mapping `local_model` anywhere wins over every `default_model`; the
/// default is only a fallback when no enabled gateway has any mapping.
#[test]
fn build_gateway_provider_codex_prefers_mapping_local_model_over_default_model() {
    let first = upstream_provider(
        "g1",
        "Gateway A",
        "https://a.example/v1",
        "sk",
        Some("gateway-default"),
    );
    let mut second = upstream_provider(
        "g2",
        "Gateway B",
        "https://b.example/v1",
        "sk",
        Some("gateway-default-2"),
    );
    second.mappings = vec![mapping("local-b", "remote-b", None)];

    let value = build_gateway_provider(
        "fus-cx",
        "codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[first, second],
    )
    .expect("codex provider must build");

    assert_eq!(
        value["model"], "local-b",
        "a later gateway's mapping local_model must beat an earlier default_model: {value}"
    );
}

#[test]
fn build_gateway_provider_codex_uses_first_non_empty_mapping_local_model() {
    let mut gateway = upstream_provider("g1", "Gateway A", "https://a.example/v1", "sk", Some(""));
    gateway.mappings = vec![
        mapping("", "ignored", None),
        mapping("local-a", "remote-a", None),
        mapping("local-b", "remote-b", None),
    ];

    let value = build_gateway_provider(
        "fus-cx",
        "codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway],
    )
    .expect("codex provider must build");

    assert_eq!(value["model"], "local-a");
}

#[test]
fn build_gateway_provider_codex_omits_model_without_any_mapping() {
    let gateway = upstream_provider("g1", "Gateway A", "https://a.example/v1", "sk", None);

    let value = build_gateway_provider(
        "fus-cx",
        "codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway],
    )
    .expect("codex provider must build");

    assert!(value.get("model").is_none(), "model key must be absent: {value}");
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

/// A brand-new key submitted with the UI mask placeholder must be treated the
/// same as a blank value: the command generates a fresh secret instead of
/// persisting the literal `"********"`.
#[test]
fn new_keys_with_mask_placeholder_get_a_random_secret() {
    with_temp_home("key-mask-autogen", |_home| {
        let created = super::commands::api_fusion_upsert_key(FusionKey {
            id: String::new(),
            label: "Masked".to_string(),
            value: "********".to_string(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        let created_value = created.keys[0].value.clone();
        assert_ne!(
            created_value, "********",
            "the mask placeholder must never be stored as a key value"
        );
        assert!(
            created_value.starts_with("sk-fusion-"),
            "a masked new key must receive a generated secret: {created_value}"
        );
        assert!(
            created_value.len() > "sk-fusion-".len(),
            "a generated secret must carry entropy after the prefix: {created_value}"
        );
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
        display_name: None,
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
        18,
        "three failed requests make six bounded attempts each; the auto-disabled provider is not contacted on the fourth request"
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
    // AC-011: a network error yields to the healthy initial-pass fallback, so
    // each inbound request records one failed network candidate without paying
    // its retry delay. Three failed inbound requests still auto-disable it.
    let _home = temp_home("e2e-network-threshold");
    let dead_url = closed_port_base_url().await;
    let (healthy_url, _) =
        spawn_json_sequence_mock(vec![(200, json!({"id": "healthy-fallback"}))]).await;

    let mut config = FusionConfig::default();
    let failed = upstream_provider(
        "a",
        "Provider A",
        &dead_url,
        "sk",
        Some("remote-model"),
    );
    let healthy = upstream_provider(
        "b",
        "Provider B",
        &healthy_url,
        "sk",
        Some("remote-model"),
    );
    config.providers = vec![failed.clone(), healthy.clone()];
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    for _ in 0..3 {
        let response = super::runtime_http::attempt_non_streaming(
            &[failed.clone(), healthy.clone()],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
        )
        .await;
        assert_eq!(response.status, 200, "network failure must yield to the healthy fallback");
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
        display_name: None,
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
            display_name: None,
        },
        ModelMapping {
            local_model: "local-a".to_string(),
            upstream_model: "remote-a".to_string(),
            protocol: None,
            display_name: None,
        },
    ];
    let mut disabled = upstream_provider("p2", "Provider Two", &upstream_url, "sk", None);
    disabled.enabled = false;
    disabled.mappings = vec![ModelMapping {
        local_model: "local-disabled".to_string(),
        upstream_model: "x".to_string(),
        protocol: None,
        display_name: None,
    }];
    let mut auto_disabled = upstream_provider("p3", "Provider Three", &upstream_url, "sk", None);
    auto_disabled.auto_disabled = true;
    auto_disabled.mappings = vec![ModelMapping {
        local_model: "local-auto".to_string(),
        upstream_model: "x".to_string(),
        protocol: None,
        display_name: None,
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
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let _ = super::runtime_http::read_http_request(&mut stream).await;
                tokio::time::sleep(std::time::Duration::from_secs(120)).await;
            });
        }
    });
    format!("http://{}", addr)
}

/// Finding C (low), deterministic path: an upstream that accepts the TCP
/// connection but never answers must not hang the forwarding request. The
/// enforced idle read timeout (60s, see `forwarding::UPSTREAM_READ_TIMEOUT`)
/// makes it a retryable failure that switches to the next candidate and counts
/// toward the failure threshold (AC-010, AC-011).
///
/// The 75s bound only encodes "must not hang": it is deliberately generous
/// because the idle budget before the first byte arrives is now 60s. It no
/// longer encodes "fails fast" — a fast first-byte deadline is not a
/// requirement, as `slow_first_byte_upstream_is_served_within_the_relaxed_budget`
/// pins down.
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
        std::time::Duration::from_secs(75),
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

/// Bind a loopback listener that accepts connections, reads the request and only
/// then thinks for 12s before sending a valid JSON response. The think time is
/// past the old 10s idle read timeout but inside the relaxed 60s budget, which
/// is exactly the slow-reasoning-model shape observed in the smoke run. The
/// marker pins the relayed body to this upstream so a fallback cannot mask it.
async fn spawn_slow_first_byte_upstream() -> String {
    const MARKER: &str = "slow-first-byte-ok";
    const THINK_TIME: std::time::Duration = std::time::Duration::from_secs(12);
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind slow-first-byte upstream");
    let addr = listener.local_addr().expect("slow-first-byte addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let Ok(_request) = super::runtime_http::read_http_request(&mut stream).await else {
                    return;
                };
                tokio::time::sleep(THINK_TIME).await;
                let body = serde_json::to_vec(&json!({
                    "id": MARKER,
                    "choices": [{"message": {"role": "assistant", "content": "slow but alive"}}]
                }))
                .unwrap_or_default();
                let header = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(&body).await;
                let _ = stream.flush().await;
            });
        }
    });
    format!("http://{}", addr)
}

/// A slow-but-alive inference upstream must be served as success: a first byte
/// at ~12s is inside the relaxed 60s idle read budget and must not be turned
/// into a 502 "network error" that counts toward the 3-strikes auto-disable
/// threshold (AC-010, AC-011).
#[tokio::test]
async fn slow_first_byte_upstream_is_served_within_the_relaxed_budget() {
    let _home = temp_home("slow-first-byte");
    let slow_url = spawn_slow_first_byte_upstream().await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &slow_url, "sk-a", Some("remote-model"));
    config.providers.push(a.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(40),
        super::runtime_http::attempt_non_streaming(
            &[a],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
        ),
    )
    .await
    .expect("a slow first byte inside the relaxed budget must not hang the relay");

    assert_eq!(
        response.status, 200,
        "a first byte at ~12s (inside the 60s idle read budget) must be served, not turned into an upstream failure; body was {}",
        String::from_utf8_lossy(&response.body)
    );
    assert!(
        String::from_utf8_lossy(&response.body).contains("slow-first-byte-ok"),
        "the slow upstream body must be relayed unchanged"
    );
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
// app_store boundary is a fake here; everything else (gateway provider building,
// provider-id reuse and ledger) runs as production code against an isolated
// config directory.
// ---------------------------------------------------------------------------

/// A previously synced API Gateway provider as it appears in the terminal
/// service provider list. `tool_config.api_fusion_gateway == true` is the stable
/// marker emitted by `build_gateway_provider`.
fn managed_gateway_provider(id: &str, tool: &str) -> Value {
    json!({
        "id": id,
        "tool": tool,
        "name": "API Gateway",
        "base_url": "http://127.0.0.1:17688",
        "api_key": "previous-local-key",
        "tool_config": {
            "api_fusion_gateway": true,
            "wire_api": "chat",
        }
    })
}

/// A user-owned provider record without the API Fusion gateway marker. The
/// stale-ledger protection must never claim or overwrite it.
fn unmarked_user_provider(id: &str, tool: &str) -> Value {
    json!({
        "id": id,
        "tool": tool,
        "name": "My Provider",
        "base_url": "https://user.example.com/v1",
        "api_key": "user-key",
        "tool_config": {
            "npm": "@ai-sdk/openai-compatible"
        }
    })
}

/// The current terminal service provider list: one already-synced gateway
/// provider per tool (recognized through the marker, under `tool_config` for
/// opencode and at the top level for codex), an unmarked user-owned opencode
/// provider, plus an unrelated legacy provider.
fn terminal_providers_payload() -> Value {
    json!({
        "providers": [
            managed_gateway_provider("managed-oc", "opencode"),
            {
                "id": "managed-cx",
                "tool": "codex",
                "name": "API Gateway",
                "base_url": "http://127.0.0.1:17688",
                "api_key": "previous-local-key",
                "api_fusion_gateway": true
            },
            unmarked_user_provider("user-oc", "opencode"),
            {
                "id": "legacy-other",
                "tool": "claude",
                "name": "Unrelated Tool",
                "base_url": "https://old.example.com",
                "api_key": "old-key"
            }
        ]
    })
}

/// A config with one enabled local key and a gateway provider carrying a model
/// mapping with and without a display name.
fn gateway_config(port: u16) -> FusionConfig {
    let mut config = FusionConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key-123"));
    let mut gateway = upstream_provider(
        "g1",
        "Gateway A",
        "https://upstream.example/v1",
        "sk-upstream",
        None,
    );
    gateway.mappings = vec![
        mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o")),
        mapping("ds", "deepseek-chat", None),
    ];
    config.providers.push(gateway);
    config
}

/// Run the real terminal sync pipeline against an injected capture-only upsert
/// and return the submitted payloads plus the returned ledger records.
async fn capture_terminal_sync(
    providers_data: &Value,
    tools: Vec<String>,
) -> (Vec<Value>, Vec<TerminalSyncRecord>) {
    let captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    let records = super::commands::apply_terminal_sync_with(
        providers_data,
        move |value| -> super::commands::UpsertFuture {
            let sink = sink.clone();
            Box::pin(async move {
                sink.lock().expect("capture lock").push(value);
                Ok(())
            })
        },
        tools,
    )
    .await
    .expect("terminal sync must succeed");
    let submitted = captured.lock().unwrap().clone();
    (submitted, records)
}

/// New behavior: syncing creates exactly one independent gateway provider per
/// requested tool, carrying the local base URL, the default local key and the
/// gateway model mapping, and never auto-activating it.
#[tokio::test]
async fn terminal_sync_with_seam_creates_one_gateway_provider_per_tool() {
    let _home = temp_home("terminal-sync-seam-create");
    let config = gateway_config(17688);
    super::storage::write_config(&config).unwrap();
    let local_base_url = super::storage::local_base_url(config.port);

    let captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    let records = super::commands::apply_terminal_sync_with(
        &json!({ "providers": [] }),
        move |value| -> super::commands::UpsertFuture {
            let sink = sink.clone();
            Box::pin(async move {
                sink.lock().expect("capture lock").push(value);
                Ok(())
            })
        },
        vec!["opencode".to_string(), "codex".to_string()],
    )
    .await
    .expect("terminal sync must succeed");

    let submitted = captured.lock().unwrap().clone();
    assert_eq!(submitted.len(), 2, "exactly one upsert per tool: {submitted:?}");

    let by_tool = |tool: &str| {
        submitted
            .iter()
            .find(|value| value["tool"] == tool)
            .unwrap_or_else(|| panic!("missing submitted record for tool {tool}"))
    };

    let opencode = by_tool("opencode");
    assert!(!opencode["id"].as_str().unwrap_or("").is_empty());
    assert_eq!(opencode["name"], "API Gateway");
    assert_eq!(opencode["base_url"], local_base_url.as_str());
    assert_eq!(opencode["api_key"], "local-key-123");
    assert_eq!(opencode["tool_config"]["api_fusion_gateway"], true);
    assert_eq!(opencode["provider_key"], "api_gateway");
    assert_eq!(
        opencode["tool_config"]["options"]["baseURL"],
        local_base_url.as_str()
    );
    assert_eq!(opencode["tool_config"]["options"]["apiKey"], "local-key-123");
    assert_eq!(
        opencode["tool_config"]["models"],
        json!({
            "gpt-4o": { "name": "GPT-4o" },
            "ds": { "name": "deepseek-chat" },
        })
    );
    assert!(opencode.get("active").is_none(), "must never auto-activate");
    assert!(opencode.get("is_active").is_none(), "must never auto-activate");

    let codex = by_tool("codex");
    assert!(!codex["id"].as_str().unwrap_or("").is_empty());
    assert_eq!(codex["name"], "API Gateway");
    assert_eq!(codex["base_url"], local_base_url.as_str());
    assert_eq!(codex["api_key"], "local-key-123");
    assert_eq!(codex["tool_config"]["api_fusion_gateway"], true);
    assert_eq!(codex["tool_config"]["wire_api"], "chat");
    assert!(codex.get("provider_key").is_none(), "codex has no provider_key");
    assert!(
        codex["tool_config"].get("options").is_none(),
        "codex has no options block"
    );
    assert!(codex.get("active").is_none(), "must never auto-activate");
    assert!(codex.get("is_active").is_none(), "must never auto-activate");

    assert_ne!(opencode["id"], codex["id"], "each tool gets its own provider");

    assert_eq!(records.len(), 2);
    for record in &records {
        assert_eq!(record.synced_key_id, "k1");
        assert_eq!(record.synced_base_url, local_base_url);
        assert!(!record.provider_id.is_empty());
        assert_eq!(
            by_tool(&record.tool)["id"].as_str(),
            Some(record.provider_id.as_str())
        );
    }
    let mut record_tools: Vec<&str> = records.iter().map(|record| record.tool.as_str()).collect();
    record_tools.sort();
    assert_eq!(record_tools, vec!["codex", "opencode"]);

    let persisted = super::storage::read_config().unwrap().terminal_syncs;
    assert_eq!(persisted.len(), 2, "one ledger entry per tool: {persisted:?}");
    for record in &records {
        assert!(
            persisted.iter().any(|entry| entry == record),
            "returned record must be persisted: {record:?}"
        );
    }
}

/// Re-syncing the same tools must not duplicate the ledger and must reuse the
/// provider ids that are already present in the terminal service provider list.
#[tokio::test]
async fn terminal_sync_with_seam_reuses_provider_per_tool_without_duplicate_ledger() {
    let _home = temp_home("terminal-sync-seam-idempotent");
    super::storage::write_config(&gateway_config(17688)).unwrap();

    let tools = vec!["opencode".to_string(), "codex".to_string()];
    let first_captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = first_captured.clone();
    let first = super::commands::apply_terminal_sync_with(
        &json!({ "providers": [] }),
        move |value| -> super::commands::UpsertFuture {
            let sink = sink.clone();
            Box::pin(async move {
                sink.lock().expect("capture lock").push(value);
                Ok(())
            })
        },
        tools.clone(),
    )
    .await
    .expect("first sync must succeed");
    assert_eq!(first.len(), 2);
    let first_payloads = first_captured.lock().unwrap().clone();
    assert_eq!(first_payloads.len(), 2);

    // Real life: the upserted records now appear in the terminal provider list.
    let providers_data = json!({ "providers": first_payloads.clone() });
    let second_captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = second_captured.clone();
    let second = super::commands::apply_terminal_sync_with(
        &providers_data,
        move |value| -> super::commands::UpsertFuture {
            let sink = sink.clone();
            Box::pin(async move {
                sink.lock().expect("capture lock").push(value);
                Ok(())
            })
        },
        tools,
    )
    .await
    .expect("second sync must succeed");
    assert_eq!(second.len(), 2);
    let second_payloads = second_captured.lock().unwrap().clone();

    for tool in ["opencode", "codex"] {
        let first_id = first_payloads
            .iter()
            .find(|value| value["tool"] == tool)
            .unwrap_or_else(|| panic!("first run missing {tool}"))["id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let second_id = second_payloads
            .iter()
            .find(|value| value["tool"] == tool)
            .unwrap_or_else(|| panic!("second run missing {tool}"))["id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        assert_eq!(
            first_id, second_id,
            "re-sync must reuse the same provider id for {tool}"
        );
    }

    let persisted = super::storage::read_config().unwrap().terminal_syncs;
    assert_eq!(
        persisted.len(),
        2,
        "re-syncing must not duplicate ledger entries: {persisted:?}"
    );
    for record in &second {
        assert!(persisted.iter().any(|entry| entry == record));
    }
}

/// When a tool already has a ledger entry whose `provider_id` matches a marked
/// gateway provider in the terminal list (with the same tool), the submitted
/// payload reuses that id instead of creating a new provider.
#[tokio::test]
async fn terminal_sync_with_seam_reuses_ledger_provider_marked_in_providers_data() {
    let _home = temp_home("terminal-sync-seam-ledger-reuse");
    let mut config = gateway_config(17688);
    for (provider_id, tool) in [("managed-oc", "opencode"), ("managed-cx", "codex")] {
        config.terminal_syncs.push(TerminalSyncRecord {
            provider_id: provider_id.to_string(),
            tool: tool.to_string(),
            synced_key_id: "k-old".to_string(),
            synced_base_url: "http://127.0.0.1:1".to_string(),
            synced_at: 9,
        });
    }
    super::storage::write_config(&config).unwrap();

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
        vec!["opencode".to_string(), "codex".to_string()],
    )
    .await
    .expect("sync must succeed");

    let submitted = captured.lock().unwrap().clone();
    assert_eq!(submitted.len(), 2);
    let by_tool = |tool: &str| {
        submitted
            .iter()
            .find(|value| value["tool"] == tool)
            .unwrap_or_else(|| panic!("missing submitted record for tool {tool}"))
    };
    assert_eq!(by_tool("opencode")["id"], "managed-oc");
    assert_eq!(by_tool("codex")["id"], "managed-cx");
    assert!(
        submitted.iter().all(|value| value["tool"] != "claude"),
        "an unrelated legacy provider must not be touched: {submitted:?}"
    );
    for record in &records {
        assert!(
            record.provider_id == "managed-oc" || record.provider_id == "managed-cx",
            "ledger provider ids must be reused: {record:?}"
        );
        assert_eq!(record.synced_key_id, "k1");
    }

    let persisted = super::storage::read_config().unwrap().terminal_syncs;
    assert_eq!(persisted.len(), 2, "no ledger duplicates: {persisted:?}");
    assert_eq!(
        persisted
            .iter()
            .filter(|record| record.tool == "opencode")
            .count(),
        1
    );
    assert_eq!(
        persisted
            .iter()
            .filter(|record| record.tool == "codex")
            .count(),
        1
    );
}

/// Stale-ledger protection: a ledger `provider_id` that matches a same-tool
/// provider WITHOUT the gateway marker belongs to the user and must not be
/// reused. With no marked gateway present, a fresh UUID v4 is generated and the
/// returned/persisted ledger records that fresh id.
#[tokio::test]
async fn terminal_sync_with_seam_does_not_reuse_unmarked_stale_ledger_provider() {
    let _home = temp_home("terminal-sync-seam-stale-ledger");
    let mut config = gateway_config(17688);
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "user-oc".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k-old".to_string(),
        synced_base_url: "http://127.0.0.1:1".to_string(),
        synced_at: 9,
    });
    super::storage::write_config(&config).unwrap();

    let providers_data = json!({ "providers": [unmarked_user_provider("user-oc", "opencode")] });
    let (submitted, records) =
        capture_terminal_sync(&providers_data, vec!["opencode".to_string()]).await;

    assert_eq!(submitted.len(), 1, "one upsert for the requested tool: {submitted:?}");
    let submitted_id = submitted[0]["id"].as_str().unwrap_or("");
    assert_ne!(
        submitted_id, "user-oc",
        "a stale ledger must not claim the user's own provider record: {submitted:?}"
    );
    let parsed = uuid::Uuid::parse_str(submitted_id)
        .expect("a stale ledger with no marked gateway must fall back to a fresh provider id");
    assert_eq!(
        parsed.get_version_num(),
        4,
        "fresh provider ids are UUID v4: {submitted_id}"
    );

    assert_eq!(
        records.len(),
        1,
        "exactly one ledger record for the requested tool"
    );
    assert_eq!(
        records[0].provider_id, submitted_id,
        "the returned ledger must record the submitted provider id"
    );
    let persisted = super::storage::read_config().unwrap().terminal_syncs;
    assert_eq!(
        persisted[0].provider_id, submitted_id,
        "the persisted ledger must record the submitted provider id"
    );
}

/// Stale-ledger protection with a marked gateway also present: the unmarked user
/// record must be skipped and the marked gateway reused instead.
#[tokio::test]
async fn terminal_sync_with_seam_prefers_marker_over_unmarked_stale_ledger() {
    let _home = temp_home("terminal-sync-seam-stale-ledger-marker");
    let mut config = gateway_config(17688);
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "user-oc".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k-old".to_string(),
        synced_base_url: "http://127.0.0.1:1".to_string(),
        synced_at: 9,
    });
    super::storage::write_config(&config).unwrap();

    let (submitted, records) = capture_terminal_sync(
        &terminal_providers_payload(),
        vec!["opencode".to_string()],
    )
    .await;

    assert_eq!(submitted.len(), 1);
    assert_eq!(
        submitted[0]["id"], "managed-oc",
        "the marked gateway must be reused instead of the user record: {submitted:?}"
    );
    assert_eq!(records[0].provider_id, "managed-oc");
}

/// Marker-only fallback: with no ledger entry at all, a same-tool provider that
/// carries the gateway marker is reused.
#[tokio::test]
async fn terminal_sync_with_seam_reuses_marker_provider_without_any_ledger() {
    let _home = temp_home("terminal-sync-seam-marker-no-ledger");
    super::storage::write_config(&gateway_config(17688)).unwrap();
    assert!(
        super::storage::read_config().unwrap().terminal_syncs.is_empty(),
        "precondition: no ledger entry"
    );

    let (submitted, records) = capture_terminal_sync(
        &terminal_providers_payload(),
        vec!["opencode".to_string()],
    )
    .await;

    assert_eq!(submitted.len(), 1);
    assert_eq!(
        submitted[0]["id"], "managed-oc",
        "the marked gateway must be reused without a ledger: {submitted:?}"
    );
    assert_eq!(records[0].provider_id, "managed-oc");
}

/// Marker-only fallback when the ledger points at an id absent from the terminal
/// provider list: the marked gateway is reused, not the missing id.
#[tokio::test]
async fn terminal_sync_with_seam_reuses_marker_provider_when_ledger_id_absent() {
    let _home = temp_home("terminal-sync-seam-marker-missing-ledger-id");
    let mut config = gateway_config(17688);
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "missing-oc".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k-old".to_string(),
        synced_base_url: "http://127.0.0.1:1".to_string(),
        synced_at: 9,
    });
    super::storage::write_config(&config).unwrap();

    let (submitted, records) = capture_terminal_sync(
        &terminal_providers_payload(),
        vec!["opencode".to_string()],
    )
    .await;

    assert_eq!(submitted.len(), 1);
    assert_eq!(
        submitted[0]["id"], "managed-oc",
        "an absent ledger id must fall back to the marked gateway: {submitted:?}"
    );
    assert_eq!(records[0].provider_id, "managed-oc");
}

/// Atomicity: when the injected upsert fails, the pipeline returns the error and
/// the persisted ledger keeps its previous value, so the ledger never claims a
/// sync that did not happen.
#[tokio::test]
async fn terminal_sync_with_seam_aborts_without_writing_ledger_on_upsert_error() {
    let _home = temp_home("terminal-sync-seam-error");
    let mut config = gateway_config(17688);
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
        &json!({ "providers": [] }),
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
        vec!["opencode".to_string(), "codex".to_string()],
    )
    .await
    .unwrap_err();

    assert!(
        error.contains("injected upsert failure"),
        "error must surface the upsert failure: {error}"
    );
    assert_eq!(*calls.lock().unwrap(), 2, "the failing target is attempted");

    let after = super::storage::read_config().unwrap().terminal_syncs;
    assert_eq!(after, before, "ledger must be unchanged when an upsert fails");
}

#[tokio::test]
async fn terminal_sync_with_seam_rejects_empty_target_tools() {
    let _home = temp_home("terminal-sync-seam-empty");
    super::storage::write_config(&gateway_config(17688)).unwrap();

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_upsert = calls.clone();
    let error = super::commands::apply_terminal_sync_with(
        &json!({ "providers": [] }),
        move |_value| -> super::commands::UpsertFuture {
            let calls = calls_for_upsert.clone();
            Box::pin(async move {
                *calls.lock().expect("call counter") += 1;
                Ok(())
            })
        },
        Vec::new(),
    )
    .await
    .unwrap_err();

    assert!(error.contains("target"), "error: {error}");
    assert_eq!(*calls.lock().unwrap(), 0, "nothing may be upserted");
}

#[tokio::test]
async fn terminal_sync_with_seam_rejects_unsupported_tools() {
    let _home = temp_home("terminal-sync-seam-unsupported");
    super::storage::write_config(&gateway_config(17688)).unwrap();

    let error = super::commands::apply_terminal_sync_with(
        &json!({ "providers": [] }),
        |_value| -> super::commands::UpsertFuture { Box::pin(async move { Ok(()) }) },
        vec!["claude".to_string()],
    )
    .await
    .unwrap_err();

    assert!(error.contains("unsupported"), "error: {error}");
}

#[tokio::test]
async fn terminal_sync_with_seam_requires_an_enabled_local_key() {
    let _home = temp_home("terminal-sync-seam-no-key");
    let mut config = gateway_config(17688);
    config.keys[0].enabled = false;
    super::storage::write_config(&config).unwrap();

    let error = super::commands::apply_terminal_sync_with(
        &json!({ "providers": [] }),
        |_value| -> super::commands::UpsertFuture { Box::pin(async move { Ok(()) }) },
        vec!["opencode".to_string()],
    )
    .await
    .unwrap_err();

    assert!(error.contains("local API key"), "error: {error}");
}

// ---------------------------------------------------------------------------
// Terminal targets projection: `terminal_targets_from` is the pure function the
// `api_fusion_terminal_targets` command delegates to. It must recognize a
// managed gateway only through the marker, never through a stale ledger that
// points at a user-owned provider.
// ---------------------------------------------------------------------------

fn target_for<'a>(
    targets: &'a [super::TerminalTarget],
    tool: &str,
) -> &'a super::TerminalTarget {
    targets
        .iter()
        .find(|target| target.tool == tool)
        .unwrap_or_else(|| panic!("missing terminal target for {tool}"))
}

/// One target per supported tool, in the canonical order, with the display
/// names the UI expects.
#[test]
fn terminal_targets_from_lists_supported_tools_in_order() {
    let config = FusionConfig::default();
    let targets = super::commands::terminal_targets_from(&config, &json!({ "providers": [] }));

    assert_eq!(targets.len(), 2, "one target per supported tool: {targets:?}");
    let tools: Vec<&str> = targets.iter().map(|target| target.tool.as_str()).collect();
    assert_eq!(tools, vec!["opencode", "codex"]);
    assert_eq!(targets[0].name, "OpenCode");
    assert_eq!(targets[1].name, "Codex");
}

/// No ledger and no marker provider: every target is unsynced and pending.
#[test]
fn terminal_targets_from_reports_unsynced_without_ledger_or_marker() {
    let mut config = FusionConfig::default();
    config.keys.push(key_named("k1", "local-key-123"));

    let targets = super::commands::terminal_targets_from(&config, &json!({ "providers": [] }));
    for target in &targets {
        assert_eq!(
            target.provider_id, None,
            "no provider may be claimed: {target:?}"
        );
        assert!(!target.synced, "must be unsynced: {target:?}");
        assert!(target.pending_sync, "must be pending when unsynced: {target:?}");
    }
}

/// A ledger id that matches a same-tool provider WITHOUT the marker is stale and
/// must not make the target look synced; the user record stays untouched.
#[test]
fn terminal_targets_from_does_not_claim_unmarked_user_provider() {
    let mut config = gateway_config(17688);
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "user-oc".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: "http://127.0.0.1:17688".to_string(),
        synced_at: 10,
    });
    let providers_data = json!({ "providers": [unmarked_user_provider("user-oc", "opencode")] });

    let targets = super::commands::terminal_targets_from(&config, &providers_data);
    let opencode = target_for(&targets, "opencode");
    assert_eq!(
        opencode.provider_id, None,
        "a stale ledger must not claim a user record: {opencode:?}"
    );
    assert!(!opencode.synced);
    assert!(opencode.pending_sync);
}

/// Marker provider present and the ledger matches the default key and local base
/// url: the target is synced and not pending.
#[test]
fn terminal_targets_from_marks_synced_when_marker_and_ledger_match() {
    let mut config = gateway_config(17688);
    config.default_key_id = Some("k1".to_string());
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "managed-oc".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: "http://127.0.0.1:17688".to_string(),
        synced_at: 10,
    });
    let providers_data =
        json!({ "providers": [managed_gateway_provider("managed-oc", "opencode")] });

    let targets = super::commands::terminal_targets_from(&config, &providers_data);
    let opencode = target_for(&targets, "opencode");
    assert_eq!(opencode.provider_id.as_deref(), Some("managed-oc"));
    assert!(opencode.synced);
    assert!(
        !opencode.pending_sync,
        "matching key and base url require no re-sync: {opencode:?}"
    );
    assert_eq!(opencode.synced_key_id.as_deref(), Some("k1"));
    assert_eq!(opencode.synced_at, Some(10));

    let codex = target_for(&targets, "codex");
    assert_eq!(codex.provider_id, None);
    assert!(!codex.synced);
    assert!(codex.pending_sync);
}

/// The ledger survives but the managed provider was deleted: the target is
/// unsynced and pending again.
#[test]
fn terminal_targets_from_reports_unsynced_when_managed_provider_deleted() {
    let mut config = gateway_config(17688);
    config.default_key_id = Some("k1".to_string());
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "managed-oc".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: "http://127.0.0.1:17688".to_string(),
        synced_at: 10,
    });

    let targets = super::commands::terminal_targets_from(&config, &json!({ "providers": [] }));
    let opencode = target_for(&targets, "opencode");
    assert_eq!(opencode.provider_id, None);
    assert!(!opencode.synced);
    assert!(
        opencode.pending_sync,
        "a deleted managed provider must require a re-sync: {opencode:?}"
    );
}

/// The managed provider is still there, but the ledger key id or base url
/// drifted: the target stays synced yet must be pending.
#[test]
fn terminal_targets_from_marks_pending_when_ledger_key_or_base_url_drifted() {
    let mut config = gateway_config(17688);
    config.default_key_id = Some("k1".to_string());
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "managed-oc".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k-old".to_string(),
        synced_base_url: "http://127.0.0.1:17688".to_string(),
        synced_at: 10,
    });
    let providers_data =
        json!({ "providers": [managed_gateway_provider("managed-oc", "opencode")] });

    let targets = super::commands::terminal_targets_from(&config, &providers_data);
    let opencode = target_for(&targets, "opencode");
    assert!(opencode.synced, "the record still points at the marked provider");
    assert!(
        opencode.pending_sync,
        "a drifted key id must require a re-sync: {opencode:?}"
    );

    config.terminal_syncs[0].synced_key_id = "k1".to_string();
    config.terminal_syncs[0].synced_base_url = "http://127.0.0.1:1".to_string();
    let targets = super::commands::terminal_targets_from(&config, &providers_data);
    let opencode = target_for(&targets, "opencode");
    assert!(opencode.synced, "the record still points at the marked provider");
    assert!(
        opencode.pending_sync,
        "a drifted base url must require a re-sync: {opencode:?}"
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
        display_name: None,
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
        display_name: None,
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
        display_name: None,
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
        display_name: None,
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

// ---------------------------------------------------------------------------
// Step 2 (20260916-api-fusion-upstream-retry): cooldown, retry headers, budget
// ---------------------------------------------------------------------------
//
// These tests exercise the observable HTTP boundary of `attempt_non_streaming`
// under a paused tokio clock (`test-util`), so a 120s retry budget is simulated
// instantly and no production delay is shortened for tests. The mock upstream
// runs on the SAME current runtime via `tokio::spawn`, never the tauri runtime.
// Each mock reply may carry extra response headers so `Retry-After` handling is
// driven through a real HTTP response.

/// One scripted mock reply: status, JSON body and optional extra headers.
#[derive(Clone)]
struct HeaderReply {
    status: u16,
    body: Value,
    headers: Vec<(String, String)>,
}

impl HeaderReply {
    fn new(status: u16, body: Value) -> Self {
        Self {
            status,
            body,
            headers: Vec::new(),
        }
    }

    fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }
}

/// Like `spawn_json_sequence_mock`, but every reply can carry response headers
/// (e.g. `retry-after-ms` / `retry-after`). The last reply repeats once the
/// sequence is exhausted. Returns the base URL and the accepted-request count.
async fn spawn_header_sequence_mock(replies: Vec<HeaderReply>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    let count = Arc::new(AtomicUsize::new(0));
    let count_for_server = count.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let count = count_for_server.clone();
            let replies = replies.clone();
            tokio::spawn(async move {
                let Ok(_request) = super::runtime_http::read_http_request(&mut stream).await else {
                    return;
                };
                let index = count.fetch_add(1, Ordering::SeqCst);
                let fallback = replies
                    .last()
                    .cloned()
                    .unwrap_or_else(|| HeaderReply::new(502, json!({})));
                let reply = replies.get(index).cloned().unwrap_or(fallback);
                let body = serde_json::to_vec(&reply.body).unwrap_or_default();
                let mut header = format!(
                    "HTTP/1.1 {} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
                    reply.status,
                    body.len()
                );
                for (name, value) in &reply.headers {
                    header.push_str(&format!("{name}: {value}\r\n"));
                }
                header.push_str("\r\n");
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(&body).await;
            });
        }
    });
    (format!("http://{}", addr), count)
}

/// A scripted streaming reply for the Step 3 retry/health boundary. Status
/// replies deliberately allow arbitrary bodies and headers so the relay sees
/// the same wire shape as a real upstream before it decides whether retrying is
/// allowed; SSE replies are the successful, completed-stream terminal case.
#[derive(Clone)]
enum StreamingReply {
    Status {
        status: u16,
        content_type: &'static str,
        body: Vec<u8>,
        headers: Vec<(&'static str, &'static str)>,
    },
    Sse(String),
}

async fn spawn_streaming_sequence_mock(
    replies: Vec<StreamingReply>,
) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    let count = Arc::new(AtomicUsize::new(0));
    let count_for_server = count.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let count = count_for_server.clone();
            let replies = replies.clone();
            tokio::spawn(async move {
                let Ok(_request) = super::runtime_http::read_http_request(&mut stream).await else {
                    return;
                };
                let index = count.fetch_add(1, Ordering::SeqCst);
                let reply = replies
                    .get(index)
                    .cloned()
                    .or_else(|| replies.last().cloned())
                    .unwrap_or(StreamingReply::Status {
                        status: 502,
                        content_type: "application/json",
                        body: b"{}".to_vec(),
                        headers: Vec::new(),
                    });
                let (status, content_type, body, headers) = match reply {
                    StreamingReply::Status {
                        status,
                        content_type,
                        body,
                        headers,
                    } => (status, content_type, body, headers),
                    StreamingReply::Sse(body) => (200, "text/event-stream", body.into_bytes(), Vec::new()),
                };
                let mut header = format!(
                    "HTTP/1.1 {status} OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n",
                    body.len()
                );
                for (name, value) in headers {
                    header.push_str(&format!("{name}: {value}\r\n"));
                }
                header.push_str("\r\n");
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(&body).await;
            });
        }
    });
    (format!("http://{}", addr), count)
}

async fn attempt_streaming_text(
    ordered: &[FusionUpstreamProvider],
    config: &mut FusionConfig,
) -> String {
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();
    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    super::runtime_http::attempt_streaming(
        &mut server,
        ordered,
        "/v1/chat/completions",
        &body,
        Some("local"),
        config,
        &HashMap::new(),
    )
    .await
    .expect("streaming attempt");
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.expect("read relay stream");
    String::from_utf8_lossy(&out).into_owned()
}

/// Run one non-streaming attempt on the paused clock and report the paused
/// elapsed time, so header-driven waits are observable without real waiting.
async fn attempt_non_streaming_timed(
    ordered: &[FusionUpstreamProvider],
    config: &mut FusionConfig,
) -> (super::runtime_http::HttpResponse, std::time::Duration) {
    let _ticker = spawn_paused_clock_ticker();
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();
    let started = tokio::time::Instant::now();
    let response = super::runtime_http::attempt_non_streaming(
        ordered,
        "/v1/chat/completions",
        &body,
        Some("local"),
        config,
        &HashMap::new(),
    )
    .await;
    (response, started.elapsed())
}

fn millis(value: u64) -> std::time::Duration {
    std::time::Duration::from_millis(value)
}

/// Keep a short timer armed on the paused clock so tokio's auto-advance moves
/// time in small bounded steps. Without it, auto-advance jumps straight to the
/// next timer while a real loopback round trip is in flight, which fires
/// reqwest's 10s connect / 60s read timeouts and turns every retry into a
/// spurious network error. The relay's own retry sleeps still complete, just in
/// bounded increments instead of one jump.
fn spawn_paused_clock_ticker() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(millis(1)).await;
        }
    })
}

/// AC-003 / REQ-003 RED: `retry-after-ms` wins over `retry-after` seconds. A
/// single 500 carrying both `retry-after-ms: 1500` and `retry-after: 9` must be
/// retried after ~1500ms, not 9s and not the default jittered backoff. Current
/// code reads no retry headers and waits its default ~2s, so this fails.
#[tokio::test(start_paused = true)]
async fn retry_policy_retry_after_ms_wins_over_seconds() {
    let _home = temp_home("retry-policy-header-priority");
    let (upstream_url, upstream_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "1500")
            .header("retry-after", "9"),
        HeaderReply::new(200, json!({"id": "recovered", "choices": []})),
    ])
    .await;

    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![a.clone()];

    let (response, elapsed) =
        attempt_non_streaming_timed(std::slice::from_ref(&a), &mut config).await;
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(response.status, 200, "must recover on the retry: {body_text}");
    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        2,
        "one initial attempt plus one header-timed retry"
    );
    assert!(
        elapsed >= millis(1400) && elapsed <= millis(1900),
        "retry-after-ms: 1500 must win over retry-after: 9, paused elapsed was {elapsed:?}"
    );
}

/// AC-003 / REQ-003 RED: an invalid high-priority `retry-after-ms` header keeps
/// looking at lower priorities, so `retry-after: 3` gives a ~3s wait instead of
/// the default backoff. Current code waits its default ~2s, so this fails.
#[tokio::test(start_paused = true)]
async fn retry_policy_invalid_retry_after_ms_falls_back_to_seconds_header() {
    let _home = temp_home("retry-policy-header-invalid-ms");
    let (upstream_url, upstream_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "not-a-number")
            .header("retry-after", "3"),
        HeaderReply::new(200, json!({"id": "recovered", "choices": []})),
    ])
    .await;

    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![a.clone()];

    let (response, elapsed) =
        attempt_non_streaming_timed(std::slice::from_ref(&a), &mut config).await;
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        2,
        "one initial attempt plus one header-timed retry"
    );
    assert_eq!(response.status, 200, "must recover on the retry: {body_text}");
    assert!(
        elapsed >= millis(2800) && elapsed <= millis(3400),
        "an invalid retry-after-ms must fall back to retry-after: 3, paused elapsed was {elapsed:?}"
    );
}

/// AC-003 / REQ-003 RED: a future HTTP date in `retry-after` is honored. The
/// header is built ~10s in the future, so the retry must wait clearly longer
/// than the default jittered backoff (~2s). Current code ignores the header.
#[tokio::test(start_paused = true)]
async fn retry_policy_future_http_date_is_honored() {
    let _home = temp_home("retry-policy-header-date");
    let when = chrono::Utc::now() + chrono::Duration::seconds(10);
    let http_date = when.format("%a, %d %b %Y %H:%M:%S GMT").to_string();

    let (upstream_url, upstream_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after", http_date),
        HeaderReply::new(200, json!({"id": "recovered", "choices": []})),
    ])
    .await;

    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![a.clone()];

    let (response, elapsed) =
        attempt_non_streaming_timed(std::slice::from_ref(&a), &mut config).await;
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        2,
        "one initial attempt plus one header-timed retry"
    );
    assert_eq!(response.status, 200, "must recover on the retry: {body_text}");
    assert!(
        elapsed >= millis(5000) && elapsed <= millis(11000),
        "a future HTTP date must be honored, paused elapsed was {elapsed:?}"
    );
}

/// AC-003 / REQ-003 RED: `retry-after-ms: 0` means retry immediately with no
/// backoff. Current code waits its default ~2s, so this fails.
#[tokio::test(start_paused = true)]
async fn retry_policy_zero_retry_after_ms_retries_immediately() {
    let _home = temp_home("retry-policy-header-zero");
    let (upstream_url, upstream_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "0"),
        HeaderReply::new(200, json!({"id": "recovered", "choices": []})),
    ])
    .await;

    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![a.clone()];

    let (response, elapsed) =
        attempt_non_streaming_timed(std::slice::from_ref(&a), &mut config).await;
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        2,
        "one initial attempt plus one immediate retry"
    );
    assert_eq!(response.status, 200, "must recover on the retry: {body_text}");
    assert!(
        elapsed < millis(500),
        "retry-after-ms: 0 must retry immediately, paused elapsed was {elapsed:?}"
    );
}

/// AC-001 / REQ-001 regression: the initial pass must reach a healthy later
/// candidate before any retry wait, even when the first candidate advertises a
/// long cooldown. A is never retried because B answers immediately.
#[tokio::test]
async fn retry_policy_initial_pass_does_not_wait_for_cooldown() {
    let _home = temp_home("retry-policy-initial-no-wait");
    let (a_url, a_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "9000"),
    ])
    .await;
    let (b_url, b_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(200, json!({"id": "from-b"}))]).await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        super::runtime_http::attempt_non_streaming(
            &[a.clone(), b.clone()],
            "/v1/chat/completions",
            &serde_json::to_vec(&json!({"model": "local"})).unwrap(),
            Some("local"),
            &mut config,
            &HashMap::new(),
        ),
    )
    .await
    .expect("the initial fallback must not wait for A's 9s cooldown");
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(response.status, 200, "B must answer the initial pass: {body_text}");
    assert!(
        body_text.contains("from-b"),
        "response must carry B's body: {body_text}"
    );
    assert_eq!(
        a_requests.load(Ordering::SeqCst),
        1,
        "A must not be retried before B answers"
    );
    assert_eq!(b_requests.load(Ordering::SeqCst), 1, "B must be tried exactly once");
}

/// AC-003 / REQ-003 RED: a provider cooling down for 9s must not block a second
/// provider whose own header is ready after ~1500ms. A is tried once, B is
/// retried at its own deadline and answers, so A is never retried. Current code
/// ignores both headers, retries A first six times, and takes ~60s, so this
/// fails on the attempt count and elapsed time.
#[tokio::test(start_paused = true)]
async fn retry_policy_cooling_provider_does_not_block_ready_candidate() {
    let _home = temp_home("retry-policy-cooling-does-not-block");
    let (a_url, a_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "9000"),
    ])
    .await;
    let (b_url, b_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "1500"),
        HeaderReply::new(200, json!({"id": "from-b"})),
    ])
    .await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let (response, elapsed) =
        attempt_non_streaming_timed(&[a.clone(), b.clone()], &mut config).await;
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(response.status, 200, "the ready candidate must recover: {body_text}");
    assert_eq!(
        b_requests.load(Ordering::SeqCst),
        2,
        "B must be retried at its own 1500ms deadline"
    );
    assert_eq!(
        a_requests.load(Ordering::SeqCst),
        1,
        "A's 9s cooldown must not be retried while B is ready sooner"
    );
    assert!(
        elapsed >= millis(1400) && elapsed <= millis(1900),
        "the ready candidate's deadline must govern the wait, paused elapsed was {elapsed:?}"
    );
}

/// AC-002 / REQ-002 regression: a provider that keeps failing is contacted at
/// most six times in one request (one initial plus five bounded retries).
#[tokio::test(start_paused = true)]
async fn retry_policy_single_provider_is_attempted_at_most_six_times() {
    let _home = temp_home("retry-policy-six-attempts");
    let (upstream_url, upstream_requests) = spawn_header_sequence_mock(vec![HeaderReply::new(
        500,
        json!({"error": {"message": "always failing"}}),
    )])
    .await;

    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![a.clone()];

    let (response, _elapsed) =
        attempt_non_streaming_timed(std::slice::from_ref(&a), &mut config).await;

    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        6,
        "one provider is attempted at most six times in a request"
    );
    assert_eq!(
        response.status, 502,
        "exhausted candidates return the all-unavailable error"
    );
}

/// AC-003 / REQ-003 RED: the cumulative actual wait is capped at 120s. With a
/// 60s `retry-after-ms` on every failure, only two waits (60s + 60s = 120s,
/// exactly the budget) are allowed, so the provider is attempted three times
/// and then the next 60s wait exceeds the remaining budget. Current code
/// ignores the header, uses the default backoff and attempts six times in
/// ~60s, so this fails.
#[tokio::test(start_paused = true)]
async fn retry_policy_stops_before_wait_exceeds_120s_budget() {
    let _home = temp_home("retry-policy-budget-120s");
    let (upstream_url, upstream_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "always failing"}}))
            .header("retry-after-ms", "60000"),
    ])
    .await;

    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![a.clone()];

    let (response, elapsed) =
        attempt_non_streaming_timed(std::slice::from_ref(&a), &mut config).await;

    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        3,
        "only waits totalling the 120s budget are allowed (attempt 1 + two 60s waits)"
    );
    assert_eq!(
        response.status, 502,
        "budget exhaustion returns the all-unavailable error"
    );
    assert!(
        elapsed >= millis(119_000) && elapsed <= millis(121_000),
        "the request must stop at the 120s wait budget, paused elapsed was {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// Step 3 (20260916-api-fusion-upstream-retry): streaming retry and health RED
// ---------------------------------------------------------------------------

/// REQ-002/REQ-003/REQ-005: before a stream emits any bytes, a retryable 503
/// with `retry-after-ms: 0` is retried immediately. Only the completed SSE
/// stream is success: it clears previously seeded health without first counting
/// the transient attempt as a separate inbound-request failure.
#[tokio::test]
async fn retry_stream_recovers_after_zero_cooldown_and_completed_sse_clears_health() {
    let _home = temp_home("retry-stream-recovery-health");
    let (upstream_url, attempts) = spawn_streaming_sequence_mock(vec![
        StreamingReply::Status {
            status: 503,
            content_type: "application/json",
            body: br#"{"error":{"message":"busy"}}"#.to_vec(),
            headers: vec![("retry-after-ms", "0")],
        },
        StreamingReply::Sse("data: {\"id\":\"recovered-stream\"}\n\ndata: [DONE]\n\n".to_string()),
    ])
    .await;

    let mut provider =
        upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    provider.consecutive_failures = 2;
    provider.last_error_at = Some(1);
    let mut config = FusionConfig::default();
    config.providers.push(provider.clone());

    let text = attempt_streaming_text(std::slice::from_ref(&provider), &mut config).await;

    assert_eq!(
        attempts.load(Ordering::SeqCst),
        2,
        "a pre-output 503 with retry-after-ms: 0 must be retried immediately"
    );
    assert!(text.contains("recovered-stream"), "completed retry stream: {text}");
    assert!(text.contains("data: [DONE]"), "completed retry stream: {text}");
    let stored = config.providers.iter().find(|item| item.id == "a").unwrap();
    assert_eq!(
        stored.consecutive_failures, 0,
        "only the completed stream is success for the inbound-request health result"
    );
    assert!(
        !stored.auto_disabled,
        "a recovered stream must not auto-disable a provider with seeded failures"
    );
}

/// REQ-002/REQ-005: a permanently failing stream gets one initial try plus at
/// most five retries, while provider health records that whole inbound request
/// once. Its retry header makes the count/health regression immediate; retry
/// delay semantics are covered by the dedicated retry-policy tests.
#[tokio::test]
async fn retry_stream_persistent_503_attempts_six_times_and_counts_health_once() {
    let _home = temp_home("retry-stream-six-attempts-health");
    let (upstream_url, attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Status {
        status: 503,
        content_type: "application/json",
        body: br#"{"error":{"message":"still busy"}}"#.to_vec(),
        headers: vec![("retry-after-ms", "0")],
    }])
    .await;

    let provider =
        upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers.push(provider.clone());
    let text = attempt_streaming_text(std::slice::from_ref(&provider), &mut config).await;

    assert_eq!(
        attempts.load(Ordering::SeqCst),
        6,
        "one initial stream attempt plus five bounded retries"
    );
    assert!(text.contains("all_providers_unavailable"), "exhausted stream: {text}");
    assert!(text.contains("data: [DONE]"), "exhausted stream: {text}");
    let stored = config.providers.iter().find(|item| item.id == "a").unwrap();
    assert_eq!(
        stored.consecutive_failures, 1,
        "six upstream failures in one inbound request count once"
    );
    assert!(!stored.auto_disabled, "one failed request is below the threshold");
}

/// REQ-004/REQ-005: a rate-limited streaming candidate may yield to a healthy
/// candidate, but 429 itself never contributes provider health failure.
#[tokio::test]
async fn retry_stream_429_switches_without_counting_provider_health() {
    let _home = temp_home("retry-stream-429-no-health");
    let (limited_url, limited_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Status {
        status: 429,
        content_type: "text/html",
        body: b"<html>rate limited</html>".to_vec(),
        headers: Vec::new(),
    }])
    .await;
    let (healthy_url, healthy_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Sse(
        "data: {\"id\":\"healthy-after-429\"}\n\ndata: [DONE]\n\n".to_string(),
    )])
    .await;

    let limited = upstream_provider("a", "Limited", &limited_url, "sk", Some("remote-default"));
    let healthy = upstream_provider("b", "Healthy", &healthy_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![limited.clone(), healthy.clone()];

    let text = attempt_streaming_text(&[limited, healthy], &mut config).await;

    assert_eq!(limited_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(healthy_attempts.load(Ordering::SeqCst), 1);
    assert!(text.contains("healthy-after-429"), "stream: {text}");
    let stored = config.providers.iter().find(|item| item.id == "a").unwrap();
    assert_eq!(stored.consecutive_failures, 0, "429 must not count toward health");
    assert!(!stored.auto_disabled, "429 must not auto-disable the provider");
}

/// REQ-004: status semantics outrank response shape. HTML authentication
/// failures disable immediately and then the stream can continue from a later
/// candidate.
#[tokio::test]
async fn retry_stream_html_401_and_403_disable_immediately() {
    for status in [401u16, 403u16] {
        let _home = temp_home(&format!("retry-stream-html-auth-{status}"));
        let (auth_url, auth_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Status {
            status,
            content_type: "text/html",
            body: b"<html>denied</html>".to_vec(),
            headers: Vec::new(),
        }])
        .await;
        let (healthy_url, healthy_attempts) = spawn_streaming_sequence_mock(vec![
            StreamingReply::Sse(format!("data: {{\"id\":\"healthy-after-{status}\"}}\n\ndata: [DONE]\n\n")),
        ])
        .await;
        let auth = upstream_provider("a", "Auth", &auth_url, "sk", Some("remote-default"));
        let healthy = upstream_provider("b", "Healthy", &healthy_url, "sk", Some("remote-default"));
        let mut config = FusionConfig::default();
        config.providers = vec![auth.clone(), healthy.clone()];

        let text = attempt_streaming_text(&[auth, healthy], &mut config).await;

        assert_eq!(auth_attempts.load(Ordering::SeqCst), 1, "status {status}");
        assert_eq!(healthy_attempts.load(Ordering::SeqCst), 1, "status {status}");
        assert!(text.contains(&format!("healthy-after-{status}")), "stream: {text}");
        let stored = config.providers.iter().find(|item| item.id == "a").unwrap();
        assert!(stored.auto_disabled, "HTML {status} must immediately disable");
        assert!(
            stored.disabled_reason.as_deref().unwrap_or("").contains(&status.to_string()),
            "HTML {status} disable reason: {:?}",
            stored.disabled_reason
        );
    }
}

/// REQ-004: a 404 is a per-request skip, not a health failure. Every later
/// candidate in the initial stream pass is still reached once.
#[tokio::test]
async fn retry_stream_404_traverses_each_candidate_once_without_health_failure() {
    let _home = temp_home("retry-stream-404-traverse");
    let (a_url, a_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Status {
        status: 404,
        content_type: "text/html",
        body: b"<html>not found a</html>".to_vec(),
        headers: Vec::new(),
    }])
    .await;
    let (b_url, b_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Status {
        status: 404,
        content_type: "text/html",
        body: b"<html>not found b</html>".to_vec(),
        headers: Vec::new(),
    }])
    .await;
    let a = upstream_provider("a", "A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "B", &b_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let text = attempt_streaming_text(&[a, b], &mut config).await;

    assert_eq!(a_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(b_attempts.load(Ordering::SeqCst), 1);
    assert!(text.contains("all_providers_unavailable"), "stream: {text}");
    for provider in &config.providers {
        assert_eq!(
            provider.consecutive_failures, 0,
            "404 must not count for {}",
            provider.id
        );
        assert!(!provider.auto_disabled, "404 must not disable {}", provider.id);
    }
}

/// REQ-004: a 413 HTML response is a caller error, so the streaming boundary
/// returns the original status and bytes without trying a fallback provider.
#[tokio::test]
async fn retry_stream_html_413_returns_unchanged_without_fallback() {
    let _home = temp_home("retry-stream-html-413");
    let body = b"<html>payload too large</html>".to_vec();
    let (rejected_url, rejected_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Status {
        status: 413,
        content_type: "text/html",
        body: body.clone(),
        headers: Vec::new(),
    }])
    .await;
    let (fallback_url, fallback_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Sse(
        "data: {\"id\":\"must-not-run\"}\n\ndata: [DONE]\n\n".to_string(),
    )])
    .await;
    let rejected = upstream_provider("a", "Rejected", &rejected_url, "sk", Some("remote-default"));
    let fallback = upstream_provider("b", "Fallback", &fallback_url, "sk", Some("remote-default"));
    let mut config = FusionConfig::default();
    config.providers = vec![rejected.clone(), fallback.clone()];

    let text = attempt_streaming_text(&[rejected, fallback], &mut config).await;

    assert!(text.starts_with("HTTP/1.1 413"), "response: {text}");
    assert!(text.ends_with(std::str::from_utf8(&body).unwrap()), "response: {text}");
    assert_eq!(rejected_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(fallback_attempts.load(Ordering::SeqCst), 0, "413 must not switch");
    assert_eq!(config.providers[0].consecutive_failures, 0, "413 must not count");
}

// ---------------------------------------------------------------------------
// Step 3 (20260916-api-fusion-upstream-retry): downstream cancellation RED
// ---------------------------------------------------------------------------

/// Send a complete request over a real loopback connection. The caller closes
/// the returned client only after the mock confirms the relay is in the desired
/// pending state. The handler stays on the current tokio runtime so disconnect
/// detection is exercised at the same boundary as `run_server` without starting
/// the shared runtime state.
async fn spawn_handle_connection(
    wants_stream: bool,
) -> (TcpStream, tokio::task::JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind relay loopback");
    let addr = listener.local_addr().expect("relay loopback address");
    let accept = tokio::spawn(async move {
        listener
            .accept()
            .await
            .expect("accept relay loopback")
            .0
    });
    let mut client = TcpStream::connect(addr).await.expect("connect relay loopback");
    let server = accept.await.expect("relay accept task");
    let handler = tokio::spawn(super::runtime_http::handle_connection(server));
    tokio::task::yield_now().await;
    let body = serde_json::to_vec(&json!({"model": "local", "stream": wants_stream}))
        .expect("encode relay request");
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: Bearer local-key\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let mut frame = request.into_bytes();
    frame.extend_from_slice(&body);
    client.write_all(&frame).await.expect("write complete relay request");
    client.flush().await.expect("flush relay request");
    (client, handler)
}

async fn wait_for_upstream_attempts(
    attempts: &AtomicUsize,
    expected: usize,
    handler: &mut tokio::task::JoinHandle<Result<(), String>>,
) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while attempts.load(Ordering::SeqCst) < expected {
            if handler.is_finished() {
                panic!("handler exited before upstream request: {:?}", handler.await);
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("upstream request must arrive");
}

/// REQ-006 / AC-006 RED: after the client has fully disconnected during a
/// retry cooldown, the relay must leave the pending delay and must not issue a
/// retry. This covers both JSON and SSE request modes through the real TCP
/// handler boundary. The current handler sleeps until the cooldown expires.
#[tokio::test]
async fn retry_cancel_disconnect_during_retry_delay_exits_without_further_upstream_attempts() {
    let mut failures = Vec::new();
    for wants_stream in [false, true] {
        let _home = temp_home(if wants_stream {
            "retry-cancel-delay-stream"
        } else {
            "retry-cancel-delay-json"
        });
        let (upstream_url, attempts) = spawn_header_sequence_mock(vec![
            HeaderReply::new(503, json!({"error": {"message": "busy"}}))
                .header("retry-after-ms", "250"),
        ])
        .await;
        let mut config = FusionConfig::default();
        config.keys.push(key_named("k1", "local-key"));
        config.providers.push(upstream_provider(
            "a",
            "Delayed Provider",
            &upstream_url,
            "sk",
            Some("remote-default"),
        ));
        super::storage::write_config(&config).expect("write relay config");

        let (client, mut handler) = spawn_handle_connection(wants_stream).await;
        wait_for_upstream_attempts(&attempts, 1, &mut handler).await;
        drop(client);

        let exited = tokio::time::timeout(std::time::Duration::from_millis(100), &mut handler).await;
        // Let a non-cancelling handler reach the retry deadline so the second
        // assertion proves the request did not continue upstream after close.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let observed_attempts = attempts.load(Ordering::SeqCst);
        if exited.is_err() {
            handler.abort();
            let _ = handler.await;
        }

        if exited.is_err() {
            failures.push(format!(
                "handler remained pending during retry delay after disconnect (stream={wants_stream})"
            ));
        }
        if observed_attempts != 1 {
            failures.push(format!(
                "disconnect issued {observed_attempts} upstream attempts during retry delay (stream={wants_stream})"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

/// REQ-006 / AC-006 RED: a client disconnect while an upstream response is
/// still pending cancels the relay immediately. The held upstream only replies
/// after the prompt-exit observation, so this cannot be satisfied by waiting
/// for the upstream read timeout. Non-streaming and SSE use the same public
/// connection boundary.
#[tokio::test]
async fn retry_cancel_disconnect_while_upstream_waits_exits_without_further_upstream_attempts() {
    let mut failures = Vec::new();
    for wants_stream in [false, true] {
        let _home = temp_home(if wants_stream {
            "retry-cancel-upstream-stream"
        } else {
            "retry-cancel-upstream-json"
        });
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind held upstream");
        let upstream_url = format!("http://{}", listener.local_addr().expect("upstream address"));
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_server = attempts.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let upstream = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept held upstream");
            super::runtime_http::read_http_request(&mut stream)
                .await
                .expect("read held upstream request");
            attempts_for_server.fetch_add(1, Ordering::SeqCst);
            let _ = entered_tx.send(());
            let _ = release_rx.await;
            let (content_type, body) = if wants_stream {
                ("text/event-stream", b"data: {\"id\":\"late\"}\n\ndata: [DONE]\n\n".as_slice())
            } else {
                ("application/json", br#"{"id":"late"}"#.as_slice())
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.write_all(body).await;
        });

        let mut config = FusionConfig::default();
        config.keys.push(key_named("k1", "local-key"));
        config.providers.push(upstream_provider(
            "a",
            "Held Provider",
            &upstream_url,
            "sk",
            Some("remote-default"),
        ));
        super::storage::write_config(&config).expect("write relay config");

        let (client, mut handler) = spawn_handle_connection(wants_stream).await;
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

        let exited = tokio::time::timeout(std::time::Duration::from_millis(100), &mut handler).await;
        let observed_attempts = attempts.load(Ordering::SeqCst);
        let _ = release_tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), upstream).await;
        if exited.is_err() {
            handler.abort();
            let _ = handler.await;
        }

        if exited.is_err() {
            failures.push(format!(
                "handler remained pending while upstream waited after disconnect (stream={wants_stream})"
            ));
        }
        if observed_attempts != 1 {
            failures.push(format!(
                "disconnect issued {observed_attempts} upstream attempts while upstream waited (stream={wants_stream})"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

/// Spec F4: deleting the current default key must fall through to the next
/// enabled key in list order instead of leaving a dangling default id.
#[test]
fn deleting_the_default_key_advances_to_the_next_enabled_key() {
    with_temp_home("delete-default-key-advance", |_home| {
        super::commands::api_fusion_upsert_key(FusionKey {
            id: "k1".to_string(),
            label: "K1".to_string(),
            value: "v1".to_string(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        super::commands::api_fusion_upsert_key(FusionKey {
            id: "k2".to_string(),
            label: "K2".to_string(),
            value: "v2".to_string(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        let defaulted = super::commands::api_fusion_set_default_key("k2".to_string()).unwrap();
        assert_eq!(defaulted.default_key_id.as_deref(), Some("k2"));

        let after_delete = super::commands::api_fusion_delete_key("k2".to_string()).unwrap();
        assert_eq!(
            after_delete.default_key_id.as_deref(),
            Some("k1"),
            "deleting the default key must advance to the next enabled key"
        );
        assert!(
            after_delete.keys.iter().any(|key| key.id == "k1"),
            "the surviving key must remain in the config"
        );
        assert!(
            !after_delete.keys.iter().any(|key| key.id == "k2"),
            "the deleted key must be gone from the config"
        );
    });
}

/// Standards S2: `api_fusion_save_config` must normalize brand-new keys whose
/// submitted value is blank or the UI mask placeholder, generating a real
/// `sk-fusion-` secret instead of persisting `""` or `"********"`.
#[tokio::test]
async fn save_config_generates_secret_for_new_keys_with_blank_or_masked_value() {
    let _home = temp_home("save-config-key-normalize");

    let mut config = FusionConfig::default();
    // Keep the listener off so the test never binds a real port.
    config.enabled = false;
    // Providers intentionally empty: only key normalization is under test.
    config.keys.push(FusionKey {
        id: "brand-new-blank".to_string(),
        label: "Brand New Blank".to_string(),
        value: String::new(),
        enabled: true,
        created_at: 1,
    });
    config.keys.push(FusionKey {
        id: "brand-new-masked".to_string(),
        label: "Brand New Masked".to_string(),
        value: "********".to_string(),
        enabled: true,
        created_at: 2,
    });

    let saved = super::commands::api_fusion_save_config(config)
        .await
        .expect("save config");

    let blank = saved
        .keys
        .iter()
        .find(|key| key.id == "brand-new-blank")
        .expect("blank-valued key must be persisted");
    assert!(
        blank.value.starts_with("sk-fusion-"),
        "a brand-new blank-valued key must receive a generated secret, got {:?}",
        blank.value
    );
    assert_ne!(
        blank.value, "********",
        "the mask placeholder must never be stored as a key value"
    );
    assert!(
        !blank.value.trim().is_empty(),
        "a generated secret must not be empty"
    );

    let masked = saved
        .keys
        .iter()
        .find(|key| key.id == "brand-new-masked")
        .expect("masked-valued key must be persisted");
    assert!(
        masked.value.starts_with("sk-fusion-"),
        "a brand-new masked key must receive a generated secret, got {:?}",
        masked.value
    );
    assert_ne!(
        masked.value, "********",
        "the mask placeholder must never be stored as a key value"
    );
    assert!(
        !masked.value.trim().is_empty(),
        "a generated secret must not be empty"
    );
}
