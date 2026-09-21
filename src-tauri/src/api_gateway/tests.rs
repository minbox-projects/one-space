use super::commands::{build_gateway_provider, default_key_for_sync, terminal_sync_pending};
use super::selection::{
    candidate_providers, classify_failure, manual_reenable, pick_candidate, register_failure,
    register_success, resolve_model_for_protocol, set_user_enabled, FailureClass, ModelResolution,
};
use super::storage::{config_path, resolve_default_key_id};
use super::{
    compute_cost, compute_cost_at_time, extract_upstream_error_text, is_off_peak, match_price_for_provider, normalize_retention_days, resolve_range, usage_tokens_from_value,
    sanitize_error_text, validate_retention_days, GatewayConfig, GatewayKey, GatewayUpstreamProvider, LogFilter,
    ModelMapping, ModelPrice, OffPeakPrice, SseUsageAccumulator, TerminalSyncRecord, TimeRange, UpstreamProtocol,
    UsageLogRecord, UsageLogStore, UsageResult, UsageTokens, DEFAULT_USAGE_RETENTION_DAYS,
    USAGE_LOG_PAGE_SIZE,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

mod templates;

fn make_temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "onespace-api-gateway-{}-{}",
        name,
        uuid::Uuid::new_v4()
    ))
}

/// Thread-local `HOME` isolation for pure config/usage tests. Kept as a
/// separate wrapper because the callers only need a scoped temp home and never
/// touch the global server. See [`isolated_temp_home`].
fn with_temp_home<T>(name: &str, f: impl FnOnce(&Path) -> T) -> T {
    let home = isolated_temp_home(name);
    f(&home.path)
}

/// Thread-local `HOME` isolation for tests whose code path resolves
/// `get_app_dir()`/`get_data_dir()` on the test thread and never drives the
/// global server. It holds no global mutex, so these tests may run in parallel
/// with each other and with the serialized server tests.
struct IsolatedTempHome {
    path: PathBuf,
    _guard: crate::config::test_home::TestHomeGuard,
}

impl Drop for IsolatedTempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn isolated_temp_home(name: &str) -> IsolatedTempHome {
    let path = make_temp_dir(name);
    fs::create_dir_all(&path).expect("create temp home");
    let guard = crate::config::test_home::TestHomeGuard::set(&path);
    IsolatedTempHome {
        path,
        _guard: guard,
    }
}

fn key(id: &str, enabled: bool) -> GatewayKey {
    GatewayKey {
        id: id.to_string(),
        label: id.to_string(),
        value: format!("value-{id}"),
        enabled,
        created_at: 1,
    }
}

fn provider(id: &str) -> GatewayUpstreamProvider {
    GatewayUpstreamProvider {
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
        template_id: None,
        ignored_models: Vec::new(),
    }
}

#[test]
fn gateway_config_round_trips_and_encrypts_secrets_on_disk() {
    with_temp_home("roundtrip", |_home| {
        let mut config = GatewayConfig::default();
        let mut first = provider("p1");
        first.api_key = "sk-super-secret-123".to_string();
        first.default_model = Some("remote-default".to_string());
        first.mappings = vec![ModelMapping {
            local_model: "local-a".to_string(),
            upstream_model: "remote-a".to_string(),
            protocol: None,
            display_name: None,
            enabled: true,
            reasoning_efforts: Vec::new(),
        }];
        config.providers.push(first);
        config.keys.push(GatewayKey {
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
/// write/read and stays optional for older rows.
#[test]
fn model_mapping_display_name_round_trips_and_stays_optional() {
    with_temp_home("mapping-display-name", |_home| {
        let mut config = GatewayConfig::default();
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
    let plain: ModelMapping =
        serde_json::from_value(json!({"local_model": "local-a", "upstream_model": "remote-a"}))
            .expect("a mapping without display_name must deserialize");
    assert_eq!(plain.display_name, None);

    let named: ModelMapping = serde_json::from_value(json!({
        "local_model": "local-a",
        "upstream_model": "remote-a",
        "display_name": "GPT-4o",
    }))
    .expect("a mapping with display_name must deserialize");
    assert_eq!(named.display_name.as_deref(), Some("GPT-4o"));

    let encoded = serde_json::to_value(&plain).expect("serialize mapping");
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
        let mut config = GatewayConfig::default();
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

/// Syncing must carry the default local key the UI shows: when the stored
/// default points at a disabled key (or is absent), the sync falls through to
/// the next enabled key instead of failing.
#[test]
fn default_key_for_sync_falls_through_to_next_enabled_key() {
    let mut stale = GatewayConfig::default();
    stale.keys = vec![key("k1", false), key("k2", true), key("k3", true)];
    stale.default_key_id = Some("k1".to_string());
    let (id, value) = default_key_for_sync(&stale).expect("stale default must fall through");
    assert_eq!(id, "k2");
    assert_eq!(value, "value-k2");

    let mut unset = GatewayConfig::default();
    unset.keys = vec![key("k1", true), key("k2", true)];
    unset.default_key_id = None;
    let (id, _) = default_key_for_sync(&unset).expect("absent default must resolve");
    assert_eq!(id, "k1");

    let mut none_enabled = GatewayConfig::default();
    none_enabled.keys = vec![key("k1", false)];
    none_enabled.default_key_id = Some("k1".to_string());
    assert!(
        default_key_for_sync(&none_enabled).is_err(),
        "no enabled key must still fail sync"
    );
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
        enabled: true,
        reasoning_efforts: Vec::new(),
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
        enabled: true,
        reasoning_efforts: Vec::new(),
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
        enabled: true,
        reasoning_efforts: Vec::new(),
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
        let mut config = GatewayConfig::default();
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

/// Process-wide `HOME` isolation shared through the global
/// `crate::lock_test_home_env` mutex.
///
/// Keep using this helper for tests that drive the global API-gateway server
/// (`start_server`/`stop_server`/`api_gateway_start`/`api_gateway_stop`/
/// `api_gateway_save_config`). The server reads its config from worker threads
/// that cannot see the thread-local override, and its `RUNNING_SERVER` state is
/// a process-wide singleton, so those tests must stay serialized.
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

async fn call_gateway(
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
    let response = request.send().await.expect("gateway request");
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let text = response.text().await.expect("gateway body");
    (status, content_type, text)
}

fn key_named(id: &str, value: &str) -> GatewayKey {
    GatewayKey {
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
) -> GatewayUpstreamProvider {
    GatewayUpstreamProvider {
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
        template_id: None,
        ignored_models: Vec::new(),
    }
}

fn mapping(local_model: &str, upstream_model: &str, display_name: Option<&str>) -> ModelMapping {
    ModelMapping {
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        protocol: None,
        display_name: display_name.map(str::to_string),
        enabled: true,
        reasoning_efforts: Vec::new(),
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

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "upstream-secret", None);
    provider.mappings = vec![ModelMapping {
        local_model: "local-a".to_string(),
        upstream_model: "remote-a".to_string(),
        protocol: None,
        display_name: None,
        enabled: true,
        reasoning_efforts: Vec::new(),
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
    let (status, _content_type, text) = call_gateway(
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

    let mut config = GatewayConfig::default();
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

    let (status, _, text) = call_gateway(
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

    let (models_status, _, _) = call_gateway(
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

    let mut config = GatewayConfig::default();
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

    let (status, _content_type, text) = call_gateway(
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
        enabled: true,
        reasoning_efforts: Vec::new(),
    }];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
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

    let (status, _content_type, text) = call_gateway(
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
// Step 1 (20260916-api-gateway-upstream-retry): bounded retry / recovery
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

/// AC-003 / REQ-002 regression (migrated to two candidates): provider A fails
/// the first attempt with a retryable 500 and succeeds on the second, while
/// provider B advertises a far-later cooldown. A must still be retried at its
/// own deadline and recover, so multi-candidate scheduling keeps this
/// observation instead of losing it to the single-candidate fast path.
/// Observable boundary: the full HTTP response returned by
/// `attempt_non_streaming` plus each provider's exact upstream request count.
#[tokio::test(start_paused = true)]
async fn attempt_non_streaming_retries_provider_after_500_then_succeeds() {
    let _home = isolated_temp_home("retry-recovery-two-providers");
    let (a_url, a_requests) = spawn_json_sequence_mock(vec![
        (500, json!({"error": {"message": "temporarily unavailable"}})),
        (200, json!({"id": "recovered", "choices": []})),
    ])
    .await;
    let (b_url, b_requests) = spawn_header_sequence_mock(vec![HeaderReply::new(
        500,
        json!({"error": {"message": "busy"}}),
    )
    .header("retry-after-ms", "60000")])
    .await;

    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let (response, _default_backoff_elapsed) = attempt_non_streaming_paused(
        &[a.clone(), b.clone()],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
    )
    .await;

    assert_eq!(
        a_requests.load(Ordering::SeqCst),
        2,
        "A makes one initial attempt plus exactly one bounded retry"
    );
    assert_eq!(
        b_requests.load(Ordering::SeqCst),
        1,
        "B's far-later cooldown must not preempt A's retry"
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
    let _home = isolated_temp_home(&format!("truncated-auth-non-streaming-{status}"));
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

    let mut config = GatewayConfig::default();
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

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        &[auth, fallback],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;

    assert_eq!(response.status, 200);
    assert!(
        String::from_utf8_lossy(&response.body).contains("healthy-fallback"),
        "healthy fallback must answer the request"
    );
    assert_eq!(auth_requests.lock().unwrap().len(), 1);
    assert_eq!(fallback_requests.lock().unwrap().len(), 1);
    assert_eq!(
        attempts.len(),
        2,
        "one entry per completed attempt of the request"
    );
    assert_eq!(attempts[0].provider_id, "auth");
    assert_eq!(attempts[0].upstream_model, "remote-default");
    assert_eq!(attempts[0].status, status);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(
        attempts[0].error_message, None,
        "the truncated error body is unreadable, so no upstream message is stored"
    );
    assert!(attempts[0].usage.is_none());
    assert!(attempts[0].duration_ms >= 1, "each attempt times itself");
    assert_eq!(attempts[1].provider_id, "fallback");
    assert_eq!(attempts[1].upstream_model, "remote-default");
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);
    assert_eq!(attempts[1].error_message, None);
    assert!(attempts[1].duration_ms >= 1);

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
    let _home = isolated_temp_home(&format!("truncated-auth-streaming-{status}"));
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

    let mut config = GatewayConfig::default();
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
    let mut attempts = Vec::new();
    super::runtime_http::attempt_streaming(
        &mut server,
        &[auth, fallback],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
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
    assert_eq!(
        attempts.len(),
        2,
        "one entry per completed streaming attempt"
    );
    assert_eq!(attempts[0].provider_id, "auth");
    assert_eq!(attempts[0].upstream_model, "remote-default");
    assert_eq!(attempts[0].status, status);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(
        attempts[0].error_message, None,
        "the truncated error body is unreadable, so no upstream message is stored"
    );
    assert!(attempts[0].duration_ms >= 1, "each attempt times itself");
    assert_eq!(attempts[1].provider_id, "fallback");
    assert_eq!(attempts[1].upstream_model, "remote-default");
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);
    assert_eq!(attempts[1].error_message, None);
    assert!(attempts[1].duration_ms >= 1);

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

    let mut config = GatewayConfig::default();
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

    let (status, _, text) = call_gateway(
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

    let (status, _, text) = call_gateway(
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

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![
        ModelMapping {
            local_model: "local-a".to_string(),
            upstream_model: "remote-a".to_string(),
            protocol: None,
            display_name: None,
            enabled: true,
            reasoning_efforts: Vec::new(),
        },
        ModelMapping {
            local_model: "local-b".to_string(),
            upstream_model: "remote-b".to_string(),
            protocol: None,
            display_name: None,
            enabled: true,
            reasoning_efforts: Vec::new(),
        },
    ];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
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

/// AC-004 / REQ-004: `GET /v1/models` lists an enabled mapping and omits a
/// disabled mapping of the same active provider, and never contacts upstream.
#[tokio::test]
async fn models_endpoint_excludes_disabled_mappings() {
    let home = temp_home("models-excludes-disabled");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "should-not-be-called"}))).await;

    let mut config = config_with_key(port);
    let mut p = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    let mut disabled = mapping("local-b", "remote-b", None);
    disabled.enabled = false;
    p.mappings = vec![mapping("local-a", "remote-a", None), disabled];
    config.providers.push(p);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "GET",
        "/v1/models",
        &[("authorization", "Bearer local-key")],
        None,
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    let ids: HashSet<String> = body["data"]
        .as_array()
        .unwrap_or_else(|| panic!("models payload must carry a data array: {text}"))
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    assert!(
        ids.contains("local-a"),
        "an enabled mapping must be listed: {text}"
    );
    assert!(
        !ids.contains("local-b"),
        "a disabled mapping must be excluded: {text}"
    );
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

    let mut config = GatewayConfig::default();
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
    let (bearer, _, _) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        body.clone(),
    )
    .await;
    assert_eq!(bearer, 200);
    let (x_api_key, _, _) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("x-api-key", "local-key")],
        body.clone(),
    )
    .await;
    assert_eq!(x_api_key, 200);
    let (wrong, _, _) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer nope")],
        body.clone(),
    )
    .await;
    assert_eq!(wrong, 401);
    let (disabled_key, _, _) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("x-api-key", "disabled-key")],
        body.clone(),
    )
    .await;
    assert_eq!(disabled_key, 401);
    let (missing, _, _) = call_gateway(port, "POST", "/v1/chat/completions", &[], body).await;
    assert_eq!(missing, 401);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn unauthorized_when_no_enabled_keys() {
    let home = temp_home("no-enabled-keys");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| MockReply::Json(200, json!({}))).await;

    let mut config = GatewayConfig::default();
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

    let (status, _, _) = call_gateway(
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
    let mut config = GatewayConfig::default();
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

    let (unknown, _, _) = call_gateway(
        port,
        "POST",
        "/v1/embeddings",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(unknown, 404);
    let (wrong_method, _, _) = call_gateway(
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

/// AC-010 / REQ-006: a malformed request line (no header terminator) is a
/// gateway-generated 400 carrying the full standard envelope.
#[tokio::test]
async fn gateway_request_parse_failure_uses_standard_error_envelope() {
    let home = temp_home("gateway-parse-error-envelope");
    let port = free_port().await;
    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let text = call_gateway_raw(port, "GARBAGE\r\n").await;
    let (status_line, body) = raw_http_status_and_body(&text);
    assert!(
        status_line.starts_with("HTTP/1.1 400"),
        "a malformed request must answer 400: {text}"
    );
    assert!(
        text.to_ascii_lowercase()
            .contains("content-type: application/json"),
        "content-type must be application/json: {text}"
    );
    assert_standard_error_envelope(&body);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-010 / REQ-006: when the encrypted config file cannot be read the gateway
/// answers 500 with the full standard envelope.
#[tokio::test]
async fn gateway_config_read_failure_uses_standard_error_envelope() {
    let home = temp_home("gateway-config-error-envelope");
    let port = free_port().await;
    let mut config = GatewayConfig::default();
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
    let path = config_path().unwrap();
    let valid = fs::read(&path).expect("read valid config");
    super::runtime_http::start_server().await.unwrap();

    fs::write(&path, b"not encrypted ciphertext").expect("corrupt config");

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 500, "config read failure must answer 500: {text}");
    assert!(
        content_type.contains("application/json"),
        "content-type must be application/json: {content_type}"
    );
    assert_standard_error_envelope(&text);

    // Restore a decryptable config so the shared server can stop cleanly.
    fs::write(&path, valid).expect("restore config");
    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-010 / REQ-006: an unknown path is a gateway-generated 404 with the full
/// standard envelope.
#[tokio::test]
async fn gateway_unknown_path_uses_standard_error_envelope() {
    let home = temp_home("gateway-unknown-path-envelope");
    let port = free_port().await;
    let config = config_with_key(port);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/embeddings",
        &[("authorization", "Bearer local-key")],
        Some(json!({"input": "x"})),
    )
    .await;
    assert_eq!(status, 404, "unknown path must answer 404: {text}");
    assert!(
        content_type.contains("application/json"),
        "content-type must be application/json: {content_type}"
    );
    assert_standard_error_envelope(&text);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-010 / REQ-006: a missing or wrong local key is a gateway-generated 401
/// with the full standard envelope.
#[tokio::test]
async fn gateway_unauthorized_uses_standard_error_envelope() {
    let home = temp_home("gateway-unauthorized-envelope");
    let port = free_port().await;
    let config = config_with_key(port);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer wrong-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 401, "unauthorized must answer 401: {text}");
    assert!(
        content_type.contains("application/json"),
        "content-type must be application/json: {content_type}"
    );
    assert_standard_error_envelope(&text);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-010 / REQ-006: a non-GET method on `/v1/models` is a gateway-generated
/// 404 with the full standard envelope.
#[tokio::test]
async fn gateway_wrong_method_on_models_uses_standard_error_envelope() {
    let home = temp_home("gateway-models-method-envelope");
    let port = free_port().await;
    let config = config_with_key(port);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/models",
        &[("authorization", "Bearer local-key")],
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 404, "a non-GET /v1/models must answer 404: {text}");
    assert!(
        content_type.contains("application/json"),
        "content-type must be application/json: {content_type}"
    );
    assert_standard_error_envelope(&text);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-010 / REQ-006: a malformed JSON body on a supported path is a
/// gateway-generated 400 with the full standard envelope.
#[tokio::test]
async fn gateway_invalid_request_body_uses_standard_error_envelope() {
    let home = temp_home("gateway-invalid-body-envelope");
    let port = free_port().await;
    let config = config_with_key(port);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let body = "{not valid json";
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: Bearer local-key\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let text = call_gateway_raw(port, &request).await;
    let (status_line, response_body) = raw_http_status_and_body(&text);
    assert!(
        status_line.starts_with("HTTP/1.1 400"),
        "a malformed body must answer 400: {text}"
    );
    assert!(
        text.to_ascii_lowercase()
            .contains("content-type: application/json"),
        "content-type must be application/json: {text}"
    );
    assert_standard_error_envelope(&response_body);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-010 / REQ-006: the non-streaming no-candidate 502 carries the full
/// standard envelope (the streaming variant is covered separately).
#[tokio::test]
async fn gateway_no_candidate_uses_standard_error_envelope() {
    let home = temp_home("gateway-no-candidate-envelope");
    let port = free_port().await;
    let mut config = config_with_key(port);
    let mut p = upstream_provider("p1", "Provider One", "http://127.0.0.1:1", "sk", None);
    p.mappings = vec![mapping("known-local", "remote-a", None)];
    config.providers.push(p);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "unknown-local"})),
    )
    .await;
    assert_eq!(status, 502, "no candidate must answer 502: {text}");
    assert!(
        content_type.contains("application/json"),
        "content-type must be application/json: {content_type}"
    );
    let body = assert_standard_error_envelope(&text);
    assert_eq!(body["error"]["code"], "all_providers_unavailable");

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

    let mut config = GatewayConfig::default();
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

    let (status, _content_type, text) = call_gateway(
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

    let mut config = GatewayConfig::default();
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

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 502, "unexpected response: {text}");
    let body = assert_standard_error_envelope(&text);
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("Provider A"), "message: {message}");
    assert!(message.contains("Provider B"), "message: {message}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-005 / REQ-003: every serviceable streaming candidate fails before any
/// byte is written, so the gateway answers HTTP 502 + `application/json` with
/// the standard envelope instead of HTTP 200 SSE.
#[tokio::test]
async fn streaming_all_fail_returns_502_json_error_envelope() {
    let home = temp_home("stream-all-fail");
    let port = free_port().await;
    let (url_a, _) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;
    let (url_b, _) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;

    let mut config = GatewayConfig::default();
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

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local", "stream": true})),
    )
    .await;
    assert_eq!(
        status, 502,
        "a pre-stream streaming failure must answer HTTP 502: {text}"
    );
    assert!(
        content_type.contains("application/json"),
        "content-type must be application/json: {content_type}"
    );
    assert!(
        !content_type.contains("text/event-stream"),
        "a pre-stream failure must not be SSE: {content_type}"
    );
    let body = assert_standard_error_envelope(&text);
    assert_eq!(body["error"]["code"], "all_providers_unavailable");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-004 / REQ-003: a streaming request with no serviceable candidate answers
/// HTTP 502 + `application/json` (not HTTP 200 SSE) with the standard envelope,
/// `error.code == "all_providers_unavailable"` and `error.param` present/null.
#[tokio::test]
async fn streaming_no_candidate_returns_502_json_error_envelope() {
    let home = temp_home("stream-no-candidate");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "should-not-run"}))).await;

    let mut config = config_with_key(port);
    let mut p = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    p.mappings = vec![mapping("known-local", "remote-a", None)];
    config.providers.push(p);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "unknown-local", "stream": true})),
    )
    .await;
    assert_eq!(
        status, 502,
        "no serviceable candidate must answer HTTP 502: {text}"
    );
    assert!(
        content_type.contains("application/json"),
        "content-type must be application/json: {content_type}"
    );
    assert!(
        !content_type.contains("text/event-stream"),
        "no-candidate must not answer SSE: {content_type}"
    );
    let body = assert_standard_error_envelope(&text);
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    assert!(
        log.lock().unwrap().is_empty(),
        "no upstream request may be issued when there is no candidate"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn streaming_switches_when_first_provider_fails_before_first_byte() {
    let _home = isolated_temp_home("stream-switch");
    let (drop_url, _drop_log) = spawn_mock_upstream(|_| MockReply::Drop).await;
    let stream_body =
        "data: {\"choices\":[{\"delta\":{\"content\":\"from-b\"}}]}\n\ndata: [DONE]\n\n";
    let (stream_url, stream_log) =
        spawn_mock_upstream(move |_| MockReply::Stream(stream_body.to_string())).await;

    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &drop_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &stream_url, "sk", Some("remote-default"));
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();

    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    let mut attempts = Vec::new();
    super::runtime_http::attempt_streaming(
        &mut server,
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await
    .unwrap();
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("from-b"), "expected second provider stream: {text}");
    assert_eq!(stream_log.lock().unwrap().len(), 1);
    assert_eq!(
        attempts.len(),
        2,
        "the failed and the successful attempt both completed"
    );
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].status, 0, "a closed connection has no HTTP status");
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert!(
        attempts[0]
            .error_message
            .as_deref()
            .unwrap_or("")
            .contains("network error"),
        "a transport failure records its network description: {:?}",
        attempts[0].error_message
    );
    assert!(attempts[0].duration_ms >= 1);
    assert_eq!(attempts[1].provider_id, "b");
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);
    assert_eq!(attempts[1].error_message, None);
    assert!(attempts[1].duration_ms >= 1);
}

#[tokio::test]
async fn streaming_terminates_after_first_byte_without_switching() {
    let _home = isolated_temp_home("stream-terminate");
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

    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &partial_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &stream_url, "sk", Some("remote-default"));
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();

    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    let mut attempts = Vec::new();
    super::runtime_http::attempt_streaming(
        &mut server,
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await
    .unwrap();
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("partial-a"), "expected first provider bytes: {text}");
    assert_eq!(
        attempts.len(),
        1,
        "the second candidate is never reached after the first byte"
    );
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].upstream_model, "remote-default");
    assert_eq!(
        attempts[0].status, 502,
        "a stream that failed after the first byte records the gateway stream failure status"
    );
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert!(
        attempts[0]
            .error_message
            .as_deref()
            .unwrap_or("")
            .contains("stream failed after first byte"),
        "the mid-stream failure records its stream description: {:?}",
        attempts[0].error_message
    );
    assert!(attempts[0].duration_ms >= 1);
    assert!(
        !text.contains("from-b"),
        "must not retry after bytes were written: {text}"
    );
    assert!(
        !text.contains("data: [DONE]"),
        "an abnormal stream must not send [DONE]: {text}"
    );
    let (_, body) = raw_http_status_and_body(&text);
    let errors = sse_error_events(&body);
    assert_eq!(
        errors.len(),
        1,
        "exactly one standalone error fragment must close the stream: {text}"
    );
    assert!(
        !errors[0]["error"]["message"].as_str().unwrap_or("").is_empty(),
        "the error fragment must carry a readable message: {text}"
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

    let mut config = GatewayConfig::default();
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

    let mut config = GatewayConfig::default();
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
    assert_eq!(status.local_base_url, format!("http://127.0.0.1:{port}/v1"));

    let (code, _, _) = call_gateway(
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
        let config = super::commands::api_gateway_get_config().unwrap();
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
    assert_eq!(opencode["provider_key"], "apigateway");
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

/// AC-005 / REQ-005: terminal sync only consumes enabled mappings. The opencode
/// model map is exactly the enabled row and the codex `model` is that row's
/// `local_model`, not the disabled row and not the provider `default_model`.
#[test]
fn build_gateway_provider_excludes_disabled_mappings() {
    let mut gateway = upstream_provider(
        "g1",
        "Gateway A",
        "https://upstream.example/v1",
        "sk",
        Some("d"),
    );
    let mut disabled = mapping("local-b", "remote-b", Some("B"));
    disabled.enabled = false;
    gateway.mappings = vec![mapping("local-a", "remote-a", Some("A")), disabled];

    let opencode = build_gateway_provider(
        "fus-oc",
        "opencode",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway.clone()],
    )
    .expect("opencode provider must build");
    assert_eq!(
        opencode["tool_config"]["models"],
        json!({ "local-a": { "name": "A" } }),
        "opencode must offer exactly the enabled mappings: {opencode}"
    );

    let codex = build_gateway_provider(
        "fus-cx",
        "codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway],
    )
    .expect("codex provider must build");
    assert_eq!(
        codex["model"], "local-a",
        "an enabled mapping must win over default_model and the disabled row: {codex}"
    );
}

/// AC-005 / REQ-005: with no enabled mapping left, codex falls back to the
/// provider `default_model` and opencode offers an empty model map.
#[test]
fn build_gateway_provider_codex_falls_back_to_default_when_all_mappings_disabled() {
    let mut gateway = upstream_provider(
        "g1",
        "Gateway A",
        "https://upstream.example/v1",
        "sk",
        Some("d"),
    );
    let mut disabled = mapping("local-b", "remote-b", Some("B"));
    disabled.enabled = false;
    gateway.mappings = vec![disabled];

    let codex = build_gateway_provider(
        "fus-cx",
        "codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway.clone()],
    )
    .expect("codex provider must build");
    assert_eq!(
        codex["model"], "d",
        "codex must fall back to default_model only when no enabled mapping exists: {codex}"
    );

    let opencode = build_gateway_provider(
        "fus-oc",
        "opencode",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway],
    )
    .expect("opencode provider must build");
    assert_eq!(
        opencode["tool_config"]["models"],
        json!({}),
        "an all-disabled gateway must offer no models: {opencode}"
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
    assert_eq!(value["tool_config"]["api_gateway_gateway"], true);
    assert!(value.get("active").is_none(), "must never auto-activate: {value}");
    assert!(value.get("is_active").is_none(), "must never auto-activate: {value}");

    assert_eq!(value["provider_key"], "apigateway");
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

/// The mapping's configured reasoning tiers are carried into the opencode model
/// entry as `variants` (plus `reasoning: true`) so the tool offers exactly the
/// gateway's strengths; a mapping without tiers keeps `{ "name" }` only.
#[test]
fn build_gateway_provider_opencode_carries_reasoning_efforts_as_variants() {
    let mut gateway = upstream_provider("g1", "Gateway A", "https://upstream.example/v1", "sk", None);
    let mut tiered = mapping(
        "ds",
        "deepseek/deepseek-v4.1-flash",
        Some("DeepSeek V4.1 Flash"),
    );
    tiered.reasoning_efforts = vec!["low".to_string(), "high".to_string(), "max".to_string()];
    let mut messy = mapping("messy", "remote-messy", None);
    messy.reasoning_efforts = vec![" high ".to_string(), "high".to_string(), "  ".to_string()];
    gateway.mappings = vec![tiered, messy, mapping("plain", "remote-plain", None)];

    let value = build_gateway_provider(
        "fus-oc",
        "opencode",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway.clone()],
    )
    .expect("opencode provider must build");

    assert_eq!(
        value["tool_config"]["models"],
        json!({
            "ds": {
                "name": "DeepSeek V4.1 Flash",
                "reasoning": true,
                "variants": {
                    "low": { "reasoningEffort": "low" },
                    "high": { "reasoningEffort": "high" },
                    "max": { "reasoningEffort": "max" },
                },
            },
            "messy": {
                "name": "remote-messy",
                "reasoning": true,
                "variants": { "high": { "reasoningEffort": "high" } },
            },
            "plain": { "name": "remote-plain" },
        }),
        "opencode must expose the mapping reasoning tiers as variants: {value}"
    );

    let codex = build_gateway_provider(
        "fus-cx",
        "codex",
        "http://127.0.0.1:17688",
        "local-key-123",
        &[gateway],
    )
    .expect("codex provider must build");
    assert_eq!(
        codex["model"], "ds",
        "codex keeps selecting the first enabled mapping's local model: {codex}"
    );
    assert!(
        codex.get("variants").is_none(),
        "codex must not receive opencode variants: {codex}"
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
    assert_eq!(value["tool_config"]["api_gateway_gateway"], true);
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
        let mut config = GatewayConfig::default();
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
        synced_base_url: "http://127.0.0.1:17688/v1".to_string(),
        synced_at: 10,
    };
    assert!(!terminal_sync_pending(
        &record,
        Some("k1"),
        "http://127.0.0.1:17688/v1"
    ));
    assert!(terminal_sync_pending(
        &record,
        Some("k2"),
        "http://127.0.0.1:17688/v1"
    ));
    assert!(terminal_sync_pending(
        &record,
        Some("k1"),
        "http://127.0.0.1:17777/v1"
    ));
    assert!(terminal_sync_pending(
        &record,
        None,
        "http://127.0.0.1:17688/v1"
    ));
}

#[test]
fn reenable_clears_auto_disabled_and_preserves_user_enabled() {
    with_temp_home("reenable-command", |_home| {
        let mut config = GatewayConfig::default();
        let mut p = provider("p1");
        p.enabled = true;
        register_failure(&mut p, FailureClass::DisableImmediately, "auth", 1);
        config.providers.push(p);
        super::storage::write_config(&config).unwrap();

        let after = super::commands::api_gateway_reenable_provider("p1".to_string()).unwrap();
        assert!(after.providers[0].enabled);
        assert!(!after.providers[0].auto_disabled);
        assert_eq!(after.providers[0].disabled_reason, None);

        // Turning user intent off must not be undone by a later manual re-enable.
        let mut reloaded = super::storage::read_config().unwrap();
        reloaded.providers[0].enabled = false;
        reloaded.providers[0].auto_disabled = true;
        reloaded.providers[0].disabled_reason = Some("boom".to_string());
        super::storage::write_config(&reloaded).unwrap();

        let after = super::commands::api_gateway_reenable_provider("p1".to_string()).unwrap();
        assert!(!after.providers[0].enabled, "user intent must be preserved");
        assert!(!after.providers[0].auto_disabled);
    });
}

#[test]
fn provider_enable_command_only_changes_user_intent() {
    with_temp_home("provider-enable", |_home| {
        let mut config = GatewayConfig::default();
        let mut p = provider("p1");
        p.auto_disabled = true;
        p.disabled_reason = Some("auth".to_string());
        config.providers.push(p);
        super::storage::write_config(&config).unwrap();

        let after =
            super::commands::api_gateway_set_provider_enabled("p1".to_string(), false).unwrap();
        assert!(!after.providers[0].enabled);
        assert!(after.providers[0].auto_disabled, "auto state independent");
        assert!(super::commands::api_gateway_set_provider_enabled("ghost".to_string(), true).is_err());
    });
}

#[test]
fn key_commands_persist_and_advance_default_key() {
    with_temp_home("key-commands", |_home| {
        let config = super::commands::api_gateway_upsert_key(GatewayKey {
            id: "k1".to_string(),
            label: "K1".to_string(),
            value: "v1".to_string(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        assert_eq!(config.default_key_id.as_deref(), Some("k1"));

        super::commands::api_gateway_upsert_key(GatewayKey {
            id: "k2".to_string(),
            label: "K2".to_string(),
            value: "v2".to_string(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        let switched = super::commands::api_gateway_set_default_key("k2".to_string()).unwrap();
        assert_eq!(switched.default_key_id.as_deref(), Some("k2"));

        // Disabling the current default advances to the next enabled key.
        let advanced = super::commands::api_gateway_upsert_key(GatewayKey {
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
        let cleared = super::commands::api_gateway_delete_key("k1".to_string()).unwrap();
        assert_eq!(cleared.default_key_id, None);
    });
}

#[test]
fn new_keys_without_a_value_get_a_random_secret() {
    with_temp_home("key-autogen", |_home| {
        let first = super::commands::api_gateway_upsert_key(GatewayKey {
            id: String::new(),
            label: "CI".to_string(),
            value: String::new(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        let first_value = first.keys[0].value.clone();
        assert!(first_value.starts_with("sk-gateway-"), "unexpected key: {first_value}");
        assert!(first_value.len() > "sk-gateway-".len());

        let second = super::commands::api_gateway_upsert_key(GatewayKey {
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
        let created = super::commands::api_gateway_upsert_key(GatewayKey {
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
            created_value.starts_with("sk-gateway-"),
            "a masked new key must receive a generated secret: {created_value}"
        );
        assert!(
            created_value.len() > "sk-gateway-".len(),
            "a generated secret must carry entropy after the prefix: {created_value}"
        );
    });
}

#[test]
fn provider_delete_removes_ledger_entry() {
    with_temp_home("provider-delete", |_home| {
        let mut config = GatewayConfig::default();
        config.providers.push(provider("p1"));
        config.terminal_syncs.push(TerminalSyncRecord {
            provider_id: "p1".to_string(),
            tool: "opencode".to_string(),
            synced_key_id: "k1".to_string(),
            synced_base_url: "http://127.0.0.1:17688".to_string(),
            synced_at: 1,
        });
        super::storage::write_config(&config).unwrap();

        let after = super::commands::api_gateway_delete_provider("p1".to_string()).unwrap();
        assert!(after.providers.is_empty());
        assert!(after.terminal_syncs.is_empty());
    });
}

#[test]
fn every_command_is_registered_in_the_invoke_handler() {
    const RUN_APP_SOURCE: &str = include_str!("../app_runtime/run_app.rs");
    const LIB_SOURCE: &str = include_str!("../lib.rs");

    assert!(LIB_SOURCE.contains("mod api_gateway;"));
    let commands = [
        "api_gateway_get_config",
        "api_gateway_save_config",
        "api_gateway_upsert_provider",
        "api_gateway_delete_provider",
        "api_gateway_set_provider_enabled",
        "api_gateway_reenable_provider",
        "api_gateway_upsert_key",
        "api_gateway_delete_key",
        "api_gateway_set_default_key",
        "api_gateway_start",
        "api_gateway_stop",
        "api_gateway_status",
        "api_gateway_terminal_targets",
        "api_gateway_configure_terminal",
        "api_gateway_sync_terminal",
        // 20260917-ai-gateway-usage-logs commands.
        "api_gateway_usage_stats",
        "api_gateway_request_logs",
        "api_gateway_usage_retention_get",
        "api_gateway_usage_retention_save",
        // 20260918-provider-templates commands.
        "api_gateway_provider_templates",
        "api_gateway_sync_provider_template",
        "api_gateway_create_provider_from_template",
        "api_gateway_delete_provider_model",
        "api_gateway_restore_provider_model",
        "api_gateway_upsert_provider_template",
        "api_gateway_delete_provider_template",
        "api_gateway_reset_provider_templates",
    ];
    for command in commands {
        let registration = format!("api_gateway::{command},");
        assert_eq!(
            RUN_APP_SOURCE.matches(&registration).count(),
            1,
            "command {command} must be registered exactly once in generate_handler!"
        );
    }
    // The usage-log commands must also be exported through `lib.rs`.
    for command in [
        "api_gateway_usage_stats",
        "api_gateway_request_logs",
        "api_gateway_usage_retention_get",
        "api_gateway_usage_retention_save",
        "api_gateway_provider_templates",
        "api_gateway_sync_provider_template",
        "api_gateway_create_provider_from_template",
        "api_gateway_delete_provider_model",
        "api_gateway_restore_provider_model",
        "api_gateway_upsert_provider_template",
        "api_gateway_delete_provider_template",
        "api_gateway_reset_provider_templates",
    ] {
        assert!(
            LIB_SOURCE.contains(command),
            "command {command} must be exported from lib.rs"
        );
    }
    // REQ-005: the legacy price-table commands are removed from the surface.
    for removed in [
        "api_gateway_model_prices_get",
        "api_gateway_model_prices_save",
    ] {
        assert!(
            !RUN_APP_SOURCE.contains(&format!("api_gateway::{removed},")),
            "the removed command {removed} must not be registered in generate_handler!"
        );
        assert!(
            !LIB_SOURCE.contains(removed),
            "the removed command {removed} must not be exported from lib.rs"
        );
    }
    // REQ-011: the standalone model-fetch command is removed together with its
    // frontend wrapper; its registration and export must be gone.
    for removed in ["api_gateway_fetch_models"] {
        assert!(
            !RUN_APP_SOURCE.contains(&format!("api_gateway::{removed},")),
            "the removed command {removed} must not be registered in generate_handler!"
        );
        assert!(
            !LIB_SOURCE.contains(removed),
            "the removed command {removed} must not be exported from lib.rs"
        );
    }
}

/// AC-007 / REQ-005: saving a provider atomically replaces exactly its own
/// price rows, drops blank-model, duplicate and unreachable rows, mirrors
/// off-peak windows into both directions, and leaves its rows unchanged when
/// no price list is submitted.
#[test]
fn upsert_provider_replaces_exactly_its_own_price_rows() {
    with_temp_home("upsert-provider-price-rows", |_home| {
        let mut p = provider("p");
        p.mappings = vec![
            mapping("local-a", "model-a", None),
            mapping("local-b", "model-b", None),
        ];
        let mut q = provider("q");
        q.mappings = vec![mapping("local-x", "model-x", None)];

        let mut config = GatewayConfig::default();
        config.providers.push(p);
        config.providers.push(q);
        config.model_prices = vec![
            priced_with_provider("p", "model-a", 1.0, 0.0, 0.0, 1.0),
            priced_with_provider("p", "model-b", 2.0, 0.0, 0.0, 2.0),
            priced_with_provider("q", "model-x", 3.0, 0.0, 0.0, 3.0),
        ];
        super::storage::write_config(&config).expect("seed config");

        let op1 = OffPeakPrice {
            start_time: "00:00".to_string(),
            end_time: "09:00".to_string(),
            input: 0.5,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 1.0,
            days: None,
        };
        let op2 = OffPeakPrice {
            start_time: "18:00".to_string(),
            end_time: "23:00".to_string(),
            input: 0.4,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 0.8,
            days: None,
        };

        let mut updated = provider("p");
        updated.name = "Provider P Renamed".to_string();
        updated.mappings = vec![mapping("local-a", "model-a", None)];

        let edited_a = ModelPrice {
            provider_id: None,
            upstream_model: "model-a".to_string(),
            input: 11.0,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 22.0,
            off_peaks: vec![op1.clone(), op2.clone()],
            off_peak: None,
        };
        let blank_model = ModelPrice {
            upstream_model: "  ".to_string(),
            input: 7.0,
            ..ModelPrice::default()
        };
        let duplicate_a = ModelPrice {
            provider_id: None,
            upstream_model: "model-a".to_string(),
            input: 99.0,
            ..ModelPrice::default()
        };
        let unreachable_z = ModelPrice {
            provider_id: None,
            upstream_model: "model-z".to_string(),
            input: 5.0,
            ..ModelPrice::default()
        };

        let saved = super::commands::api_gateway_upsert_provider(
            updated.clone(),
            Some(vec![edited_a, blank_model, duplicate_a, unreachable_z]),
        )
        .expect("upsert with prices");

        let p_rows: Vec<&ModelPrice> = saved
            .model_prices
            .iter()
            .filter(|row| row.provider_id.as_deref() == Some("p"))
            .collect();
        assert_eq!(
            p_rows.len(),
            1,
            "only one reachable A row may remain: {p_rows:?}"
        );
        let a = p_rows[0];
        assert_eq!(a.upstream_model, "model-a");
        assert_eq!(a.provider_id.as_deref(), Some("p"));
        assert_eq!(a.input, 11.0, "the submitted edit must win");
        assert_eq!(a.output, 22.0);
        assert_eq!(
            a.off_peak,
            Some(op1.clone()),
            "the first window mirrors into off_peak"
        );
        assert_eq!(
            a.off_peaks,
            vec![op1.clone(), op2.clone()],
            "both submitted windows are kept"
        );

        assert!(
            saved
                .model_prices
                .iter()
                .all(|row| !row.upstream_model.trim().is_empty()),
            "a blank-model row must leave no trace"
        );
        assert!(
            saved.model_prices.iter().all(|row| row.input != 99.0),
            "a duplicate row must not overwrite the first"
        );
        assert!(
            saved
                .model_prices
                .iter()
                .all(|row| row.upstream_model != "model-z"),
            "a row for a model unreachable from the provider must leave no trace"
        );

        let q_rows: Vec<&ModelPrice> = saved
            .model_prices
            .iter()
            .filter(|row| row.provider_id.as_deref() == Some("q"))
            .collect();
        assert_eq!(q_rows.len(), 1);
        assert_eq!(q_rows[0].upstream_model, "model-x");
        assert_eq!(q_rows[0].input, 3.0, "another provider's rows stay untouched");

        // `prices: None` leaves that provider's rows unchanged.
        let mut renamed = updated.clone();
        renamed.name = "Provider P Renamed Again".to_string();
        let after_none = super::commands::api_gateway_upsert_provider(renamed, None)
            .expect("upsert without prices");
        let mut before_rows: Vec<ModelPrice> = saved
            .model_prices
            .iter()
            .filter(|row| row.provider_id.as_deref() == Some("p"))
            .cloned()
            .collect();
        let mut after_rows: Vec<ModelPrice> = after_none
            .model_prices
            .iter()
            .filter(|row| row.provider_id.as_deref() == Some("p"))
            .cloned()
            .collect();
        before_rows.sort_by(|left, right| left.upstream_model.cmp(&right.upstream_model));
        after_rows.sort_by(|left, right| left.upstream_model.cmp(&right.upstream_model));
        assert_eq!(
            before_rows, after_rows,
            "an absent price list must leave the provider's rows unchanged"
        );

        // Legacy `off_peak` must mirror into `off_peaks`.
        let legacy_op = OffPeakPrice {
            start_time: "01:00".to_string(),
            end_time: "05:00".to_string(),
            input: 0.25,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 0.5,
            days: Some(vec![1, 2, 3]),
        };
        let legacy_row = ModelPrice {
            provider_id: None,
            upstream_model: "model-a".to_string(),
            input: 12.0,
            off_peak: Some(legacy_op.clone()),
            off_peaks: Vec::new(),
            ..ModelPrice::default()
        };
        let mirrored = super::commands::api_gateway_upsert_provider(
            updated.clone(),
            Some(vec![legacy_row]),
        )
        .expect("upsert a legacy off_peak row");
        let mirrored_row = mirrored
            .model_prices
            .iter()
            .find(|row| {
                row.provider_id.as_deref() == Some("p") && row.upstream_model == "model-a"
            })
            .expect("the mirrored row must exist");
        assert_eq!(
            mirrored_row.off_peaks,
            vec![legacy_op],
            "a legacy off_peak must mirror into off_peaks"
        );
    });
}

/// AC-008 / REQ-005: a new provider's submitted rows are bound to the generated
/// id, overwriting any submitted foreign provider id, and survive a reopen.
#[test]
fn upsert_provider_binds_new_provider_rows_to_generated_id() {
    with_temp_home("upsert-new-provider-prices", |_home| {
        let mut new_provider = provider("");
        new_provider.name = "Brand New".to_string();
        new_provider.mappings = vec![mapping("local-new", "remote-new", None)];
        let submitted = priced_with_provider("other", "remote-new", 1.0, 0.0, 0.0, 2.0);

        let config =
            super::commands::api_gateway_upsert_provider(new_provider, Some(vec![submitted]))
                .expect("creating a priced provider");

        let created = config
            .providers
            .iter()
            .find(|candidate| candidate.id.starts_with("gw-"))
            .expect("the new provider must receive a generated gw- id");
        let rows: Vec<&ModelPrice> = config
            .model_prices
            .iter()
            .filter(|row| row.provider_id.as_deref() == Some(created.id.as_str()))
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "the submitted row must bind to the generated id: {:?}",
            config.model_prices
        );
        assert_eq!(rows[0].upstream_model, "remote-new");
        assert!(
            config
                .model_prices
                .iter()
                .all(|row| row.provider_id.as_deref() != Some("other")),
            "a submitted foreign provider id must be overwritten"
        );

        let reloaded = super::storage::read_config().expect("reopen config");
        let reloaded_provider = reloaded
            .providers
            .iter()
            .find(|candidate| candidate.id == created.id)
            .expect("the generated provider must persist");
        assert!(reloaded_provider.id.starts_with("gw-"));
        let reloaded_rows: Vec<&ModelPrice> = reloaded
            .model_prices
            .iter()
            .filter(|row| row.provider_id.as_deref() == Some(created.id.as_str()))
            .collect();
        assert_eq!(reloaded_rows.len(), 1);
        assert_eq!(reloaded_rows[0].upstream_model, "remote-new");
    });
}

/// REQ-005 counterexample: deleting a provider removes its price rows and
/// leaves every other provider's rows intact.
#[test]
fn delete_provider_removes_its_price_rows_but_keeps_others() {
    with_temp_home("delete-provider-prices", |_home| {
        let mut p = provider("p");
        p.mappings = vec![mapping("local-p", "remote-p", None)];
        let mut q = provider("q");
        q.mappings = vec![mapping("local-q", "remote-q", None)];

        let mut config = GatewayConfig::default();
        config.providers.push(p);
        config.providers.push(q);
        config.model_prices = vec![
            priced_with_provider("p", "remote-p", 1.0, 0.0, 0.0, 1.0),
            priced_with_provider("q", "remote-q", 2.0, 0.0, 0.0, 2.0),
        ];
        super::storage::write_config(&config).expect("seed config");

        let after = super::commands::api_gateway_delete_provider("p".to_string())
            .expect("delete the provider");
        assert!(
            after.providers.iter().all(|candidate| candidate.id != "p"),
            "the provider must be removed"
        );
        assert!(
            after
                .model_prices
                .iter()
                .all(|row| row.provider_id.as_deref() != Some("p")),
            "the deleted provider's rows must be gone: {:?}",
            after.model_prices
        );
        let q_row = after
            .model_prices
            .iter()
            .find(|row| row.provider_id.as_deref() == Some("q"))
            .expect("the other provider's row must survive");
        assert_eq!(q_row.upstream_model, "remote-q");
        assert_eq!(q_row.input, 2.0);
    });
}

/// AC-009 boundary / REQ-008: `apply_delete_provider_model` removes the
/// provider-scoped price row together with the mapping for both manual and
/// template-bound providers without waiting for a persisted normalization pass,
/// records the ignored model on a bound provider, and leaves an unrelated
/// default-model row reachable.
#[test]
fn delete_model_removes_provider_price_row_for_manual_and_bound_providers() {
    with_temp_home("delete-model-price-row", |_home| {
        // Manual provider: the row is removed in memory and stays removed.
        let mut manual = provider("manual");
        manual.template_id = None;
        manual.default_model = None;
        manual.mappings = vec![mapping("local-m", "model-m", None)];
        let mut config = GatewayConfig::default();
        config.providers.push(manual);
        config.model_prices = vec![priced_with_provider(
            "manual",
            "model-m",
            1.0,
            0.0,
            0.0,
            1.0,
        )];

        super::templates::apply_delete_provider_model(
            &mut config,
            "manual",
            "model-m",
            |_next| Ok(()),
        )
        .expect("deleting a manual mapping must succeed");
        assert!(
            !config.model_prices.iter().any(|row| {
                row.provider_id.as_deref() == Some("manual") && row.upstream_model == "model-m"
            }),
            "a manual provider's row is removed with the mapping, not later"
        );

        super::storage::write_config(&config).expect("persist after the delete");
        let reloaded = super::storage::read_config().expect("reload config");
        assert!(
            reloaded.model_prices.iter().all(|row| {
                !(row.provider_id.as_deref() == Some("manual")
                    && row.upstream_model == "model-m")
            }),
            "the removed row must not reappear after a persisted write"
        );

        // Template-bound provider: the row is removed and the model is ignored.
        let mut bound = provider("bound");
        bound.template_id = Some("opencode-zen".to_string());
        bound.mappings = vec![mapping("local-b", "model-b", None)];
        let mut config = GatewayConfig::default();
        config.providers.push(bound);
        config.model_prices = vec![priced_with_provider("bound", "model-b", 2.0, 0.0, 0.0, 2.0)];

        super::templates::apply_delete_provider_model(
            &mut config,
            "bound",
            "model-b",
            |_next| Ok(()),
        )
        .expect("deleting a bound mapping must succeed");
        let bound = config
            .providers
            .iter()
            .find(|candidate| candidate.id == "bound")
            .expect("the bound provider must exist");
        assert!(
            bound.ignored_models.iter().any(|id| id == "model-b"),
            "a bound provider must record the deleted model"
        );
        assert!(
            !config.model_prices.iter().any(|row| {
                row.provider_id.as_deref() == Some("bound") && row.upstream_model == "model-b"
            }),
            "a bound provider's row is removed with the mapping"
        );

        // Default model X (X != M) stays reachable after mapping M is deleted.
        let mut with_default = provider("default-provider");
        with_default.template_id = None;
        with_default.default_model = Some("model-x".to_string());
        with_default.mappings = vec![mapping("local-m", "model-m", None)];
        let mut config = GatewayConfig::default();
        config.providers.push(with_default);
        config.model_prices = vec![priced_with_provider(
            "default-provider",
            "model-x",
            3.0,
            0.0,
            0.0,
            3.0,
        )];

        super::templates::apply_delete_provider_model(
            &mut config,
            "default-provider",
            "model-m",
            |_next| Ok(()),
        )
        .expect("deleting the mapping must succeed");
        super::storage::write_config(&config).expect("persist the default-model config");
        let reloaded = super::storage::read_config().expect("reload config");
        let x_row = reloaded
            .model_prices
            .iter()
            .find(|row| {
                row.provider_id.as_deref() == Some("default-provider")
                    && row.upstream_model == "model-x"
            })
            .expect("the default-model row must survive the delete");
        assert_eq!(x_row.input, 3.0);
    });
}

/// B1 command return shape: creating from a template must return the refreshed
/// `GatewayConfig` (like the delete/restore commands) so the caller can find the
/// new provider and open its detail from a single call.
#[test]
fn create_provider_from_template_command_returns_config() {
    with_temp_home("create-from-template-command", |_home| {
        super::storage::write_config(&GatewayConfig::default()).expect("write config");

        let result = super::commands::api_gateway_create_provider_from_template(
            "opencode-zen".into(),
            "T".into(),
            "".into(),
            UpstreamProtocol::ChatCompletions,
            "sk-test".into(),
        )
        .expect("a non-blank API key must create the provider");

        assert!(
            result
                .providers
                .iter()
                .any(|provider| provider.template_id.as_deref() == Some("opencode-zen")),
            "the command must return the config containing the new template provider"
        );
    });
}

#[tokio::test]
async fn no_candidate_model_returns_all_unavailable_without_upstream_request() {
    let home = temp_home("no-candidate");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "should-not-run"}))).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut p = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    p.mappings = vec![ModelMapping {
        local_model: "known-local".to_string(),
        upstream_model: "remote-a".to_string(),
        protocol: None,
        display_name: None,
        enabled: true,
        reasoning_efforts: Vec::new(),
    }];
    config.providers.push(p);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "unknown-local"})),
    )
    .await;
    assert_eq!(status, 502, "unexpected response: {text}");
    let body = assert_standard_error_envelope(&text);
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
    let _home = isolated_temp_home("non-json-retry");
    let (non_json_url, non_json_log) =
        spawn_mock_upstream(|_| MockReply::Stream("this is not json".to_string())).await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &non_json_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk", Some("remote-default"));
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;
    assert_eq!(response.status, 200);
    assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
    assert_eq!(non_json_log.lock().unwrap().len(), 1);
    assert_eq!(ok_log.lock().unwrap().len(), 1);
    assert_eq!(attempts.len(), 2, "both attempts completed");
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert!(attempts[0].duration_ms >= 1);
    assert_eq!(attempts[1].provider_id, "b");
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);
    assert_eq!(attempts[1].error_message, None);
}

#[tokio::test]
async fn return_to_client_error_is_passed_through_without_switching_or_disabling() {
    let _home = isolated_temp_home("return-to-client");
    let (bad_request_url, bad_request_log) =
        spawn_mock_upstream(|_| MockReply::Json(400, json!({"error": {"message": "bad request"}})))
            .await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    let a = upstream_provider("a", "Provider A", &bad_request_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk", Some("remote-default"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;
    assert_eq!(response.status, 400, "upstream client error must pass through");
    assert_eq!(bad_request_log.lock().unwrap().len(), 1);
    assert!(ok_log.lock().unwrap().is_empty(), "must not switch on 4xx");
    assert_eq!(
        attempts.len(),
        1,
        "a ReturnToClient attempt is the request's only completed attempt"
    );
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].provider_name, "Provider A");
    assert_eq!(attempts[0].upstream_model, "remote-default");
    assert_eq!(attempts[0].status, 400);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(
        attempts[0].error_message.as_deref(),
        Some("bad request"),
        "the standard upstream error.message is extracted"
    );
    assert!(attempts[0].usage.is_none());
    assert!(attempts[0].duration_ms >= 1);
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

fn config_with_key(port: u16) -> GatewayConfig {
    let mut config = GatewayConfig::default();
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
    let candidates: Vec<GatewayUpstreamProvider> =
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
    let _home = isolated_temp_home("e2e-failover-network");
    let (drop_url, drop_log) = spawn_mock_upstream(|_| MockReply::Drop).await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &drop_url, "sk-a", Some("remote-model"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local-model"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
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
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].status, 0, "a network failure has no HTTP status");
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert!(attempts[0].duration_ms >= 1);
    assert_eq!(attempts[1].provider_id, "b");
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);
}

#[tokio::test]
async fn end_to_end_5xx_falls_back_and_tries_first_candidate_once() {
    // AC-010: first candidate 5xx -> second provider succeeds; first is not retried.
    let _home = isolated_temp_home("e2e-failover-5xx");
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

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local-model"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;

    assert_eq!(response.status, 200, "5xx must fall back");
    assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
    assert_eq!(fail_log.lock().unwrap().len(), 1);
    assert_eq!(ok_log.lock().unwrap().len(), 1);
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].status, 503);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(attempts[0].error_message.as_deref(), Some("down"));
    assert_eq!(attempts[1].provider_id, "b");
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);

    let a_stored = config.providers.iter().find(|p| p.id == "a").unwrap();
    assert_eq!(a_stored.consecutive_failures, 1);
    assert!(!a_stored.auto_disabled, "a single 5xx must not disable the provider");
}

#[tokio::test]
async fn end_to_end_auth_failures_disable_immediately_and_switch() {
    // AC-011: 401/403 disable the provider right away, record the reason, and the
    // request continues on the next candidate.
    for status in [401u16, 403u16] {
        let home = isolated_temp_home(&format!("e2e-auth-{status}"));
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

        let mut attempts = Vec::new();
        let response = super::runtime_http::attempt_non_streaming(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
            &mut attempts,
        )
        .await;

        assert_eq!(response.status, 200, "status {status} must fall back");
        assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
        assert_eq!(auth_log.lock().unwrap().len(), 1);
        assert_eq!(ok_log.lock().unwrap().len(), 1);
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].provider_id, "a");
        assert_eq!(attempts[0].status, status);
        assert_eq!(attempts[0].result, UsageResult::Failure);
        assert_eq!(attempts[0].error_message.as_deref(), Some("denied"));
        assert_eq!(attempts[1].provider_id, "b");
        assert_eq!(attempts[1].status, 200);

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
        let (status, _, text) = call_gateway(
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

    let (status, _, text) = call_gateway(
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
        "each of the three failed requests makes exactly one attempt for the single candidate; the auto-disabled provider is not contacted on the fourth request"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

#[tokio::test]
async fn end_to_end_network_errors_accumulate_and_disable() {
    // AC-011: a network error yields to the healthy initial-pass fallback, so
    // each inbound request records one failed network candidate without paying
    // its retry delay. Three failed inbound requests still auto-disable it.
    let _home = isolated_temp_home("e2e-network-threshold");
    let dead_url = closed_port_base_url().await;
    let (healthy_url, _) =
        spawn_json_sequence_mock(vec![(200, json!({"id": "healthy-fallback"}))]).await;

    let mut config = GatewayConfig::default();
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
        let mut attempts = Vec::new();
        let response = super::runtime_http::attempt_non_streaming(
            &[failed.clone(), healthy.clone()],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
            &mut attempts,
        )
        .await;
        assert_eq!(response.status, 200, "network failure must yield to the healthy fallback");
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].provider_id, "a");
        assert_eq!(attempts[0].status, 0, "a network failure has no HTTP status");
        assert_eq!(attempts[0].result, UsageResult::Failure);
        assert_eq!(attempts[1].provider_id, "b");
        assert_eq!(attempts[1].result, UsageResult::Success);
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
        let home = isolated_temp_home(&format!("e2e-transient-{status}"));
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

        let mut attempts = Vec::new();
        let response = super::runtime_http::attempt_non_streaming(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
            &mut attempts,
        )
        .await;

        assert_eq!(response.status, 200, "status {status} must switch");
        assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
        assert_eq!(transient_log.lock().unwrap().len(), 1);
        assert_eq!(ok_log.lock().unwrap().len(), 1);
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].provider_id, "a");
        assert_eq!(attempts[0].status, status);
        assert_eq!(attempts[0].result, UsageResult::Failure);
        assert_eq!(attempts[0].error_message.as_deref(), Some("transient"));
        assert_eq!(attempts[1].provider_id, "b");
        assert_eq!(attempts[1].result, UsageResult::Success);

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
        let home = isolated_temp_home(&format!("e2e-client-{status}"));
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

        let mut attempts = Vec::new();
        let response = super::runtime_http::attempt_non_streaming(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
            &mut attempts,
        )
        .await;

        assert_eq!(response.status, status, "status {status} must pass through");
        assert_eq!(bad_log.lock().unwrap().len(), 1);
        assert!(ok_log.lock().unwrap().is_empty(), "status {status} must not switch");
        assert_eq!(
            attempts.len(),
            1,
            "a ReturnToClient attempt ends the request without switching"
        );
        assert_eq!(attempts[0].provider_id, "a");
        assert_eq!(attempts[0].status, status);
        assert_eq!(attempts[0].result, UsageResult::Failure);
        assert_eq!(attempts[0].error_message.as_deref(), Some("bad request"));

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
    let home = isolated_temp_home("e2e-non-json-2xx");
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
        let mut attempts = Vec::new();
        let response = super::runtime_http::attempt_non_streaming(
            &[a.clone(), b.clone()],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
            &mut attempts,
        )
        .await;
        assert_eq!(response.status, 200, "attempt {attempt}");
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].provider_id, "a");
        assert_eq!(attempts[0].result, UsageResult::Failure);
        assert_eq!(attempts[1].provider_id, "b");
        assert_eq!(attempts[1].result, UsageResult::Success);
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
    let home = isolated_temp_home("e2e-non-json-500");
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

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local-model"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;
    assert_eq!(response.status, 200);
    assert!(String::from_utf8_lossy(&response.body).contains("from-b"));
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].status, 500);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(
        attempts[0].error_message.as_deref(),
        Some("upstream error"),
        "an HTML body yields its readable text without markup"
    );
    assert_eq!(attempts[1].provider_id, "b");
    assert_eq!(attempts[1].result, UsageResult::Success);
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

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-model"})),
    )
    .await;
    assert_eq!(status, 502, "unexpected response: {text}");
    let body = assert_standard_error_envelope(&text);
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("Provider A"), "message: {message}");
    assert!(message.contains("Provider B"), "message: {message}");
    assert!(message.contains("HTTP 500"), "message: {message}");
    assert!(message.contains("HTTP 503"), "message: {message}");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-005 / REQ-003: end-to-end streaming all-unavailable is HTTP 502 JSON with
/// the standard envelope; it is no longer HTTP 200 SSE and carries no `[DONE]`.
#[tokio::test]
async fn end_to_end_all_unavailable_streaming_returns_502_json_envelope() {
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

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-model", "stream": true})),
    )
    .await;
    assert_eq!(
        status, 502,
        "streaming all-unavailable must answer HTTP 502: {text}"
    );
    assert!(
        content_type.contains("application/json"),
        "content-type must be application/json: {content_type}"
    );
    assert!(
        !content_type.contains("text/event-stream"),
        "a pre-stream failure must not be SSE: {content_type}"
    );
    assert!(
        !text.contains("data: [DONE]"),
        "the JSON error body must not carry an SSE terminator: {text}"
    );
    let value = assert_standard_error_envelope(&text);
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
        enabled: true,
        reasoning_efforts: Vec::new(),
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
    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(chat_body.clone()),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let (status, _, text) = call_gateway(
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
            enabled: true,
            reasoning_efforts: Vec::new(),
        },
        ModelMapping {
            local_model: "local-a".to_string(),
            upstream_model: "remote-a".to_string(),
            protocol: None,
            display_name: None,
            enabled: true,
            reasoning_efforts: Vec::new(),
        },
    ];
    let mut disabled = upstream_provider("p2", "Provider Two", &upstream_url, "sk", None);
    disabled.enabled = false;
    disabled.mappings = vec![ModelMapping {
        local_model: "local-disabled".to_string(),
        upstream_model: "x".to_string(),
        protocol: None,
        display_name: None,
        enabled: true,
        reasoning_efforts: Vec::new(),
    }];
    let mut auto_disabled = upstream_provider("p3", "Provider Three", &upstream_url, "sk", None);
    auto_disabled.auto_disabled = true;
    auto_disabled.mappings = vec![ModelMapping {
        local_model: "local-auto".to_string(),
        upstream_model: "x".to_string(),
        protocol: None,
        display_name: None,
        enabled: true,
        reasoning_efforts: Vec::new(),
    }];
    let no_model = upstream_provider("p4", "Provider Four", &upstream_url, "sk", None);
    config
        .providers
        .extend([active, disabled, auto_disabled, no_model]);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, text) = call_gateway(
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

    let (status, _, text) = call_gateway(
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

    let (status, _, _) = call_gateway(
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

/// Finding A (error): `api_gateway_start` / `api_gateway_stop` must persist
/// `GatewayConfig.enabled` so the enable intent survives a reload (AC-003,
/// AC-019). The test reads the flag back from disk after each command.
#[tokio::test]
async fn api_gateway_start_and_stop_persist_enabled_flag() {
    let _home = temp_home("enabled-persist");
    let port = free_port().await;
    let mut config = config_with_key(port);
    config.enabled = false;
    super::storage::write_config(&config).unwrap();

    let started = super::commands::api_gateway_start().await.unwrap();
    assert!(started.running, "start must report a running server");
    let enabled_after_start = super::storage::read_config().unwrap().enabled;

    let stopped = super::commands::api_gateway_stop().await.unwrap();
    assert!(!stopped.running, "stop must report a stopped server");
    let enabled_after_stop = super::storage::read_config().unwrap().enabled;

    assert!(
        enabled_after_start,
        "api_gateway_start must persist enabled=true (reloaded {enabled_after_start})"
    );
    assert!(
        !enabled_after_stop,
        "api_gateway_stop must persist enabled=false (reloaded {enabled_after_stop})"
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
/// Deliberately real-time (not `start_paused`): this drives a genuine connect to
/// a non-routable TEST-NET address. The connect is completed by the real OS
/// network stack, so a paused clock cannot coordinate it: measured under
/// `start_paused` the attempt raced to the 60s read timeout and never observed
/// the real route failure, changing which production timeout fired. The 15s
/// wall bound below is therefore a real-time guard.
#[tokio::test]
async fn blackhole_connection_timeout_is_retryable_and_switches_within_bound() {
    let _home = isolated_temp_home("connect-timeout-retry");
    let blackhole_url = "http://192.0.2.1:81";
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", blackhole_url, "sk-a", Some("remote-model"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    let mut attempts = Vec::new();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        super::runtime_http::attempt_non_streaming(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
            &HashMap::new(),
            &mut attempts,
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
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert!(attempts[0].duration_ms >= 1);
    // A direct connect timeout records status 0 with a `network error: …` text.
    // This environment proxies TEST-NET-1 and answers a real HTTP 502 instead,
    // so neither value is asserted here; the local-mock tests
    // `end_to_end_network_failure_falls_back_and_tries_first_candidate_once` and
    // `streaming_all_unavailable_network_error_logs_zero_status` pin that shape.
    assert_eq!(attempts[1].provider_id, "b");
    assert_eq!(attempts[1].result, UsageResult::Success);
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
#[tokio::test(start_paused = true)]
async fn unresponsive_upstream_is_a_retryable_timeout_not_a_hang() {
    let _home = isolated_temp_home("unresponsive-timeout");
    let hang_url = spawn_unresponsive_upstream().await;
    let (ok_url, ok_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "from-b"}))).await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &hang_url, "sk-a", Some("remote-model"));
    let b = upstream_provider("b", "Provider B", &ok_url, "sk-b", Some("remote-model"));
    config.providers.push(a.clone());
    config.providers.push(b.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    let (response, _elapsed) = tokio::time::timeout(
        std::time::Duration::from_secs(75),
        attempt_non_streaming_paused(
            &[a, b],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
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
#[tokio::test(start_paused = true)]
async fn slow_first_byte_upstream_is_served_within_the_relaxed_budget() {
    let _home = isolated_temp_home("slow-first-byte");
    let slow_url = spawn_slow_first_byte_upstream().await;

    let mut config = config_with_key(0);
    let a = upstream_provider("a", "Provider A", &slow_url, "sk-a", Some("remote-model"));
    config.providers.push(a.clone());
    let body = serde_json::to_vec(&json!({"model": "local-model"})).unwrap();

    let (response, _elapsed) = tokio::time::timeout(
        std::time::Duration::from_secs(40),
        attempt_non_streaming_paused(
            &[a],
            "/v1/chat/completions",
            &body,
            Some("local-model"),
            &mut config,
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
    let _home = isolated_temp_home("stream-non-json-2xx");
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
    let mut attempts = Vec::new();
    super::runtime_http::attempt_streaming(
        &mut server,
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await
    .unwrap();
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out);

    assert_eq!(
        attempts.len(),
        2,
        "the rejected 2xx stream and the served stream both completed"
    );
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(
        attempts[0].status, 502,
        "a 2xx body that is not a valid SSE stream keeps the gateway stream failure status"
    );
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(
        attempts[0].error_message.as_deref(),
        Some("this is not json"),
        "the rejected body yields its readable summary"
    );
    assert!(attempts[0].duration_ms >= 1);
    assert_eq!(attempts[1].provider_id, "b");
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);
    assert!(attempts[1].duration_ms >= 1);
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
    super::commands::api_gateway_start().await.unwrap();

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

    super::commands::api_gateway_stop().await.unwrap();

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
/// service provider list. `tool_config.api_gateway_gateway == true` is the stable
/// marker emitted by `build_gateway_provider`.
fn managed_gateway_provider(id: &str, tool: &str) -> Value {
    json!({
        "id": id,
        "tool": tool,
        "name": "API Gateway",
        "base_url": "http://127.0.0.1:17688",
        "api_key": "previous-local-key",
        "tool_config": {
            "api_gateway_gateway": true,
            "wire_api": "chat",
        }
    })
}

/// A user-owned provider record without the API Gateway gateway marker. The
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
/// provider, plus an unrelated provider.
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
                "api_gateway_gateway": true
            },
            unmarked_user_provider("user-oc", "opencode"),
            {
                "id": "unrelated-other",
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
fn gateway_config(port: u16) -> GatewayConfig {
    let mut config = GatewayConfig::default();
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
/// gateway model mapping. The submitted payload itself carries no
/// `active`/`is_active` flag; opencode activation plus projection to
/// opencode.json are applied separately via the service-provider active list
/// and projection after the upsert succeeds.
#[tokio::test]
async fn terminal_sync_with_seam_creates_one_gateway_provider_per_tool() {
    let _home = isolated_temp_home("terminal-sync-seam-create");
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
    assert_eq!(opencode["tool_config"]["api_gateway_gateway"], true);
    assert_eq!(opencode["provider_key"], "apigateway");
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
    assert_eq!(codex["tool_config"]["api_gateway_gateway"], true);
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
    let _home = isolated_temp_home("terminal-sync-seam-idempotent");
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
    let _home = isolated_temp_home("terminal-sync-seam-ledger-reuse");
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
        "an unrelated provider must not be touched: {submitted:?}"
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
    let _home = isolated_temp_home("terminal-sync-seam-stale-ledger");
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
    let _home = isolated_temp_home("terminal-sync-seam-stale-ledger-marker");
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
    let _home = isolated_temp_home("terminal-sync-seam-marker-no-ledger");
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
    let _home = isolated_temp_home("terminal-sync-seam-marker-missing-ledger-id");
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
    let _home = isolated_temp_home("terminal-sync-seam-error");
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
    let _home = isolated_temp_home("terminal-sync-seam-empty");
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
    let _home = isolated_temp_home("terminal-sync-seam-unsupported");
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
    let _home = isolated_temp_home("terminal-sync-seam-no-key");
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
// `api_gateway_terminal_targets` command delegates to. It must recognize a
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
    let config = GatewayConfig::default();
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
    let mut config = GatewayConfig::default();
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
        synced_base_url: "http://127.0.0.1:17688/v1".to_string(),
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
        synced_base_url: "http://127.0.0.1:17688/v1".to_string(),
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
        synced_base_url: "http://127.0.0.1:17688/v1".to_string(),
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
// Plan 20260916-api-gateway-per-model-endpoint, Step 1 (RED)
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

/// Assert the OpenAI standard error envelope (AC-010): `error.message`,
/// `error.type` and `error.code` are non-empty strings and `error.param` exists
/// as `null`. Returns the parsed body so a caller can assert a specific code or
/// message.
fn assert_standard_error_envelope(text: &str) -> Value {
    let body: Value = serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("error body must be valid JSON ({error}): {text}"));
    let error = body
        .get("error")
        .unwrap_or_else(|| panic!("response must carry an error object: {text}"));
    for field in ["message", "type", "code"] {
        let value = error.get(field).and_then(Value::as_str).unwrap_or("");
        assert!(
            !value.is_empty(),
            "error.{field} must be a non-empty string: {text}"
        );
    }
    match error.get("param") {
        Some(param) => assert!(param.is_null(), "error.param must be null: {text}"),
        None => panic!("error object must carry a param field: {text}"),
    }
    body
}

/// Send one raw HTTP/1.1 request to the gateway and return the full response.
/// Used for malformed requests/bodies that `call_gateway`'s JSON encoder cannot
/// produce, and for direct `attempt_streaming` transport assertions.
async fn call_gateway_raw(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect gateway");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write raw request");
    stream.shutdown().await.expect("half-close raw request");
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.expect("read raw response");
    String::from_utf8_lossy(&out).into_owned()
}

/// Split a raw relay response (status line + headers + body) into its status
/// line and body so a direct-call streaming test can assert the transport
/// without a full HTTP client.
fn raw_http_status_and_body(text: &str) -> (String, String) {
    let (head, body) = text
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("raw response is missing the header terminator: {text}"));
    let status = head.lines().next().unwrap_or_default().to_string();
    (status, body.to_string())
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
        let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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

    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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

    let (responses_status, _, responses_text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "deepseek-v4.1-flash", "input": "hi"})),
    )
    .await;
    let (chat_status, _, chat_text) = call_gateway(
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

    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "mimo-v2.5", "input": "hi"})),
    )
    .await;
    let (stream_status, stream_content_type, stream_text) = call_gateway(
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

    assert_eq!(status, 502, "non-streaming mismatch must be 502: {text}");
    let body = assert_standard_error_envelope(&text);
    assert_eq!(
        body["error"]["code"], "all_providers_unavailable",
        "body={text}"
    );

    assert_eq!(
        stream_status, 502,
        "a pre-stream mismatch must answer HTTP 502: {stream_text}"
    );
    assert!(
        stream_content_type.contains("application/json"),
        "streaming mismatch content-type must be JSON: {stream_content_type}"
    );
    assert!(
        !stream_content_type.contains("text/event-stream"),
        "a pre-stream mismatch must not answer SSE: {stream_content_type}"
    );
    assert!(!stream_text.contains("data: [DONE]"), "body: {stream_text}");
    let stream_body = assert_standard_error_envelope(&stream_text);
    assert_eq!(
        stream_body["error"], body["error"],
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
        enabled: true,
        reasoning_efforts: Vec::new(),
    }];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (responses_status, _, responses_text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "unknown-local", "input": "hi"})),
    )
    .await;
    let (chat_status, _, chat_text) = call_gateway(
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
    let body = assert_standard_error_envelope(&chat_text);
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

    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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

    let (chat_status, _, chat_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "shared-model", "messages": []})),
    )
    .await;
    let (responses_status, _, responses_text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "shared-model", "input": "hi"})),
    )
    .await;
    let (models_status, _, models_text) = call_gateway(
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

    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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

    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "mimo-v2.5", "input": "hi"})),
    )
    .await;
    let (stream_status, stream_content_type, stream_text) = call_gateway(
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
    let body = assert_standard_error_envelope(&text);
    assert_eq!(
        body["error"]["code"], "all_providers_unavailable",
        "{summary}"
    );
    let message = body["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("/chat/completions"),
        "the 502 must name the endpoint the requested model is configured for: {summary}"
    );

    assert_eq!(stream_status, 502, "{summary}");
    assert!(
        stream_content_type.contains("application/json"),
        "{summary}"
    );
    assert!(
        !stream_content_type.contains("text/event-stream"),
        "{summary}"
    );
    let stream_body = assert_standard_error_envelope(&stream_text);
    let stream_message = stream_body["error"]["message"].as_str().unwrap_or("");
    assert!(
        stream_message.contains("/chat/completions"),
        "the streaming error must carry the same endpoint hint: {summary}"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-007: a mapping without a protocol field (the shape of existing encrypted
/// configs) inherits the provider protocol instead of defaulting to
/// `chat_completions`.
#[tokio::test]
async fn mapping_without_protocol_inherits_the_provider_protocol() {
    let home = temp_home("per-model-inherit");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut config = config_with_key(port);
    let mut provider = upstream_provider("p1", "OpenCode Go", &upstream_url, "sk-upstream", None);
    provider.protocol = UpstreamProtocol::Responses;
    provider.mappings = vec![ModelMapping {
        local_model: "inherit-local".to_string(),
        upstream_model: "inherit-remote".to_string(),
        protocol: None,
        display_name: None,
        enabled: true,
        reasoning_efforts: Vec::new(),
    }];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "inherit-local", "input": "hi"})),
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
        sent["model"], "inherit-remote",
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
    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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

    let (responses_status, _, responses_text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "inherit-local", "input": "hi"})),
    )
    .await;
    let (chat_status, _, chat_text) = call_gateway(
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
    let body = assert_standard_error_envelope(&chat_text);
    assert_eq!(
        body["error"]["code"], "all_providers_unavailable",
        "body={chat_text}"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// Plan 20260916-api-gateway-per-model-endpoint, Step 3 (cross-module E2E)
//
// Each case drives the real local relay listener (`call_gateway` -> loopback
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

    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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

    let (responses_status, responses_type, responses_text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "deepseek-v4.1-flash", "input": "hi", "stream": true})),
    )
    .await;
    let (chat_status, chat_type, chat_text) = call_gateway(
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

    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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
        let (status, _content_type, text) = call_gateway(
            port,
            "POST",
            "/v1/responses",
            &[("authorization", "Bearer local-key")],
            Some(json!({"model": "mimo-v2.5", "input": "hi"})),
        )
        .await;
        assert_eq!(
            status, 502,
            "mismatched attempt {attempt} must stay all-unavailable: {text}"
        );
        let body = assert_standard_error_envelope(&text);
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
        enabled: true,
        reasoning_efforts: Vec::new(),
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
        enabled: true,
        reasoning_efforts: Vec::new(),
    }];
    config.providers.push(chat_record);
    config.providers.push(responses_record);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (chat_status, _, chat_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "shared-model", "messages": []})),
    )
    .await;
    let (responses_status, _, responses_text) = call_gateway(
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

    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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

    let (responses_status, _, responses_text) = call_gateway(
        port,
        "POST",
        "/v1/responses",
        &[("authorization", "Bearer local-key")],
        Some(responses_body.clone()),
    )
    .await;
    let (chat_status, _, chat_text) = call_gateway(
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

    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
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

    let (status, _content_type, text) = call_gateway(
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
// Step 2 (20260916-api-gateway-upstream-retry): cooldown, retry headers, budget
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
    let (url, count, _arrivals) = spawn_header_sequence_mock_recording(replies).await;
    (url, count)
}

/// Accepted-request arrivals on the current tokio clock. Paused tests use the
/// delta between two arrivals to bound a retry wait without folding in the
/// auto-advance that happens while real loopback I/O is in flight.
type HeaderArrivals = Arc<Mutex<Vec<tokio::time::Instant>>>;

/// `spawn_header_sequence_mock` plus the tokio `Instant` of each accepted
/// request, so a paused-clock test can assert the retry wait between two
/// attempts to the same provider.
async fn spawn_header_sequence_mock_recording(
    replies: Vec<HeaderReply>,
) -> (String, Arc<AtomicUsize>, HeaderArrivals) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    let count = Arc::new(AtomicUsize::new(0));
    let count_for_server = count.clone();
    let arrivals: HeaderArrivals = Arc::new(Mutex::new(Vec::new()));
    let arrivals_for_server = arrivals.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let count = count_for_server.clone();
            let replies = replies.clone();
            let arrivals = arrivals_for_server.clone();
            tokio::spawn(async move {
                let Ok(_request) = super::runtime_http::read_http_request(&mut stream).await else {
                    return;
                };
                arrivals.lock().expect("header mock arrivals").push(tokio::time::Instant::now());
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
    (format!("http://{}", addr), count, arrivals)
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
    ordered: &[GatewayUpstreamProvider],
    config: &mut GatewayConfig,
) -> String {
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();
    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    // The buffer is per request and owned by the caller; this helper only checks
    // the transmitted text, so its own buffer stays local.
    let mut attempts = Vec::new();
    super::runtime_http::attempt_streaming(
        &mut server,
        ordered,
        "/v1/chat/completions",
        &body,
        Some("local"),
        config,
        &HashMap::new(),
        &mut attempts,
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
    ordered: &[GatewayUpstreamProvider],
    config: &mut GatewayConfig,
) -> (super::runtime_http::HttpResponse, std::time::Duration) {
    let _ticker = spawn_paused_clock_ticker();
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();
    let started = tokio::time::Instant::now();
    // Local per-request buffer: this helper only reports the response and the
    // paused elapsed time.
    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        ordered,
        "/v1/chat/completions",
        &body,
        Some("local"),
        config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;
    (response, started.elapsed())
}

/// Like `attempt_non_streaming_timed`, but for callers that build their own
/// request body/requested model and just need the paused clock plus the bounded
/// 1ms ticker so real loopback I/O completes before reqwest's real timeouts.
async fn attempt_non_streaming_paused(
    ordered: &[GatewayUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    config: &mut GatewayConfig,
) -> (super::runtime_http::HttpResponse, std::time::Duration) {
    let _ticker = spawn_paused_clock_ticker();
    let started = tokio::time::Instant::now();
    // Local per-request buffer: this helper only reports the response and the
    // paused elapsed time.
    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        ordered,
        path,
        body,
        requested,
        config,
        &HashMap::new(),
        &mut attempts,
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

/// AC-003 / REQ-002 regression (migrated to two candidates): `retry-after-ms`
/// wins over `retry-after` seconds. A carries both `retry-after-ms: 1500` and
/// `retry-after: 9` on its 500 and must be retried after ~1500ms, not 9s and
/// not the default jittered backoff. B advertises a far-later cooldown so A's
/// own header governs the earliest-deadline retry.
#[tokio::test(start_paused = true)]
async fn retry_policy_retry_after_ms_wins_over_seconds() {
    let _home = isolated_temp_home("retry-policy-header-priority");
    let (a_url, a_requests, arrivals) = spawn_header_sequence_mock_recording(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "1500")
            .header("retry-after", "9"),
        HeaderReply::new(200, json!({"id": "recovered", "choices": []})),
    ])
    .await;
    let (b_url, b_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "60000")])
        .await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let (response, _total_elapsed) =
        attempt_non_streaming_timed(&[a.clone(), b.clone()], &mut config).await;
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(response.status, 200, "must recover on the retry: {body_text}");
    assert_eq!(
        a_requests.load(Ordering::SeqCst),
        2,
        "A gets one initial attempt plus one header-timed retry"
    );
    assert_eq!(
        b_requests.load(Ordering::SeqCst),
        1,
        "B's 60s cooldown is later, so it must not be retried before A recovers"
    );
    // Measure the wait between the two upstream attempts: the whole-request clock
    // also advances while real loopback I/O is in flight under `start_paused`.
    let arrivals = arrivals.lock().expect("header mock arrivals");
    assert_eq!(arrivals.len(), 2, "one initial attempt plus one header-timed retry");
    let wait = arrivals[1] - arrivals[0];
    assert!(
        wait >= millis(1400) && wait <= millis(1900),
        "retry-after-ms: 1500 must win over retry-after: 9, paused wait between attempts was {wait:?}"
    );
}

/// AC-003 / REQ-002 regression (migrated to two candidates): an invalid
/// high-priority `retry-after-ms` header makes A keep looking at lower
/// priorities, so `retry-after: 3` gives a ~3s wait instead of the default
/// backoff. B's far-later cooldown leaves A's header as the earliest deadline.
#[tokio::test(start_paused = true)]
async fn retry_policy_invalid_retry_after_ms_falls_back_to_seconds_header() {
    let _home = isolated_temp_home("retry-policy-header-invalid-ms");
    let (a_url, a_requests, arrivals) = spawn_header_sequence_mock_recording(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "not-a-number")
            .header("retry-after", "3"),
        HeaderReply::new(200, json!({"id": "recovered", "choices": []})),
    ])
    .await;
    let (b_url, b_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "60000")])
        .await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let (response, _total_elapsed) =
        attempt_non_streaming_timed(&[a.clone(), b.clone()], &mut config).await;
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(
        a_requests.load(Ordering::SeqCst),
        2,
        "A gets one initial attempt plus one header-timed retry"
    );
    assert_eq!(
        b_requests.load(Ordering::SeqCst),
        1,
        "B's 60s cooldown is later, so it must not be retried before A recovers"
    );
    assert_eq!(response.status, 200, "must recover on the retry: {body_text}");
    // Measure the wait between the two upstream attempts instead of the whole
    // request: the whole-request clock also advances while real loopback I/O is
    // in flight under `start_paused`, which is unrelated to the retry header.
    let arrivals = arrivals.lock().expect("header mock arrivals");
    assert_eq!(arrivals.len(), 2, "one initial attempt plus one header-timed retry");
    let wait = arrivals[1] - arrivals[0];
    assert!(
        wait >= millis(2800) && wait <= millis(3400),
        "an invalid retry-after-ms must fall back to retry-after: 3, paused wait between attempts was {wait:?}"
    );
}

/// AC-003 / REQ-002 regression (migrated to two candidates): a future HTTP date
/// in A's `retry-after` is honored. The header is built ~10s in the future, so
/// A's retry must wait clearly longer than the default jittered backoff (~2s).
/// B advertises a far-later cooldown so A's date remains the earliest deadline.
#[tokio::test(start_paused = true)]
async fn retry_policy_future_http_date_is_honored() {
    let _home = isolated_temp_home("retry-policy-header-date");
    let when = chrono::Utc::now() + chrono::Duration::seconds(10);
    let http_date = when.format("%a, %d %b %Y %H:%M:%S GMT").to_string();

    let (a_url, a_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after", http_date),
        HeaderReply::new(200, json!({"id": "recovered", "choices": []})),
    ])
    .await;
    let (b_url, b_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "60000")])
        .await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let (response, elapsed) =
        attempt_non_streaming_timed(&[a.clone(), b.clone()], &mut config).await;
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(
        a_requests.load(Ordering::SeqCst),
        2,
        "A gets one initial attempt plus one header-timed retry"
    );
    assert_eq!(
        b_requests.load(Ordering::SeqCst),
        1,
        "B's 60s cooldown is later, so it must not be retried before A recovers"
    );
    assert_eq!(response.status, 200, "must recover on the retry: {body_text}");
    assert!(
        elapsed >= millis(5000) && elapsed <= millis(11000),
        "a future HTTP date must be honored, paused elapsed was {elapsed:?}"
    );
}

/// AC-003 / REQ-002 regression (migrated to two candidates): `retry-after-ms: 0`
/// means A retries immediately with no backoff. B's far-later cooldown keeps A
/// as the earliest ready candidate, so the zero-delay observation is preserved.
#[tokio::test(start_paused = true)]
async fn retry_policy_zero_retry_after_ms_retries_immediately() {
    let _home = isolated_temp_home("retry-policy-header-zero");
    let (a_url, a_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "0"),
        HeaderReply::new(200, json!({"id": "recovered", "choices": []})),
    ])
    .await;
    let (b_url, b_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "60000")])
        .await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let (response, elapsed) =
        attempt_non_streaming_timed(&[a.clone(), b.clone()], &mut config).await;
    let body_text = String::from_utf8_lossy(&response.body);

    assert_eq!(
        a_requests.load(Ordering::SeqCst),
        2,
        "A gets one initial attempt plus one immediate retry"
    );
    assert_eq!(
        b_requests.load(Ordering::SeqCst),
        1,
        "B's 60s cooldown is later, so it must not be retried before A recovers"
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
    let _home = isolated_temp_home("retry-policy-initial-no-wait");
    let (a_url, a_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "9000"),
    ])
    .await;
    let (b_url, b_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(200, json!({"id": "from-b"}))]).await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let mut attempts = Vec::new();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        super::runtime_http::attempt_non_streaming(
            &[a.clone(), b.clone()],
            "/v1/chat/completions",
            &serde_json::to_vec(&json!({"model": "local"})).unwrap(),
            Some("local"),
            &mut config,
            &HashMap::new(),
            &mut attempts,
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
    assert_eq!(
        attempts.len(),
        2,
        "A's completed failure and B's success are both buffered"
    );
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].upstream_model, "remote-default");
    assert_eq!(attempts[0].status, 500);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(attempts[0].error_message.as_deref(), Some("busy"));
    assert!(attempts[0].duration_ms >= 1);
    assert_eq!(attempts[1].provider_id, "b");
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);
    assert_eq!(attempts[1].error_message, None);
    assert!(attempts[1].duration_ms >= 1);
}

/// AC-003 / REQ-003 RED: a provider cooling down for 9s must not block a second
/// provider whose own header is ready after ~1500ms. A is tried once, B is
/// retried at its own deadline and answers, so A is never retried. Current code
/// ignores both headers, retries A first six times, and takes ~60s, so this
/// fails on the attempt count and elapsed time.
#[tokio::test(start_paused = true)]
async fn retry_policy_cooling_provider_does_not_block_ready_candidate() {
    let _home = isolated_temp_home("retry-policy-cooling-does-not-block");
    let (a_url, a_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "9000"),
    ])
    .await;
    let (b_url, b_requests, b_arrivals) = spawn_header_sequence_mock_recording(vec![
        HeaderReply::new(500, json!({"error": {"message": "busy"}}))
            .header("retry-after-ms", "1500"),
        HeaderReply::new(200, json!({"id": "from-b"})),
    ])
    .await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let (response, _total_elapsed) =
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
    // A's initial I/O also advances the paused clock, so measure B's own wait
    // between its two attempts rather than the whole request.
    let b_arrivals = b_arrivals.lock().expect("header mock arrivals");
    assert_eq!(b_arrivals.len(), 2, "B must be tried once then retried once");
    let wait = b_arrivals[1] - b_arrivals[0];
    assert!(
        wait >= millis(1400) && wait <= millis(1900),
        "the ready candidate's deadline must govern the wait, paused wait between B attempts was {wait:?}"
    );
}

/// AC-003 / REQ-002 regression (migrated to two candidates): a provider that
/// keeps failing is contacted at most six times in one request (one initial
/// plus five bounded retries). A is always preferred first because both
/// candidates have zero-delay headers and A precedes B on ties, so A exhausts
/// its own cap before B is ever retried; both counters are asserted.
#[tokio::test(start_paused = true)]
async fn retry_policy_provider_is_attempted_at_most_six_times() {
    let _home = isolated_temp_home("retry-policy-six-attempts");
    let (a_url, a_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(500, json!({"error": {"message": "always failing"}}))
            .header("retry-after-ms", "0")])
            .await;
    let (b_url, b_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(500, json!({"error": {"message": "always failing"}}))
            .header("retry-after-ms", "0")])
            .await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let (response, _elapsed) =
        attempt_non_streaming_timed(&[a.clone(), b.clone()], &mut config).await;

    assert_eq!(
        a_requests.load(Ordering::SeqCst),
        6,
        "A is attempted at most six times in a request"
    );
    assert_eq!(
        b_requests.load(Ordering::SeqCst),
        6,
        "B is also capped at six attempts once A is exhausted"
    );
    assert_eq!(
        response.status, 502,
        "exhausted candidates return the all-unavailable error"
    );
}

/// AC-003 / REQ-002 regression (migrated to two candidates): the cumulative
/// actual wait is capped at 120s. With a 60s `retry-after-ms` on every failure
/// from both candidates, the scheduler alternates A/B at the 60s and 120s
/// deadlines (A first on ties), so each provider is attempted three times and
/// the next 60s wait exceeds the exhausted budget. Recomputed for two
/// candidates: A=3, B=3, paused elapsed exactly the 120s budget.
#[tokio::test(start_paused = true)]
async fn retry_policy_stops_before_wait_exceeds_120s_budget() {
    let _home = isolated_temp_home("retry-policy-budget-120s");
    let (a_url, a_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(500, json!({"error": {"message": "always failing"}}))
            .header("retry-after-ms", "60000")])
            .await;
    let (b_url, b_requests) =
        spawn_header_sequence_mock(vec![HeaderReply::new(500, json!({"error": {"message": "always failing"}}))
            .header("retry-after-ms", "60000")])
            .await;

    let a = upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone(), b.clone()];

    let (response, elapsed) =
        attempt_non_streaming_timed(&[a.clone(), b.clone()], &mut config).await;

    assert_eq!(
        a_requests.load(Ordering::SeqCst),
        3,
        "A gets one initial attempt plus two 60s waits inside the 120s budget"
    );
    assert_eq!(
        b_requests.load(Ordering::SeqCst),
        3,
        "B gets one initial attempt plus two 60s waits inside the 120s budget"
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
// Step 1 (20260918-gateway-retry-and-openai-errors): single-candidate fast fail
// ---------------------------------------------------------------------------

/// AC-001 / REQ-001 RED: exactly one serviceable candidate that returns HTTP 500
/// must be attempted once and then fail the request immediately with the
/// standard 502 envelope. Today the single candidate enters the retry queue, so
/// the upstream is contacted six times (the `retry-after-ms: 0` header keeps
/// this RED run fast and must never be consumed as a wait).
#[tokio::test]
async fn single_candidate_500_non_streaming_fails_fast_without_retry() {
    let _home = temp_home("single-candidate-500-non-streaming");
    let (upstream_url, upstream_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(500, json!({"error": {"message": "boom"}}))
            .header("retry-after-ms", "0"),
    ])
    .await;

    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone()];
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        std::slice::from_ref(&a),
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;

    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        1,
        "a single candidate must be attempted exactly once with no backoff retry"
    );
    assert_eq!(
        attempts.len(),
        1,
        "one entry for the single completed failure attempt"
    );
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].provider_name, "Provider A");
    assert_eq!(attempts[0].upstream_model, "remote-default");
    assert_eq!(attempts[0].status, 500);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(attempts[0].error_message.as_deref(), Some("boom"));
    assert!(attempts[0].usage.is_none());
    assert!(
        attempts[0].duration_ms >= 1,
        "the attempt measures its own elapsed time"
    );
    assert_eq!(response.status, 502, "the gateway must fail with HTTP 502");
    let parsed: Value =
        serde_json::from_slice(&response.body).expect("standard JSON error envelope");
    assert_eq!(
        parsed.pointer("/error/code").and_then(|value| value.as_str()),
        Some("all_providers_unavailable"),
        "body: {}",
        String::from_utf8_lossy(&response.body)
    );
}

/// AC-002 / REQ-001 RED: exactly one serviceable candidate that returns HTTP 429
/// with a valid `retry-after-ms` header must not consume that header for a wait;
/// it is attempted once and the request fails with the standard 502 envelope.
#[tokio::test]
async fn single_candidate_429_with_retry_header_non_streaming_fails_fast_without_retry() {
    let _home = temp_home("single-candidate-429-header-non-streaming");
    let (upstream_url, upstream_requests) = spawn_header_sequence_mock(vec![
        HeaderReply::new(429, json!({"error": {"message": "slow down"}}))
            .header("retry-after-ms", "0"),
    ])
    .await;

    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone()];
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        std::slice::from_ref(&a),
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;

    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        1,
        "a single 429 candidate must be attempted exactly once"
    );
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].upstream_model, "remote-default");
    assert_eq!(attempts[0].status, 429);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(attempts[0].error_message.as_deref(), Some("slow down"));
    assert!(attempts[0].duration_ms >= 1);
    assert_eq!(response.status, 502, "the gateway must fail with HTTP 502");
    let parsed: Value =
        serde_json::from_slice(&response.body).expect("standard JSON error envelope");
    assert_eq!(
        parsed.pointer("/error/code").and_then(|value| value.as_str()),
        Some("all_providers_unavailable"),
        "body: {}",
        String::from_utf8_lossy(&response.body)
    );
}

/// AC-002 / REQ-001 RED: a single-candidate 429 *without* a retry header must
/// still be attempted exactly once and fail immediately; the paused clock keeps
/// the default-backoff RED run fast (today it retries six times).
#[tokio::test(start_paused = true)]
async fn single_candidate_429_without_retry_header_non_streaming_fails_fast_without_retry() {
    let _home = temp_home("single-candidate-429-no-header-non-streaming");
    let _ticker = spawn_paused_clock_ticker();
    let (upstream_url, upstream_requests) = spawn_header_sequence_mock(vec![HeaderReply::new(
        429,
        json!({"error": {"message": "slow down"}}),
    )])
    .await;

    let a = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone()];
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        std::slice::from_ref(&a),
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;

    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        1,
        "a single 429 candidate must be attempted exactly once even without a retry header"
    );
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].status, 429);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(attempts[0].error_message.as_deref(), Some("slow down"));
    assert!(attempts[0].duration_ms >= 1);
    assert_eq!(response.status, 502, "the gateway must fail with HTTP 502");
    let parsed: Value =
        serde_json::from_slice(&response.body).expect("standard JSON error envelope");
    assert_eq!(
        parsed.pointer("/error/code").and_then(|value| value.as_str()),
        Some("all_providers_unavailable"),
        "body: {}",
        String::from_utf8_lossy(&response.body)
    );
}

/// AC-011 / REQ-001 + AC-005 / REQ-003: a single serviceable streaming
/// candidate whose upstream fails retryably before any byte is contacted
/// exactly once and the terminal transport is HTTP 502 + `application/json`
/// with the standard envelope, never HTTP 200 SSE. `attempt_streaming_text`
/// drives the real streaming path and drains the response, proving the single
/// attempt does not hang.
#[tokio::test]
async fn single_candidate_streaming_retryable_failure_attempts_upstream_once() {
    let _home = temp_home("single-candidate-streaming-fast-fail");
    let (upstream_url, upstream_requests) =
        spawn_streaming_sequence_mock(vec![StreamingReply::Status {
            status: 500,
            content_type: "application/json",
            body: br#"{"error":{"message":"boom"}}"#.to_vec(),
            headers: vec![("retry-after-ms", "0")],
        }])
        .await;

    let provider =
        upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers.push(provider.clone());

    let text = attempt_streaming_text(std::slice::from_ref(&provider), &mut config).await;

    assert_eq!(
        upstream_requests.load(Ordering::SeqCst),
        1,
        "a single streaming candidate must be attempted exactly once with no backoff retry"
    );
    let (status_line, body) = raw_http_status_and_body(&text);
    assert!(
        status_line.starts_with("HTTP/1.1 502"),
        "a pre-stream failure must answer HTTP 502: {text}"
    );
    assert!(
        text.to_ascii_lowercase()
            .contains("content-type: application/json"),
        "content-type must be application/json: {text}"
    );
    assert!(
        !text.contains("text/event-stream"),
        "a pre-stream failure must not answer SSE: {text}"
    );
    let envelope = assert_standard_error_envelope(&body);
    assert_eq!(envelope["error"]["code"], "all_providers_unavailable");
}

// ---------------------------------------------------------------------------
// Step 3 (20260916-api-gateway-upstream-retry): streaming retry and health RED
// ---------------------------------------------------------------------------

/// REQ-002/REQ-005 regression (migrated to two candidates): before a stream
/// emits any bytes, a retryable 503 with `retry-after-ms: 0` is retried
/// immediately. A recovers on its second attempt; B also advertises a zero
/// cooldown but A wins the tie, so B is only tried on the first pass. Only the
/// completed SSE stream is success: it clears A's previously seeded health
/// without first counting the transient attempt as a separate inbound-request
/// failure.
#[tokio::test]
async fn retry_stream_recovers_after_zero_cooldown_and_completed_sse_clears_health() {
    let _home = isolated_temp_home("retry-stream-recovery-health");
    let (a_url, a_attempts) = spawn_streaming_sequence_mock(vec![
        StreamingReply::Status {
            status: 503,
            content_type: "application/json",
            body: br#"{"error":{"message":"busy"}}"#.to_vec(),
            headers: vec![("retry-after-ms", "0")],
        },
        StreamingReply::Sse("data: {\"id\":\"recovered-stream\"}\n\ndata: [DONE]\n\n".to_string()),
    ])
    .await;
    let (b_url, b_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Status {
        status: 503,
        content_type: "application/json",
        body: br#"{"error":{"message":"busy"}}"#.to_vec(),
        headers: vec![("retry-after-ms", "0")],
    }])
    .await;

    let mut provider =
        upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    provider.consecutive_failures = 2;
    provider.last_error_at = Some(1);
    let other = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers.push(provider.clone());
    config.providers.push(other.clone());

    let text = attempt_streaming_text(&[provider.clone(), other.clone()], &mut config).await;

    assert_eq!(
        a_attempts.load(Ordering::SeqCst),
        2,
        "A's pre-output 503 with retry-after-ms: 0 must be retried immediately"
    );
    assert_eq!(
        b_attempts.load(Ordering::SeqCst),
        1,
        "A wins the zero-deadline tie and recovers before B is retried"
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

/// REQ-002/REQ-005 regression (migrated to two candidates): a permanently
/// failing stream gets one initial try plus at most five retries per provider,
/// while provider health records the whole inbound request once. Both A and B
/// stay at zero cooldown, so A exhausts its cap first and then B; the terminal
/// transport shape (SSE today, 502 JSON after Step 2) is intentionally not
/// asserted here so the per-provider count and health observations stay stable.
#[tokio::test]
async fn retry_stream_persistent_503_attempts_six_times_and_counts_health_once() {
    let _home = isolated_temp_home("retry-stream-six-attempts-health");
    let (a_url, a_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Status {
        status: 503,
        content_type: "application/json",
        body: br#"{"error":{"message":"still busy"}}"#.to_vec(),
        headers: vec![("retry-after-ms", "0")],
    }])
    .await;
    let (b_url, b_attempts) = spawn_streaming_sequence_mock(vec![StreamingReply::Status {
        status: 503,
        content_type: "application/json",
        body: br#"{"error":{"message":"still busy"}}"#.to_vec(),
        headers: vec![("retry-after-ms", "0")],
    }])
    .await;

    let provider =
        upstream_provider("a", "Provider A", &a_url, "sk", Some("remote-default"));
    let other = upstream_provider("b", "Provider B", &b_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers.push(provider.clone());
    config.providers.push(other.clone());
    let text = attempt_streaming_text(&[provider.clone(), other.clone()], &mut config).await;

    assert_eq!(
        a_attempts.load(Ordering::SeqCst),
        6,
        "A gets one initial stream attempt plus five bounded retries"
    );
    assert_eq!(
        b_attempts.load(Ordering::SeqCst),
        6,
        "B gets one initial stream attempt plus five bounded retries once A is exhausted"
    );
    assert!(
        text.contains("all_providers_unavailable"),
        "exhausted stream: {text}"
    );
    let stored = config.providers.iter().find(|item| item.id == "a").unwrap();
    assert_eq!(
        stored.consecutive_failures, 1,
        "A's six upstream failures in one inbound request count once"
    );
    assert!(!stored.auto_disabled, "one failed request is below the threshold");
}

/// REQ-004/REQ-005: a rate-limited streaming candidate may yield to a healthy
/// candidate, but 429 itself never contributes provider health failure.
#[tokio::test]
async fn retry_stream_429_switches_without_counting_provider_health() {
    let _home = isolated_temp_home("retry-stream-429-no-health");
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
    let mut config = GatewayConfig::default();
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
        let _home = isolated_temp_home(&format!("retry-stream-html-auth-{status}"));
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
        let mut config = GatewayConfig::default();
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
    let _home = isolated_temp_home("retry-stream-404-traverse");
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
    let mut config = GatewayConfig::default();
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

/// REQ-004/AC-006: a 413 HTML response is a caller error, so the streaming
/// boundary keeps the original status but wraps the non-standard body in the
/// standard gateway envelope, without trying a fallback provider.
#[tokio::test]
async fn retry_stream_html_413_returns_unchanged_without_fallback() {
    let _home = isolated_temp_home("retry-stream-html-413");
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
    let mut config = GatewayConfig::default();
    config.providers = vec![rejected.clone(), fallback.clone()];

    let text = attempt_streaming_text(&[rejected, fallback], &mut config).await;

    assert!(text.starts_with("HTTP/1.1 413"), "response: {text}");
    let (_, response_body) = raw_http_status_and_body(&text);
    let envelope = assert_standard_error_envelope(&response_body);
    let message = envelope["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("413"),
        "the wrapped message must name the upstream status: {message}"
    );
    assert_eq!(rejected_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(fallback_attempts.load(Ordering::SeqCst), 0, "413 must not switch");
    assert_eq!(config.providers[0].consecutive_failures, 0, "413 must not count");
}

// ---------------------------------------------------------------------------
// Step 3 (20260916-api-gateway-upstream-retry): downstream cancellation RED
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

/// REQ-006 / AC-007 (extended by 20260920-gateway-log-attempts-and-upstream-errors):
/// after the client has fully disconnected, the relay must abandon the pending
/// work and must not issue a retry, while the log keeps every already completed
/// failure attempt as a non-terminal row and appends exactly one terminal
/// `cancelled` row with no provider. Both candidates answer 503 with their own
/// message and a short retry deadline, so the fallback-first pass completes both
/// attempts in either randomized candidate order and both retries are queued
/// behind the delay when the disconnect arrives. This covers both JSON and SSE
/// request modes through the real TCP handler boundary.
#[tokio::test(start_paused = true)]
async fn retry_cancel_disconnect_during_retry_delay_exits_without_further_upstream_attempts() {
    let _ticker = spawn_paused_clock_ticker();
    let mut failures = Vec::new();
    for wants_stream in [false, true] {
        let _home = isolated_temp_home(if wants_stream {
            "retry-cancel-delay-stream"
        } else {
            "retry-cancel-delay-json"
        });
        let (busy_url, busy_attempts) = spawn_header_sequence_mock(vec![
            HeaderReply::new(503, json!({"error": {"message": "busy"}}))
                .header("retry-after-ms", "250"),
        ])
        .await;
        let (waiting_url, waiting_attempts) = spawn_header_sequence_mock(vec![
            HeaderReply::new(503, json!({"error": {"message": "waiting"}}))
                .header("retry-after-ms", "250"),
        ])
        .await;
        let mut config = GatewayConfig::default();
        config.keys.push(key_named("k1", "local-key"));
        config.providers.push(upstream_provider(
            "a",
            "Busy Provider",
            &busy_url,
            "sk",
            Some("remote-default"),
        ));
        config.providers.push(upstream_provider(
            "b",
            "Waiting Provider",
            &waiting_url,
            "sk",
            Some("remote-default"),
        ));
        super::storage::write_config(&config).expect("write relay config");

        let (client, mut handler) = spawn_handle_connection(wants_stream).await;
        // Both candidates fail retryably in the fallback-first pass, so each
        // server sees exactly one request in either randomized order and both
        // retries are queued behind the 250ms delay when the client goes away.
        wait_for_upstream_attempts(&busy_attempts, 1, &mut handler).await;
        wait_for_upstream_attempts(&waiting_attempts, 1, &mut handler).await;
        // Each mock answers right after its counter reaches one, so both 503
        // bodies are already waiting to be read. Give the relay a scheduling
        // turn to buffer both attempts before the client goes away; the sleep
        // stays well inside the 250ms retry deadline, so no retry has started.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(client);

        let exited = tokio::time::timeout(std::time::Duration::from_millis(100), &mut handler).await;
        // Let a non-cancelling handler reach the retry deadline so the second
        // assertion proves the request did not continue upstream after close.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let observed_busy = busy_attempts.load(Ordering::SeqCst);
        let observed_waiting = waiting_attempts.load(Ordering::SeqCst);
        if exited.is_err() {
            handler.abort();
            let _ = handler.await;
        }

        if exited.is_err() {
            failures.push(format!(
                "handler remained pending during retry delay after disconnect (stream={wants_stream})"
            ));
        }
        if observed_busy != 1 || observed_waiting != 1 {
            failures.push(format!(
                "disconnect issued further upstream attempts during retry delay (stream={wants_stream}): busy={observed_busy}, waiting={observed_waiting}"
            ));
        }
        // Cancelled inbound requests persist no rows (REQ-001 / AC-002).
        // The two completed attempt failures and the synthetic cancelled row
        // are all discarded when the tool connection disappears during the
        // retry delay.
        let records = default_usage_store().all_records().unwrap_or_default();
        assert!(
            records.is_empty(),
            "a cancelled request must write zero log rows (completed attempts + synthetic terminal): {} observed",
            records.len()
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

/// REQ-006 / AC-006 RED: a client disconnect while an upstream response is
/// still pending cancels the relay immediately. The held upstream only replies
/// after the prompt-exit observation, so this cannot be satisfied by waiting
/// for the upstream read timeout. Non-streaming and SSE use the same public
/// connection boundary.
#[tokio::test(start_paused = true)]
async fn retry_cancel_disconnect_while_upstream_waits_exits_without_further_upstream_attempts() {
    let _ticker = spawn_paused_clock_ticker();
    let mut failures = Vec::new();
    for wants_stream in [false, true] {
        let _home = isolated_temp_home(if wants_stream {
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

        let mut config = GatewayConfig::default();
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

// ---------------------------------------------------------------------------
// 20260917-ai-gateway-usage-logs Step 1: pricing, retention, range and storage
// ---------------------------------------------------------------------------

fn tokens(input: u64, cache_read: u64, cache_write: u64, output: u64) -> UsageTokens {
    UsageTokens {
        input_tokens: input,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        output_tokens: output,
    }
}

fn sample_record(
    timestamp_ms: i64,
    local_model: &str,
    upstream_model: &str,
    provider_id: &str,
    provider_name: &str,
    result: UsageResult,
    amount: Option<f64>,
    token_counts: UsageTokens,
) -> UsageLogRecord {
    sample_attempt_record(
        timestamp_ms,
        local_model,
        upstream_model,
        provider_id,
        provider_name,
        result,
        true,
        None,
        amount,
        token_counts,
    )
}

/// One log row of a completed upstream attempt (REQ-001). `terminal` marks the
/// request's single terminal row; `error_message` carries the already extracted,
/// sanitized upstream failure text.
fn sample_attempt_record(
    timestamp_ms: i64,
    local_model: &str,
    upstream_model: &str,
    provider_id: &str,
    provider_name: &str,
    result: UsageResult,
    terminal: bool,
    error_message: Option<&str>,
    amount: Option<f64>,
    token_counts: UsageTokens,
) -> UsageLogRecord {
    UsageLogRecord {
        timestamp_ms,
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        provider_id: provider_id.to_string(),
        provider_name: provider_name.to_string(),
        result,
        status: 200,
        input_tokens: token_counts.input_tokens,
        cache_read_tokens: token_counts.cache_read_tokens,
        cache_write_tokens: token_counts.cache_write_tokens,
        output_tokens: token_counts.output_tokens,
        total_tokens: token_counts.total(),
        amount,
        duration_ms: 5,
        error_message: error_message.map(str::to_string),
        terminal,
    }
}

fn rfc3339_millis(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .expect("valid rfc3339")
        .timestamp_millis()
}

/// AC-007 / compatibility: an `api_gateway.json` written before this feature —
/// has no usage fields, yet still deserializes with the documented
/// defaults, and the new fields round-trip without disturbing existing ones.
/// Writes always target the `api_gateway.json` file.
#[test]
fn gateway_config_accepts_older_json_and_round_trips_usage_fields() {
    let older = serde_json::json!({
        "enabled": true,
        "port": 17688,
        "providers": [],
        "keys": [],
        "default_key_id": null,
        "terminal_syncs": []
    });
    let config: GatewayConfig = serde_json::from_value(older).expect("older config parses");
    assert!(config.enabled);
    assert_eq!(config.usage_retention_days, DEFAULT_USAGE_RETENTION_DAYS);
    assert!(config.model_prices.is_empty());

    let mut updated = config;
    updated.usage_retention_days = 30;
    updated.model_prices.push(ModelPrice {
        provider_id: None,
        upstream_model: "remote-a".to_string(),
        input: 1.0,
        cache_read: 2.0,
        cache_write: 3.0,
        output: 4.0,
        off_peaks: Vec::new(),
        off_peak: None,
    });
    let encoded = serde_json::to_string(&updated).expect("encode config");
    let decoded: GatewayConfig = serde_json::from_str(&encoded).expect("round trip config");
    assert_eq!(decoded.usage_retention_days, 30);
    assert_eq!(decoded.model_prices.len(), 1);
    assert_eq!(decoded.model_prices[0].upstream_model, "remote-a");
    assert_eq!(decoded.model_prices[0].output, 4.0);
    assert_eq!(decoded.model_prices[0].off_peak, None);
    assert_eq!(decoded.model_prices[0].off_peaks.len(), 0);

    // Missing per-tier prices default to zero, not a deserialize error.
    let partial: ModelPrice =
        serde_json::from_value(serde_json::json!({ "upstream_model": "remote-b" })).unwrap();
    assert_eq!(partial.provider_id, None);
    assert_eq!(partial.input, 0.0);
    assert_eq!(partial.cache_read, 0.0);
    assert_eq!(partial.cache_write, 0.0);
    assert_eq!(partial.output, 0.0);
    assert_eq!(partial.off_peak, None);
    assert_eq!(partial.off_peaks.len(), 0);

    // OffPeak round trip test (single off_peak and off_peaks)
    let with_off_peak = ModelPrice {
        provider_id: Some("prov-test".to_string()),
        upstream_model: "remote-c".to_string(),
        input: 2.0,
        cache_read: 1.0,
        cache_write: 2.0,
        output: 4.0,
        off_peaks: vec![
            OffPeakPrice {
                start_time: "00:30".to_string(),
                end_time: "08:30".to_string(),
                input: 1.0,
                cache_read: 0.5,
                cache_write: 1.0,
                output: 2.0,
                days: None,
            },
            OffPeakPrice {
                start_time: "12:00".to_string(),
                end_time: "14:00".to_string(),
                input: 1.5,
                cache_read: 0.8,
                cache_write: 1.5,
                output: 3.0,
                days: None,
            },
        ],
        off_peak: None,
    };
    let encoded_op = serde_json::to_string(&with_off_peak).unwrap();
    let decoded_op: ModelPrice = serde_json::from_str(&encoded_op).unwrap();
    assert_eq!(decoded_op.off_peaks, with_off_peak.off_peaks);
    assert_eq!(decoded_op.effective_off_peaks().len(), 2);

    // Backward compatibility: JSON with single `off_peak` deserializes and effective_off_peaks returns it
    let single_json = serde_json::json!({
        "upstream_model": "single-model",
        "input": 1.0,
        "cache_read": 0.5,
        "cache_write": 1.0,
        "output": 2.0,
        "off_peak": {
            "start_time": "00:00",
            "end_time": "08:00",
            "input": 0.5,
            "cache_read": 0.25,
            "cache_write": 0.5,
            "output": 1.0
        }
    });
    let single_decoded: ModelPrice = serde_json::from_value(single_json).unwrap();
    assert_eq!(single_decoded.effective_off_peaks().len(), 1);
    assert_eq!(single_decoded.effective_off_peaks()[0].start_time, "00:00");
}

/// AC-004 / AC-005 / REQ-004 / REQ-006: four-tier cost math and exact,
/// case-sensitive provider-scoped model matching with `None` for unpriced models.
#[test]
fn usage_pricing_matches_exact_model_and_sums_four_tiers() {
    let price = ModelPrice {
        provider_id: Some("prov-x".to_string()),
        upstream_model: "gpt-x".to_string(),
        input: 1.0,
        cache_read: 0.5,
        cache_write: 2.0,
        output: 4.0,
        off_peaks: Vec::new(),
        off_peak: None,
    };
    let prices = vec![price.clone()];
    assert_eq!(
        match_price_for_provider("prov-x", "gpt-x", &prices),
        Some(&price)
    );
    assert_eq!(
        match_price_for_provider("prov-x", "gpt-x ", &prices),
        None,
        "no trimming"
    );
    assert_eq!(
        match_price_for_provider("prov-x", "GPT-X", &prices),
        None,
        "no case folding"
    );
    assert_eq!(match_price_for_provider("prov-x", "gpt-y", &prices), None);

    let cost = compute_cost(&price, &tokens(1_000_000, 2_000_000, 500_000, 1_000_000));
    assert!((cost - 7.0).abs() < 1e-9, "expected $7.00, got {cost}");

    // Missing fields are zero and never affect the other tiers.
    let only_input = compute_cost(&price, &tokens(2_000_000, 0, 0, 0));
    assert!((only_input - 2.0).abs() < 1e-9);
}

/// AC-005 / REQ-003: a price row is matched only by its own provider and the
/// exact, case-sensitive upstream model; the global row and any other
/// provider's row never price a request.
#[test]
fn provider_scoped_price_matching_never_uses_global_or_foreign_rows() {
    let global_price = ModelPrice {
        provider_id: None,
        upstream_model: "gpt-4o".to_string(),
        input: 2.5,
        cache_read: 1.25,
        cache_write: 2.5,
        output: 10.0,
        off_peaks: Vec::new(),
        off_peak: None,
    };
    let provider_a_price = ModelPrice {
        provider_id: Some("prov-a".to_string()),
        upstream_model: "gpt-4o".to_string(),
        input: 2.0,
        cache_read: 1.0,
        cache_write: 2.0,
        output: 8.0,
        off_peaks: Vec::new(),
        off_peak: None,
    };
    let provider_b_price = ModelPrice {
        provider_id: Some("prov-b".to_string()),
        upstream_model: "gpt-4o".to_string(),
        input: 3.0,
        cache_read: 1.5,
        cache_write: 3.0,
        output: 12.0,
        off_peaks: Vec::new(),
        off_peak: None,
    };

    let prices = vec![
        global_price.clone(),
        provider_a_price.clone(),
        provider_b_price.clone(),
    ];

    assert_eq!(
        match_price_for_provider("prov-a", "gpt-4o", &prices),
        Some(&provider_a_price),
        "each provider must get its own row"
    );
    assert_eq!(
        match_price_for_provider("prov-b", "gpt-4o", &prices),
        Some(&provider_b_price),
        "each provider must get its own row"
    );

    // A provider with no row never borrows the global row or another provider's.
    assert_eq!(
        match_price_for_provider("prov-unknown", "gpt-4o", &prices),
        None,
        "the global row must never match a provider"
    );

    // Exact, case-sensitive matching; no trimming and no case folding.
    assert_eq!(
        match_price_for_provider("prov-a", "Gpt-4o", &prices),
        None,
        "model matching must be case-sensitive"
    );
    assert_eq!(
        match_price_for_provider("prov-a", "unknown-model", &prices),
        None
    );
}

/// AC-006 / REQ-004: normalization copies each global row into a provider that
/// reaches that model, never overwrites an existing scoped row, deletes
/// unmatched/orphan/unreachable rows, deduplicates keeping the first, and is
/// idempotent.
#[test]
fn normalize_config_migrates_global_rows_and_drops_unreachable_rows() {
    let mut p = provider("p");
    p.mappings = vec![mapping("local-a", "remote-a", None)];
    p.default_model = Some("remote-default".to_string());
    let mut q = provider("q");
    q.mappings = vec![mapping("local-q", "remote-q", None)];

    let mut config = GatewayConfig::default();
    config.providers.push(p);
    config.providers.push(q);
    config.model_prices = vec![
        priced("remote-a", 1.0, 0.0, 0.0, 0.0),
        priced("remote-default", 2.0, 0.0, 0.0, 0.0),
        priced("no-provider-model", 3.0, 0.0, 0.0, 0.0),
        priced_with_provider("p", "remote-a", 9.0, 0.0, 0.0, 0.0),
        priced_with_provider("ghost", "remote-a", 5.0, 0.0, 0.0, 0.0),
        priced_with_provider("p", "unreachable", 5.0, 0.0, 0.0, 0.0),
        priced_with_provider("q", "remote-q", 4.0, 0.0, 0.0, 0.0),
        priced_with_provider("q", "remote-q", 5.0, 0.0, 0.0, 0.0),
    ];

    super::storage::normalize_config(&mut config);

    assert!(
        config.model_prices.iter().all(|row| row.provider_id.is_some()),
        "every row must carry a provider id: {:?}",
        config.model_prices
    );
    assert!(
        config
            .model_prices
            .iter()
            .all(|row| row.provider_id.as_deref() != Some("ghost")),
        "a row for a nonexistent provider must be dropped"
    );
    assert!(
        config
            .model_prices
            .iter()
            .all(|row| row.upstream_model != "no-provider-model"),
        "an unmatched global row must be deleted"
    );
    assert!(
        config.model_prices.iter().all(|row| {
            !(row.provider_id.as_deref() == Some("p") && row.upstream_model == "unreachable")
        }),
        "a row unreachable from its provider must be dropped"
    );

    // P keeps its own scoped row and gains the migrated default-model row.
    let p_rows: Vec<&ModelPrice> = config
        .model_prices
        .iter()
        .filter(|row| row.provider_id.as_deref() == Some("p"))
        .collect();
    assert_eq!(
        p_rows.len(),
        2,
        "P must hold exactly remote-a and remote-default: {p_rows:?}"
    );
    let p_remote_a = p_rows
        .iter()
        .find(|row| row.upstream_model == "remote-a")
        .expect("P keeps its own remote-a row");
    assert_eq!(
        p_remote_a.input, 9.0,
        "the pre-existing scoped row must not be overwritten or duplicated"
    );
    let p_default = p_rows
        .iter()
        .find(|row| row.upstream_model == "remote-default")
        .expect("the global default-model row must migrate to P");
    assert_eq!(p_default.input, 2.0);

    // Q's two duplicate rows collapse to exactly one, keeping the first.
    let q_rows: Vec<&ModelPrice> = config
        .model_prices
        .iter()
        .filter(|row| row.provider_id.as_deref() == Some("q"))
        .collect();
    assert_eq!(
        q_rows.len(),
        1,
        "duplicate rows must collapse to one: {q_rows:?}"
    );
    assert_eq!(q_rows[0].upstream_model, "remote-q");
    assert_eq!(q_rows[0].input, 4.0, "deduplication keeps the first row");

    // Idempotence: a second normalization changes nothing.
    let mut again = config.clone();
    super::storage::normalize_config(&mut again);
    assert_eq!(
        again.model_prices, config.model_prices,
        "normalization must be idempotent"
    );
}

/// AC-006 counterexample / REQ-004: a legacy encrypted `api_gateway.json`
/// carrying global rows stays readable, migrates the reachable global row,
/// deletes the unmatched one, preserves providers/keys/default key/ledger/
/// retention, and is stable across a further load-write cycle with only
/// provider-scoped rows persisted.
#[test]
fn legacy_config_file_with_global_rows_migrates_on_load_and_persists_on_write() {
    with_temp_home("legacy-global-prices", |_home| {
        let legacy = json!({
            "enabled": true,
            "port": 17688,
            "providers": [
                {
                    "id": "p",
                    "name": "Provider P",
                    "base_url": "https://p.example.com/v1",
                    "api_key": "sk-p",
                    "default_model": "remote-default",
                    "protocol": "chat_completions",
                    "mappings": [
                        {"local_model": "local-a", "upstream_model": "remote-a", "enabled": true}
                    ]
                },
                {
                    "id": "q",
                    "name": "Provider Q",
                    "base_url": "https://q.example.com/v1",
                    "api_key": "sk-q",
                    "protocol": "chat_completions",
                    "mappings": [
                        {"local_model": "local-q", "upstream_model": "remote-q", "enabled": true}
                    ]
                }
            ],
            "keys": [
                {"id": "k1", "label": "k1", "value": "local-key", "enabled": true, "created_at": 1}
            ],
            "default_key_id": "k1",
            "terminal_syncs": [
                {
                    "provider_id": "p",
                    "tool": "opencode",
                    "synced_key_id": "k1",
                    "synced_base_url": "http://127.0.0.1:17688",
                    "synced_at": 7
                }
            ],
            "usage_retention_days": 30,
            "model_prices": [
                {"upstream_model": "remote-a", "input": 1.0, "cache_read": 0.0, "cache_write": 0.0, "output": 2.0},
                {"upstream_model": "unmatched-model", "input": 3.0, "cache_read": 0.0, "cache_write": 0.0, "output": 4.0}
            ]
        });

        let password = crate::crypto::get_or_init_master_password().expect("master password");
        let encrypted =
            crate::crypto::encrypt(&legacy.to_string(), &password).expect("encrypt legacy config");
        fs::write(config_path().expect("config path"), encrypted).expect("write legacy config");

        let first = super::storage::read_config().expect("legacy config must stay readable");
        assert_eq!(first.providers.len(), 2, "providers must be intact");
        assert_eq!(first.providers[0].mappings[0].upstream_model, "remote-a");
        assert_eq!(
            first.providers[0].default_model.as_deref(),
            Some("remote-default")
        );
        assert_eq!(first.keys.len(), 1, "keys must be intact");
        assert_eq!(first.keys[0].value, "local-key");
        assert_eq!(first.default_key_id.as_deref(), Some("k1"));
        assert_eq!(first.terminal_syncs.len(), 1, "terminal_syncs must be intact");
        assert_eq!(first.terminal_syncs[0].provider_id, "p");
        assert_eq!(first.usage_retention_days, 30, "retention must be intact");

        assert!(
            first.model_prices.iter().all(|row| row.provider_id.is_some()),
            "migration must leave only provider-scoped rows: {:?}",
            first.model_prices
        );
        assert!(
            first
                .model_prices
                .iter()
                .all(|row| row.upstream_model != "unmatched-model"),
            "an unmatched global row must be deleted"
        );
        let migrated = first
            .model_prices
            .iter()
            .find(|row| {
                row.provider_id.as_deref() == Some("p") && row.upstream_model == "remote-a"
            })
            .expect("P must receive the migrated remote-a row");
        assert_eq!(migrated.output, 2.0);

        super::storage::write_config(&first).expect("persist the migrated config");
        let second = super::storage::read_config().expect("reload the persisted config");
        assert_eq!(
            serde_json::to_value(&second).expect("second config"),
            serde_json::to_value(&first).expect("first config"),
            "a further load-write cycle must change nothing"
        );
        assert!(
            second.model_prices.iter().all(|row| row.provider_id.is_some()),
            "every persisted row must carry a provider id"
        );
    });
}

#[test]
fn is_off_peak_window_and_midnight_crossing() {
    use chrono::TimeZone;
    let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let make_utc8_ms = |h: u32, m: u32| {
        tz.with_ymd_and_hms(2026, 9, 18, h, m, 0)
            .unwrap()
            .timestamp_millis()
    };

    let ms_0400 = make_utc8_ms(4, 0);
    let ms_0800 = make_utc8_ms(8, 0);
    let ms_0830 = make_utc8_ms(8, 30);
    let ms_0900 = make_utc8_ms(9, 0);

    // Normal daytime/morning window: 00:30 to 08:30
    assert!(is_off_peak(ms_0400, "00:30", "08:30"), "04:00 UTC+8 is in 00:30-08:30");
    assert!(is_off_peak(ms_0800, "00:30", "08:30"), "08:00 UTC+8 is in 00:30-08:30");
    assert!(!is_off_peak(ms_0830, "00:30", "08:30"), "08:30 UTC+8 is at boundary end (exclusive)");
    assert!(!is_off_peak(ms_0900, "00:30", "08:30"), "09:00 UTC+8 is outside 00:30-08:30");

    // Overnight window: 22:00 to 06:00
    // 04:00 UTC+8 is in 22:00-06:00
    assert!(is_off_peak(ms_0400, "22:00", "06:00"));
    // 08:00 UTC+8 is outside 22:00-06:00
    assert!(!is_off_peak(ms_0800, "22:00", "06:00"));

    // Equal times: zero duration window -> false
    assert!(!is_off_peak(ms_0800, "08:00", "08:00"));
    // Invalid time strings -> false
    assert!(!is_off_peak(ms_0800, "invalid", "08:00"));
}

#[test]
fn compute_cost_at_time_applies_off_peak_pricing_when_active() {
    use chrono::TimeZone;
    let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let make_utc8_ms = |h: u32, m: u32| {
        tz.with_ymd_and_hms(2026, 9, 18, h, m, 0)
            .unwrap()
            .timestamp_millis()
    };

    let standard_price = ModelPrice {
        provider_id: None,
        upstream_model: "deepseek-chat".to_string(),
        input: 2.0,
        cache_read: 1.0,
        cache_write: 2.0,
        output: 4.0,
        off_peaks: Vec::new(),
        off_peak: Some(OffPeakPrice {
            start_time: "00:30".to_string(),
            end_time: "08:30".to_string(),
            input: 1.0, // 50% discount
            cache_read: 0.5,
            cache_write: 1.0,
            output: 2.0,
            days: None,
        }),
    };

    let test_tokens = tokens(1_000_000, 1_000_000, 1_000_000, 1_000_000);

    // 04:00 UTC+8 (off-peak)
    let off_peak_ms = make_utc8_ms(4, 0);
    let off_peak_cost = compute_cost_at_time(&standard_price, &test_tokens, off_peak_ms);
    // (1.0 + 0.5 + 1.0 + 2.0) = 4.5
    assert!((off_peak_cost - 4.5).abs() < 1e-9, "expected $4.50, got {off_peak_cost}");

    // 14:00 UTC+8 (peak / standard)
    let peak_ms = make_utc8_ms(14, 0);
    let peak_cost = compute_cost_at_time(&standard_price, &test_tokens, peak_ms);
    // (2.0 + 1.0 + 2.0 + 4.0) = 9.0
    assert!((peak_cost - 9.0).abs() < 1e-9, "expected $9.00, got {peak_cost}");

    // Price without off-peak always returns standard cost regardless of time
    let mut no_off_peak = standard_price.clone();
    no_off_peak.off_peak = None;
    assert_eq!(compute_cost_at_time(&no_off_peak, &test_tokens, off_peak_ms), 9.0);
    assert_eq!(compute_cost_at_time(&no_off_peak, &test_tokens, peak_ms), 9.0);
}

#[test]
fn compute_cost_at_time_applies_multiple_off_peak_pricing_windows() {
    use chrono::TimeZone;
    let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let make_utc8_ms = |h: u32, m: u32| {
        tz.with_ymd_and_hms(2026, 9, 18, h, m, 0)
            .unwrap()
            .timestamp_millis()
    };

    let multi_off_peak_price = ModelPrice {
        provider_id: None,
        upstream_model: "qwen-max".to_string(),
        input: 4.0,
        cache_read: 2.0,
        cache_write: 4.0,
        output: 8.0,
        off_peaks: vec![
            // Window 1: Night valley 00:00 - 08:00
            OffPeakPrice {
                start_time: "00:00".to_string(),
                end_time: "08:00".to_string(),
                input: 1.0,
                cache_read: 0.5,
                cache_write: 1.0,
                output: 2.0,
                days: None,
            },
            // Window 2: Lunch valley 12:00 - 14:00
            OffPeakPrice {
                start_time: "12:00".to_string(),
                end_time: "14:00".to_string(),
                input: 2.0,
                cache_read: 1.0,
                cache_write: 2.0,
                output: 4.0,
                days: None,
            },
        ],
        off_peak: None,
    };

    let test_tokens = tokens(1_000_000, 1_000_000, 1_000_000, 1_000_000);

    // 03:00 UTC+8 (hits Window 1: 1.0 + 0.5 + 1.0 + 2.0 = 4.5)
    let w1_cost = compute_cost_at_time(&multi_off_peak_price, &test_tokens, make_utc8_ms(3, 0));
    assert!((w1_cost - 4.5).abs() < 1e-9, "expected $4.50, got {w1_cost}");

    // 13:00 UTC+8 (hits Window 2: 2.0 + 1.0 + 2.0 + 4.0 = 9.0)
    let w2_cost = compute_cost_at_time(&multi_off_peak_price, &test_tokens, make_utc8_ms(13, 0));
    assert!((w2_cost - 9.0).abs() < 1e-9, "expected $9.00, got {w2_cost}");

    // 10:00 UTC+8 (outside both windows -> standard price: 4.0 + 2.0 + 4.0 + 8.0 = 18.0)
    let std_cost = compute_cost_at_time(&multi_off_peak_price, &test_tokens, make_utc8_ms(10, 0));
    assert!((std_cost - 18.0).abs() < 1e-9, "expected $18.00, got {std_cost}");
}

// ---------------------------------------------------------------------------
// 20260918-provider-templates Step 1: weekday-scoped off-peak windows
//
// These are the RED behavior tests for REQ-008 / AC-012. They exercise the
// public pricing boundary (`ModelPrice` / `OffPeakPrice.days` and
// `compute_cost_at_time`), never the matching internals.
// ---------------------------------------------------------------------------

/// Real UTC milliseconds for a wall-clock instant in UTC+8.
fn utc8_timestamp_ms(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    use chrono::TimeZone;
    let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    tz.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
        .timestamp_millis()
}

/// Guard the fixtures against calendar drift: assert the timestamp really lands
/// on the intended Sunday-based weekday (0 = Sunday .. 6 = Saturday).
fn assert_utc8_weekday(timestamp_ms: i64, expected_sunday_based: u32) {
    use chrono::Datelike;
    let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let dt = chrono::DateTime::from_timestamp_millis(timestamp_ms)
        .unwrap()
        .with_timezone(&tz);
    assert_eq!(dt.weekday().num_days_from_sunday(), expected_sunday_based);
}

/// One-mega-token in every tier: the amount equals the sum of the four prices.
fn one_mega_each() -> UsageTokens {
    tokens(1_000_000, 1_000_000, 1_000_000, 1_000_000)
}

/// AC-012 fixture: a workday window [09:00,12:00) days Mon-Fri and a weekend
/// window [09:00,12:00) days Sat/Sun, with distinct off-peak tiers and a
/// standard (peak) tier. The identical clock range makes the weekday the only
/// discriminator. Hand-computed sums for one mega token each:
/// standard = 4.0+2.0+4.0+8.0 = 18.0; workday = 1.0+0.5+1.0+2.0 = 4.5;
/// weekend = 1.5+0.75+1.5+3.0 = 6.75.
fn weekday_and_weekend_off_peak_price() -> ModelPrice {
    ModelPrice {
        provider_id: None,
        upstream_model: "deepseek-chat".to_string(),
        input: 4.0,
        cache_read: 2.0,
        cache_write: 4.0,
        output: 8.0,
        off_peaks: vec![
            OffPeakPrice {
                start_time: "09:00".to_string(),
                end_time: "12:00".to_string(),
                input: 1.0,
                cache_read: 0.5,
                cache_write: 1.0,
                output: 2.0,
                days: Some(vec![1, 2, 3, 4, 5]),
            },
            OffPeakPrice {
                start_time: "09:00".to_string(),
                end_time: "12:00".to_string(),
                input: 1.5,
                cache_read: 0.75,
                cache_write: 1.5,
                output: 3.0,
                days: Some(vec![0, 6]),
            },
        ],
        off_peak: None,
    }
}

/// AC-012: Saturday 10:00 must hit the weekend window even though the earlier
/// workday window covers the same clock range; without weekday matching the
/// workday window would win and price the weekdays' cheaper tier.
#[test]
fn compute_cost_at_time_off_peak_days_saturday_hits_weekend_window() {
    let price = weekday_and_weekend_off_peak_price();
    let saturday_10 = utc8_timestamp_ms(2026, 9, 19, 10, 0);
    assert_utc8_weekday(saturday_10, 6);

    let cost = compute_cost_at_time(&price, &one_mega_each(), saturday_10);
    assert!(
        (cost - 6.75).abs() < 1e-9,
        "Saturday 10:00 should use the weekend off-peak tier ($6.75), got {cost}"
    );
}

/// AC-012: Wednesday 10:00 must hit the workday window, not the weekend one.
#[test]
fn compute_cost_at_time_off_peak_days_wednesday_hits_weekday_window() {
    let price = weekday_and_weekend_off_peak_price();
    let wednesday_10 = utc8_timestamp_ms(2026, 9, 16, 10, 0);
    assert_utc8_weekday(wednesday_10, 3);

    let cost = compute_cost_at_time(&price, &one_mega_each(), wednesday_10);
    assert!(
        (cost - 4.5).abs() < 1e-9,
        "Wednesday 10:00 should use the workday off-peak tier ($4.50), got {cost}"
    );
}

/// AC-012: Wednesday 13:00 is outside both windows and must use the standard tier.
#[test]
fn compute_cost_at_time_off_peak_days_wednesday_afternoon_uses_standard_price() {
    let price = weekday_and_weekend_off_peak_price();
    let wednesday_13 = utc8_timestamp_ms(2026, 9, 16, 13, 0);
    assert_utc8_weekday(wednesday_13, 3);

    let cost = compute_cost_at_time(&price, &one_mega_each(), wednesday_13);
    assert!(
        (cost - 18.0).abs() < 1e-9,
        "Wednesday 13:00 should use the standard tier ($18.00), got {cost}"
    );
}

/// REQ-008 regression: `days: None` and `days: Some(vec![])` keep the exact
/// pre-extension [start,end) result and both mean every day; the empty set
/// serializes like an absent field.
fn legacy_off_peak_price(days: Option<Vec<u8>>) -> ModelPrice {
    ModelPrice {
        provider_id: None,
        upstream_model: "legacy-model".to_string(),
        input: 4.0,
        cache_read: 2.0,
        cache_write: 4.0,
        output: 8.0,
        off_peaks: vec![OffPeakPrice {
            start_time: "00:30".to_string(),
            end_time: "08:30".to_string(),
            input: 1.0,
            cache_read: 0.5,
            cache_write: 1.0,
            output: 2.0,
            days,
        }],
        off_peak: None,
    }
}

#[test]
fn compute_cost_at_time_off_peak_days_absent_reproduces_legacy_result() {
    let with_none = legacy_off_peak_price(None);
    let with_empty = legacy_off_peak_price(Some(Vec::new()));
    let test_tokens = one_mega_each();

    let off_peak_ms = utc8_timestamp_ms(2026, 9, 18, 4, 0);
    let peak_ms = utc8_timestamp_ms(2026, 9, 18, 14, 0);

    let none_off_peak = compute_cost_at_time(&with_none, &test_tokens, off_peak_ms);
    assert!(
        (none_off_peak - 4.5).abs() < 1e-9,
        "legacy window at 04:00 should stay $4.50, got {none_off_peak}"
    );
    let none_peak = compute_cost_at_time(&with_none, &test_tokens, peak_ms);
    assert!(
        (none_peak - 18.0).abs() < 1e-9,
        "legacy window at 14:00 should stay $18.00, got {none_peak}"
    );

    assert_eq!(
        compute_cost_at_time(&with_empty, &test_tokens, off_peak_ms),
        none_off_peak,
        "an empty weekday set must behave exactly like an absent one"
    );
    assert_eq!(
        compute_cost_at_time(&with_empty, &test_tokens, peak_ms),
        none_peak,
        "an empty weekday set must behave exactly like an absent one"
    );

    let value = serde_json::to_value(&with_empty).unwrap();
    assert!(
        value["off_peaks"][0].get("days").is_none(),
        "an empty weekday set must serialize as an absent field, got {value}"
    );
}

/// REQ-008: with identical windows, the first `effective_off_peaks()` entry that
/// matches wins. Window 1 is Sunday-only, window 2 is every day.
#[test]
fn compute_cost_at_time_off_peak_days_first_matching_window_wins() {
    let price = ModelPrice {
        provider_id: None,
        upstream_model: "precedence-model".to_string(),
        input: 4.0,
        cache_read: 2.0,
        cache_write: 4.0,
        output: 8.0,
        off_peaks: vec![
            OffPeakPrice {
                start_time: "09:00".to_string(),
                end_time: "12:00".to_string(),
                input: 1.0,
                cache_read: 0.5,
                cache_write: 1.0,
                output: 2.0,
                days: Some(vec![0]),
            },
            OffPeakPrice {
                start_time: "09:00".to_string(),
                end_time: "12:00".to_string(),
                input: 1.5,
                cache_read: 0.75,
                cache_write: 1.5,
                output: 3.0,
                days: Some(vec![]),
            },
        ],
        off_peak: None,
    };
    let test_tokens = one_mega_each();

    let sunday_10 = utc8_timestamp_ms(2026, 9, 20, 10, 0);
    assert_utc8_weekday(sunday_10, 0);
    let monday_10 = utc8_timestamp_ms(2026, 9, 21, 10, 0);
    assert_utc8_weekday(monday_10, 1);

    let sunday_cost = compute_cost_at_time(&price, &test_tokens, sunday_10);
    assert!(
        (sunday_cost - 4.5).abs() < 1e-9,
        "Sunday should take the first (Sunday-only) window, got {sunday_cost}"
    );
    let monday_cost = compute_cost_at_time(&price, &test_tokens, monday_10);
    assert!(
        (monday_cost - 6.75).abs() < 1e-9,
        "Monday should fall through to the every-day window, got {monday_cost}"
    );
}

/// REQ-008 time boundary: an overnight window scoped to `days=[5]` (Friday)
/// matches the request's own UTC+8 weekday, so Friday 23:00 hits but the
/// Saturday 01:00 continuation does not.
fn overnight_off_peak_price(days: Option<Vec<u8>>) -> ModelPrice {
    ModelPrice {
        provider_id: None,
        upstream_model: "overnight-model".to_string(),
        input: 4.0,
        cache_read: 2.0,
        cache_write: 4.0,
        output: 8.0,
        off_peaks: vec![OffPeakPrice {
            start_time: "22:00".to_string(),
            end_time: "02:00".to_string(),
            input: 1.0,
            cache_read: 0.5,
            cache_write: 1.0,
            output: 2.0,
            days,
        }],
        off_peak: None,
    }
}

#[test]
fn compute_cost_at_time_off_peak_days_overnight_matches_request_weekday() {
    let price = overnight_off_peak_price(Some(vec![5]));
    let test_tokens = one_mega_each();

    let friday_23 = utc8_timestamp_ms(2026, 9, 18, 23, 0);
    assert_utc8_weekday(friday_23, 5);
    let saturday_01 = utc8_timestamp_ms(2026, 9, 19, 1, 0);
    assert_utc8_weekday(saturday_01, 6);
    let friday_noon = utc8_timestamp_ms(2026, 9, 18, 12, 0);
    assert_utc8_weekday(friday_noon, 5);

    let friday_night = compute_cost_at_time(&price, &test_tokens, friday_23);
    assert!(
        (friday_night - 4.5).abs() < 1e-9,
        "Friday 23:00 should hit the Friday overnight window, got {friday_night}"
    );
    let saturday_early = compute_cost_at_time(&price, &test_tokens, saturday_01);
    assert!(
        (saturday_early - 18.0).abs() < 1e-9,
        "Saturday 01:00 is the request's own Saturday and must not hit a Friday window, got {saturday_early}"
    );
    let friday_day = compute_cost_at_time(&price, &test_tokens, friday_noon);
    assert!(
        (friday_day - 18.0).abs() < 1e-9,
        "Friday noon is outside 22:00-02:00, got {friday_day}"
    );
}

#[test]
fn compute_cost_at_time_off_peak_days_zero_duration_never_matches() {
    let price = ModelPrice {
        provider_id: None,
        upstream_model: "zero-duration-model".to_string(),
        input: 4.0,
        cache_read: 2.0,
        cache_write: 4.0,
        output: 8.0,
        off_peaks: vec![OffPeakPrice {
            start_time: "08:00".to_string(),
            end_time: "08:00".to_string(),
            input: 1.0,
            cache_read: 0.5,
            cache_write: 1.0,
            output: 2.0,
            days: Some(vec![6]),
        }],
        off_peak: None,
    };

    let saturday_08 = utc8_timestamp_ms(2026, 9, 19, 8, 0);
    assert_utc8_weekday(saturday_08, 6);

    let cost = compute_cost_at_time(&price, &one_mega_each(), saturday_08);
    assert!(
        (cost - 18.0).abs() < 1e-9,
        "start == end must stay a never-matching zero-duration window, got {cost}"
    );
}

/// REQ-008 numeric boundary: writing `[5,1,1,9]` round-trips as `[1,5]`
/// (deduplicated, sorted, out-of-range dropped).
#[test]
fn off_peak_days_round_trip_normalizes_sorts_and_drops_out_of_range() {
    let price = ModelPrice {
        provider_id: None,
        upstream_model: "normalized-model".to_string(),
        input: 4.0,
        cache_read: 2.0,
        cache_write: 4.0,
        output: 8.0,
        off_peaks: vec![OffPeakPrice {
            start_time: "09:00".to_string(),
            end_time: "12:00".to_string(),
            input: 1.0,
            cache_read: 0.5,
            cache_write: 1.0,
            output: 2.0,
            days: Some(vec![5, 1, 1, 9]),
        }],
        off_peak: None,
    };

    let value = serde_json::to_value(&price).unwrap();
    assert_eq!(
        value["off_peaks"][0]["days"].to_string(),
        "[1,5]",
        "serialized weekday set must be deduplicated, sorted and in range"
    );

    let decoded: ModelPrice = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.off_peaks[0].days, Some(vec![1u8, 5u8]));

    // The normalized set still matches Monday and excludes Saturday.
    let test_tokens = one_mega_each();
    let monday_10 = utc8_timestamp_ms(2026, 9, 21, 10, 0);
    assert_utc8_weekday(monday_10, 1);
    let saturday_10 = utc8_timestamp_ms(2026, 9, 19, 10, 0);
    assert_utc8_weekday(saturday_10, 6);

    let monday_cost = compute_cost_at_time(&decoded, &test_tokens, monday_10);
    assert!((monday_cost - 4.5).abs() < 1e-9, "Monday should match [1,5], got {monday_cost}");
    let saturday_cost = compute_cost_at_time(&decoded, &test_tokens, saturday_10);
    assert!((saturday_cost - 18.0).abs() < 1e-9, "Saturday is outside [1,5], got {saturday_cost}");
}

/// REQ-008 numeric boundary: an all-invalid weekday set becomes `None`, omits
/// the serialized field and applies the window every day.
#[test]
fn off_peak_days_all_invalid_become_none_and_apply_every_day() {
    let price = ModelPrice {
        provider_id: None,
        upstream_model: "invalid-days-model".to_string(),
        input: 4.0,
        cache_read: 2.0,
        cache_write: 4.0,
        output: 8.0,
        off_peaks: vec![OffPeakPrice {
            start_time: "00:00".to_string(),
            end_time: "08:00".to_string(),
            input: 1.0,
            cache_read: 0.5,
            cache_write: 1.0,
            output: 2.0,
            days: Some(vec![7, 9]),
        }],
        off_peak: None,
    };

    let value = serde_json::to_value(&price).unwrap();
    assert!(
        value["off_peaks"][0].get("days").is_none(),
        "an all-invalid weekday set must serialize as an absent field, got {value}"
    );

    let decoded: ModelPrice = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.off_peaks[0].days, None);

    let test_tokens = one_mega_each();
    let saturday_03 = utc8_timestamp_ms(2026, 9, 19, 3, 0);
    assert_utc8_weekday(saturday_03, 6);
    let monday_03 = utc8_timestamp_ms(2026, 9, 21, 3, 0);
    assert_utc8_weekday(monday_03, 1);

    let saturday_cost = compute_cost_at_time(&decoded, &test_tokens, saturday_03);
    assert!(
        (saturday_cost - 4.5).abs() < 1e-9,
        "a None weekday set applies every day (Saturday), got {saturday_cost}"
    );
    let monday_cost = compute_cost_at_time(&decoded, &test_tokens, monday_03);
    assert!(
        (monday_cost - 4.5).abs() < 1e-9,
        "a None weekday set applies every day (Monday), got {monday_cost}"
    );
}

/// AC-012 / REQ-010: only 1-365 is accepted; invalid values are rejected and
/// persisted garbage falls back to the default instead of propagating.
#[test]
fn retention_validation_accepts_one_and_365_and_rejects_zero_and_400() {
    assert!(validate_retention_days(0).is_err());
    assert!(validate_retention_days(400).is_err());
    assert!(validate_retention_days(-1).is_err());
    assert_eq!(validate_retention_days(1).expect("1 is valid"), 1);
    assert_eq!(validate_retention_days(90).expect("90 is valid"), 90);
    assert_eq!(validate_retention_days(365).expect("365 is valid"), 365);
    assert_eq!(normalize_retention_days(0), DEFAULT_USAGE_RETENTION_DAYS);
    assert_eq!(normalize_retention_days(400), DEFAULT_USAGE_RETENTION_DAYS);
    assert_eq!(normalize_retention_days(7), 7);
}

/// AC-003 / REQ-003: field mapping across OpenAI chat, Responses and
/// Anthropic-style cache fields; missing fields become 0.
#[test]
fn usage_field_mapping_handles_provider_shapes_and_missing_fields() {
    let chat = serde_json::json!({
        "prompt_tokens": 11,
        "completion_tokens": 7,
        "prompt_tokens_details": { "cached_tokens": 3 }
    });
    let mapped = usage_tokens_from_value(&chat);
    assert_eq!(mapped, tokens(11, 3, 0, 7));

    let responses = serde_json::json!({
        "input_tokens": 20,
        "output_tokens": 4,
        "input_tokens_details": { "cached_tokens": 5 },
        "cache_creation_input_tokens": 6
    });
    let mapped = usage_tokens_from_value(&responses);
    assert_eq!(mapped, tokens(20, 5, 6, 4));

    let anthropic = serde_json::json!({
        "prompt_tokens": 9,
        "completion_tokens": 1,
        "cache_read_input_tokens": 8,
        "cache_creation_input_tokens": 2
    });
    let mapped = usage_tokens_from_value(&anthropic);
    assert_eq!(mapped, tokens(9, 8, 2, 1));

    // Partial usage: missing tiers are zero without disturbing present ones.
    let partial = usage_tokens_from_value(&serde_json::json!({ "input_tokens": 3 }));
    assert_eq!(partial, tokens(3, 0, 0, 0));
    assert_eq!(partial.total(), 3);
}

/// AC-013 / REQ-011: range resolution uses UTC+8 midnight boundaries.
#[test]
fn usage_range_resolution_uses_utc8_boundaries() {
    // 00:30 on 2026-09-17 in UTC+8 (still 2026-09-16 in UTC).
    let now = rfc3339_millis("2026-09-17T00:30:00+08:00");
    let today = resolve_range(Some(1), now);
    assert_eq!(
        today.start_ms,
        Some(rfc3339_millis("2026-09-17T00:00:00+08:00"))
    );
    assert_eq!(
        today.end_ms,
        Some(rfc3339_millis("2026-09-18T00:00:00+08:00"))
    );

    // `近 7 天` includes today plus the six previous natural days.
    let week = resolve_range(Some(7), now);
    assert_eq!(
        week.start_ms,
        Some(rfc3339_millis("2026-09-11T00:00:00+08:00"))
    );
    assert_eq!(week.end_ms, today.end_ms);

    // 15:30Z is 23:30 in UTC+8 and must still belong to the 16th local day.
    let near_midnight = rfc3339_millis("2026-09-16T15:30:00Z");
    assert_eq!(
        resolve_range(Some(1), near_midnight).start_ms,
        Some(rfc3339_millis("2026-09-16T00:00:00+08:00"))
    );
    // 16:30Z has already crossed local midnight into the 17th.
    let after_midnight = rfc3339_millis("2026-09-16T16:30:00Z");
    assert_eq!(
        resolve_range(Some(1), after_midnight).start_ms,
        Some(rfc3339_millis("2026-09-17T00:00:00+08:00"))
    );

    assert_eq!(resolve_range(None, now), TimeRange::default());
    assert_eq!(resolve_range(Some(0), now), TimeRange::default());
}

/// AC-002 (parsing half): SSE usage survives arbitrary chunk boundaries and
/// absent usage stays `None`.
#[test]
fn sse_usage_accumulator_parses_usage_across_chunk_boundaries() {
    let mut accumulator = SseUsageAccumulator::default();
    accumulator.feed(b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n");
    assert_eq!(accumulator.usage(), None);
    accumulator.feed(b"data: {\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,");
    accumulator.feed(b"\"prompt_tokens_details\":{\"cached_tokens\":3}}}\n\ndata: [DONE]\n\n");
    assert_eq!(accumulator.usage(), Some(tokens(11, 3, 0, 7)));
}

fn usage_store(name: &str) -> (PathBuf, UsageLogStore) {
    let dir = make_temp_dir(name);
    let store = UsageLogStore::at(dir.join("api_gateway_usage.db"));
    (dir, store)
}

/// AC-010 / REQ-009: records persist across a fresh connection and come back
/// newest-first.
#[test]
fn usage_store_survives_reopen_and_returns_ordered_records() {
    let (dir, store) = usage_store("usage-reopen");
    let now = super::now_millis();
    store
        .append(
            &sample_record(
                now - 1_000,
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(0.25),
                tokens(1, 2, 3, 4),
            ),
            365,
        )
        .unwrap();
    store
        .append(
            &sample_record(
                now,
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Failure,
                None,
                UsageTokens::default(),
            ),
            365,
        )
        .unwrap();

    let reopened = UsageLogStore::at(dir.join("api_gateway_usage.db"));
    let records = reopened.all_records().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].timestamp_ms, now, "newest first");
    assert_eq!(records[0].amount, None, "unpriced stays unpriced");
    assert_eq!(records[1].amount, Some(0.25));
    assert_eq!(records[1].total_tokens, 10);
    let _ = fs::remove_dir_all(&dir);
}

/// AC-010 / REQ-009: writing with retention 7 deletes only the 10-day-old row.
#[test]
fn usage_store_retention_cleanup_deletes_only_expired_records() {
    let (dir, store) = usage_store("usage-retention");
    let now = super::now_millis();
    let day = 86_400_000i64;
    store
        .append(
            &sample_record(
                now - 10 * day,
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(1.0),
                tokens(1, 0, 0, 0),
            ),
            365,
        )
        .unwrap();
    store
        .append(
            &sample_record(
                now - 2 * day,
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(1.0),
                tokens(1, 0, 0, 0),
            ),
            365,
        )
        .unwrap();
    assert_eq!(store.count().unwrap(), 2);

    store
        .append(
            &sample_record(
                now,
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(1.0),
                tokens(1, 0, 0, 0),
            ),
            7,
        )
        .unwrap();

    let remaining = store.all_records().unwrap();
    assert_eq!(remaining.len(), 2, "10-day-old row must be deleted");
    assert!(remaining.iter().all(|record| record.timestamp_ms > now - 7 * day));
    let _ = fs::remove_dir_all(&dir);
}

/// AC-015 / AC-016 / AC-018: cards, model/provider detail and UTC+8 day buckets.
#[test]
fn usage_store_stats_aggregates_models_providers_and_utc8_buckets() {
    let (dir, store) = usage_store("usage-stats");
    let day_one = rfc3339_millis("2026-09-15T10:00:00+08:00");
    let day_two = rfc3339_millis("2026-09-16T10:00:00+08:00");
    // local-a via Provider One
    store.append(&sample_record(day_one, "local-a", "remote-a", "p1", "Provider One", UsageResult::Success, Some(0.5), tokens(10, 0, 0, 5)), 365).unwrap();
    store.append(&sample_record(day_two, "local-a", "remote-a", "p1", "Provider One", UsageResult::Failure, Some(2.0), tokens(100, 0, 0, 0)), 365).unwrap();
    // local-a via Provider Two
    store.append(&sample_record(day_one, "local-a", "remote-a", "p2", "Provider Two", UsageResult::Success, Some(1.0), tokens(20, 0, 0, 10)), 365).unwrap();
    // local-b unpriced
    store.append(&sample_record(day_one, "local-b", "remote-b", "p1", "Provider One", UsageResult::Success, None, tokens(1, 0, 0, 1)), 365).unwrap();

    let stats = store.usage_stats(&TimeRange::default(), false).unwrap();
    assert_eq!(stats.granularity, "day");
    assert_eq!(stats.totals.request_count, 4);
    assert_eq!(stats.totals.total_tokens, 10 + 5 + 100 + 20 + 10 + 2);
    assert!((stats.totals.amount - 3.5).abs() < 1e-9);
    assert_eq!(stats.totals.unpriced_count, 1);

    let local_a = stats
        .models
        .iter()
        .find(|row| row.local_model == "local-a")
        .expect("local-a row");
    assert_eq!(local_a.metrics.request_count, 3);
    assert_eq!(local_a.providers.len(), 2, "only called providers appear");
    let provider_one = local_a
        .providers
        .iter()
        .find(|row| row.provider_id == "p1")
        .expect("provider one detail");
    assert_eq!(provider_one.metrics.request_count, 2);

    let local_b = stats
        .models
        .iter()
        .find(|row| row.local_model == "local-b")
        .expect("local-b row");
    assert_eq!(local_b.metrics.amount, 0.0);
    assert_eq!(local_b.metrics.unpriced_count, 1);

    assert_eq!(stats.buckets.len(), 2, "two UTC+8 days with data");
    assert_eq!(stats.buckets[0].label, "2026-09-15");
    assert_eq!(stats.buckets[1].label, "2026-09-16");

    // Single-day range buckets by UTC+8 hour.
    let single = store
        .usage_stats(
            &resolve_range(Some(1), rfc3339_millis("2026-09-15T12:00:00+08:00")),
            true,
        )
        .unwrap();
    assert_eq!(single.granularity, "hour");
    assert_eq!(single.buckets.len(), 1);
    assert_eq!(single.buckets[0].label, "10:00");
    let _ = fs::remove_dir_all(&dir);
}

/// AC-020 / AC-021 / AC-022: filtering, grouping (error excludes cancelled) and
/// 50-row pagination with clamped pages.
#[test]
fn usage_store_logs_filter_group_and_paginate() {
    let (dir, store) = usage_store("usage-logs");
    let base = rfc3339_millis("2026-09-16T08:00:00+08:00");
    for index in 0..120i64 {
        let result = match index % 3 {
            0 => UsageResult::Success,
            1 => UsageResult::Failure,
            _ => UsageResult::Cancelled,
        };
        let model = if index % 2 == 0 { "local-a" } else { "local-b" };
        store
            .append(
                &sample_record(
                    base + index * 1_000,
                    model,
                    "remote-a",
                    "p1",
                    "Provider One",
                    result,
                    Some(0.1),
                    tokens(1, 0, 0, 1),
                ),
                365,
            )
            .unwrap();
    }

    // User-visible queries exclude cancelled rows.
    // Raw all_records still returns all 120 physical rows.
    let raw_all = store.all_records().unwrap_or_default();
    assert_eq!(raw_all.len(), 120, "physical rows include cancelled");

    let page_one = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .unwrap();
    assert_eq!(USAGE_LOG_PAGE_SIZE, 50);
    assert_eq!(page_one.page_size, 50);
    assert_eq!(page_one.records.len(), 50);
    assert_eq!(page_one.total, 80, "cancelled excluded from total (120 - 40)");
    assert_eq!(page_one.total_pages, 2, "80 / 50 = 2 pages");
    assert!(
        page_one.records[0].timestamp_ms > page_one.records[49].timestamp_ms,
        "newest first"
    );

    let page_two = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 2)
        .unwrap();
    assert_eq!(page_two.page, 2);
    assert_eq!(page_two.records.len(), 30);

    let clamped = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 99)
        .unwrap();
    assert_eq!(clamped.page, 2, "out-of-range page clamps to the last page");

    let failed = store
        .query_logs(
            &TimeRange::default(),
            &LogFilter {
                status: Some(UsageResult::Failure),
                model: None,
            },
            1,
        )
        .unwrap();
    assert_eq!(failed.total, 40);
    assert!(failed.records.iter().all(|r| r.result == UsageResult::Failure));

    let local_b = store
        .query_logs(
            &TimeRange::default(),
            &LogFilter {
                status: Some(UsageResult::Failure),
                model: Some("local-b".to_string()),
            },
            1,
        )
        .unwrap();
    assert_eq!(local_b.total, 20);
    assert!(local_b
        .records
        .iter()
        .all(|r| r.local_model == "local-b" && r.result == UsageResult::Failure));

    // Grouped results also exclude cancelled from request_count and last_request_at.
    let grouped = store
        .group_logs(&TimeRange::default(), &LogFilter::default(), "day")
        .unwrap();
    assert_eq!(grouped.len(), 1, "all records share one UTC+8 day");
    assert_eq!(grouped[0].group, "2026-09-16");
    assert_eq!(grouped[0].request_count, 80, "cancelled excluded from grouped count");
    assert_eq!(grouped[0].error_count, 40, "cancelled is not an error");

    let by_model = store
        .group_logs(&TimeRange::default(), &LogFilter::default(), "model")
        .unwrap();
    assert_eq!(by_model.len(), 2);
    assert_eq!(by_model.iter().map(|g| g.request_count).sum::<u32>(), 80);
    let _ = fs::remove_dir_all(&dir);
}

/// REQ-001 / AC-001 privacy: no credential, header or body text can reach the
/// log file or query results. REQ-004 (20260920-gateway-log-attempts Step 1):
/// error text produced by the extraction and sanitization helpers from a body
/// that echoes credentials must be redacted before it is stored.
#[test]
fn usage_store_never_contains_credentials_headers_or_bodies() {
    let (dir, store) = usage_store("usage-privacy");
    let secret = "sk-upstream-super-secret";
    let bearer_token = "AbCdEf0123456789xyzXYZ";
    let sk_token = "sk-live-1a2b3c4d5e6f7a8b";
    store
        .append(
            &sample_record(
                1_000,
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(0.1),
                tokens(1, 1, 1, 1),
            ),
            365,
        )
        .unwrap();

    // A stored failure row whose message came out of the real helpers, from a
    // body that echoes the provider key and two token-shaped credentials.
    let upstream_body = json!({
        "error": {
            "message": format!(
                "upstream rejected {secret} and {sk_token} for credential Bearer {bearer_token}"
            ),
            "type": "authentication_error",
        }
    })
    .to_string();
    let extracted = extract_upstream_error_text(upstream_body.as_bytes())
        .expect("a standard envelope must yield its message");
    assert!(
        extracted.contains(secret),
        "the fixture body must echo the provider key: {extracted}"
    );
    let sanitized = sanitize_error_text(&extracted, secret).expect("sanitized text survives");
    assert!(!sanitized.contains(secret));
    assert!(!sanitized.contains(bearer_token));
    assert!(!sanitized.contains(sk_token));
    assert!(
        sanitized.contains("[redacted]"),
        "the provider key must be redacted: {sanitized}"
    );
    store
        .append(
            &sample_attempt_record(
                super::now_millis(),
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Failure,
                false,
                Some(sanitized.as_str()),
                None,
                UsageTokens::default(),
            ),
            365,
        )
        .unwrap();
    let stored_error = store
        .all_records()
        .unwrap()
        .into_iter()
        .find_map(|record| record.error_message)
        .expect("the failure row's sanitized message must be stored");
    assert!(!stored_error.contains(secret));
    assert!(!stored_error.contains(bearer_token));
    assert!(!stored_error.contains(sk_token));
    assert!(stored_error.contains("[redacted]"));

    let raw = fs::read(dir.join("api_gateway_usage.db")).expect("read usage db");
    let raw_text = String::from_utf8_lossy(&raw);
    assert!(!raw_text.contains(secret));
    assert!(
        !raw_text.contains(bearer_token),
        "no Bearer token may reach the log file"
    );
    assert!(
        !raw_text.contains(sk_token),
        "no sk-shaped token may reach the log file"
    );
    assert!(!raw_text.contains("authorization"));
    assert!(!raw_text.contains("api_key"));

    let serialized = serde_json::to_string(&store.all_records().unwrap()).unwrap();
    assert!(!serialized.contains(secret));
    assert!(!serialized.contains(bearer_token));
    assert!(!serialized.contains(sk_token));
    assert!(!serialized.contains("authorization"));
    assert!(!serialized.contains("\"body\""));
    assert!(!serialized.contains("\"headers\""));
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// 20260917-ai-gateway-usage-logs Step 2: forwarding capture (mock upstream)
// ---------------------------------------------------------------------------

fn default_usage_store() -> UsageLogStore {
    UsageLogStore::default_store().expect("default usage store")
}

/// The relay records after the response is on the wire, so poll briefly.
async fn wait_for_usage_logs(expected: u32) -> Vec<UsageLogRecord> {
    let store = default_usage_store();
    for _ in 0..400 {
        let records = store.all_records().unwrap_or_default();
        if records.len() as u32 >= expected {
            return records;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    store.all_records().unwrap_or_default()
}

fn priced(upstream_model: &str, input: f64, cache_read: f64, cache_write: f64, output: f64) -> ModelPrice {
    ModelPrice {
        provider_id: None,
        upstream_model: upstream_model.to_string(),
        input,
        cache_read,
        cache_write,
        output,
        off_peaks: Vec::new(),
        off_peak: None,
    }
}

#[allow(dead_code)]
fn priced_with_provider(
    provider_id: &str,
    upstream_model: &str,
    input: f64,
    cache_read: f64,
    cache_write: f64,
    output: f64,
) -> ModelPrice {
    ModelPrice {
        provider_id: Some(provider_id.to_string()),
        upstream_model: upstream_model.to_string(),
        input,
        cache_read,
        cache_write,
        output,
        off_peaks: Vec::new(),
        off_peak: None,
    }
}

/// AC-001 / AC-003 / REQ-001: a successful non-streaming forward persists one
/// success row with the mapped model/provider, usage tiers and fixed amount,
/// and never any credential.
#[tokio::test]
async fn usage_log_records_successful_non_streaming_forward_and_privacy() {
    let home = temp_home("usage-forward-success");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "chatcmpl",
                "choices": [{"message": {"role": "assistant", "content": "ok"}}],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 5,
                    "prompt_tokens_details": {"cached_tokens": 2}
                }
            }),
        )
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "upstream-secret", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.usage_retention_days = 90;
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, _text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 200);

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1, "exactly one log row per forwarded request");
    let record = &records[0];
    assert!(
        record.terminal,
        "a single completed attempt is the request's terminal row"
    );
    assert_eq!(record.error_message, None, "a success stores no error message");
    assert_eq!(record.result, UsageResult::Success);
    assert_eq!(record.local_model, "local-a");
    assert_eq!(record.upstream_model, "remote-a");
    assert_eq!(record.provider_id, "p1");
    assert_eq!(record.provider_name, "Provider One");
    assert_eq!(record.status, 200);
    assert_eq!(record.input_tokens, 10);
    assert_eq!(record.cache_read_tokens, 2);
    assert_eq!(record.cache_write_tokens, 0);
    assert_eq!(record.output_tokens, 5);
    assert_eq!(record.total_tokens, 17);
    let expected = compute_cost(
        &priced("remote-a", 1.0, 0.5, 2.0, 4.0),
        &tokens(10, 2, 0, 5),
    );
    assert!((record.amount.expect("priced") - expected).abs() < 1e-12);
    assert!(record.duration_ms >= 1, "duration must be positive");

    let serialized = serde_json::to_string(&records).unwrap();
    assert!(!serialized.contains("local-key"));
    assert!(!serialized.contains("upstream-secret"));

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-003 / REQ-003: a successful response without `usage` still creates a row
/// with zero tokens and zero cost.
#[tokio::test]
async fn usage_log_records_zero_tokens_when_upstream_omits_usage() {
    let home = temp_home("usage-forward-no-usage");
    let port = free_port().await;
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "x", "choices": []}))).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, _text) = call_gateway(
        port,
        "POST",
        "/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "stream": false})),
    )
    .await;
    assert_eq!(status, 200);

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1, "row exists even without usage");
    let record = &records[0];
    assert!(record.terminal, "the completed attempt is terminal");
    assert_eq!(record.error_message, None, "a success stores no error message");
    assert_eq!(record.result, UsageResult::Success);
    assert_eq!(record.total_tokens, 0);
    assert_eq!(record.amount, Some(0.0), "priced model with zero tokens costs 0");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-008 / REQ-007: an upstream error response is logged as failure with zero
/// tokens and the caller still receives the upstream status unchanged.
#[tokio::test]
async fn usage_log_records_failure_for_upstream_error_response() {
    let home = temp_home("usage-forward-upstream-error");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(400, json!({"error": {"message": "bad request"}}))
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 400, "caller must receive the upstream status: {text}");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert!(
        record.terminal,
        "a single ReturnToClient attempt is the request's terminal row"
    );
    assert_eq!(record.result, UsageResult::Failure);
    assert_eq!(record.status, 400);
    assert_eq!(
        record.error_message.as_deref(),
        Some("bad request"),
        "the upstream error.message is recorded instead of the generic status text"
    );
    assert_eq!(record.total_tokens, 0);
    assert_eq!(record.amount, Some(0.0));

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-008 / REQ-007: a request with no serving upstream returns 502 and is
/// logged as failure with zero tokens.
#[tokio::test]
async fn usage_log_records_failure_when_no_upstream_can_serve() {
    let home = temp_home("usage-forward-unavailable");
    let port = free_port().await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", "http://127.0.0.1:1", "sk", None);
    provider.mappings = vec![mapping("local-b", "remote-b", None)];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, _text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 502);

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert!(
        record.terminal,
        "the no-candidate row is the request's single terminal row"
    );
    assert_eq!(record.result, UsageResult::Failure);
    assert_eq!(record.status, 502);
    assert_eq!(
        record.error_message, None,
        "a request that reached no upstream records no error message"
    );
    assert_eq!(record.provider_id, "", "no provider is attributed");
    assert_eq!(record.provider_name, "");
    assert_eq!(record.local_model, "local-a");
    assert_eq!(record.upstream_model, "");
    assert_eq!(record.total_tokens, 0);
    assert_eq!(record.amount.unwrap_or(0.0), 0.0);

    let stats = default_usage_store()
        .usage_stats(&TimeRange::default(), false)
        .unwrap();
    assert_eq!(stats.totals.request_count, 1);
    assert_eq!(stats.totals.total_tokens, 0);
    assert_eq!(stats.totals.amount, 0.0);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-011 / REQ-005: a log database that cannot be opened degrades to a
/// swallowed log-write failure — the forwarded request still returns its normal
/// upstream response — and a later open works again once the obstacle is gone.
#[tokio::test]
async fn usage_log_write_failure_is_swallowed_and_the_forwarded_request_still_succeeds() {
    let home = temp_home("usage-log-write-swallowed");
    let port = free_port().await;

    // A directory at the database path makes every open of the log file fail,
    // exactly like an unopenable or unmigratable store.
    let app_dir = crate::config::get_app_dir().expect("app dir");
    let db_path = app_dir.join(super::USAGE_DB_FILE);
    fs::create_dir_all(&db_path).expect("create a directory at the database path");

    let upstream_body = json!({
        "id": "chatcmpl",
        "choices": [{"message": {"role": "assistant", "content": "ok"}}],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "prompt_tokens_details": {"cached_tokens": 2}
        }
    });
    let expected_body = upstream_body.clone();
    let (upstream_url, upstream_log) =
        spawn_mock_upstream(move |_| MockReply::Json(200, upstream_body.clone())).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider =
        upstream_provider("p1", "Provider One", &upstream_url, "upstream-secret", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.usage_retention_days = 90;
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;

    // The caller sees the completely normal forwarded response: the upstream
    // status, the upstream JSON body and no error envelope.
    assert_eq!(
        status, 200,
        "the caller still receives the upstream status: {text}"
    );
    assert!(
        content_type.contains("application/json"),
        "the forwarded content-type is unchanged: {content_type}"
    );
    let body: Value = serde_json::from_str(&text).expect("the forwarded body is JSON");
    assert_eq!(
        body, expected_body,
        "the upstream body must reach the caller unchanged: {text}"
    );
    assert!(
        body.get("error").is_none(),
        "the swallowed log-write failure must never surface as an error envelope: {text}"
    );
    assert!(
        !text.contains("usage log write failed"),
        "the swallowed warning text must never reach the caller: {text}"
    );
    assert_eq!(
        upstream_log.lock().expect("mock log").len(),
        1,
        "the request really was forwarded to the mock upstream"
    );

    // The handler writes its row right after the response reaches the socket;
    // give that attempt the same bounded settling window the neighbouring log
    // tests use so it has run and failed before the obstacle is removed.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let store = UsageLogStore::default_store().expect("default usage store path");
    let error = store
        .count()
        .expect_err("the directory at the database path keeps the log unopenable");
    assert!(
        !error.is_empty(),
        "the unopenable store must report a reason: {error}"
    );
    assert!(
        db_path.is_dir(),
        "the failed log write must not replace the directory"
    );

    // Retry clause (REQ-005): once the obstacle is gone, a later open works and
    // the write succeeds; the swallowed request left no row behind.
    fs::remove_dir(&db_path).expect("remove the directory at the database path");
    let timestamp_ms = super::now_millis();
    UsageLogStore::default_store()
        .expect("default usage store after the obstacle is gone")
        .append(
            &sample_record(
                timestamp_ms,
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(0.1),
                tokens(1, 0, 0, 1),
            ),
            365,
        )
        .expect("a later open retries the write once the path is usable");
    assert_eq!(
        store.count().expect("count after the retry"),
        1,
        "the swallowed write stored no row and the retried write stored exactly one"
    );
    let stored = store.all_records().expect("read the retried row");
    assert_eq!(stored.len(), 1, "exactly the retried row is readable");
    assert_eq!(stored[0].timestamp_ms, timestamp_ms);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-002 / AC-009 / REQ-002 / REQ-003 / REQ-008: streaming forwards the
/// upstream bytes verbatim and captures usage; a no-candidate streaming failure
/// is HTTP 502 JSON (not HTTP 200 SSE) and still records one failure row.
#[tokio::test]
async fn streaming_forward_preserves_bytes_captures_usage_and_fails_all_unavailable() {
    let home = temp_home("usage-forward-stream");
    let port = free_port().await;
    let sse = "data: {\"id\":\"x\",\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n\
               data: {\"id\":\"x\",\"choices\":[{\"delta\":{}}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"prompt_tokens_details\":{\"cached_tokens\":3}}}\n\n\
               data: [DONE]\n\n"
        .to_string();
    let sse_for_mock = sse.clone();
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::Stream(sse_for_mock.clone())).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "stream": true})),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(text, sse, "forwarded bytes must be identical to upstream");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert!(record.terminal, "the completed stream is the terminal row");
    assert_eq!(record.error_message, None, "a success stores no error message");
    assert_eq!(record.result, UsageResult::Success);
    assert_eq!(record.input_tokens, 11);
    assert_eq!(record.cache_read_tokens, 3);
    assert_eq!(record.output_tokens, 7);
    let expected = compute_cost(&priced("remote-a", 1.0, 0.5, 2.0, 4.0), &tokens(11, 3, 0, 7));
    assert!((record.amount.expect("priced") - expected).abs() < 1e-12);

    // Streaming with no serving upstream: HTTP 502 JSON envelope is failure.
    let (empty_status, empty_ct, empty_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "unknown-local", "stream": true})),
    )
    .await;
    assert_eq!(
        empty_status, 502,
        "a pre-stream streaming failure must answer HTTP 502: {empty_text}"
    );
    assert!(
        empty_ct.contains("application/json"),
        "content-type must be application/json: {empty_ct}"
    );
    let empty_body = assert_standard_error_envelope(&empty_text);
    assert_eq!(empty_body["error"]["code"], "all_providers_unavailable");

    let records = wait_for_usage_logs(2).await;
    assert_eq!(records.len(), 2);
    let failure = records
        .iter()
        .find(|record| record.result == UsageResult::Failure)
        .expect("streaming all-unavailable row");
    assert_eq!(
        failure.status, 502,
        "the log records the gateway failure status, not the transport status"
    );
    assert!(
        failure.terminal,
        "the no-candidate row is the request's terminal row"
    );
    assert_eq!(
        failure.error_message, None,
        "a request that reached no upstream records no error message"
    );
    assert_eq!(failure.provider_id, "", "no provider is attributed");
    assert_eq!(failure.total_tokens, 0);
    assert_eq!(failure.amount.unwrap_or(0.0), 0.0);
    assert_eq!(
        records.iter().filter(|record| record.terminal).count(),
        2,
        "each of the two requests has exactly one terminal row"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-009 / REQ-003 / REQ-008: a streaming all-unavailable failure answers HTTP
/// 502 JSON but still records the real upstream HTTP status in the request log,
/// never the transport status.
#[tokio::test]
async fn streaming_all_unavailable_logs_real_upstream_status() {
    let home = temp_home("usage-forward-stream-status");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(503, json!({"error": {"message": "upstream down"}}))
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "stream": true})),
    )
    .await;
    assert_eq!(
        status, 502,
        "streaming all-unavailable must answer HTTP 502: {text}"
    );
    assert!(
        content_type.contains("application/json"),
        "content-type must be application/json: {content_type}"
    );
    let body = assert_standard_error_envelope(&text);
    assert_eq!(body["error"]["code"], "all_providers_unavailable");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert!(record.terminal, "the completed attempt is the terminal row");
    assert_eq!(record.result, UsageResult::Failure);
    assert_eq!(
        record.status, 503,
        "the log must show the real upstream failure status, not the transport status"
    );
    assert_eq!(record.provider_id, "p1");
    assert_eq!(record.local_model, "local-a");
    assert_eq!(record.upstream_model, "remote-a");
    assert_eq!(
        record.error_message.as_deref(),
        Some("upstream down"),
        "the upstream error.message is recorded"
    );
    assert_eq!(record.total_tokens, 0);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-009 / REQ-003 / REQ-008: a streaming all-unavailable caused by upstream
/// connection errors answers HTTP 502 JSON but records status 0, never the
/// transport status.
#[tokio::test(start_paused = true)]
async fn streaming_all_unavailable_network_error_logs_zero_status() {
    let _home = isolated_temp_home("usage-forward-stream-network");
    let _ticker = spawn_paused_clock_ticker();
    let (drop_url, _log) = spawn_mock_upstream(|_| MockReply::Drop).await;
    let provider = upstream_provider("p1", "Provider One", &drop_url, "sk", Some("remote-a"));
    let mut config = GatewayConfig::default();
    config.providers = vec![provider.clone()];
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();

    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    let mut attempts = Vec::new();
    let capture = super::runtime_http::attempt_streaming(
        &mut server,
        std::slice::from_ref(&provider),
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await
    .expect("streaming attempt");
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.expect("read relay stream");
    let text = String::from_utf8_lossy(&out).into_owned();
    let (status_line, body) = raw_http_status_and_body(&text);
    assert!(
        status_line.starts_with("HTTP/1.1 502"),
        "a pre-stream failure must answer HTTP 502: {text}"
    );
    assert!(
        text.to_ascii_lowercase()
            .contains("content-type: application/json"),
        "content-type must be application/json: {text}"
    );
    assert!(
        !text.contains("text/event-stream"),
        "a pre-stream failure must not answer SSE: {text}"
    );
    let envelope = assert_standard_error_envelope(&body);
    assert_eq!(envelope["error"]["code"], "all_providers_unavailable");
    assert_eq!(
        capture.status, 0,
        "a network failure has no HTTP status and must not be logged as 200"
    );
    assert_eq!(
        attempts.len(),
        1,
        "one entry for the single completed network failure"
    );
    assert_eq!(attempts[0].provider_id, "p1");
    assert_eq!(attempts[0].upstream_model, "remote-a");
    assert_eq!(attempts[0].status, 0);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert!(
        attempts[0]
            .error_message
            .as_deref()
            .unwrap_or("")
            .contains("network error"),
        "a network failure records its network description: {:?}",
        attempts[0].error_message
    );
    assert!(attempts[0].usage.is_none());
    assert!(attempts[0].duration_ms >= 1);
}

/// AC-008 / REQ-007: 401, `GET /v1/models` and unknown paths/methods add no row.
#[tokio::test]
async fn unauthorized_models_and_unknown_routes_are_not_logged() {
    let home = temp_home("usage-forward-not-logged");
    let port = free_port().await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (unauthorized, _ct, _body) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer wrong-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(unauthorized, 401);

    let (models, _ct, _body) =
        call_gateway(port, "GET", "/v1/models", &[("authorization", "Bearer local-key")], None).await;
    assert_eq!(models, 200);

    let (unknown_path, _ct, _body) = call_gateway(
        port,
        "POST",
        "/v1/embeddings",
        &[("authorization", "Bearer local-key")],
        Some(json!({})),
    )
    .await;
    assert_eq!(unknown_path, 404);

    let (unknown_method, _ct, _body) = call_gateway(
        port,
        "GET",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        None,
    )
    .await;
    assert_eq!(unknown_method, 404);

    // These requests never record; give any erroneous write a moment to appear.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(default_usage_store().count().unwrap(), 0);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-002 / REQ-001: a downstream disconnect mid-forward discards all buffered
/// log rows; no cancelled terminal row is written.
#[tokio::test]
async fn downstream_cancel_writes_zero_rows() {
    let home = isolated_temp_home("usage-forward-cancelled");
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind held upstream");
    let upstream_url = format!("http://{}", listener.local_addr().expect("upstream address"));
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let upstream = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept held upstream");
        super::runtime_http::read_http_request(&mut stream)
            .await
            .expect("read held upstream request");
        let _ = entered_tx.send(());
        let _ = release_rx.await;
        let body = br#"{"id":"late"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.write_all(body).await;
    });

    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    config
        .providers
        .push(upstream_provider("p1", "Held Provider", &upstream_url, "sk", Some("remote-default")));
    super::storage::write_config(&config).expect("write relay config");

    let (client, mut handler) = spawn_handle_connection(false).await;
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

    // Cancelled inbound requests persist no rows (REQ-001 / AC-002).
    let records = default_usage_store().all_records().unwrap_or_default();
    assert!(
        records.is_empty(),
        "a cancelled request must write zero log rows: {} observed",
        records.len()
    );

    let stats = default_usage_store()
        .usage_stats(&TimeRange::default(), false)
        .unwrap();
    assert_eq!(
        stats.totals.request_count, 0,
        "cancelled requests are excluded from usage statistics"
    );
    assert_eq!(stats.totals.total_tokens, 0);
    assert_eq!(stats.totals.amount, 0.0);
    assert_eq!(stats.totals.unpriced_count, 0);
    drop(home);
}

/// AC-005: an unpriced upstream model records `None` and contributes 0 to the
/// aggregate amount while still counting as an unpriced request.
#[tokio::test]
async fn unpriced_model_records_none_amount_and_excludes_it_from_totals() {
    let home = temp_home("usage-forward-unpriced");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "x",
                "choices": [],
                "usage": {"prompt_tokens": 5, "completion_tokens": 5}
            }),
        )
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-unpriced", None)];
    config.providers.push(provider);
    // No price row for `remote-unpriced`.
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _ct, _text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 200);

    let records = wait_for_usage_logs(1).await;
    assert!(records[0].terminal, "the completed attempt is terminal");
    assert_eq!(records[0].error_message, None);
    assert_eq!(records[0].amount, None, "unpriced stays None");
    assert_eq!(records[0].total_tokens, 10);

    let stats = default_usage_store()
        .usage_stats(&TimeRange::default(), false)
        .unwrap();
    assert_eq!(stats.totals.request_count, 1);
    assert_eq!(stats.totals.amount, 0.0, "unpriced excluded from amount");
    assert_eq!(stats.totals.unpriced_count, 1);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// 20260917-ai-gateway-usage-logs Step 3: commands, merge semantics, registration
// ---------------------------------------------------------------------------

/// REQ-005: upserting a provider together with its price rows replaces only
/// that provider's rows and preserves other providers, keys, default key,
/// terminal_syncs and retention.
#[test]
fn upsert_provider_with_prices_preserves_existing_config_fields() {
    with_temp_home("upsert-prices-preserve", |_home| {
        let mut config = GatewayConfig::default();
        let mut p1 = provider("p1");
        p1.mappings = vec![mapping("local-a", "remote-a", None)];
        config.providers.push(p1);
        let mut q = provider("q");
        q.mappings = vec![mapping("local-q", "remote-q", None)];
        config.providers.push(q);
        config.keys.push(key("k1", true));
        config.default_key_id = Some("k1".to_string());
        config.terminal_syncs.push(TerminalSyncRecord {
            provider_id: "p1".to_string(),
            tool: "opencode".to_string(),
            synced_key_id: "k1".to_string(),
            synced_base_url: "http://127.0.0.1:17688".to_string(),
            synced_at: 7,
        });
        config.usage_retention_days = 30;
        super::storage::write_config(&config).expect("seed config");

        let mut edited = provider("p1");
        edited.mappings = vec![mapping("local-a", "remote-a", None)];
        let saved = super::commands::api_gateway_upsert_provider(
            edited,
            Some(vec![priced_with_provider(
                "p1",
                "remote-a",
                2.0,
                0.0,
                0.0,
                3.0,
            )]),
        )
        .expect("upsert with prices");

        assert_eq!(saved.providers.len(), 2, "the other provider must remain");
        assert!(saved.providers.iter().any(|candidate| candidate.id == "q"));
        assert!(saved.model_prices.iter().any(|row| {
            row.provider_id.as_deref() == Some("p1")
                && row.upstream_model == "remote-a"
                && row.output == 3.0
        }));

        let reloaded = super::storage::read_config().expect("reload config");
        assert_eq!(reloaded.providers.len(), 2);
        assert!(reloaded.providers.iter().any(|candidate| candidate.id == "q"));
        assert_eq!(
            reloaded
                .providers
                .iter()
                .find(|candidate| candidate.id == "p1")
                .expect("p1 must remain")
                .api_key,
            "sk-test"
        );
        assert_eq!(reloaded.keys.len(), 1);
        assert_eq!(reloaded.keys[0].value, "value-k1");
        assert_eq!(reloaded.default_key_id.as_deref(), Some("k1"));
        assert_eq!(reloaded.terminal_syncs.len(), 1);
        assert_eq!(reloaded.usage_retention_days, 30, "retention untouched");
        assert_eq!(reloaded.model_prices.len(), 1);
        assert_eq!(reloaded.model_prices[0].provider_id.as_deref(), Some("p1"));
        assert_eq!(reloaded.model_prices[0].upstream_model, "remote-a");
        assert_eq!(reloaded.model_prices[0].output, 3.0);
    });
}

/// AC-012 / REQ-010: invalid retention is rejected with an actionable error and
/// the stored value plus the rest of the config are untouched; 1 and 365 work.
#[test]
fn api_gateway_usage_retention_save_rejects_invalid_and_keeps_stored_value() {
    with_temp_home("retention-save", |_home| {
        let mut config = GatewayConfig::default();
        config.usage_retention_days = 30;
        config.providers.push(provider("p1"));
        config.keys.push(key("k1", true));
        super::storage::write_config(&config).expect("seed config");

        let zero = super::commands::api_gateway_usage_retention_save(0).unwrap_err();
        assert!(zero.contains("1") && zero.contains("365"), "actionable error: {zero}");
        assert!(super::commands::api_gateway_usage_retention_save(400).is_err());

        assert_eq!(
            super::commands::api_gateway_usage_retention_get().unwrap(),
            30,
            "stored value unchanged after rejection"
        );
        let reloaded = super::storage::read_config().unwrap();
        assert_eq!(reloaded.usage_retention_days, 30);
        assert_eq!(reloaded.providers.len(), 1, "rejection must not rewrite config");

        assert_eq!(
            super::commands::api_gateway_usage_retention_save(1).unwrap(),
            1
        );
        assert_eq!(
            super::commands::api_gateway_usage_retention_save(365).unwrap(),
            365
        );
        assert_eq!(
            super::commands::api_gateway_usage_retention_get().unwrap(),
            365
        );
        assert_eq!(
            super::storage::read_config().unwrap().providers.len(),
            1,
            "accepted saves must also preserve providers"
        );
    });
}

/// AC-015 / AC-016 / AC-020 / AC-021 / AC-022: the stats/logs commands resolve
/// the range, aggregate, group, filter and paginate entirely in the backend.
#[test]
fn api_gateway_usage_stats_and_request_logs_commands_aggregate_and_paginate() {
    with_temp_home("usage-commands", |_home| {
        let store = UsageLogStore::default_store().expect("usage store");
        let now = super::now_millis();
        store
            .append(
                &sample_record(
                    now - 1_000,
                    "local-a",
                    "remote-a",
                    "p1",
                    "Provider One",
                    UsageResult::Success,
                    Some(0.5),
                    tokens(10, 0, 0, 5),
                ),
                90,
            )
            .unwrap();
        store
            .append(
                &sample_record(
                    now - 500,
                    "local-a",
                    "remote-a",
                    "p2",
                    "Provider Two",
                    UsageResult::Failure,
                    Some(1.0),
                    tokens(20, 0, 0, 10),
                ),
                90,
            )
            .unwrap();

        let stats = super::commands::api_gateway_usage_stats(None).unwrap();
        assert_eq!(stats.granularity, "day");
        assert_eq!(stats.totals.request_count, 2);
        assert_eq!(stats.totals.total_tokens, 45);
        assert!((stats.totals.amount - 1.5).abs() < 1e-9);
        assert_eq!(stats.models.len(), 1);
        assert_eq!(stats.models[0].providers.len(), 2, "per-provider detail");
        assert_eq!(stats.buckets.len(), 1, "one day bucket");

        let today = super::commands::api_gateway_usage_stats(Some(1)).unwrap();
        assert_eq!(today.granularity, "hour", "today buckets by hour");

        let page = super::commands::api_gateway_request_logs(None, None, None, None, None).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, 50);
        assert_eq!(page.records.len(), 2);
        assert!(
            page.records[0].timestamp_ms > page.records[1].timestamp_ms,
            "newest first"
        );

        let failed = super::commands::api_gateway_request_logs(
            None,
            None,
            Some("failure".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(failed.total, 1);
        assert_eq!(failed.records[0].result, UsageResult::Failure);

        let by_model = super::commands::api_gateway_request_logs(
            None,
            Some("model".to_string()),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(by_model.group_by.as_deref(), Some("model"));
        assert_eq!(by_model.groups.len(), 1);
        assert_eq!(by_model.groups[0].error_count, 1, "failure counted as error");

        let by_day = super::commands::api_gateway_request_logs(
            None,
            Some("day".to_string()),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(by_day.groups.len(), 1);
        assert_eq!(by_day.groups[0].request_count, 2);
        assert_eq!(by_day.groups[0].error_count, 1);

        let clamped = super::commands::api_gateway_request_logs(
            None,
            None,
            None,
            None,
            Some(99),
        )
        .unwrap();
        assert_eq!(clamped.page, 1, "page clamps to the only page");

        // AC-021: a filter with no matches returns an empty, error-free page.
        let empty = super::commands::api_gateway_request_logs(
            None,
            None,
            Some("failure".to_string()),
            Some("does-not-exist".to_string()),
            None,
        )
        .unwrap();
        assert_eq!(empty.total, 0);
        assert!(empty.records.is_empty());
        assert_eq!(empty.total_pages, 1);

        assert!(super::commands::api_gateway_request_logs(
            None,
            Some("bogus".to_string()),
            None,
            None,
            None,
        )
        .is_err());
    });
}

/// REQ-001: the unversioned `/responses` path also enters the normalized flow
/// and is logged with the Responses usage shape.
#[tokio::test]
async fn usage_log_records_unversioned_responses_path() {
    let home = temp_home("usage-forward-responses");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "resp",
                "output": [],
                "usage": {
                    "input_tokens": 4,
                    "output_tokens": 6,
                    "input_tokens_details": {"cached_tokens": 1}
                }
            }),
        )
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.protocol = UpstreamProtocol::Responses;
    provider.mappings = vec![mapping("local-r", "remote-r", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-r", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/responses",
        &[("x-api-key", "local-key")],
        Some(json!({"model": "local-r"})),
    )
    .await;
    assert_eq!(status, 200, "unexpected response: {text}");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert!(record.terminal, "the completed attempt is terminal");
    assert_eq!(record.error_message, None, "a success stores no error message");
    assert_eq!(record.result, UsageResult::Success);
    assert_eq!(record.local_model, "local-r");
    assert_eq!(record.upstream_model, "remote-r");
    assert_eq!(record.input_tokens, 4);
    assert_eq!(record.cache_read_tokens, 1);
    assert_eq!(record.output_tokens, 6);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// 20260917-ai-gateway-usage-logs Step 4: behavior-test hardening for gaps left
// by the Step 1-3 coverage (cache-write mapping, streaming fallbacks, price
// history immutability, retention/page boundaries, grouping/filter depth and
// end-to-end privacy).
// ---------------------------------------------------------------------------

/// AC-001 / AC-003 / AC-004: a non-streaming response reporting both cache
/// read and cache write tiers maps every tier, and the write tier is priced too.
#[tokio::test]
async fn forwarding_records_cache_read_and_write_tiers_non_streaming() {
    let home = temp_home("usage-forward-cache-tiers");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "x",
                "choices": [{"message": {"role": "assistant", "content": "ok"}}],
                "usage": {
                    "prompt_tokens": 100,
                    "completion_tokens": 40,
                    "cache_read_input_tokens": 20,
                    "cache_creation_input_tokens": 10
                }
            }),
        )
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, _text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 200);

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert!(record.terminal, "the completed attempt is terminal");
    assert_eq!(record.error_message, None, "a success stores no error message");
    assert_eq!(record.result, UsageResult::Success);
    assert_eq!(record.input_tokens, 100);
    assert_eq!(record.cache_read_tokens, 20);
    assert_eq!(record.cache_write_tokens, 10);
    assert_eq!(record.output_tokens, 40);
    assert_eq!(record.total_tokens, 170);
    // Independent expected value: $1/1M input, $0.50/1M cache-read,
    // $2/1M cache-write, $4/1M output.
    let expected = (100.0 * 1.0 + 20.0 * 0.5 + 10.0 * 2.0 + 40.0 * 4.0) / 1_000_000.0;
    assert!((record.amount.expect("priced") - expected).abs() < 1e-12);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-002 / AC-003: streaming usage carrying cache read and cache write tiers
/// is parsed from the SSE tail, priced across all four tiers, and the bytes the
/// caller receives stay identical to the upstream stream.
#[tokio::test]
async fn streaming_forward_records_cache_read_and_write_tiers_and_preserves_bytes() {
    let home = temp_home("usage-stream-cache-tiers");
    let port = free_port().await;
    let sse = "data: {\"id\":\"x\",\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n\
               data: {\"id\":\"x\",\"choices\":[{\"delta\":{}}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"prompt_tokens_details\":{\"cached_tokens\":3},\"cache_creation_input_tokens\":5}}\n\n\
               data: [DONE]\n\n"
        .to_string();
    let sse_for_mock = sse.clone();
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::Stream(sse_for_mock.clone())).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "stream": true})),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(text, sse, "forwarded bytes must be identical to upstream");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert!(record.terminal, "the completed stream is terminal");
    assert_eq!(record.error_message, None, "a success stores no error message");
    assert_eq!(record.result, UsageResult::Success);
    assert_eq!(record.input_tokens, 11);
    assert_eq!(record.cache_read_tokens, 3);
    assert_eq!(record.cache_write_tokens, 5);
    assert_eq!(record.output_tokens, 7);
    assert_eq!(record.total_tokens, 26);
    let expected = (11.0 * 1.0 + 3.0 * 0.5 + 5.0 * 2.0 + 7.0 * 4.0) / 1_000_000.0;
    assert!((record.amount.expect("priced") - expected).abs() < 1e-12);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-003: a completed SSE stream with no usage chunk still records one success
/// row with zero tokens and a zero (not missing) amount for a priced model.
#[tokio::test]
async fn streaming_success_without_usage_records_zero_tokens() {
    let home = temp_home("usage-stream-no-usage");
    let port = free_port().await;
    let sse = "data: {\"id\":\"x\",\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n\
               data: [DONE]\n\n"
        .to_string();
    let sse_for_mock = sse.clone();
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::Stream(sse_for_mock.clone())).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "stream": true})),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(text, sse);

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1, "row exists even when the stream omits usage");
    assert!(records[0].terminal, "the completed stream is terminal");
    assert_eq!(records[0].error_message, None);
    assert_eq!(records[0].result, UsageResult::Success);
    assert_eq!(records[0].total_tokens, 0);
    assert_eq!(records[0].amount, Some(0.0));

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-008 / REQ-007: a streaming request rejected by its upstream before any
/// byte is logged as failure with the upstream status, and the caller still
/// receives that status unchanged.
#[tokio::test]
async fn streaming_upstream_client_error_is_logged_failure_and_returned_unchanged() {
    let home = temp_home("usage-stream-client-error");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(400, json!({"error": {"message": "rejected"}}))
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "stream": true})),
    )
    .await;
    assert_eq!(status, 400, "caller receives the upstream status: {text}");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    assert!(
        records[0].terminal,
        "a single ReturnToClient attempt is the request's terminal row"
    );
    assert_eq!(records[0].result, UsageResult::Failure);
    assert_eq!(records[0].status, 400);
    assert_eq!(
        records[0].error_message.as_deref(),
        Some("rejected"),
        "the upstream error.message is recorded"
    );
    assert_eq!(records[0].total_tokens, 0);
    assert_eq!(records[0].amount.unwrap_or(0.0), 0.0);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-002 / REQ-001: a downstream disconnect mid-forward while streaming
/// discards all buffered log rows; no cancelled terminal row is written.
#[tokio::test]
async fn downstream_cancel_during_streaming_writes_zero_rows() {
    let home = isolated_temp_home("usage-forward-cancel-stream");
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind held upstream");
    let upstream_url = format!("http://{}", listener.local_addr().expect("upstream address"));
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let upstream = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept held upstream");
        super::runtime_http::read_http_request(&mut stream)
            .await
            .expect("read held upstream request");
        let _ = entered_tx.send(());
        let _ = release_rx.await;
        // Never start the stream; the client disconnects while the upstream waits.
    });

    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "p1",
        "Held Provider",
        &upstream_url,
        "sk",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).expect("write relay config");

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

    // Cancelled inbound requests persist no rows (REQ-001 / AC-002).
    let records = default_usage_store().all_records().unwrap_or_default();
    assert!(
        records.is_empty(),
        "a cancelled streaming request must write zero log rows: {} observed",
        records.len()
    );

    let stats = default_usage_store()
        .usage_stats(&TimeRange::default(), false)
        .unwrap();
    assert_eq!(
        stats.totals.request_count, 0,
        "cancelled requests are excluded from usage statistics"
    );
    assert_eq!(stats.totals.total_tokens, 0);
    assert_eq!(stats.totals.amount, 0.0);
    drop(home);
}

/// AC-001 privacy: after a real forwarded request the persisted database and
/// the returned records contain neither the local Key, the upstream ApiKey, a
/// forwarded request header value, nor the request/response body text.
#[tokio::test]
async fn forwarded_logs_never_contain_keys_headers_or_bodies() {
    let home = temp_home("usage-forward-privacy-strong");
    let port = free_port().await;
    let local_key = "sk-gateway-local-PRIVACY-9a1b";
    let upstream_key = "sk-upstream-PRIVACY-4c2d";
    let header_marker = "marker-header-PRIVACY-7e3f";
    let body_marker = "body-PRIVACY-1c8a";
    let response_marker = "response-PRIVACY-5d0b";
    let (upstream_url, _log) = spawn_mock_upstream(move |_| {
        MockReply::Json(
            200,
            json!({
                "id": "x",
                "choices": [{"message": {"role": "assistant", "content": response_marker}}],
                "usage": {"prompt_tokens": 3, "completion_tokens": 2}
            }),
        )
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", local_key));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, upstream_key, None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let authorization = format!("Bearer {local_key}");
    let headers = [
        ("authorization", authorization.as_str()),
        ("x-privacy-marker", header_marker),
    ];
    let (status, _content_type, _text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &headers,
        Some(json!({"model": "local-a", "prompt": body_marker})),
    )
    .await;
    assert_eq!(status, 200);

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    assert!(records[0].terminal, "the completed attempt is terminal");
    assert_eq!(records[0].error_message, None);
    let raw = fs::read(
        crate::config::get_app_dir()
            .expect("app dir")
            .join("api_gateway_usage.db"),
    )
    .expect("read usage db");
    let raw_text = String::from_utf8_lossy(&raw);
    let serialized = serde_json::to_string(&records).expect("serialize records");
    for secret in [
        local_key,
        upstream_key,
        header_marker,
        body_marker,
        response_marker,
    ] {
        assert!(!raw_text.contains(secret), "usage db leaked {secret}");
        assert!(!serialized.contains(secret), "returned records leaked {secret}");
    }
    assert!(!raw_text.contains("authorization"));
    assert!(!raw_text.contains("x-api-key"));
    assert!(!serialized.contains("\"body\""));
    assert!(!serialized.contains("\"headers\""));

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-004 / AC-005: the amount is fixed when the row is recorded. Pricing an
/// initially unpriced model and later changing the price never rewrites the
/// historical rows, while a request made after the change is priced with it.
#[tokio::test]
async fn price_change_does_not_alter_historical_amounts() {
    let home = temp_home("usage-price-history");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "x",
                "choices": [],
                "usage": {"prompt_tokens": 1000, "completion_tokens": 500}
            }),
        )
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    // `remote-a` starts unpriced.
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let auth: &[(&str, &str)] = &[("authorization", "Bearer local-key")];

    let (status, _ct, _text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        auth,
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 200);
    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1);
    assert!(records[0].terminal, "the completed attempt is terminal");
    assert_eq!(records[0].error_message, None);
    assert_eq!(records[0].amount, None, "unpriced at record time stays unpriced");

    // Adding a price later must not retroactively price the existing row, but
    // must price the next request. Prices now ride along with the provider.
    let current = super::storage::read_config().expect("read config before pricing");
    let p1 = current
        .providers
        .iter()
        .find(|candidate| candidate.id == "p1")
        .expect("p1 must exist")
        .clone();
    super::commands::api_gateway_upsert_provider(
        p1,
        Some(vec![priced_with_provider(
            "p1",
            "remote-a",
            1.0,
            0.0,
            0.0,
            2.0,
        )]),
    )
    .unwrap();
    let (status, _ct, _text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        auth,
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 200);
    let records = wait_for_usage_logs(2).await;
    assert_eq!(records.len(), 2);
    let expected_a = (1000.0 * 1.0 + 500.0 * 2.0) / 1_000_000.0;
    let priced_record = records
        .iter()
        .find(|record| record.amount.is_some())
        .expect("second request is priced");
    assert!((priced_record.amount.unwrap() - expected_a).abs() < 1e-12);

    // Changing the price again must not rewrite any history.
    let current = super::storage::read_config().expect("read config before re-pricing");
    let p1 = current
        .providers
        .iter()
        .find(|candidate| candidate.id == "p1")
        .expect("p1 must exist")
        .clone();
    super::commands::api_gateway_upsert_provider(
        p1,
        Some(vec![priced_with_provider(
            "p1",
            "remote-a",
            9.0,
            9.0,
            9.0,
            9.0,
        )]),
    )
    .unwrap();
    let history = default_usage_store().all_records().unwrap();
    assert_eq!(history.len(), 2);
    assert!(
        history.iter().all(|record| record.terminal),
        "each single-attempt request writes one terminal row"
    );
    assert_eq!(
        history.iter().filter(|record| record.amount.is_none()).count(),
        1,
        "the originally unpriced row stays unpriced"
    );
    let frozen = history
        .iter()
        .find(|record| record.amount.is_some())
        .expect("priced history");
    assert!(
        (frozen.amount.unwrap() - expected_a).abs() < 1e-12,
        "historical amount is fixed at record time"
    );
    let new_price_amount = (1000.0 * 9.0 + 500.0 * 9.0) / 1_000_000.0;
    assert!(
        history
            .iter()
            .all(|record| (record.amount.unwrap_or(0.0) - new_price_amount).abs() > 1e-12),
        "no historical row uses the new price"
    );

    let stats = default_usage_store()
        .usage_stats(&TimeRange::default(), false)
        .unwrap();
    assert_eq!(stats.totals.request_count, 2);
    assert!((stats.totals.amount - expected_a).abs() < 1e-9, "totals use frozen amounts");
    assert_eq!(stats.totals.unpriced_count, 1);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-010 / REQ-009: the 365-day boundary keeps an old row, then a write at the
/// 1-day boundary deletes everything older than one day and nothing inside it.
#[test]
fn retention_one_and_365_deletion_boundaries() {
    let (dir, store) = usage_store("usage-retention-boundaries");
    let now = super::now_millis();
    let day = 86_400_000i64;

    // Retention 365 keeps both a 300-day-old row and a 2-day-old row.
    for (age, model) in [(300, "local-old"), (2, "local-stale"), (0, "local-fresh")] {
        store
            .append(
                &sample_record(
                    now - age * day,
                    model,
                    "remote-a",
                    "p1",
                    "Provider One",
                    UsageResult::Success,
                    Some(1.0),
                    tokens(1, 0, 0, 0),
                ),
                365,
            )
            .unwrap();
    }
    let at_365 = store.all_records().unwrap();
    assert_eq!(at_365.len(), 3, "retention 365 keeps rows inside a year");
    assert!(at_365.iter().any(|record| record.local_model == "local-old"));

    // Tightening to 1 day deletes both out-of-window rows and never the fresh one.
    store
        .append(
            &sample_record(
                now,
                "local-newest",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(1.0),
                tokens(1, 0, 0, 0),
            ),
            1,
        )
        .unwrap();
    let at_1 = store.all_records().unwrap();
    assert_eq!(at_1.len(), 2, "retention 1 keeps only the two fresh rows");
    assert!(
        at_1.iter().all(|record| record.timestamp_ms > now - day),
        "every remaining row is inside the 1-day window"
    );
    assert!(
        at_1.iter().any(|record| record.local_model == "local-newest"),
        "the newly written row survives its own cleanup"
    );
    assert!(!at_1.iter().any(|record| record.local_model == "local-stale"));
    assert!(!at_1.iter().any(|record| record.local_model == "local-old"));
    let _ = fs::remove_dir_all(&dir);
}

/// AC-022 / REQ-019: 50-row pages clamp when the selected range shrinks below
/// the requested page, and page 0 behaves as the default first page.
#[test]
fn logs_page_clamps_when_range_shrinks_and_defaults_to_first_page() {
    let (dir, store) = usage_store("usage-page-clamp");
    let wide_start = rfc3339_millis("2026-09-15T00:00:00+08:00");
    let wide_end = rfc3339_millis("2026-09-16T00:00:00+08:00");
    let narrow_start = wide_end;
    let narrow_end = rfc3339_millis("2026-09-17T00:00:00+08:00");
    for index in 0..120i64 {
        store
            .append(
                &sample_record(
                    wide_start + 8 * 3_600_000 + index * 1_000,
                    "local-a",
                    "remote-a",
                    "p1",
                    "Provider One",
                    UsageResult::Success,
                    Some(0.1),
                    tokens(1, 0, 0, 1),
                ),
                365,
            )
            .unwrap();
    }
    for index in 0..10i64 {
        store
            .append(
                &sample_record(
                    narrow_start + 8 * 3_600_000 + index * 1_000,
                    "local-b",
                    "remote-a",
                    "p1",
                    "Provider One",
                    UsageResult::Success,
                    Some(0.1),
                    tokens(1, 0, 0, 1),
                ),
                365,
            )
            .unwrap();
    }

    let wide = TimeRange {
        start_ms: Some(wide_start),
        end_ms: Some(wide_end),
    };
    let page_three = store
        .query_logs(&wide, &LogFilter::default(), 3)
        .unwrap();
    assert_eq!(page_three.total, 120);
    assert_eq!(page_three.total_pages, 3);
    assert_eq!(page_three.page, 3);
    assert_eq!(page_three.records.len(), 20);
    assert_eq!(page_three.page_size, 50);

    let narrow = TimeRange {
        start_ms: Some(narrow_start),
        end_ms: Some(narrow_end),
    };
    let clamped = store
        .query_logs(&narrow, &LogFilter::default(), 3)
        .unwrap();
    assert_eq!(clamped.total, 10);
    assert_eq!(clamped.total_pages, 1);
    assert_eq!(
        clamped.page, 1,
        "an out-of-range page converges instead of showing a blank page"
    );
    assert_eq!(clamped.records.len(), 10);

    let defaulted = store.query_logs(&wide, &LogFilter::default(), 0).unwrap();
    assert_eq!(defaulted.page, 1, "page 0 is treated as the default first page");
    assert_eq!(defaulted.records.len(), 50);
    let _ = fs::remove_dir_all(&dir);
}

/// AC-020 / REQ-017: per-day grouping reports the last request time and counts
/// only `failure` as an error (never `cancelled`); model grouping follows the
/// same error rule.
#[test]
/// AC-004 / REQ-002: grouped model/day results exclude cancelled rows from
/// request_count, error_count and last_request_at.
#[test]
fn grouped_rows_exclude_cancelled_from_errors_and_report_last_request() {
    let (dir, store) = usage_store("usage-groups-errors");
    let day_one = rfc3339_millis("2026-09-15T10:00:00+08:00");
    let day_two = rfc3339_millis("2026-09-16T10:00:00+08:00");
    let day_one_last = day_one + 3_000;
    let day_two_last = day_two + 2_000;
    store
        .append(&sample_record(day_one, "local-a", "remote-a", "p1", "Provider One", UsageResult::Success, Some(0.1), tokens(1, 0, 0, 1)), 365)
        .unwrap();
    store
        .append(&sample_record(day_one + 1_000, "local-a", "remote-a", "p1", "Provider One", UsageResult::Failure, Some(0.1), tokens(1, 0, 0, 1)), 365)
        .unwrap();
    store
        .append(&sample_record(day_one_last, "local-a", "remote-a", "p1", "Provider One", UsageResult::Cancelled, Some(0.1), tokens(1, 0, 0, 1)), 365)
        .unwrap();
    // Day two rows are inserted out of order to prove the last time is by value.
    store
        .append(&sample_record(day_two_last, "local-b", "remote-a", "p2", "Provider Two", UsageResult::Failure, Some(0.2), tokens(1, 0, 0, 1)), 365)
        .unwrap();
    store
        .append(&sample_record(day_two, "local-b", "remote-a", "p2", "Provider Two", UsageResult::Cancelled, Some(0.2), tokens(1, 0, 0, 1)), 365)
        .unwrap();

    let days = store
        .group_logs(&TimeRange::default(), &LogFilter::default(), "day")
        .unwrap();
    assert_eq!(days.len(), 2);
    let first = days
        .iter()
        .find(|group| group.group == "2026-09-15")
        .expect("day one group");
    assert_eq!(first.request_count, 2, "cancelled excluded from request count");
    assert_eq!(first.error_count, 1, "cancelled is not an error");
    assert_eq!(first.last_request_at_ms, day_one + 1_000, "cancelled timestamp excluded from last_request");
    let second = days
        .iter()
        .find(|group| group.group == "2026-09-16")
        .expect("day two group");
    assert_eq!(second.request_count, 1, "cancelled excluded from request count");
    assert_eq!(second.error_count, 1);
    assert_eq!(second.last_request_at_ms, day_two_last, "non-cancelled row determines last_request");

    let models = store
        .group_logs(&TimeRange::default(), &LogFilter::default(), "model")
        .unwrap();
    let local_a = models
        .iter()
        .find(|group| group.group == "local-a")
        .expect("local-a group");
    assert_eq!(local_a.request_count, 2, "cancelled excluded from request count");
    assert_eq!(local_a.error_count, 1, "cancelled is not an error");
    let local_b = models
        .iter()
        .find(|group| group.group == "local-b")
        .expect("local-b group");
    assert_eq!(local_b.request_count, 1, "cancelled excluded from request count");
    assert_eq!(local_b.error_count, 1);
    let _ = fs::remove_dir_all(&dir);
}

/// AC-005 / REQ-002: a legacy `status="cancelled"` filter returns an empty page
/// (total = 0, total_pages = 1, no records). Other filter combinations still
/// work against non-cancelled rows.
#[test]
fn logs_filter_by_status_and_model_together_exclude_cancelled() {
    let (dir, store) = usage_store("usage-filter-compose");
    let base = rfc3339_millis("2026-09-16T08:00:00+08:00");
    let cases = [
        ("local-a", UsageResult::Success),
        ("local-a", UsageResult::Failure),
        ("local-a", UsageResult::Cancelled),
        ("local-b", UsageResult::Cancelled),
        ("local-b", UsageResult::Failure),
        ("local-b", UsageResult::Failure),
    ];
    for (index, (model, result)) in cases.iter().enumerate() {
        store
            .append(
                &sample_record(
                    base + index as i64 * 1_000,
                    model,
                    "remote-a",
                    "p1",
                    "Provider One",
                    *result,
                    Some(0.1),
                    tokens(1, 0, 0, 1),
                ),
                365,
            )
            .unwrap();
    }

    // A cancelled status filter returns an empty, valid page (REQ-002 / AC-005).
    let cancelled = store
        .query_logs(
            &TimeRange::default(),
            &LogFilter {
                status: Some(UsageResult::Cancelled),
                model: None,
            },
            1,
        )
        .unwrap();
    assert_eq!(cancelled.total, 0, "cancelled excluded from user-visible queries");
    assert!(cancelled.records.is_empty());
    assert_eq!(cancelled.total_pages, 1, "an empty result has one page");

    let cancelled_b = store
        .query_logs(
            &TimeRange::default(),
            &LogFilter {
                status: Some(UsageResult::Cancelled),
                model: Some("local-b".to_string()),
            },
            1,
        )
        .unwrap();
    assert_eq!(cancelled_b.total, 0);
    assert!(cancelled_b.records.is_empty());

    let failed_b = store
        .query_logs(
            &TimeRange::default(),
            &LogFilter {
                status: Some(UsageResult::Failure),
                model: Some("local-b".to_string()),
            },
            1,
        )
        .unwrap();
    assert_eq!(failed_b.total, 2);

    let none = store
        .query_logs(
            &TimeRange::default(),
            &LogFilter {
                status: Some(UsageResult::Success),
                model: Some("local-b".to_string()),
            },
            1,
        )
        .unwrap();
    assert_eq!(none.total, 0);
    assert!(none.records.is_empty());
    assert_eq!(none.total_pages, 1);
    let _ = fs::remove_dir_all(&dir);
}

/// AC-013 / AC-018 / REQ-011 / REQ-015: day buckets split at UTC+8 midnight
/// (not UTC midnight), and a single-day range buckets by hour while returning
/// only hours that actually have data (never a 24-row blank grid).
#[test]
fn utc8_buckets_split_at_local_midnight_for_days_and_hours() {
    let (dir, store) = usage_store("usage-utc8-buckets");
    let before_midnight = rfc3339_millis("2026-09-16T23:30:00+08:00");
    let after_midnight = rfc3339_millis("2026-09-17T00:30:00+08:00");
    store
        .append(&sample_record(before_midnight, "local-a", "remote-a", "p1", "Provider One", UsageResult::Success, Some(0.1), tokens(7, 0, 0, 3)), 365)
        .unwrap();
    store
        .append(&sample_record(after_midnight, "local-a", "remote-a", "p1", "Provider One", UsageResult::Success, Some(0.1), tokens(1, 0, 0, 1)), 365)
        .unwrap();

    // The two rows are an hour apart in UTC but land in two UTC+8 natural days.
    let days = store.usage_stats(&TimeRange::default(), false).unwrap();
    assert_eq!(days.granularity, "day");
    assert_eq!(
        days.buckets
            .iter()
            .map(|bucket| bucket.label.as_str())
            .collect::<Vec<_>>(),
        vec!["2026-09-16", "2026-09-17"]
    );

    let single = store
        .usage_stats(&resolve_range(Some(1), before_midnight), true)
        .unwrap();
    assert_eq!(single.granularity, "hour");
    assert!(single.buckets.len() <= 24, "a day can never exceed 24 hour rows");
    assert_eq!(single.buckets.len(), 1, "only the 23:00 hour has data");
    assert_eq!(single.buckets[0].label, "23:00");
    assert_eq!(single.buckets[0].metrics.total_tokens, 10);
    let _ = fs::remove_dir_all(&dir);
}

/// AC-013 / REQ-011: the "近 30 天" quick range includes today plus the previous
/// 29 natural days anchored at UTC+8 midnight; non-positive selectors mean all.
#[test]
fn resolve_range_covers_30_days_inclusive_and_treats_nonpositive_as_all() {
    let now = rfc3339_millis("2026-09-17T00:30:00+08:00");
    let range = resolve_range(Some(30), now);
    assert_eq!(
        range.start_ms,
        Some(rfc3339_millis("2026-08-19T00:00:00+08:00")),
        "today plus 29 previous days"
    );
    assert_eq!(
        range.end_ms,
        Some(rfc3339_millis("2026-09-18T00:00:00+08:00"))
    );
    assert_eq!(resolve_range(Some(0), now), TimeRange::default());
    assert_eq!(resolve_range(Some(-1), now), TimeRange::default());
}

// ---------------------------------------------------------------------------
// 20260917-ai-gateway-usage-logs review-repair round (regression tests)
// ---------------------------------------------------------------------------

/// AC-003 / REQ-003 (repair F1): an upstream error response that nevertheless
/// carries a JSON `usage` object must be recorded as a failure with zero tokens
/// and zero cost in every tier. Error accounting never trusts upstream usage.
#[tokio::test]
async fn usage_log_zeroes_usage_for_error_response_with_usage_body() {
    let home = temp_home("usage-forward-error-usage-body");
    let port = free_port().await;
    let (upstream_url, _log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            400,
            json!({
                "error": {"message": "bad request"},
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 50,
                    "cache_read_input_tokens": 30,
                    "cache_creation_input_tokens": 20
                }
            }),
        )
    })
    .await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(provider);
    config.model_prices = vec![priced("remote-a", 1.0, 0.5, 2.0, 4.0)];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a"})),
    )
    .await;
    assert_eq!(status, 400, "caller must receive the upstream status: {text}");

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1, "exactly one log row per forwarded request");
    let record = &records[0];
    assert!(
        record.terminal,
        "a single ReturnToClient attempt is the request's terminal row"
    );
    assert_eq!(record.result, UsageResult::Failure);
    assert_eq!(record.status, 400);
    assert_eq!(
        record.error_message.as_deref(),
        Some("bad request"),
        "the upstream error.message is recorded"
    );
    assert_eq!(
        record.input_tokens, 0,
        "an error response must never contribute input tokens"
    );
    assert_eq!(record.cache_read_tokens, 0);
    assert_eq!(record.cache_write_tokens, 0);
    assert_eq!(
        record.output_tokens, 0,
        "an error response must never contribute output tokens"
    );
    assert_eq!(record.total_tokens, 0);
    assert_eq!(
        record.amount.unwrap_or(0.0),
        0.0,
        "an error response must never contribute cost"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-005 / AC-008 (repair F4): `unpriced_count` counts only requests that
/// reached an upstream model with no matching price row. Cancelled rows (and any
/// row with an empty `upstream_model`) must not be counted or shown as unpriced.
#[test]
fn usage_stats_unpriced_count_excludes_rows_without_upstream_model() {
    let (dir, store) = usage_store("usage-unpriced-upstream-model");
    let base = rfc3339_millis("2026-09-16T08:00:00+08:00");
    // Reached upstream `remote-unpriced` but no price row exists.
    store
        .append(
            &sample_record(
                base,
                "local-unpriced",
                "remote-unpriced",
                "p1",
                "Provider One",
                UsageResult::Success,
                None,
                tokens(10, 0, 0, 5),
            ),
            365,
        )
        .unwrap();
    // A cancelled request never reached an upstream model.
    store
        .append(
            &sample_record(
                base + 1_000,
                "local-cancelled",
                "",
                "p1",
                "Provider One",
                UsageResult::Cancelled,
                None,
                UsageTokens::default(),
            ),
            365,
        )
        .unwrap();

    let stats = store.usage_stats(&TimeRange::default(), false).unwrap();
    assert_eq!(
        stats.totals.request_count, 1,
        "only the request that reached an upstream model counts"
    );
    assert_eq!(
        stats.totals.unpriced_count, 1,
        "only the request that reached an unpriced upstream model is unpriced"
    );

    assert!(
        !stats
            .models
            .iter()
            .any(|row| row.local_model == "local-cancelled"),
        "a cancelled row with no upstream model is excluded from model statistics"
    );

    let unpriced = stats
        .models
        .iter()
        .find(|row| row.local_model == "local-unpriced")
        .expect("unpriced row");
    assert_eq!(unpriced.metrics.unpriced_count, 1);
    assert_eq!(stats.unpriced_items.len(), 1);
    assert_eq!(stats.unpriced_items[0].provider_id, "p1");
    assert_eq!(stats.unpriced_items[0].provider_name, "Provider One");
    assert_eq!(stats.unpriced_items[0].local_model, "local-unpriced");
    assert_eq!(stats.unpriced_items[0].upstream_model, "remote-unpriced");
    assert_eq!(stats.unpriced_items[0].count, 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn usage_stats_returns_unpriced_items_with_provider_and_models() {
    let (dir, store) = usage_store("usage-unpriced-items-breakdown");
    let base = rfc3339_millis("2026-09-16T08:00:00+08:00");

    // Provider A, Model A: 2 unpriced requests
    for i in 0..2 {
        store
            .append(
                &sample_record(
                    base + i * 1_000,
                    "gpt-4o",
                    "gpt-4o-2024-08-06",
                    "p-openai",
                    "OpenAI",
                    UsageResult::Success,
                    None,
                    tokens(10, 0, 0, 5),
                ),
                365,
            )
            .unwrap();
    }

    // Provider B, Model B: 3 unpriced requests
    for i in 0..3 {
        store
            .append(
                &sample_record(
                    base + 10_000 + i * 1_000,
                    "deepseek-chat",
                    "deepseek-v3",
                    "p-deepseek",
                    "DeepSeek",
                    UsageResult::Success,
                    None,
                    tokens(10, 0, 0, 5),
                ),
                365,
            )
            .unwrap();
    }

    // Provider A, Model A with priced request (amount = Some(0.005))
    store
        .append(
            &sample_record(
                base + 20_000,
                "gpt-4o",
                "gpt-4o-2024-08-06",
                "p-openai",
                "OpenAI",
                UsageResult::Success,
                Some(0.005),
                tokens(10, 0, 0, 5),
            ),
            365,
        )
        .unwrap();

    let stats = store.usage_stats(&TimeRange::default(), false).unwrap();
    assert_eq!(stats.totals.request_count, 6);
    assert_eq!(stats.totals.unpriced_count, 5);
    assert_eq!(stats.unpriced_items.len(), 2);

    // Sorted by count DESC: DeepSeek (3) first, then OpenAI (2)
    assert_eq!(stats.unpriced_items[0].provider_id, "p-deepseek");
    assert_eq!(stats.unpriced_items[0].provider_name, "DeepSeek");
    assert_eq!(stats.unpriced_items[0].local_model, "deepseek-chat");
    assert_eq!(stats.unpriced_items[0].upstream_model, "deepseek-v3");
    assert_eq!(stats.unpriced_items[0].count, 3);

    assert_eq!(stats.unpriced_items[1].provider_id, "p-openai");
    assert_eq!(stats.unpriced_items[1].provider_name, "OpenAI");
    assert_eq!(stats.unpriced_items[1].local_model, "gpt-4o");
    assert_eq!(stats.unpriced_items[1].upstream_model, "gpt-4o-2024-08-06");
    assert_eq!(stats.unpriced_items[1].count, 2);

    let _ = fs::remove_dir_all(&dir);
}

/// AC-021 / REQ-018 (repair F3): the ungrouped page response exposes a bounded,
/// distinct, non-empty in-range model facet that is independent of the current
/// page and of the model filter, so the frontend can offer every in-range model.
#[test]
fn request_logs_page_exposes_in_range_model_facet() {
    let (dir, store) = usage_store("usage-model-facet");
    let base = rfc3339_millis("2026-09-16T08:00:00+08:00");
    // Oldest row has no local model (e.g. a cancelled request).
    store
        .append(
            &sample_record(
                base - 1_000,
                "",
                "",
                "p1",
                "Provider One",
                UsageResult::Cancelled,
                None,
                UsageTokens::default(),
            ),
            365,
        )
        .unwrap();
    // Second-oldest row is the only record of `local-hidden`, beyond page 1.
    store
        .append(
            &sample_record(
                base,
                "local-hidden",
                "remote-hidden",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(0.1),
                tokens(1, 0, 0, 1),
            ),
            365,
        )
        .unwrap();
    // 51 newer rows of `local-visible` fill page 1 and push both rows off it.
    for index in 0..51i64 {
        store
            .append(
                &sample_record(
                    base + (index + 1) * 1_000,
                    "local-visible",
                    "remote-visible",
                    "p1",
                    "Provider One",
                    UsageResult::Success,
                    Some(0.1),
                    tokens(1, 0, 0, 1),
                ),
                365,
            )
            .unwrap();
    }

    let page_one = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .unwrap();
    assert_eq!(page_one.records.len(), USAGE_LOG_PAGE_SIZE as usize);
    assert!(
        page_one
            .records
            .iter()
            .all(|record| record.local_model == "local-visible"),
        "page 1 must not contain the hidden model"
    );

    let value = serde_json::to_value(&page_one).unwrap();
    let facet = value
        .get("models")
        .and_then(Value::as_array)
        .expect("UsageLogsPage must serialize a `models` facet");
    let mut names: Vec<String> = facet
        .iter()
        .filter_map(|entry| entry.as_str().map(str::to_string))
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["local-hidden".to_string(), "local-visible".to_string()],
        "facet must be distinct, non-empty and independent of the current page"
    );

    // The facet must ignore the model filter itself.
    let filtered = store
        .query_logs(
            &TimeRange::default(),
            &LogFilter {
                status: None,
                model: Some("local-visible".to_string()),
            },
            1,
        )
        .unwrap();
    let filtered_value = serde_json::to_value(&filtered).unwrap();
    let filtered_names: Vec<&str> = filtered_value
        .get("models")
        .and_then(Value::as_array)
        .expect("UsageLogsPage must serialize a `models` facet")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        filtered_names.contains(&"local-hidden"),
        "the model facet must ignore the active model filter, got {filtered_names:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Spec F4: deleting the current default key must fall through to the next
/// enabled key in list order instead of leaving a dangling default id.
#[test]
fn deleting_the_default_key_advances_to_the_next_enabled_key() {
    with_temp_home("delete-default-key-advance", |_home| {
        super::commands::api_gateway_upsert_key(GatewayKey {
            id: "k1".to_string(),
            label: "K1".to_string(),
            value: "v1".to_string(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        super::commands::api_gateway_upsert_key(GatewayKey {
            id: "k2".to_string(),
            label: "K2".to_string(),
            value: "v2".to_string(),
            enabled: true,
            created_at: 0,
        })
        .unwrap();
        let defaulted = super::commands::api_gateway_set_default_key("k2".to_string()).unwrap();
        assert_eq!(defaulted.default_key_id.as_deref(), Some("k2"));

        let after_delete = super::commands::api_gateway_delete_key("k2".to_string()).unwrap();
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

/// Standards S2: `api_gateway_save_config` must normalize brand-new keys whose
/// submitted value is blank or the UI mask placeholder, generating a real
/// `sk-gateway-` secret instead of persisting `""` or `"********"`.
#[tokio::test]
async fn save_config_generates_secret_for_new_keys_with_blank_or_masked_value() {
    let _home = temp_home("save-config-key-normalize");

    let mut config = GatewayConfig::default();
    // Keep the listener off so the test never binds a real port.
    config.enabled = false;
    // Providers intentionally empty: only key normalization is under test.
    config.keys.push(GatewayKey {
        id: "brand-new-blank".to_string(),
        label: "Brand New Blank".to_string(),
        value: String::new(),
        enabled: true,
        created_at: 1,
    });
    config.keys.push(GatewayKey {
        id: "brand-new-masked".to_string(),
        label: "Brand New Masked".to_string(),
        value: "********".to_string(),
        enabled: true,
        created_at: 2,
    });

    let saved = super::commands::api_gateway_save_config(config)
        .await
        .expect("save config");

    let blank = saved
        .keys
        .iter()
        .find(|key| key.id == "brand-new-blank")
        .expect("blank-valued key must be persisted");
    assert!(
        blank.value.starts_with("sk-gateway-"),
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
        masked.value.starts_with("sk-gateway-"),
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

// ---------------------------------------------------------------------------
// 20260918-gateway-retry-and-openai-errors Step 3: upstream 4xx normalization
// and the mid-stream error fragment (REQ-004/REQ-005, AC-006..AC-009)
// ---------------------------------------------------------------------------

/// Parse every `\n\n`-delimited SSE event in a relay body and return the JSON
/// payload of its `data:` line(s). Empty keep-alives and `[DONE]` are skipped.
/// An event boundary is required, so a fragment appended without a blank-line
/// separator stays invisible to this parser.
fn parse_sse_events(body: &str) -> Vec<Value> {
    let mut events = Vec::new();
    for segment in body.split("\n\n") {
        let payload = segment
            .lines()
            .filter_map(|line| line.trim_end_matches('\r').strip_prefix("data:"))
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n");
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&payload) {
            events.push(value);
        }
    }
    events
}

/// The independently parseable SSE error fragments (AC-008). A conforming
/// mid-stream failure emits exactly one, carrying a non-empty `error.message`.
fn sse_error_events(body: &str) -> Vec<Value> {
    parse_sse_events(body)
        .into_iter()
        .filter(|event| event.get("error").map(|value| value.is_object()).unwrap_or(false))
        .collect()
}

/// AC-006/REQ-004: a non-streaming upstream 4xx whose body is not valid JSON
/// keeps the upstream status but is wrapped in the standard gateway envelope
/// whose message names the status and readable upstream text, and never a
/// credential or request header.
#[tokio::test]
async fn non_streaming_upstream_html_400_is_wrapped_in_standard_envelope() {
    let _home = temp_home("step3-non-stream-html-400");
    let html = b"<html><body>upstream rejected the payload</body></html>".to_vec();
    let (upstream_url, upstream_log) =
        spawn_mock_upstream(move |_| MockReply::Raw(400, "text/html", html.clone())).await;
    let provider = upstream_provider(
        "a",
        "Provider A",
        &upstream_url,
        "sk-upstream-secret",
        Some("remote-default"),
    );
    let mut config = GatewayConfig::default();
    config.providers.push(provider.clone());
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();
    let headers = HashMap::from([
        ("authorization".to_string(), "Bearer local-secret".to_string()),
        ("x-request-marker".to_string(), "header-secret".to_string()),
    ]);

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        std::slice::from_ref(&provider),
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &headers,
        &mut attempts,
    )
    .await;

    assert_eq!(response.status, 400, "the upstream status must be kept");
    assert_eq!(
        upstream_log.lock().unwrap().len(),
        1,
        "a single candidate is attempted exactly once"
    );
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].provider_name, "Provider A");
    assert_eq!(attempts[0].upstream_model, "remote-default");
    assert_eq!(attempts[0].status, 400);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    let attempt_message = attempts[0]
        .error_message
        .as_deref()
        .expect("an HTML failure body yields readable text");
    assert_eq!(
        attempt_message, "upstream rejected the payload",
        "markup is removed and whitespace collapsed"
    );
    for secret in ["sk-upstream-secret", "local-secret", "header-secret"] {
        assert!(
            !attempt_message.contains(secret),
            "the recorded attempt message must not leak {secret}: {attempt_message}"
        );
    }
    assert!(attempts[0].usage.is_none());
    assert!(attempts[0].duration_ms >= 1);
    let text = String::from_utf8_lossy(&response.body).into_owned();
    let envelope = assert_standard_error_envelope(&text);
    let message = envelope["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("400"),
        "the envelope message must name the upstream status: {message}"
    );
    assert!(
        message.contains("upstream rejected the payload"),
        "the envelope message must carry readable upstream information: {message}"
    );
    for secret in ["sk-upstream-secret", "local-secret", "header-secret"] {
        assert!(
            !message.contains(secret),
            "the envelope message must not leak {secret}: {message}"
        );
    }
}

/// AC-006/REQ-004: the streaming branch wraps a non-standard upstream 4xx body
/// the same way while keeping the upstream status line.
#[tokio::test]
async fn streaming_upstream_html_400_is_wrapped_in_standard_envelope() {
    let _home = temp_home("step3-stream-html-400");
    let html = b"<html><body>upstream rejected the payload</body></html>".to_vec();
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::Raw(400, "text/html", html.clone())).await;
    let provider = upstream_provider(
        "a",
        "Provider A",
        &upstream_url,
        "sk-upstream-secret",
        Some("remote-default"),
    );
    let mut config = GatewayConfig::default();
    config.providers.push(provider.clone());

    let text = attempt_streaming_text(std::slice::from_ref(&provider), &mut config).await;
    let (status_line, body) = raw_http_status_and_body(&text);
    assert_eq!(status_line, "HTTP/1.1 400 Bad Request", "response: {text}");
    let envelope = assert_standard_error_envelope(&body);
    let message = envelope["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("400"),
        "the envelope message must name the upstream status: {message}"
    );
    assert!(
        message.contains("upstream rejected the payload"),
        "the envelope message must carry readable upstream information: {message}"
    );
    assert!(
        !message.contains("sk-upstream-secret"),
        "the envelope message must not leak the upstream key: {message}"
    );
}

/// AC-007/REQ-004: a non-streaming upstream 4xx whose body is valid JSON with an
/// `error` object must be passed through byte-for-byte, never re-wrapped.
#[tokio::test]
async fn non_streaming_upstream_json_400_is_passed_through_byte_for_byte() {
    let _home = temp_home("step3-non-stream-json-400");
    let upstream_body = json!({
        "error": {
            "message": "bad request",
            "type": "invalid_request_error",
            "code": "bad_request",
            "param": "model",
        }
    });
    let expected = serde_json::to_vec(&upstream_body).unwrap();
    let for_mock = upstream_body.clone();
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::Json(400, for_mock.clone())).await;
    let provider = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers.push(provider.clone());
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        std::slice::from_ref(&provider),
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;

    assert_eq!(response.status, 400, "the upstream status must be kept");
    assert_eq!(
        response.body, expected,
        "a standard upstream error body must pass through byte-for-byte"
    );
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].status, 400);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(
        attempts[0].error_message.as_deref(),
        Some("bad request"),
        "the standard upstream error.message is extracted"
    );
    assert!(attempts[0].duration_ms >= 1);
}

/// AC-007/REQ-004: the streaming branch passes a standard upstream 4xx JSON body
/// through byte-for-byte after the header block.
#[tokio::test]
async fn streaming_upstream_json_400_is_passed_through_byte_for_byte() {
    let _home = temp_home("step3-stream-json-400");
    let upstream_body = json!({
        "error": {
            "message": "bad request",
            "type": "invalid_request_error",
            "code": "bad_request",
            "param": "model",
        }
    });
    let expected = serde_json::to_vec(&upstream_body).unwrap();
    let for_mock = upstream_body.clone();
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::Json(400, for_mock.clone())).await;
    let provider = upstream_provider("a", "Provider A", &upstream_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers.push(provider.clone());

    let text = attempt_streaming_text(std::slice::from_ref(&provider), &mut config).await;
    let (status_line, body) = raw_http_status_and_body(&text);
    assert_eq!(status_line, "HTTP/1.1 400 Bad Request", "response: {text}");
    assert_eq!(
        body.as_bytes(),
        expected.as_slice(),
        "a standard upstream error body must pass through byte-for-byte"
    );
}

/// AC-008/REQ-005: after the first byte has been forwarded, a mid-stream upstream
/// read failure must complete the event boundary and append exactly one
/// standalone parseable error fragment, and must never send `[DONE]`.
#[tokio::test]
async fn mid_stream_failure_appends_standalone_error_fragment_without_done() {
    let _home = temp_home("step3-mid-stream-fragment");
    // Deliberately no trailing newline: the last forwarded byte is not a newline.
    let partial = "data: {\"choices\":[{\"delta\":{\"content\":\"partial-a\"}}]}".to_string();
    let declared = partial.len() + 500;
    let (partial_url, partial_log) =
        spawn_mock_upstream(move |_| MockReply::PartialStream(partial.clone(), declared)).await;
    let provider = upstream_provider("a", "Provider A", &partial_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers.push(provider.clone());

    let text = attempt_streaming_text(std::slice::from_ref(&provider), &mut config).await;
    let (status_line, body) = raw_http_status_and_body(&text);
    assert!(status_line.starts_with("HTTP/1.1 200"), "response: {text}");
    assert!(
        body.contains("partial-a"),
        "the forwarded bytes must reach the caller: {text}"
    );
    assert!(
        !body.contains("[DONE]"),
        "an abnormal stream must not send [DONE]: {text}"
    );
    let errors = sse_error_events(&body);
    assert_eq!(
        errors.len(),
        1,
        "exactly one standalone parseable error fragment is required: {text}"
    );
    let message = errors[0]["error"]["message"].as_str().unwrap_or("");
    assert!(
        !message.is_empty(),
        "the error fragment must carry a non-empty message: {text}"
    );
    assert_eq!(partial_log.lock().unwrap().len(), 1);
}

/// AC-009/REQ-005: a mid-stream failure returns a 502 `ForwardCapture` flagged
/// as an upstream error, keeps the accumulated usage, and never contacts another
/// candidate; the bytes on the wire still close with one error fragment and no
/// `[DONE]`.
#[tokio::test]
async fn mid_stream_failure_capture_is_failure_keeps_usage_and_skips_other_candidates() {
    let _home = temp_home("step3-mid-stream-capture");
    // A usage-bearing event followed by an unterminated partial event.
    let partial = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"partial-a\"}}],",
        "\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3,",
        "\"prompt_tokens_details\":{\"cached_tokens\":2}}}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"tail\"}}]}",
    )
    .to_string();
    let declared = partial.len() + 500;
    let (partial_url, partial_log) =
        spawn_mock_upstream(move |_| MockReply::PartialStream(partial.clone(), declared)).await;
    let (fallback_url, fallback_log) = spawn_mock_upstream(|_| {
        MockReply::Stream(
            "data: {\"choices\":[{\"delta\":{\"content\":\"from-b\"}}]}\n\ndata: [DONE]\n\n"
                .to_string(),
        )
    })
    .await;

    let a = upstream_provider("a", "Provider A", &partial_url, "sk", Some("remote-default"));
    let b = upstream_provider("b", "Provider B", &fallback_url, "sk", Some("remote-default"));
    let mut config = GatewayConfig::default();
    config.providers = vec![a.clone(), b.clone()];
    let body = serde_json::to_vec(&json!({"model": "local", "stream": true})).unwrap();

    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    let mut attempts = Vec::new();
    let capture = super::runtime_http::attempt_streaming(
        &mut server,
        &[a, b],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await
    .expect("streaming attempt");
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.expect("read relay stream");
    let text = String::from_utf8_lossy(&out).into_owned();

    assert_eq!(capture.status, 502, "a mid-stream failure is recorded as 502");
    assert!(
        capture.upstream_error,
        "the capture must flag the upstream stream error"
    );
    assert_eq!(
        capture.usage,
        Some(tokens(7, 2, 0, 3)),
        "the accumulated usage must survive the mid-stream failure"
    );
    assert_eq!(
        attempts.len(),
        1,
        "the candidate that failed mid-stream is the request's only completed attempt"
    );
    assert_eq!(attempts[0].provider_id, "a");
    assert_eq!(attempts[0].upstream_model, "remote-default");
    assert_eq!(attempts[0].status, 502);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert!(
        attempts[0]
            .error_message
            .as_deref()
            .unwrap_or("")
            .contains("stream failed after first byte"),
        "the attempt records the stream failure description: {:?}",
        attempts[0].error_message
    );
    assert_eq!(
        attempts[0].usage,
        Some(tokens(7, 2, 0, 3)),
        "the attempt row keeps the usage accumulated before the failure"
    );
    assert!(attempts[0].duration_ms >= 1);
    let (_, body_text) = raw_http_status_and_body(&text);
    assert_eq!(
        sse_error_events(&body_text).len(),
        1,
        "the stream must close with one error fragment: {text}"
    );
    assert!(
        !body_text.contains("[DONE]"),
        "an abnormal stream must not send [DONE]: {text}"
    );
    assert_eq!(partial_log.lock().unwrap().len(), 1);
    assert!(
        fallback_log.lock().unwrap().is_empty(),
        "must not switch candidates after the first byte"
    );
}

/// AC-009/REQ-005 end-to-end: through the real listener a mid-stream failure
/// yields one error fragment on the wire and exactly one `failure` log row with
/// status 502 that keeps the accumulated usage.
#[tokio::test]
async fn mid_stream_failure_end_to_end_logs_one_failure_with_usage() {
    let home = temp_home("step3-mid-stream-e2e");
    let port = free_port().await;
    let partial = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"partial-a\"}}],",
        "\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3,",
        "\"prompt_tokens_details\":{\"cached_tokens\":2}}}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"tail\"}}]}",
    )
    .to_string();
    let declared = partial.len() + 500;
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::PartialStream(partial.clone(), declared)).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    let mut a = upstream_provider("a", "Provider A", &upstream_url, "sk", None);
    a.mappings = vec![mapping("local-a", "remote-a", None)];
    config.providers.push(a);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-a", "stream": true})),
    )
    .await;
    assert_eq!(status, 200, "the SSE status is committed before the failure: {text}");
    assert!(
        text.contains("partial-a"),
        "the partial bytes must reach the caller: {text}"
    );
    assert!(
        !text.contains("[DONE]"),
        "an abnormal stream must not send [DONE]: {text}"
    );
    assert_eq!(
        sse_error_events(&text).len(),
        1,
        "exactly one parseable error fragment is required: {text}"
    );

    let records = wait_for_usage_logs(1).await;
    assert_eq!(records.len(), 1, "exactly one failure row for the request");
    let record = &records[0];
    assert!(
        record.terminal,
        "the mid-stream failure is the request's terminal row"
    );
    assert_eq!(record.result, UsageResult::Failure);
    assert_eq!(record.status, 502);
    assert_eq!(record.provider_id, "a");
    assert_eq!(record.local_model, "local-a");
    assert_eq!(record.upstream_model, "remote-a");
    let message = record
        .error_message
        .as_deref()
        .expect("a mid-stream failure records its stream description");
    assert!(
        message.contains("stream failed after first byte"),
        "the stored error text is the stream failure description: {message}"
    );
    assert!(record.duration_ms >= 1);
    assert_eq!(record.input_tokens, 7, "accumulated input tokens must be kept");
    assert_eq!(
        record.cache_read_tokens, 2,
        "accumulated cache-read tokens must be kept"
    );
    assert_eq!(record.output_tokens, 3, "accumulated output tokens must be kept");
    // The row sums all four usage tiers (7 + 2 + 0 + 3).
    assert_eq!(record.total_tokens, 12, "all four usage tiers are summed");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

// ---------------------------------------------------------------------------
// Plan 20260918-gateway-per-model-mapping-disable, Step 1 (RED)
// Per-mapping `enabled`: default, persistence, routing exclusion and
// provider-toggle independence.
// ---------------------------------------------------------------------------

/// AC-001/REQ-001: a mapping without an `enabled` field loads as enabled, the
/// field is always serialized, and a disabled value survives write/read.
#[test]
fn mapping_enabled_defaults_true_for_older_config_and_survives_round_trip() {
    with_temp_home("mapping-enabled-persist", |_home| {
        let mut config: GatewayConfig = serde_json::from_value(json_config_with_key(
            17688,
            vec![json_provider(
                "p1",
                "Provider One",
                "https://api.example.com/v1",
                "chat_completions",
                None,
                vec![
                    json_mapping("m1-local", "m1-remote", Some("chat_completions")),
                    json_mapping("m2-local", "m2-remote", Some("chat_completions")),
                ],
            )],
        ))
        .expect("an older config without mapping.enabled must deserialize");

        assert!(
            config.providers[0].mappings.iter().all(|row| row.enabled),
            "every older mapping without an enabled field must default to enabled"
        );

        let serialized = serde_json::to_value(&config).expect("serialize config");
        let rows = serialized["providers"][0]["mappings"]
            .as_array()
            .expect("mappings must be an array");
        for (index, row) in rows.iter().enumerate() {
            assert!(
                row.get("enabled").is_some(),
                "serialized mapping {index} must include the enabled field: {serialized}"
            );
            assert_eq!(
                row["enabled"], true,
                "an older mapping must serialize as enabled: {serialized}"
            );
        }

        config.providers[0].mappings[1].enabled = false;
        super::storage::write_config(&config).expect("write config");
        let reloaded = super::storage::read_config().expect("read config");
        assert!(
            reloaded.providers[0].mappings[0].enabled,
            "an enabled mapping must stay enabled across write/read"
        );
        assert!(
            !reloaded.providers[0].mappings[1].enabled,
            "a disabled mapping must stay disabled across write/read"
        );

        let serialized = serde_json::to_value(&reloaded).expect("encode reloaded config");
        assert_eq!(
            serialized["providers"][0]["mappings"][1]["enabled"], false,
            "the reloaded disabled mapping must serialize as false: {serialized}"
        );
    });
}

/// AC-002/REQ-002: a disabled mapping is never served, blocks the provider's
/// `default_model` fallback for its local model, and drops the provider from the
/// candidate set; enabled and unmapped models keep their existing behavior.
#[test]
fn disabled_mapping_is_excluded_and_blocks_default_fallback() {
    let mut disabled_m2 = mapping("m2-local", "m2-remote", None);
    disabled_m2.enabled = false;
    let mut p = provider("p1");
    p.default_model = Some("d".to_string());
    p.mappings = vec![mapping("m1-local", "m1-remote", None), disabled_m2];

    let m2 = resolve_model_for_protocol(&p, Some("m2-local"), UpstreamProtocol::ChatCompletions);
    assert!(
        matches!(m2, ModelResolution::NoMatch),
        "a request that only matches a disabled mapping must be NoMatch, never a default fallback; got {m2:?}"
    );

    let m1 = resolve_model_for_protocol(&p, Some("m1-local"), UpstreamProtocol::ChatCompletions);
    assert!(
        matches!(m1, ModelResolution::Serve(ref model) if model.as_str() == "m1-remote"),
        "an enabled mapping must be served with its own upstream model; got {m1:?}"
    );

    let fallback =
        resolve_model_for_protocol(&p, Some("unmapped"), UpstreamProtocol::ChatCompletions);
    assert!(
        matches!(fallback, ModelResolution::Serve(ref model) if model.as_str() == "d"),
        "a model that matches no mapping must still fall back to default_model; got {fallback:?}"
    );

    let providers = [p];
    let candidates = candidate_providers(&providers, Some("m2-local"), UpstreamProtocol::ChatCompletions);
    assert!(
        candidates.is_empty(),
        "a provider whose only match is disabled must not be a candidate: {candidates:?}"
    );
}

/// AC-002/REQ-002 (HTTP): requesting a disabled mapping returns the standard 502
/// `all_providers_unavailable` error without contacting upstream, while enabled
/// and unmapped models still reach upstream through the same process.
#[tokio::test]
async fn disabled_mapping_request_returns_all_providers_unavailable() {
    let home = temp_home("mapping-disabled-http");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let config: GatewayConfig = serde_json::from_value(json_config_with_key(
        port,
        vec![json_provider(
            "p1",
            "Provider One",
            &upstream_url,
            "chat_completions",
            Some("d"),
            vec![
                json_mapping("m1-local", "m1-remote", None),
                json!({"local_model": "m2-local", "upstream_model": "m2-remote", "enabled": false}),
            ],
        )],
    ))
    .expect("decode config containing a disabled mapping");
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "m2-local", "messages": []})),
    )
    .await;
    assert_eq!(
        status, 502,
        "a request whose only match is disabled must fail with 502: {text}"
    );
    let body = assert_standard_error_envelope(&text);
    assert_eq!(
        body["error"]["code"], "all_providers_unavailable",
        "a disabled mapping must produce all_providers_unavailable: {text}"
    );
    let captured = log.lock().unwrap().clone();
    assert!(
        captured.is_empty(),
        "the disabled mapping must never reach upstream: {}",
        captured_summary(&captured)
    );

    let (m1_status, _content_type, m1_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "m1-local", "messages": []})),
    )
    .await;
    assert_eq!(
        m1_status, 200,
        "the enabled mapping must still be served: {m1_text}"
    );

    let (fallback_status, _content_type, fallback_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "unmapped-local", "messages": []})),
    )
    .await;
    assert_eq!(
        fallback_status, 200,
        "a model matching no mapping must still use default_model: {fallback_text}"
    );

    let captured = log.lock().unwrap().clone();
    assert_eq!(
        captured.len(),
        2,
        "only the enabled mapping and the unmapped fallback may reach upstream: {}",
        captured_summary(&captured)
    );
    let m1_sent: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(
        m1_sent["model"], "m1-remote",
        "the enabled mapping must forward its own upstream model: {}",
        captured_summary(&captured)
    );
    let fallback_sent: Value = serde_json::from_slice(&captured[1].body).unwrap();
    assert_eq!(
        fallback_sent["model"], "d",
        "an unmapped model must forward the provider default model: {}",
        captured_summary(&captured)
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-002/REQ-002 (HTTP): a disabled mapping on provider A must not shadow the
/// same local model on another provider. When A's only match is disabled but B
/// has an enabled mapping for that local model, the request must be served by B:
/// A is never contacted and never falls back to its `default_model`, while B's
/// key and mapped upstream model are used.
#[tokio::test]
async fn disabled_mapping_on_one_provider_is_still_served_by_another_provider() {
    let home = temp_home("mapping-disabled-on-one-provider");
    let port = free_port().await;
    let (upstream_url, log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok"}))).await;

    let mut disabled = mapping("local-x", "a-remote", None);
    disabled.enabled = false;
    let mut provider_a = upstream_provider(
        "a",
        "Provider A",
        &upstream_url,
        "sk-a",
        Some("a-default"),
    );
    provider_a.mappings = vec![disabled];

    let mut provider_b = upstream_provider(
        "b",
        "Provider B",
        &upstream_url,
        "sk-b",
        None,
    );
    provider_b.mappings = vec![mapping("local-x", "b-remote", None)];

    let mut config = config_with_key(port);
    config.providers.push(provider_a);
    config.providers.push(provider_b);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-x", "messages": []})),
    )
    .await;
    assert_eq!(
        status, 200,
        "provider B's enabled mapping must still serve the local model: {text}"
    );

    let captured = log.lock().unwrap().clone();
    assert_eq!(
        captured.len(),
        1,
        "exactly provider B may be contacted; A's disabled mapping must not be: {}",
        captured_summary(&captured)
    );
    assert_eq!(
        captured[0].headers.get("authorization").map(String::as_str),
        Some("Bearer sk-b"),
        "the request must be served by provider B, not A: {}",
        captured_summary(&captured)
    );
    let sent: Value = serde_json::from_slice(&captured[0].body).unwrap_or_else(|error| {
        panic!(
            "upstream body must be JSON ({error}): {}",
            captured_summary(&captured)
        )
    });
    assert_eq!(
        sent["model"], "b-remote",
        "provider B's mapping must set the upstream model, never A's default: {}",
        captured_summary(&captured)
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-003/REQ-003: toggling the provider (user disable/re-enable) and clearing
/// an auto-disable never rewrite any mapping's `enabled`, including across a
/// config round trip.
#[test]
fn provider_disable_and_reenable_preserves_mapping_enabled_state() {
    with_temp_home("mapping-provider-toggle", |_home| {
        let mut disabled_b = mapping("b-local", "b-remote", None);
        disabled_b.enabled = false;
        let mut p = provider("p1");
        p.mappings = vec![mapping("a-local", "a-remote", None), disabled_b];

        set_user_enabled(&mut p, false);
        assert!(!p.enabled, "user disable must clear the provider intent flag");
        assert!(
            p.mappings[0].enabled,
            "provider disable must not touch mapping A"
        );
        assert!(
            !p.mappings[1].enabled,
            "provider disable must not touch mapping B"
        );

        set_user_enabled(&mut p, true);
        assert!(p.enabled, "user re-enable must set the provider intent flag");
        assert!(
            p.mappings[0].enabled,
            "provider re-enable must not touch mapping A"
        );
        assert!(
            !p.mappings[1].enabled,
            "provider re-enable must not touch mapping B"
        );

        register_failure(&mut p, FailureClass::DisableImmediately, "auth", 1);
        assert!(p.auto_disabled, "an auth failure must auto-disable the provider");
        manual_reenable(&mut p);
        assert!(!p.auto_disabled, "manual re-enable must clear auto-disabled");
        assert!(
            p.mappings[0].enabled,
            "manual re-enable must not touch mapping A"
        );
        assert!(
            !p.mappings[1].enabled,
            "manual re-enable must not touch mapping B"
        );

        let mut config = GatewayConfig::default();
        config.providers.push(p);
        super::storage::write_config(&config).expect("write config");
        let reloaded = super::storage::read_config().expect("read config");
        assert!(
            reloaded.providers[0].mappings[0].enabled,
            "mapping A must stay enabled after the provider toggle round trip"
        );
        assert!(
            !reloaded.providers[0].mappings[1].enabled,
            "mapping B must stay disabled after the provider toggle round trip"
        );
    });
}

/// Legacy cleanup removes only the `api_fusion`-era files while coexisting
/// new `api_gateway` files stay byte-identical.
#[test]
fn cleanup_legacy_files_removes_only_legacy_files() {
    let _home = isolated_temp_home("legacy-cleanup");
    let dir = crate::config::get_app_dir().expect("app dir");
    fs::create_dir_all(&dir).expect("create app dir");
    fs::write(dir.join(super::CONFIG_FILE), b"new-config").expect("write new config");
    fs::write(dir.join(super::USAGE_DB_FILE), b"new-db").expect("write new db");
    fs::write(
        dir.join(super::LEGACY_CONFIG_FILE_NAME),
        b"legacy-config",
    )
    .expect("write legacy config");
    fs::write(
        dir.join(super::LEGACY_USAGE_DB_FILE_NAME),
        b"legacy-db",
    )
    .expect("write legacy db");

    super::storage::cleanup_legacy_files();

    assert!(
        !dir.join(super::LEGACY_CONFIG_FILE_NAME).exists(),
        "legacy config must be gone"
    );
    assert!(
        !dir.join(super::LEGACY_USAGE_DB_FILE_NAME).exists(),
        "legacy usage db must be gone"
    );
    assert_eq!(
        fs::read(dir.join(super::CONFIG_FILE)).expect("read new config"),
        b"new-config",
        "new config must stay intact"
    );
    assert_eq!(
        fs::read(dir.join(super::USAGE_DB_FILE)).expect("read new db"),
        b"new-db",
        "new usage db must stay intact"
    );
}

/// Legacy cleanup with no legacy files present succeeds silently and is idempotent.
#[test]
fn cleanup_legacy_files_succeeds_without_legacy_files() {
    let _home = isolated_temp_home("legacy-cleanup-absent");
    let dir = crate::config::get_app_dir().expect("app dir");
    fs::create_dir_all(&dir).expect("create app dir");

    super::storage::cleanup_legacy_files();
    super::storage::cleanup_legacy_files();
}

#[test]
fn query_model_reasoning_efforts_matches_families_and_ignores_prefixes() {
    use super::storage::query_model_reasoning_efforts;

    // GPT-5.6 / GPT-5.5
    assert_eq!(
        query_model_reasoning_efforts("gpt-5.6-luna"),
        vec!["low", "medium", "high", "xhigh", "max"]
    );
    assert_eq!(
        query_model_reasoning_efforts("gpt-5.5"),
        vec!["low", "medium", "high", "xhigh", "max"]
    );

    // GPT-5.4 / GPT-5.3
    assert_eq!(
        query_model_reasoning_efforts("gpt-5.4-mini"),
        vec!["low", "medium", "high", "xhigh"]
    );
    assert_eq!(
        query_model_reasoning_efforts("gpt-5.3-codex"),
        vec!["low", "medium", "high", "xhigh"]
    );

    // DeepSeek V4
    assert_eq!(
        query_model_reasoning_efforts("deepseek/deepseek-v4-flash"),
        vec!["low", "high", "max"]
    );
    assert_eq!(
        query_model_reasoning_efforts("deepseek-v4-flash-free"),
        vec!["low", "high", "max"]
    );

    // Gemini
    assert_eq!(
        query_model_reasoning_efforts("google/gemini-3.7-flash"),
        vec!["low", "medium", "high"]
    );

    // Kimi
    assert_eq!(
        query_model_reasoning_efforts("moonshotai/Kimi-K3"),
        vec!["low", "high", "max"]
    );
    assert_eq!(
        query_model_reasoning_efforts("kimi-k2.7-code"),
        vec!["low", "high", "max"]
    );

    // GLM
    assert_eq!(
        query_model_reasoning_efforts("zai-org/GLM-5.3"),
        vec!["low", "high", "max"]
    );
    assert_eq!(
        query_model_reasoning_efforts("glm-5.2"),
        vec!["low", "medium", "high", "xhigh", "max"]
    );

    // Qwen Max
    assert_eq!(
        query_model_reasoning_efforts("Qwen/Qwen3.8-Max"),
        vec!["low", "medium", "xhigh"]
    );

    // Non-reasoning models
    assert!(query_model_reasoning_efforts("mimo-v2.5").is_empty());
    assert!(query_model_reasoning_efforts("hy3").is_empty());
    assert!(query_model_reasoning_efforts("big-pickle").is_empty());
}

#[test]
fn normalize_template_prices_and_efforts_populates_prices_and_efforts_and_is_idempotent() {
    use super::types_config::{ProviderTemplate, ProviderTemplateModel, ProviderTemplateState};
    let mut config = GatewayConfig::default();

    // 1. Setup a provider
    let mut p = provider("p1");
    p.template_id = Some("tpl-test".to_string());
    p.mappings = vec![
        ModelMapping {
            local_model: "my-ds".to_string(),
            upstream_model: "deepseek-v4-flash".to_string(),
            enabled: true,
            protocol: None,
            display_name: None,
            reasoning_efforts: Vec::new(),
        },
        ModelMapping {
            local_model: "my-plain".to_string(),
            upstream_model: "plain-model".to_string(),
            enabled: true,
            protocol: None,
            display_name: None,
            reasoning_efforts: Vec::new(),
        },
    ];
    config.providers.push(p);

    // 2. Setup model prices for p1
    config.model_prices = vec![
        priced_with_provider("p1", "deepseek-v4-flash", 0.3, 0.006, 0.0, 1.2),
        priced_with_provider("p1", "plain-model", 0.1, 0.0, 0.0, 0.2),
    ];

    // 3. Setup a provider template
    let template = ProviderTemplate {
        id: "tpl-test".to_string(),
        name: "Test Template".to_string(),
        description: "Testing".to_string(),
        base_url: "https://test.api".to_string(),
        protocol: UpstreamProtocol::ChatCompletions,
        source: "https://test.api/models".to_string(),
        models_url: Some("https://test.api/models".to_string()),
        models: vec![
            ProviderTemplateModel {
                upstream_model: "deepseek-v4-flash".to_string(),
                local_model: None,
                display_name: None,
                protocol: None,
                enabled: true,
                input: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                output: 0.0,
                off_peaks: Vec::new(),
                reasoning_efforts: Vec::new(),
            },
            ProviderTemplateModel {
                upstream_model: "gpt-5.6-luna".to_string(),
                local_model: None,
                display_name: None,
                protocol: None,
                enabled: true,
                input: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                output: 0.0,
                off_peaks: Vec::new(),
                reasoning_efforts: Vec::new(),
            },
            ProviderTemplateModel {
                upstream_model: "plain-model".to_string(),
                local_model: None,
                display_name: None,
                protocol: None,
                enabled: true,
                input: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                output: 0.0,
                off_peaks: Vec::new(),
                reasoning_efforts: Vec::new(),
            },
        ],
        icon: None,
    };

    config.provider_templates.push(ProviderTemplateState {
        template_id: "tpl-test".to_string(),
        template: Some(template),
        synced_at: Some(100),
        source: None,
    });

    // Run normalize_config
    super::storage::normalize_config(&mut config);

    // Verify provider mappings reasoning efforts populated
    let prov = &config.providers[0];
    let ds_mapping = prov.mappings.iter().find(|m| m.upstream_model == "deepseek-v4-flash").unwrap();
    assert_eq!(ds_mapping.reasoning_efforts, vec!["low", "high", "max"]);
    let plain_mapping = prov.mappings.iter().find(|m| m.upstream_model == "plain-model").unwrap();
    assert!(plain_mapping.reasoning_efforts.is_empty());

    // Verify template models
    let tpl_state = &config.provider_templates[0];
    let tpl = tpl_state.template.as_ref().unwrap();

    // 1) deepseek-v4-flash in template:
    let tpl_ds = tpl.models.iter().find(|m| m.upstream_model == "deepseek-v4-flash").unwrap();
    assert_eq!(tpl_ds.input, 0.3);
    assert_eq!(tpl_ds.cache_read, 0.006);
    assert_eq!(tpl_ds.output, 1.2);
    assert_eq!(tpl_ds.local_model.as_deref(), Some("my-ds"));
    assert_eq!(tpl_ds.reasoning_efforts, vec!["low", "high", "max"]);

    // 2) gpt-5.6-luna in template (not in provider prices, but gets real reasoning efforts):
    let tpl_luna = tpl.models.iter().find(|m| m.upstream_model == "gpt-5.6-luna").unwrap();
    assert_eq!(tpl_luna.input, 0.0);
    assert_eq!(tpl_luna.reasoning_efforts, vec!["low", "medium", "high", "xhigh", "max"]);

    // 3) plain-model in template (gets price from provider, empty reasoning efforts):
    let tpl_plain = tpl.models.iter().find(|m| m.upstream_model == "plain-model").unwrap();
    assert_eq!(tpl_plain.input, 0.1);
    assert_eq!(tpl_plain.output, 0.2);
    assert_eq!(tpl_plain.local_model.as_deref(), Some("my-plain"));
    assert!(tpl_plain.reasoning_efforts.is_empty());

    // Verify idempotency
    let before_tpl = config.provider_templates.clone();
    let before_prices = config.model_prices.clone();
    let before_mappings = config.providers[0].mappings.clone();
    super::storage::normalize_config(&mut config);
    assert_eq!(config.provider_templates, before_tpl, "templates must be unchanged on second normalize");
    assert_eq!(config.model_prices, before_prices, "prices must be unchanged on second normalize");
    assert_eq!(config.providers[0].mappings, before_mappings, "mappings must be unchanged on second normalize");
}

// ---------------------------------------------------------------------------
// 20260920-gateway-log-attempts-and-upstream-errors Step 1 (RED): per-attempt
// rows, terminal-only statistics, error-text extraction/sanitization/bounding,
// the request-log record fields and the idempotent log-database migration
// (REQ-002..REQ-006; AC-010, AC-011, AC-013).
// ---------------------------------------------------------------------------

/// AC-013 / REQ-002 / REQ-005: the statistics, buckets, per-model and
/// per-provider breakdowns and the grouped counters count terminal rows only,
/// while the ungrouped page, its total and its model facet cover attempt rows.
#[test]
fn usage_stats_and_groups_count_only_terminal_rows_while_logs_show_attempts() {
    let (dir, store) = usage_store("usage-terminal-only");

    let day_one_ten = rfc3339_millis("2026-09-15T10:00:00+08:00");
    let day_one_eleven_thirty = rfc3339_millis("2026-09-15T11:30:00+08:00");
    let day_one_attempt = rfc3339_millis("2026-09-15T12:45:00+08:00");
    let day_two_nine_fifteen = rfc3339_millis("2026-09-16T09:15:00+08:00");
    let day_two_attempt = rfc3339_millis("2026-09-16T09:25:00+08:00");

    // Slice order is the insertion order inside one connection, so rows that
    // share a timestamp come back by descending row id (newest first).
    let batch = vec![
        // Non-terminal failure whose model and provider also appear on terminal
        // rows: it must not add a request, tokens, cost or an error.
        sample_attempt_record(
            day_one_attempt,
            "local-a",
            "remote-a",
            "p1",
            "Provider One",
            UsageResult::Failure,
            false,
            Some("first attempt failed"),
            Some(0.5),
            tokens(10, 0, 0, 5),
        ),
        // Non-terminal failure whose model appears on no terminal row and which
        // is unpriced: the statistics must count neither it nor its model.
        sample_attempt_record(
            day_two_attempt,
            "local-attempt-only",
            "remote-attempt",
            "p-attempt",
            "Provider Attempt",
            UsageResult::Failure,
            false,
            Some("only attempt failed"),
            None,
            tokens(100, 0, 0, 100),
        ),
        // Non-terminal success on a model/provider that also has terminal rows.
        sample_attempt_record(
            day_one_ten,
            "local-b",
            "remote-b",
            "p2",
            "Provider Two",
            UsageResult::Success,
            false,
            None,
            Some(0.25),
            tokens(7, 0, 0, 3),
        ),
        // Terminal rows: the only rows any aggregate may count.
        sample_record(
            day_one_ten,
            "local-a",
            "remote-a",
            "p1",
            "Provider One",
            UsageResult::Success,
            Some(1.0),
            tokens(10, 2, 1, 5),
        ),
        sample_record(
            day_one_eleven_thirty,
            "local-a",
            "remote-a",
            "p2",
            "Provider Two",
            UsageResult::Success,
            Some(2.0),
            tokens(20, 3, 0, 10),
        ),
        sample_record(
            day_two_nine_fifteen,
            "local-b",
            "remote-b",
            "p1",
            "Provider One",
            UsageResult::Failure,
            None,
            tokens(5, 0, 4, 5),
        ),
    ];
    store
        .append_batch(&batch, 365)
        .expect("append_batch must store every row of the slice");
    assert_eq!(store.count().unwrap(), 6);

    // --- totals, buckets and breakdowns over terminal rows only ------------
    let stats = store.usage_stats(&TimeRange::default(), false).unwrap();
    assert_eq!(stats.granularity, "day");
    assert_eq!(stats.totals.request_count, 3, "only terminal rows are requests");
    assert_eq!(stats.totals.input_tokens, 10 + 20 + 5);
    assert_eq!(stats.totals.cache_read_tokens, 2 + 3);
    assert_eq!(stats.totals.cache_write_tokens, 1 + 4);
    assert_eq!(stats.totals.output_tokens, 5 + 10 + 5);
    assert_eq!(stats.totals.total_tokens, 18 + 33 + 14);
    assert!((stats.totals.amount - 3.0).abs() < 1e-9);
    assert_eq!(
        stats.totals.unpriced_count, 1,
        "only the unpriced terminal row may be counted, never the unpriced attempt"
    );

    assert_eq!(stats.buckets.len(), 2);
    assert_eq!(stats.buckets[0].label, "2026-09-15");
    assert_eq!(stats.buckets[0].metrics.request_count, 2);
    assert_eq!(stats.buckets[0].metrics.total_tokens, 18 + 33);
    assert!((stats.buckets[0].metrics.amount - 3.0).abs() < 1e-9);
    assert_eq!(stats.buckets[0].metrics.unpriced_count, 0);
    assert_eq!(stats.buckets[1].label, "2026-09-16");
    assert_eq!(stats.buckets[1].metrics.request_count, 1);
    assert_eq!(stats.buckets[1].metrics.total_tokens, 14);
    assert_eq!(stats.buckets[1].metrics.unpriced_count, 1);

    // The same rows bucketed by UTC+8 hour: the attempt-only hour must not
    // appear and hours shared with attempts count terminal rows only.
    let hourly = store.usage_stats(&TimeRange::default(), true).unwrap();
    assert_eq!(hourly.granularity, "hour");
    assert_eq!(
        hourly
            .buckets
            .iter()
            .map(|bucket| bucket.label.as_str())
            .collect::<Vec<_>>(),
        vec!["09:00", "10:00", "11:00"],
        "the 12:45 attempt must not create a bucket"
    );
    let nine = hourly
        .buckets
        .iter()
        .find(|bucket| bucket.label == "09:00")
        .expect("09:00 bucket");
    assert_eq!(nine.metrics.request_count, 1);
    assert_eq!(nine.metrics.total_tokens, 14);
    let ten = hourly
        .buckets
        .iter()
        .find(|bucket| bucket.label == "10:00")
        .expect("10:00 bucket");
    assert_eq!(ten.metrics.request_count, 1);
    assert_eq!(ten.metrics.total_tokens, 18);
    let eleven = hourly
        .buckets
        .iter()
        .find(|bucket| bucket.label == "11:00")
        .expect("11:00 bucket");
    assert_eq!(eleven.metrics.request_count, 1);
    assert_eq!(eleven.metrics.total_tokens, 33);

    assert_eq!(
        stats
            .models
            .iter()
            .map(|row| row.local_model.as_str())
            .collect::<Vec<_>>(),
        vec!["local-a", "local-b"],
        "the attempt-only model must not appear in the statistics"
    );
    let local_a = stats
        .models
        .iter()
        .find(|row| row.local_model == "local-a")
        .expect("local-a row");
    assert_eq!(local_a.metrics.request_count, 2);
    assert_eq!(local_a.metrics.total_tokens, 18 + 33);
    assert!((local_a.metrics.amount - 3.0).abs() < 1e-9);
    assert_eq!(local_a.metrics.unpriced_count, 0);
    assert_eq!(local_a.providers.len(), 2);
    let provider_one = local_a
        .providers
        .iter()
        .find(|row| row.provider_id == "p1")
        .expect("p1 detail");
    assert_eq!(provider_one.metrics.request_count, 1);
    assert_eq!(provider_one.metrics.total_tokens, 18);
    let provider_two = local_a
        .providers
        .iter()
        .find(|row| row.provider_id == "p2")
        .expect("p2 detail");
    assert_eq!(provider_two.metrics.request_count, 1);
    assert_eq!(provider_two.metrics.total_tokens, 33);
    let local_b = stats
        .models
        .iter()
        .find(|row| row.local_model == "local-b")
        .expect("local-b row");
    assert_eq!(local_b.metrics.request_count, 1);
    assert_eq!(local_b.metrics.total_tokens, 14);
    assert_eq!(local_b.metrics.unpriced_count, 1);
    assert_eq!(local_b.providers.len(), 1);

    // --- grouped views count terminal rows only ----------------------------
    let by_model = store
        .group_logs(&TimeRange::default(), &LogFilter::default(), "model")
        .unwrap();
    assert_eq!(
        by_model
            .iter()
            .map(|group| group.group.as_str())
            .collect::<Vec<_>>(),
        vec!["local-b", "local-a"],
        "the attempt-only model must not appear in grouped rows"
    );
    let model_b = by_model
        .iter()
        .find(|group| group.group == "local-b")
        .expect("local-b group");
    assert_eq!(model_b.request_count, 1);
    assert_eq!(model_b.error_count, 1);
    assert_eq!(
        model_b.last_request_at_ms, day_two_nine_fifteen,
        "the later attempt must not become the last request"
    );
    let model_a = by_model
        .iter()
        .find(|group| group.group == "local-a")
        .expect("local-a group");
    assert_eq!(model_a.request_count, 2);
    assert_eq!(model_a.error_count, 0, "a non-terminal failure is not an error");
    assert_eq!(model_a.last_request_at_ms, day_one_eleven_thirty);

    let by_day = store
        .group_logs(&TimeRange::default(), &LogFilter::default(), "day")
        .unwrap();
    assert_eq!(by_day.len(), 2);
    let first_day = by_day
        .iter()
        .find(|group| group.group == "2026-09-15")
        .expect("day one group");
    assert_eq!(first_day.request_count, 2);
    assert_eq!(first_day.error_count, 0, "the 12:45 attempt must not add an error");
    assert_eq!(
        first_day.last_request_at_ms, day_one_eleven_thirty,
        "the later attempt must not become the last request"
    );
    let second_day = by_day
        .iter()
        .find(|group| group.group == "2026-09-16")
        .expect("day two group");
    assert_eq!(second_day.request_count, 1);
    assert_eq!(second_day.error_count, 1);
    assert_eq!(second_day.last_request_at_ms, day_two_nine_fifteen);

    // --- the ungrouped list keeps every row --------------------------------
    let page = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .unwrap();
    assert_eq!(page.total, 6, "the ungrouped list counts every stored row");
    assert_eq!(page.total_pages, 1);
    assert_eq!(page.records.len(), 6);
    let order = page
        .records
        .iter()
        .map(|record| {
            (
                record.timestamp_ms,
                record.provider_id.as_str(),
                record.terminal,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        vec![
            (day_two_attempt, "p-attempt", false),
            (day_two_nine_fifteen, "p1", true),
            (day_one_attempt, "p1", false),
            (day_one_eleven_thirty, "p2", true),
            (day_one_ten, "p1", true),
            (day_one_ten, "p2", false),
        ],
        "newest first, attempt rows visible, equal timestamps in insertion order"
    );
    assert_eq!(
        page.records[0].error_message.as_deref(),
        Some("only attempt failed")
    );
    assert_eq!(page.records[1].error_message, None);
    assert_eq!(
        page.models,
        vec![
            "local-a".to_string(),
            "local-attempt-only".to_string(),
            "local-b".to_string(),
        ],
        "the facet must include a model that exists only on non-terminal rows"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// REQ-005: `append_batch` writes the whole slice through one connection, treats
/// an empty slice as a no-op and still applies the retention cleanup.
#[test]
fn usage_store_append_batch_handles_empty_slices_and_applies_retention() {
    let (dir, store) = usage_store("usage-append-batch");
    let now = super::now_millis();
    let day = 86_400_000i64;

    store
        .append_batch(&[], 365)
        .expect("an empty slice must succeed");
    assert_eq!(store.count().unwrap(), 0, "an empty slice must not add rows");

    store
        .append_batch(
            &[
                sample_record(
                    now - 10 * day,
                    "local-old",
                    "remote-a",
                    "p1",
                    "Provider One",
                    UsageResult::Success,
                    Some(0.1),
                    tokens(1, 0, 0, 1),
                ),
                sample_record(
                    now - 2 * day,
                    "local-fresh",
                    "remote-a",
                    "p1",
                    "Provider One",
                    UsageResult::Success,
                    Some(0.1),
                    tokens(1, 0, 0, 1),
                ),
            ],
            365,
        )
        .expect("a two-row batch must succeed");
    assert_eq!(store.count().unwrap(), 2);

    store
        .append_batch(
            &[sample_record(
                now,
                "local-newest",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(0.1),
                tokens(1, 0, 0, 1),
            )],
            7,
        )
        .expect("a single-row batch must succeed");
    let remaining = store.all_records().unwrap();
    assert_eq!(remaining.len(), 2, "retention 7 must delete the 10-day-old row");
    assert!(!remaining
        .iter()
        .any(|record| record.local_model == "local-old"));
    assert!(remaining
        .iter()
        .any(|record| record.local_model == "local-newest"));
    let _ = fs::remove_dir_all(&dir);
}

/// AC-010 / REQ-003: extraction prefers a standard `error.message`, otherwise
/// summarizes the body (lossy decode, markup removed, whitespace collapsed) and
/// returns `None` when nothing readable remains.
#[test]
fn extract_upstream_error_text_prefers_error_message_and_summarizes_bodies() {
    let envelope =
        br#"{"error":{"message":"upstream rate limit exceeded","type":"rate_limit_error"}}"#;
    assert_eq!(
        extract_upstream_error_text(envelope).as_deref(),
        Some("upstream rate limit exceeded")
    );

    // A JSON body without a usable `error.message` falls back to the body text.
    let other_json = br#"{"error":{"message":"","code":"bad_request"}}"#;
    assert_eq!(
        extract_upstream_error_text(other_json).as_deref(),
        Some(r#"{"error":{"message":"","code":"bad_request"}}"#)
    );

    let html = b"<html>\n<body>\n<h1>Bad Gateway</h1>\n<p>origin   refused\nconnection</p>\n</body>\n</html>";
    let summary = extract_upstream_error_text(html).expect("an HTML body yields readable text");
    assert_eq!(summary, "Bad Gateway origin refused connection");
    assert!(
        !summary.contains('<') && !summary.contains('>'),
        "markup must be removed: {summary}"
    );
    assert!(!summary.contains("  "), "whitespace must be collapsed: {summary}");

    let non_utf8 = b"\xff\xfeBad gateway \x80 from upstream";
    let lossy = extract_upstream_error_text(non_utf8).expect("non-UTF-8 bytes still yield text");
    assert_eq!(lossy, "\u{fffd}\u{fffd}Bad gateway \u{fffd} from upstream");

    assert_eq!(
        extract_upstream_error_text(b""),
        None,
        "an empty body has no readable text"
    );
    assert_eq!(
        extract_upstream_error_text(b"   \n\t  "),
        None,
        "a whitespace-only body has no readable text"
    );
    assert_eq!(
        extract_upstream_error_text(b"<html>\n<body></body>\n</html>"),
        None,
        "a markup-only body has no readable text"
    );

    // The lossily decoded text must still be writable: extraction feeds
    // `error_message` and a non-UTF-8 body must never fail a log write.
    let (dir, store) = usage_store("usage-error-text-lossy");
    let sanitized =
        sanitize_error_text(&lossy, "sk-unrelated").expect("the lossy summary survives sanitization");
    store
        .append_batch(
            &[sample_attempt_record(
                super::now_millis(),
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Failure,
                false,
                Some(sanitized.as_str()),
                None,
                UsageTokens::default(),
            )],
            365,
        )
        .expect("a lossy-decoded error message must be writable");
    let stored = store.all_records().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].error_message.as_deref(), Some(sanitized.as_str()));
    let _ = fs::remove_dir_all(&dir);
}

/// AC-010 / REQ-004: sanitization replaces the provider key, masks credential
/// shapes, bounds the text at 4096 characters on a Unicode boundary and returns
/// `None` when nothing readable remains.
#[test]
fn sanitize_error_text_redacts_masks_and_bounds_text() {
    // The provider key is replaced verbatim wherever it appears.
    let api_key = "SAFE_FIXTURE_upstream-key-12345";
    let echoed = format!("request with {api_key} was rejected; retry without {api_key}");
    let sanitized = sanitize_error_text(&echoed, api_key).expect("text survives sanitization");
    assert_eq!(
        sanitized,
        "request with [redacted] was rejected; retry without [redacted]"
    );
    assert!(!sanitized.contains(api_key));
    assert_eq!(sanitized.matches("[redacted]").count(), 2);

    // 4096 characters is stored unchanged, without a truncation marker.
    let exact = "x".repeat(4096);
    let bounded = sanitize_error_text(&exact, "unrelated-key").expect("bounded text survives");
    assert_eq!(bounded, exact, "exactly 4096 characters must be stored unchanged");
    assert!(!bounded.ends_with('…'), "no ellipsis without truncation");

    // 4097 characters is capped at 4096 characters and marked as truncated.
    let over = "y".repeat(4097);
    let truncated = sanitize_error_text(&over, "unrelated-key").expect("truncated text survives");
    assert!(
        truncated.chars().count() <= 4096,
        "the bound is 4096 characters, got {}",
        truncated.chars().count()
    );
    assert!(truncated.ends_with('…'), "truncation must be marked: {truncated}");
    assert!(
        over.starts_with(truncated.trim_end_matches('…')),
        "only a prefix of the original may survive"
    );

    // A multi-byte body must truncate on a character boundary.
    let cjk = "错误".repeat(3000);
    let bounded_cjk = sanitize_error_text(&cjk, "unrelated-key").expect("CJK text survives");
    assert!(bounded_cjk.chars().count() <= 4096);
    assert!(bounded_cjk.ends_with('…'), "CJK text must be marked as truncated");
    let kept = bounded_cjk.trim_end_matches('…');
    assert!(cjk.starts_with(kept), "truncation must not split a character");
    assert!(kept.chars().all(|character| character == '错' || character == '误'));
    assert!(std::str::from_utf8(bounded_cjk.as_bytes()).is_ok());

    // Token-shaped credentials are masked so the complete token is absent.
    let sk_token = "sk-live-1a2b3c4d5e6f7a8b";
    let bearer_token = "AbCdEf0123456789xyzXYZ";
    let credential_text = format!("upstream echoed {sk_token} and sent Bearer {bearer_token} back");
    let masked = sanitize_error_text(&credential_text, "unrelated-key").expect("text survives");
    assert!(
        !masked.contains(sk_token),
        "the complete sk-shaped token must be absent: {masked}"
    );
    assert!(
        !masked.contains(bearer_token),
        "the complete Bearer token must be absent: {masked}"
    );

    // Text that is empty after sanitization stores no message.
    assert_eq!(sanitize_error_text("", "unrelated-key"), None);
    assert_eq!(sanitize_error_text("   \n\t  ", "unrelated-key"), None);
}

/// Column names of `usage_logs` read straight from the file, independent of the
/// store's own migration logic.
fn usage_log_table_columns(path: &Path) -> Vec<String> {
    let connection = rusqlite::Connection::open(path).expect("open raw sqlite connection");
    let mut statement = connection
        .prepare("PRAGMA table_info(usage_logs)")
        .expect("prepare PRAGMA table_info");
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("run PRAGMA table_info")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect PRAGMA table_info");
    columns
}

/// AC-011 / REQ-005 / REQ-006: an `api_gateway_usage.db` written by the previous
/// release gains both columns idempotently, keeps its pre-migration statistics,
/// treats historical rows as terminal with no message, and exposes the stored
/// values of new attempt and terminal rows through the request-log command.
#[test]
fn usage_store_migrates_pre_upgrade_database_and_exposes_new_fields() {
    with_temp_home("usage-migration-payload", |_home| {
        let app_dir = crate::config::get_app_dir().expect("app dir");
        let db_path = app_dir.join(super::USAGE_DB_FILE);

        // The pre-upgrade file: the previous release's table, no new columns.
        let legacy = rusqlite::Connection::open(&db_path).expect("create pre-upgrade db");
        legacy
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
        let legacy_success = rfc3339_millis("2026-09-15T10:00:00+08:00");
        let legacy_failure = rfc3339_millis("2026-09-16T10:00:00+08:00");
        legacy
            .execute(
                "INSERT INTO usage_logs (
                    timestamp_ms, local_model, upstream_model, provider_id, provider_name,
                    result, status, input_tokens, cache_read_tokens, cache_write_tokens,
                    output_tokens, total_tokens, amount, duration_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    legacy_success,
                    "local-a",
                    "remote-a",
                    "p1",
                    "Provider One",
                    "success",
                    200i64,
                    10i64,
                    0i64,
                    0i64,
                    5i64,
                    15i64,
                    Some(0.5f64),
                    5i64
                ],
            )
            .expect("insert the pre-upgrade success row");
        legacy
            .execute(
                "INSERT INTO usage_logs (
                    timestamp_ms, local_model, upstream_model, provider_id, provider_name,
                    result, status, input_tokens, cache_read_tokens, cache_write_tokens,
                    output_tokens, total_tokens, amount, duration_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    legacy_failure,
                    "local-b",
                    "remote-b",
                    "p2",
                    "Provider Two",
                    "failure",
                    500i64,
                    20i64,
                    0i64,
                    0i64,
                    10i64,
                    30i64,
                    Option::<f64>::None,
                    7i64
                ],
            )
            .expect("insert the pre-upgrade failure row");
        drop(legacy);

        // Opening through any store operation migrates the file.
        let store = UsageLogStore::at(&db_path);
        let legacy_stats = store
            .usage_stats(&TimeRange::default(), false)
            .expect("statistics over the migrated rows");

        let columns = usage_log_table_columns(&db_path);
        assert_eq!(
            columns
                .iter()
                .filter(|name| name.as_str() == "error_message")
                .count(),
            1,
            "error_message must be added exactly once: {columns:?}"
        );
        assert_eq!(
            columns
                .iter()
                .filter(|name| name.as_str() == "terminal")
                .count(),
            1,
            "terminal must be added exactly once: {columns:?}"
        );
        let distinct_columns = columns.iter().collect::<HashSet<_>>();
        assert_eq!(
            columns.len(),
            distinct_columns.len(),
            "the migration must not duplicate columns: {columns:?}"
        );

        let legacy_records = store.all_records().expect("read the migrated rows");
        assert_eq!(legacy_records.len(), 2);
        assert!(
            legacy_records.iter().all(|record| record.terminal),
            "pre-upgrade rows must count as terminal: {legacy_records:?}"
        );
        assert!(
            legacy_records
                .iter()
                .all(|record| record.error_message.is_none()),
            "pre-upgrade rows must report no error message: {legacy_records:?}"
        );

        assert_eq!(legacy_stats.totals.request_count, 2);
        assert_eq!(legacy_stats.totals.input_tokens, 30);
        assert_eq!(legacy_stats.totals.cache_read_tokens, 0);
        assert_eq!(legacy_stats.totals.cache_write_tokens, 0);
        assert_eq!(legacy_stats.totals.output_tokens, 15);
        assert_eq!(legacy_stats.totals.total_tokens, 45);
        assert!((legacy_stats.totals.amount - 0.5).abs() < 1e-9);
        assert_eq!(legacy_stats.totals.unpriced_count, 1);
        assert_eq!(legacy_stats.buckets.len(), 2);
        assert_eq!(legacy_stats.buckets[0].label, "2026-09-15");
        assert_eq!(legacy_stats.buckets[0].metrics.request_count, 1);
        assert_eq!(legacy_stats.buckets[0].metrics.total_tokens, 15);
        assert_eq!(legacy_stats.buckets[1].label, "2026-09-16");
        assert_eq!(legacy_stats.buckets[1].metrics.request_count, 1);
        assert_eq!(legacy_stats.buckets[1].metrics.total_tokens, 30);
        assert_eq!(legacy_stats.buckets[1].metrics.unpriced_count, 1);

        // A second open must change neither the schema nor the rows.
        let second_open = UsageLogStore::at(&db_path);
        let _ = second_open
            .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
            .expect("query through a second open");
        assert_eq!(
            usage_log_table_columns(&db_path),
            columns,
            "a second open must not duplicate columns"
        );
        assert_eq!(second_open.count().expect("count after the second open"), 2);

        // New attempt and terminal rows expose their stored fields.
        let attempt_at = rfc3339_millis("2026-09-17T09:00:00+08:00");
        let terminal_at = rfc3339_millis("2026-09-17T09:00:02+08:00");
        store
            .append_batch(
                &[
                    sample_attempt_record(
                        attempt_at,
                        "local-a",
                        "remote-a",
                        "p1",
                        "Provider One",
                        UsageResult::Failure,
                        false,
                        Some("upstream 500: gateway exploded"),
                        None,
                        UsageTokens::default(),
                    ),
                    sample_record(
                        terminal_at,
                        "local-a",
                        "remote-a",
                        "p2",
                        "Provider Two",
                        UsageResult::Success,
                        Some(1.5),
                        tokens(4, 0, 0, 4),
                    ),
                ],
                365,
            )
            .expect("append the attempt row and its terminal row");

        let page = store
            .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
            .expect("query the page");
        assert_eq!(page.total, 4);
        assert_eq!(page.records[0].timestamp_ms, terminal_at);
        assert!(page.records[0].terminal);
        assert_eq!(page.records[0].error_message, None);
        assert_eq!(page.records[1].timestamp_ms, attempt_at);
        assert!(!page.records[1].terminal, "the attempt row must be non-terminal");
        assert_eq!(
            page.records[1].error_message.as_deref(),
            Some("upstream 500: gateway exploded")
        );

        // The request-log command payload carries both stored fields.
        let command_page = super::commands::api_gateway_request_logs(None, None, None, None, None)
            .expect("request-log command");
        let payload = serde_json::to_value(&command_page).expect("serialize the command payload");
        assert_eq!(payload["total"], json!(4));
        let records = payload["records"].as_array().expect("records array");
        assert_eq!(records.len(), 4);
        assert_eq!(records[0]["terminal"], json!(true));
        assert_eq!(
            records[0]["error_message"],
            Value::Null,
            "an absent message must serialize as null"
        );
        assert_eq!(records[1]["terminal"], json!(false));
        assert_eq!(
            records[1]["error_message"],
            json!("upstream 500: gateway exploded")
        );

        // Reopening changes nothing further.
        let reopened = UsageLogStore::at(&db_path);
        assert_eq!(reopened.count().expect("count after reopen"), 4);
        assert_eq!(
            usage_log_table_columns(&db_path),
            columns,
            "reopening must not change the schema"
        );
        let stored_attempt = reopened
            .all_records()
            .expect("read after reopen")
            .into_iter()
            .find(|record| !record.terminal && record.provider_id == "p1")
            .expect("stored attempt row");
        assert_eq!(
            stored_attempt.error_message.as_deref(),
            Some("upstream 500: gateway exploded")
        );
    });
}

/// AC-011 / REQ-005: a store that cannot open its file reports the failure as an
/// `Err` from `append_batch` instead of panicking and leaves no side effect.
#[test]
fn usage_store_append_batch_reports_unopenable_paths_without_side_effects() {
    let dir = make_temp_dir("usage-append-unopenable");
    fs::create_dir_all(&dir).expect("create temp dir");
    let db_path = dir.join("api_gateway_usage.db");
    // A directory at the database path makes SQLite's open fail.
    fs::create_dir_all(&db_path).expect("create a directory at the database path");
    let store = UsageLogStore::at(&db_path);

    let error = store
        .append_batch(
            &[sample_record(
                super::now_millis(),
                "local-a",
                "remote-a",
                "p1",
                "Provider One",
                UsageResult::Success,
                Some(0.1),
                tokens(1, 0, 0, 1),
            )],
            365,
        )
        .expect_err("a path that cannot be opened must be reported as an error");
    assert!(!error.is_empty(), "the failure must carry a message");

    assert!(
        db_path.is_dir(),
        "the failing store must not replace the directory"
    );
    assert!(
        fs::read_dir(&db_path)
            .expect("read the directory at the database path")
            .next()
            .is_none(),
        "a failed open must not write any database file"
    );
    assert_eq!(
        fs::read_dir(&dir).expect("read temp dir").count(),
        1,
        "the failed store must not create sibling files"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// REQ-006: a payload written before the change deserializes with the documented
/// defaults, and new records serialize both fields with their stored values.
#[test]
fn usage_log_record_serde_defaults_and_round_trips_new_fields() {
    let older = json!({
        "timestamp_ms": 1_789_000_000_000i64,
        "local_model": "local-a",
        "upstream_model": "remote-a",
        "provider_id": "p1",
        "provider_name": "Provider One",
        "result": "success",
        "status": 200,
        "input_tokens": 1,
        "cache_read_tokens": 2,
        "cache_write_tokens": 3,
        "output_tokens": 4,
        "total_tokens": 10,
        "amount": 0.5,
        "duration_ms": 5
    });
    let record: UsageLogRecord = serde_json::from_value(older).expect("older payload parses");
    assert_eq!(record.error_message, None, "a missing message means no message");
    assert!(
        record.terminal,
        "a missing terminal flag must default to terminal"
    );

    let attempt = sample_attempt_record(
        2_000,
        "local-a",
        "remote-a",
        "p1",
        "Provider One",
        UsageResult::Failure,
        false,
        Some("upstream exploded"),
        None,
        UsageTokens::default(),
    );
    let attempt_value = serde_json::to_value(&attempt).unwrap();
    assert_eq!(attempt_value["error_message"], json!("upstream exploded"));
    assert_eq!(attempt_value["terminal"], json!(false));

    let terminal = sample_record(
        3_000,
        "local-b",
        "remote-b",
        "p2",
        "Provider Two",
        UsageResult::Success,
        Some(0.5),
        tokens(1, 0, 0, 1),
    );
    let terminal_value = serde_json::to_value(&terminal).unwrap();
    assert_eq!(
        terminal_value["error_message"],
        Value::Null,
        "an absent message must serialize as null"
    );
    assert_eq!(terminal_value["terminal"], json!(true));

    let decoded: UsageLogRecord =
        serde_json::from_value(terminal_value).expect("new payload round trips");
    assert_eq!(decoded, terminal);
}

// ---------------------------------------------------------------------------
// 20260920-gateway-log-attempts-and-upstream-errors Step 2 (RED): per-attempt
// logging through the forwarding path. Every completed upstream attempt of one
// inbound request appends one buffer entry; at request end the handler writes
// one row per entry in attempt order, stamps exactly one terminal row and
// appends the synthetic `cancelled` row when the downstream client goes away
// (REQ-001, REQ-002, REQ-003; AC-001..AC-009).
// ---------------------------------------------------------------------------

/// Wait until the ungrouped list holds exactly `expected` rows, so the rows of
/// one multi-attempt request are never read half-written, then return the
/// newest-first snapshot.
async fn wait_for_exact_usage_logs(expected: u32) -> Vec<UsageLogRecord> {
    let store = default_usage_store();
    for _ in 0..400 {
        let records = store.all_records().unwrap_or_default();
        if records.len() as u32 == expected {
            return records;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let records = store.all_records().unwrap_or_default();
    panic!(
        "expected exactly {expected} log rows, observed {}: {records:?}",
        records.len()
    );
}

/// Wait until the newest-first ungrouped list holds exactly `expected` rows for
/// `local_model` and return them. Each end-to-end iteration of the
/// randomized-order cases uses its own local model, so its rows stay isolated
/// from the other requests of the same test.
async fn wait_for_model_usage_logs(local_model: &str, expected: u32) -> Vec<UsageLogRecord> {
    let store = default_usage_store();
    let filter = LogFilter {
        status: None,
        model: Some(local_model.to_string()),
    };
    for _ in 0..400 {
        let page = store
            .query_logs(&TimeRange::default(), &filter, 1)
            .unwrap_or_else(|error| panic!("query logs for {local_model}: {error}"));
        if page.total == expected {
            return page.records;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let page = store
        .query_logs(&TimeRange::default(), &filter, 1)
        .unwrap_or_else(|error| panic!("query logs for {local_model}: {error}"));
    panic!(
        "expected exactly {expected} log rows for {local_model}, observed {}",
        page.total
    );
}

/// AC-001 / REQ-001 direct boundary: with two candidates in explicit order, the
/// failed first attempt and the successful second attempt are both buffered, in
/// completion order, each with its own provider, upstream model, status,
/// message, usage and per-attempt duration.
#[tokio::test]
async fn attempt_buffer_records_failed_then_successful_attempts_in_completion_order() {
    let _home = isolated_temp_home("attempt-buffer-failure-then-success");
    let (failing_url, _failing_log) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "first provider exploded"}}))
    })
    .await;
    let (success_url, _success_log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "served",
                "choices": [],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5}
            }),
        )
    })
    .await;

    let failing = upstream_provider(
        "failing",
        "Failing Provider",
        &failing_url,
        "sk",
        Some("remote-failing"),
    );
    let success = upstream_provider(
        "success",
        "Success Provider",
        &success_url,
        "sk",
        Some("remote-success"),
    );
    let mut config = GatewayConfig::default();
    config.providers = vec![failing.clone(), success.clone()];
    let body = serde_json::to_vec(&json!({"model": "local"})).unwrap();

    let mut attempts = Vec::new();
    let response = super::runtime_http::attempt_non_streaming(
        &[failing, success],
        "/v1/chat/completions",
        &body,
        Some("local"),
        &mut config,
        &HashMap::new(),
        &mut attempts,
    )
    .await;

    assert_eq!(response.status, 200);
    assert_eq!(
        attempts.len(),
        2,
        "one entry per completed upstream attempt"
    );
    assert_eq!(attempts[0].provider_id, "failing");
    assert_eq!(attempts[0].provider_name, "Failing Provider");
    assert_eq!(attempts[0].upstream_model, "remote-failing");
    assert_eq!(attempts[0].status, 500);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(
        attempts[0].error_message.as_deref(),
        Some("first provider exploded")
    );
    assert!(attempts[0].usage.is_none());
    assert!(attempts[0].duration_ms >= 1);
    assert_eq!(attempts[1].provider_id, "success");
    assert_eq!(attempts[1].provider_name, "Success Provider");
    assert_eq!(attempts[1].upstream_model, "remote-success");
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);
    assert_eq!(attempts[1].error_message, None);
    assert_eq!(attempts[1].usage, Some(tokens(10, 0, 0, 5)));
    assert!(attempts[1].duration_ms >= 1);
}

/// AC-002 / REQ-001 / REQ-002 / REQ-003: three candidates that all answer 500
/// with distinct standard bodies are retried to their bounded caps; the log
/// holds one row per upstream attempt the mock servers actually received, each
/// bound to its own provider and message, exactly one terminal row records the
/// observed upstream status instead of the transport 502, the caller still
/// receives the unchanged 502 envelope, and the statistics count one request
/// and one error.
#[tokio::test]
async fn all_candidates_failed_request_writes_one_row_per_completed_attempt() {
    let home = temp_home("usage-one-row-per-attempt");
    let port = free_port().await;
    let (url_a, log_a) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "provider a exploded"}}))
    })
    .await;
    let (url_b, log_b) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "provider b exploded"}}))
    })
    .await;
    let (url_c, log_c) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "provider c exploded"}}))
    })
    .await;

    let mut config = config_with_key(port);
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &url_a,
        "sk-a",
        Some("remote-a"),
    ));
    config.providers.push(upstream_provider(
        "b",
        "Provider B",
        &url_b,
        "sk-b",
        Some("remote-b"),
    ));
    config.providers.push(upstream_provider(
        "c",
        "Provider C",
        &url_c,
        "sk-c",
        Some("remote-c"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-all-failed"})),
    )
    .await;
    assert_eq!(status, 502, "unexpected response: {text}");
    assert!(
        content_type.contains("application/json"),
        "an exhausted non-streaming request answers JSON: {content_type}"
    );
    let body = assert_standard_error_envelope(&text);
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    let message = body["error"]["message"].as_str().unwrap_or("");
    for name in ["Provider A", "Provider B", "Provider C"] {
        assert!(message.contains(name), "message must name {name}: {message}");
    }

    let received_a = log_a.lock().unwrap().len() as u32;
    let received_b = log_b.lock().unwrap().len() as u32;
    let received_c = log_c.lock().unwrap().len() as u32;
    assert_eq!(received_a, 6, "A is attempted once plus five bounded retries");
    assert_eq!(received_b, 6, "B is attempted once plus five bounded retries");
    assert_eq!(received_c, 6, "C is attempted once plus five bounded retries");
    let expected_rows = received_a + received_b + received_c;

    let records = wait_for_exact_usage_logs(expected_rows).await;
    assert_eq!(
        records.len(),
        expected_rows as usize,
        "one row per completed upstream attempt"
    );
    let cases = [
        ("a", "remote-a", "provider a exploded", received_a),
        ("b", "remote-b", "provider b exploded", received_b),
        ("c", "remote-c", "provider c exploded", received_c),
    ];
    for (provider_id, upstream_model, provider_message, received) in cases {
        let rows = records
            .iter()
            .filter(|record| record.provider_id == provider_id)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), received as usize, "{provider_id} row count");
        for row in rows {
            assert_eq!(row.local_model, "local-all-failed");
            assert_eq!(row.upstream_model, upstream_model);
            assert_eq!(row.status, 500);
            assert_eq!(row.result, UsageResult::Failure);
            assert_eq!(row.error_message.as_deref(), Some(provider_message));
            assert_eq!(row.total_tokens, 0);
            assert!(row.duration_ms >= 1);
        }
    }
    assert_eq!(
        records.iter().filter(|record| record.terminal).count(),
        1,
        "exactly one row is the request's terminal row"
    );
    let terminal = records
        .iter()
        .find(|record| record.terminal)
        .expect("terminal row");
    assert_eq!(
        terminal.status, 500,
        "the terminal row keeps the last observed upstream status, not the transport 502"
    );
    assert_eq!(terminal.result, UsageResult::Failure);
    assert!(
        ["a", "b", "c"].contains(&terminal.provider_id.as_str()),
        "the terminal row is one of the completed attempt rows"
    );

    let store = default_usage_store();
    let stats = store.usage_stats(&TimeRange::default(), false).unwrap();
    assert_eq!(
        stats.totals.request_count, 1,
        "one request, and the attempt rows do not count"
    );
    assert_eq!(stats.totals.total_tokens, 0);
    assert_eq!(stats.totals.amount, 0.0);
    assert_eq!(
        stats.totals.unpriced_count, 1,
        "only the terminal row is an unpriced request"
    );
    let grouped = store
        .group_logs(&TimeRange::default(), &LogFilter::default(), "model")
        .unwrap();
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].request_count, 1);
    assert_eq!(grouped[0].error_count, 1);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-003 / REQ-001 / REQ-003: exactly one serviceable candidate answering 429
/// is attempted once with no backoff wait; its terminal row keeps the observed
/// 429 and the upstream message, and the caller receives the standard 502.
#[tokio::test]
async fn single_candidate_429_failure_keeps_upstream_status_on_the_terminal_row() {
    let home = temp_home("usage-single-429-terminal");
    let port = free_port().await;
    let (upstream_url, log) = spawn_mock_upstream(|_| {
        MockReply::Json(429, json!({"error": {"message": "slow down"}}))
    })
    .await;

    let mut config = config_with_key(port);
    let mut provider = upstream_provider("only", "Only Provider", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-429", "remote-429", None)];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let started = std::time::Instant::now();
    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-429"})),
    )
    .await;
    let elapsed = started.elapsed();
    assert_eq!(status, 502, "the gateway must fail with HTTP 502: {text}");
    let body = assert_standard_error_envelope(&text);
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    assert_eq!(
        log.lock().unwrap().len(),
        1,
        "a single candidate is attempted exactly once"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(1500),
        "the single-candidate path must not wait for a backoff, elapsed {elapsed:?}"
    );

    let records = wait_for_exact_usage_logs(1).await;
    let record = &records[0];
    assert!(record.terminal);
    assert_eq!(record.result, UsageResult::Failure);
    assert_eq!(record.status, 429);
    assert_eq!(record.provider_id, "only");
    assert_eq!(record.local_model, "local-429");
    assert_eq!(record.upstream_model, "remote-429");
    assert_eq!(record.error_message.as_deref(), Some("slow down"));
    assert_eq!(record.total_tokens, 0);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-004 / REQ-001 / REQ-003: a standard upstream 400 envelope reaches the
/// caller byte-for-byte and is logged as the request's terminal failure row with
/// the upstream status and the extracted message.
#[tokio::test]
async fn passed_through_client_error_is_terminal_and_keeps_the_upstream_message() {
    let home = temp_home("usage-terminal-400");
    let port = free_port().await;
    let upstream_body = json!({
        "error": {
            "message": "bad request",
            "type": "invalid_request_error",
            "code": "bad_request",
            "param": "model",
        }
    });
    let expected = serde_json::to_string(&upstream_body).unwrap();
    let for_mock = upstream_body.clone();
    let (upstream_url, _log) =
        spawn_mock_upstream(move |_| MockReply::Json(400, for_mock.clone())).await;

    let mut config = config_with_key(port);
    let mut provider = upstream_provider("p1", "Provider One", &upstream_url, "sk", None);
    provider.mappings = vec![mapping("local-400", "remote-400", None)];
    config.providers.push(provider);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local-400"})),
    )
    .await;
    assert_eq!(status, 400, "caller must receive the upstream status: {text}");
    assert_eq!(
        text, expected,
        "a standard upstream error body must pass through byte-for-byte"
    );

    let records = wait_for_exact_usage_logs(1).await;
    let record = &records[0];
    assert!(
        record.terminal,
        "a single ReturnToClient attempt is the request's terminal row"
    );
    assert_eq!(record.result, UsageResult::Failure);
    assert_eq!(record.status, 400);
    assert_eq!(record.provider_id, "p1");
    assert_eq!(record.upstream_model, "remote-400");
    assert_eq!(record.error_message.as_deref(), Some("bad request"));
    assert_eq!(record.total_tokens, 0);
    assert_eq!(record.amount, None, "the failed attempt is unpriced");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-001 / REQ-001 / REQ-002 / REQ-003 end-to-end: the candidate order is
/// randomized per request, so each iteration uses its own local model and the
/// assertions follow the order the mock servers actually saw. When the failing
/// candidate is tried first, the request writes a non-terminal failure row for
/// it plus one terminal success row for the provider that served it; the
/// statistics always count one request per inbound request with the successful
/// attempt's tokens and amount only.
#[tokio::test]
async fn failed_then_successful_request_writes_attempt_and_terminal_rows() {
    let home = temp_home("usage-attempt-then-terminal");
    let port = free_port().await;
    let (failing_url, failing_log) = spawn_mock_upstream(|_| {
        MockReply::Json(500, json!({"error": {"message": "first provider exploded"}}))
    })
    .await;
    let (success_url, success_log) = spawn_mock_upstream(|_| {
        MockReply::Json(
            200,
            json!({
                "id": "served",
                "choices": [],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5}
            }),
        )
    })
    .await;

    let mut config = config_with_key(port);
    config.providers.push(upstream_provider(
        "failing",
        "Failing Provider",
        &failing_url,
        "sk",
        Some("remote-failing"),
    ));
    config.providers.push(upstream_provider(
        "success",
        "Success Provider",
        &success_url,
        "sk",
        Some("remote-success"),
    ));
    let price = priced_with_provider("success", "remote-success", 1.0, 0.0, 0.0, 2.0);
    let expected_amount = compute_cost(&price, &tokens(10, 0, 0, 5));
    config.model_prices = vec![price];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    const REQUESTS: u32 = 24;
    let mut failed_first_observed = false;
    for index in 0..REQUESTS {
        let local_model = format!("local-ac001-{index}");
        let failing_before = failing_log.lock().unwrap().len() as u32;
        let success_before = success_log.lock().unwrap().len() as u32;
        let (status, _content_type, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(json!({"model": local_model})),
        )
        .await;
        assert_eq!(status, 200, "request {index} must be served: {text}");
        let failing_attempts = failing_log.lock().unwrap().len() as u32 - failing_before;
        let success_attempts = success_log.lock().unwrap().len() as u32 - success_before;
        assert_eq!(success_attempts, 1, "the serving provider is contacted once");
        assert!(
            failing_attempts <= 1,
            "the failing provider is never retried after the success"
        );

        let rows = wait_for_model_usage_logs(&local_model, 1 + failing_attempts).await;
        let success_row = rows
            .iter()
            .find(|record| record.provider_id == "success")
            .expect("the serving provider must own a row");
        assert!(success_row.terminal, "the serving attempt is the terminal row");
        assert_eq!(success_row.result, UsageResult::Success);
        assert_eq!(success_row.status, 200);
        assert_eq!(success_row.local_model, local_model);
        assert_eq!(success_row.upstream_model, "remote-success");
        assert_eq!(success_row.input_tokens, 10);
        assert_eq!(success_row.output_tokens, 5);
        assert_eq!(success_row.total_tokens, 15);
        assert_eq!(success_row.error_message, None);
        assert!((success_row.amount.expect("priced") - expected_amount).abs() < 1e-12);
        assert!(success_row.duration_ms >= 1);
        assert_eq!(
            rows.iter().filter(|record| record.terminal).count(),
            1,
            "exactly one terminal row per request"
        );

        if failing_attempts == 1 {
            failed_first_observed = true;
            let failure_row = rows
                .iter()
                .find(|record| record.provider_id == "failing")
                .expect("the failed attempt must own a row");
            assert!(
                !failure_row.terminal,
                "the superseded failing attempt is not terminal"
            );
            assert_eq!(failure_row.result, UsageResult::Failure);
            assert_eq!(failure_row.status, 500);
            assert_eq!(failure_row.local_model, local_model);
            assert_eq!(failure_row.upstream_model, "remote-failing");
            assert_eq!(
                failure_row.error_message.as_deref(),
                Some("first provider exploded")
            );
            assert_eq!(failure_row.total_tokens, 0);
            assert_eq!(failure_row.amount, None);
            assert!(failure_row.duration_ms >= 1);
            assert_eq!(
                rows[0].provider_id, "success",
                "a newest-first query lists the terminal row before the earlier attempt"
            );
        } else {
            assert!(
                !rows.iter().any(|record| record.provider_id == "failing"),
                "the failing provider was never contacted and owns no row"
            );
        }
    }
    assert!(
        failed_first_observed,
        "the randomized candidate order must have tried the failing provider first at least once"
    );

    let store = default_usage_store();
    let stats = store.usage_stats(&TimeRange::default(), false).unwrap();
    assert_eq!(
        stats.totals.request_count, REQUESTS,
        "attempt rows are not inbound requests"
    );
    assert_eq!(stats.totals.total_tokens, REQUESTS as u64 * 15);
    assert!(
        (stats.totals.amount - REQUESTS as f64 * expected_amount).abs() < 1e-9,
        "only the successful attempts contribute cost"
    );
    assert_eq!(stats.totals.unpriced_count, 0);
    let grouped = store
        .group_logs(&TimeRange::default(), &LogFilter::default(), "day")
        .unwrap();
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].request_count, REQUESTS);
    assert_eq!(grouped[0].error_count, 0, "no terminal row failed");

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-005 / REQ-001 / REQ-003 end-to-end: a streaming candidate whose 2xx body
/// is not an SSE stream fails before any byte and is superseded by the second
/// candidate. The candidate order is randomized, so each iteration uses its own
/// local model and asserts against the order the mock servers actually saw: the
/// rejected attempt keeps a non-terminal failure row for its provider and the
/// stream that answered owns the terminal success row.
#[tokio::test]
async fn pre_first_byte_stream_failure_switches_and_logs_both_attempts() {
    let home = temp_home("usage-stream-pre-first-byte-attempts");
    let port = free_port().await;
    let (failing_url, failing_log) = spawn_mock_upstream(|_| {
        MockReply::Raw(200, "text/plain", b"this is not json".to_vec())
    })
    .await;
    let sse = "data: {\"id\":\"served\",\"choices\":[{\"delta\":{\"content\":\"from-success\"}}]}\n\n\
               data: [DONE]\n\n"
        .to_string();
    let sse_for_mock = sse.clone();
    let (success_url, success_log) =
        spawn_mock_upstream(move |_| MockReply::Stream(sse_for_mock.clone())).await;

    let mut config = config_with_key(port);
    config.providers.push(upstream_provider(
        "failing",
        "Failing Provider",
        &failing_url,
        "sk",
        Some("remote-failing"),
    ));
    config.providers.push(upstream_provider(
        "success",
        "Success Provider",
        &success_url,
        "sk",
        Some("remote-success"),
    ));
    config.model_prices = vec![priced_with_provider(
        "success",
        "remote-success",
        1.0,
        0.0,
        0.0,
        2.0,
    )];
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    const REQUESTS: u32 = 24;
    let mut failed_first_observed = false;
    for index in 0..REQUESTS {
        let local_model = format!("local-ac005-{index}");
        let failing_before = failing_log.lock().unwrap().len() as u32;
        let success_before = success_log.lock().unwrap().len() as u32;
        let (status, content_type, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key")],
            Some(json!({"model": local_model, "stream": true})),
        )
        .await;
        assert_eq!(status, 200, "request {index} must stream: {text}");
        assert!(
            content_type.contains("text/event-stream"),
            "the served candidate must answer SSE: {content_type}"
        );
        assert!(
            text.contains("from-success"),
            "the client must receive the serving candidate's stream: {text}"
        );
        let failing_attempts = failing_log.lock().unwrap().len() as u32 - failing_before;
        let success_attempts = success_log.lock().unwrap().len() as u32 - success_before;
        assert_eq!(success_attempts, 1, "the serving provider is contacted once");
        assert!(
            failing_attempts <= 1,
            "the rejected candidate is never retried after the success"
        );

        let rows = wait_for_model_usage_logs(&local_model, 1 + failing_attempts).await;
        let success_row = rows
            .iter()
            .find(|record| record.provider_id == "success")
            .expect("the serving provider must own a row");
        assert!(success_row.terminal, "the served stream is the terminal row");
        assert_eq!(success_row.result, UsageResult::Success);
        assert_eq!(success_row.status, 200);
        assert_eq!(success_row.local_model, local_model);
        assert_eq!(success_row.upstream_model, "remote-success");
        assert_eq!(success_row.error_message, None);
        assert_eq!(success_row.total_tokens, 0);
        assert_eq!(rows.iter().filter(|record| record.terminal).count(), 1);

        if failing_attempts == 1 {
            failed_first_observed = true;
            let failure_row = rows
                .iter()
                .find(|record| record.provider_id == "failing")
                .expect("the rejected stream must own a row");
            assert!(
                !failure_row.terminal,
                "the rejected stream is not the request's terminal row"
            );
            assert_eq!(failure_row.result, UsageResult::Failure);
            assert_eq!(
                failure_row.status, 502,
                "a 2xx body that is not a valid SSE stream keeps the gateway stream failure status"
            );
            assert_eq!(failure_row.local_model, local_model);
            assert_eq!(failure_row.upstream_model, "remote-failing");
            assert_eq!(
                failure_row.error_message.as_deref(),
                Some("this is not json"),
                "the rejected body yields its readable summary"
            );
            assert_eq!(failure_row.total_tokens, 0);
            assert!(failure_row.duration_ms >= 1);
            assert_eq!(
                rows[0].provider_id, "success",
                "a newest-first query lists the terminal row before the earlier attempt"
            );
        } else {
            assert!(
                !rows.iter().any(|record| record.provider_id == "failing"),
                "the rejected candidate was never contacted and owns no row"
            );
        }
    }
    assert!(
        failed_first_observed,
        "the randomized candidate order must have tried the failing provider first at least once"
    );

    let store = default_usage_store();
    let stats = store.usage_stats(&TimeRange::default(), false).unwrap();
    assert_eq!(stats.totals.request_count, REQUESTS);
    assert_eq!(stats.totals.total_tokens, 0);
    assert_eq!(stats.totals.amount, 0.0);
    assert_eq!(stats.totals.unpriced_count, 0);

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// A downstream disconnect while a successful upstream response is buffered
/// but not yet delivered discards the entire buffered log set — no success-attempt
/// row and no synthetic cancelled terminal row are persisted. The upstream body is
/// far larger than any socket buffer, so the handler is still blocked writing when
/// the client resets the connection.
#[tokio::test]
async fn attempt_buffer_discarded_on_downstream_disconnect() {
    let _home = isolated_temp_home("attempt-delivery-cancelled");
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind large-body upstream");
    let upstream_url = format!(
        "http://{}",
        listener.local_addr().expect("large-body upstream address")
    );
    let (sent_tx, sent_rx) = tokio::sync::oneshot::channel();
    let large_body = serde_json::to_vec(&json!({
        "id": "large-success",
        "choices": [],
        "filler": "x".repeat(8 * 1024 * 1024),
        "usage": {"prompt_tokens": 4, "completion_tokens": 2}
    }))
    .expect("encode the large success body");
    assert!(
        large_body.len() > 4 * 1024 * 1024,
        "the fixture must exceed any socket buffer, got {} bytes",
        large_body.len()
    );
    let upstream = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept large-body upstream");
        super::runtime_http::read_http_request(&mut stream)
            .await
            .expect("read large-body request");
        let header = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            large_body.len()
        );
        let _ = stream.write_all(header.as_bytes()).await;
        let _ = stream.write_all(&large_body).await;
        let _ = stream.flush().await;
        let _ = sent_tx.send(());
        std::future::pending::<()>().await;
    });

    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "p1",
        "Large Provider",
        &upstream_url,
        "sk",
        Some("remote-large"),
    ));
    let price = priced_with_provider("p1", "remote-large", 1.0, 0.0, 0.0, 2.0);
    let expected_amount = compute_cost(&price, &tokens(4, 0, 0, 2));
    config.model_prices = vec![price];
    super::storage::write_config(&config).expect("write relay config");

    let (client, mut handler) = spawn_handle_connection(false).await;
    let sent = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::select! {
            signal = sent_rx => signal,
            result = &mut handler => panic!("handler exited before the upstream body was sent: {result:?}"),
        }
    })
    .await
    .expect("the large upstream body must be sent");
    sent.expect("large body sent signal");
    // Let the relay finish the successful attempt and block on the downstream
    // write, which cannot complete because the client never reads.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    // SO_LINGER 0 makes the close send a reset instead of a FIN, so the blocked
    // downstream write fails immediately.
    #[allow(deprecated)]
    client
        .set_linger(Some(std::time::Duration::from_secs(0)))
        .expect("set SO_LINGER on the relay client");
    drop(client);

    let exited = tokio::time::timeout(std::time::Duration::from_secs(5), &mut handler).await;
    assert!(
        exited.is_ok(),
        "the handler must exit after the downstream reset: {exited:?}"
    );
    upstream.abort();

    // Cancelled inbound requests persist no rows (REQ-001 / AC-002).
    // The completed success attempt row AND the synthetic cancelled terminal row
    // are both discarded.
    let records = default_usage_store().all_records().unwrap_or_default();
    assert!(
        records.is_empty(),
        "a cancelled delivery must discard all buffered log rows: {} observed",
        records.len()
    );

    let stats = default_usage_store()
        .usage_stats(&TimeRange::default(), false)
        .unwrap();
    assert_eq!(
        stats.totals.request_count, 0,
        "cancelled requests contribute zero to usage statistics"
    );
    assert_eq!(stats.totals.total_tokens, 0);
    assert_eq!(stats.totals.amount, 0.0);
    assert_eq!(
        stats.totals.unpriced_count, 0,
        "no upstream model was reached"
    );
}

/// 行为规格：`usage_stats` 的 totals/buckets/models/providers 必须排除
/// `result='cancelled'` 的合成终态行。没有候选上游的 failure 行仍然计入
/// totals/models 的失败计数，但它绝不产生 `provider_id=''` 的空白服务商行。
#[test]
fn usage_stats_excludes_cancelled_and_hides_empty_provider() {
    // (1) 同一模型下混合 success、无候选 failure 与 cancelled。
    let (dir, store) = usage_store("usage-stats-cancelled-empty-provider");
    let success_at = rfc3339_millis("2026-09-15T10:00:00+08:00");
    let no_candidate_at = rfc3339_millis("2026-09-15T11:00:00+08:00");
    let cancelled_at = rfc3339_millis("2026-09-17T10:00:00+08:00");
    store
        .append_batch(
            &[
                sample_record(
                    success_at,
                    "local-a",
                    "remote-a",
                    "prov-a",
                    "Provider A",
                    UsageResult::Success,
                    Some(0.5),
                    tokens(10, 0, 0, 5),
                ),
                // 没有候选上游：请求未到达任何服务商就失败，仍是终态行。
                sample_record(
                    no_candidate_at,
                    "local-a",
                    "",
                    "",
                    "",
                    UsageResult::Failure,
                    None,
                    UsageTokens::default(),
                ),
                // 用户取消：合成终态行，绝不应进入用量统计。
                sample_record(
                    cancelled_at,
                    "local-a",
                    "",
                    "",
                    "",
                    UsageResult::Cancelled,
                    None,
                    UsageTokens::default(),
                ),
            ],
            365,
        )
        .expect("append_batch must store every row");
    assert_eq!(store.count().unwrap(), 3);

    let stats = store.usage_stats(&TimeRange::default(), false).unwrap();

    let mut failures: Vec<String> = Vec::new();
    if stats.totals.request_count != 2 {
        failures.push(format!(
            "totals.request_count = {} (expected 2: success + no-candidate failure, never the cancelled row)",
            stats.totals.request_count
        ));
    }
    if stats.totals.total_tokens != 15 {
        failures.push(format!(
            "totals.total_tokens = {} (expected 15: only the success row carries tokens)",
            stats.totals.total_tokens
        ));
    }
    if stats.totals.amount != 0.5 {
        failures.push(format!(
            "totals.amount = {} (expected 0.5: only the priced success row)",
            stats.totals.amount
        ));
    }
    if stats.totals.unpriced_count != 0 {
        failures.push(format!(
            "totals.unpriced_count = {} (expected 0: the no-candidate failure never reached an upstream model, so it is not unpriced)",
            stats.totals.unpriced_count
        ));
    }

    match stats.models.iter().find(|row| row.local_model == "local-a") {
        None => failures.push("the local-a model row is missing".to_string()),
        Some(row) => {
            if row.metrics.request_count != 2 {
                failures.push(format!(
                    "models[local-a].request_count = {} (expected 2: cancelled excluded)",
                    row.metrics.request_count
                ));
            }
            let blank: Vec<String> = row
                .providers
                .iter()
                .filter(|provider| {
                    provider.provider_id.is_empty() || provider.provider_name.is_empty()
                })
                .map(|provider| {
                    format!(
                        "id={:?} name={:?}",
                        provider.provider_id, provider.provider_name
                    )
                })
                .collect();
            if !blank.is_empty() {
                failures.push(format!(
                    "blank provider rows leaked into models[local-a].providers: {blank:?}"
                ));
            }
            let real: Vec<&super::UsageProviderRow> = row
                .providers
                .iter()
                .filter(|provider| provider.provider_id == "prov-a")
                .collect();
            if real.len() != 1 {
                failures.push(format!(
                    "models[local-a].providers has {} rows for prov-a (expected 1)",
                    real.len()
                ));
            } else if real[0].metrics.request_count != 1 {
                failures.push(format!(
                    "providers[prov-a].request_count = {} (expected 1: only the success row)",
                    real[0].metrics.request_count
                ));
            }
        }
    }

    if stats.buckets.iter().any(|bucket| bucket.label == "2026-09-17") {
        failures.push(
            "a cancelled-only UTC+8 day created a bucket (expected no 2026-09-17 bucket)"
                .to_string(),
        );
    }
    match stats
        .buckets
        .iter()
        .find(|bucket| bucket.label == "2026-09-15")
    {
        None => failures.push("the 2026-09-15 bucket is missing".to_string()),
        Some(bucket) => {
            if bucket.metrics.request_count != 2 {
                failures.push(format!(
                    "buckets[2026-09-15].request_count = {} (expected 2: cancelled excluded)",
                    bucket.metrics.request_count
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "usage_stats must exclude cancelled rows and blank providers:\n- {}",
        failures.join("\n- ")
    );
    let _ = fs::remove_dir_all(&dir);

    // (2) 只有一条 cancelled 的请求：整份统计必须完全为空。
    let (dir, only_cancelled) = usage_store("usage-stats-cancelled-only");
    only_cancelled
        .append(
            &sample_record(
                rfc3339_millis("2026-09-18T09:00:00+08:00"),
                "local-cancelled",
                "",
                "",
                "",
                UsageResult::Cancelled,
                None,
                UsageTokens::default(),
            ),
            365,
        )
        .unwrap();
    let empty = only_cancelled
        .usage_stats(&TimeRange::default(), false)
        .unwrap();

    let mut cancelled_only: Vec<String> = Vec::new();
    if empty.totals.request_count != 0 {
        cancelled_only.push(format!(
            "totals.request_count = {} (expected 0 for a cancelled-only request)",
            empty.totals.request_count
        ));
    }
    if empty.totals.total_tokens != 0 {
        cancelled_only.push(format!(
            "totals.total_tokens = {} (expected 0)",
            empty.totals.total_tokens
        ));
    }
    if !empty.models.is_empty() {
        cancelled_only.push(format!(
            "models = {:?} (a cancelled-only request must not create a model row)",
            empty
                .models
                .iter()
                .map(|row| row.local_model.clone())
                .collect::<Vec<_>>()
        ));
    }
    if !empty.buckets.is_empty() {
        cancelled_only.push(format!(
            "buckets = {:?} (a cancelled-only request must not create a bucket)",
            empty
                .buckets
                .iter()
                .map(|bucket| bucket.label.clone())
                .collect::<Vec<_>>()
        ));
    }
    assert!(
        cancelled_only.is_empty(),
        "a cancelled-only request must be invisible to usage stats:\n- {}",
        cancelled_only.join("\n- ")
    );
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Plan 20260920-gateway-session-affinity, Step 1 (RED): rule layer of session
// affinity. These behavior tests are written against the frozen interface in
// `selection.rs` (`SESSION_ID_HEADERS`, `resolve_session_id`,
// `SessionAffinityStore`, `reorder_bound_first`, `session_affinity`) which
// does not exist yet, so they fail to compile until the implementation step
// adds exactly that API. Every case uses its own local store; only the final
// boundary case touches the process-global accessor to assert it exists.
// ---------------------------------------------------------------------------

/// AC-002, AC-003, AC-004, AC-014: every accepted spelling alone resolves, the
/// full precedence order wins with all seven present, and `session-id` beats
/// `thread-id`.
#[test]
fn session_affinity_resolve_session_id_uses_precedence_order_and_every_spelling() {
    assert_eq!(
        super::selection::SESSION_ID_HEADERS,
        [
            "x-session-affinity",
            "x-opencode-session",
            "session-id",
            "session_id",
            "conversation_id",
            "thread-id",
            "x-session-id",
        ]
    );

    let spellings = [
        ("x-session-affinity", "sess-affinity"),
        ("x-opencode-session", "sess-opencode"),
        ("session-id", "sess-dash"),
        ("session_id", "sess-underscore"),
        ("conversation_id", "sess-conversation"),
        ("thread-id", "sess-thread"),
        ("x-session-id", "sess-x"),
    ];
    for (header, value) in spellings {
        let mut headers = HashMap::new();
        headers.insert(header.to_string(), value.to_string());
        assert_eq!(
            super::selection::resolve_session_id(&headers).as_deref(),
            Some(value),
            "header {header} alone must resolve"
        );
    }

    // `X-Session-Id` arrives lower-cased in the inbound map.
    let mut upper = HashMap::new();
    upper.insert("x-session-id".to_string(), "sess-upper".to_string());
    assert_eq!(
        super::selection::resolve_session_id(&upper).as_deref(),
        Some("sess-upper")
    );

    // Full precedence: all seven present carrying different values.
    let mut all = HashMap::new();
    all.insert("x-session-affinity".to_string(), "v-affinity".to_string());
    all.insert("x-opencode-session".to_string(), "v-opencode".to_string());
    all.insert("session-id".to_string(), "v-dash".to_string());
    all.insert("session_id".to_string(), "v-underscore".to_string());
    all.insert("conversation_id".to_string(), "v-conversation".to_string());
    all.insert("thread-id".to_string(), "v-thread".to_string());
    all.insert("x-session-id".to_string(), "v-x".to_string());
    assert_eq!(
        super::selection::resolve_session_id(&all).as_deref(),
        Some("v-affinity"),
        "the first header of the precedence list must win"
    );

    // `session-id` beats `thread-id` when both carry different values.
    let mut pair = HashMap::new();
    pair.insert("session-id".to_string(), "sess-root".to_string());
    pair.insert("thread-id".to_string(), "thread-other".to_string());
    assert_eq!(
        super::selection::resolve_session_id(&pair).as_deref(),
        Some("sess-root")
    );
}

/// AC-005, AC-015: empty, whitespace-only and absent values resolve to `None`;
/// an empty higher-precedence header does not block a lower one; forbidden
/// headers never resolve, with and without a known header present.
#[test]
fn session_affinity_resolve_session_id_ignores_empty_values_and_forbidden_headers() {
    assert_eq!(
        super::selection::resolve_session_id(&HashMap::new()),
        None,
        "absent headers must resolve to no session"
    );

    for value in ["", "   ", "\t\n "] {
        let mut headers = HashMap::new();
        headers.insert("session-id".to_string(), value.to_string());
        assert_eq!(
            super::selection::resolve_session_id(&headers),
            None,
            "empty/whitespace-only value {value:?} counts as absent"
        );
    }

    // An empty higher-precedence header does not block a lower one with a value.
    let mut fallthrough = HashMap::new();
    fallthrough.insert("x-session-affinity".to_string(), "   ".to_string());
    fallthrough.insert("session-id".to_string(), "sess-fallback".to_string());
    assert_eq!(
        super::selection::resolve_session_id(&fallthrough).as_deref(),
        Some("sess-fallback")
    );

    // Forbidden headers never resolve on their own.
    for header in [
        "x-codex-turn-state",
        "x-codex-window-id",
        "originator",
        "x-parent-session-id",
    ] {
        let mut headers = HashMap::new();
        headers.insert(header.to_string(), "some-value".to_string());
        assert_eq!(
            super::selection::resolve_session_id(&headers),
            None,
            "forbidden header {header} must never become the session identity"
        );
    }

    // Forbidden headers never shadow a known header either.
    let mut mixed = HashMap::new();
    mixed.insert("x-codex-turn-state".to_string(), "turn-1".to_string());
    mixed.insert("x-codex-window-id".to_string(), "win-1".to_string());
    mixed.insert("originator".to_string(), "codex".to_string());
    mixed.insert("x-parent-session-id".to_string(), "parent-1".to_string());
    mixed.insert("session-id".to_string(), "sess-real".to_string());
    assert_eq!(
        super::selection::resolve_session_id(&mixed).as_deref(),
        Some("sess-real")
    );
}

/// AC-006: bindings are keyed by session plus trimmed model; two models of one
/// session never share a binding and another session is unaffected.
#[test]
fn session_affinity_bindings_are_per_session_and_model() {
    let mut store = super::selection::SessionAffinityStore::new();

    let first_m1 =
        store.resolve_order(Some("s"), Some("m1"), || vec![provider("a"), provider("b")]);
    assert_eq!(first_m1.bound_provider_id.as_deref(), Some("a"));
    let first_m2 =
        store.resolve_order(Some("s"), Some("m2"), || vec![provider("b"), provider("a")]);
    assert_eq!(first_m2.bound_provider_id.as_deref(), Some("b"));

    assert_eq!(
        store
            .lookup("s", "m1")
            .as_ref()
            .map(|binding| binding.provider_id.as_str()),
        Some("a")
    );
    assert_eq!(
        store
            .lookup("s", "m2")
            .as_ref()
            .map(|binding| binding.provider_id.as_str()),
        Some("b")
    );

    // Resolving M1 again with a shuffled base that puts B first still returns A
    // first while leaving M2's binding alone.
    let again = store.resolve_order(Some("s"), Some("m1"), || vec![provider("b"), provider("a")]);
    let ids: Vec<&str> = again.ordered.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
    assert_eq!(again.bound_provider_id.as_deref(), Some("a"));
    assert_eq!(
        store
            .lookup("s", "m2")
            .as_ref()
            .map(|binding| binding.provider_id.as_str()),
        Some("b"),
        "reusing M1 must not disturb M2"
    );

    // A different session has no binding.
    assert_eq!(store.lookup("other", "m1"), None);
    let other = store.resolve_order(Some("other"), Some("m1"), || {
        vec![provider("b"), provider("a")]
    });
    assert_eq!(other.bound_provider_id.as_deref(), Some("b"));
}

/// AC-007 counterexample / REQ-001: the reorder helper moves only the bound
/// provider to the front; it never drops, adds or filters a candidate.
#[test]
fn session_affinity_reorder_moves_only_the_bound_provider_to_the_front() {
    let ids_of = |ordered: &[GatewayUpstreamProvider]| {
        ordered
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>()
    };

    // Bound C moves to the front; the remaining order is untouched.
    let moved = super::selection::reorder_bound_first(
        vec![provider("a"), provider("b"), provider("c")],
        Some("s"),
        Some("m"),
        Some("c"),
    );
    assert_eq!(ids_of(&moved), vec!["c", "a", "b"]);

    // Bound A leaves the list unchanged.
    let same = super::selection::reorder_bound_first(
        vec![provider("a"), provider("b"), provider("c")],
        Some("s"),
        Some("m"),
        Some("a"),
    );
    assert_eq!(ids_of(&same), vec!["a", "b", "c"]);

    // A bound id that is not in the list returns the list unchanged.
    let missing = super::selection::reorder_bound_first(
        vec![provider("a"), provider("b")],
        Some("s"),
        Some("m"),
        Some("zzz"),
    );
    assert_eq!(ids_of(&missing), vec!["a", "b"]);

    // No session, an empty or whitespace-only model, and no bound id each
    // return the list unchanged.
    for (session, model, bound) in [
        (None, Some("m"), Some("b")),
        (Some("s"), None, Some("b")),
        (Some("s"), Some(""), Some("b")),
        (Some("s"), Some("   "), Some("b")),
        (Some("s"), Some("m"), None),
    ] {
        let unchanged = super::selection::reorder_bound_first(
            vec![provider("a"), provider("b")],
            session,
            model,
            bound,
        );
        assert_eq!(
            ids_of(&unchanged),
            vec!["a", "b"],
            "session={session:?} model={model:?} bound={bound:?} must not reorder"
        );
        assert_eq!(unchanged.len(), 2, "the list must never grow or shrink");
    }

    // The list never grows or shrinks even when reordering.
    assert_eq!(moved.len(), 3);
}

/// AC-001, REQ-006 at rule level: the first resolve binds the first shuffled
/// candidate at selection time; the second resolve reuses it bound-first with
/// the remaining order untouched.
#[test]
fn session_affinity_resolve_order_binds_first_at_selection_and_reuses_it() {
    let mut store = super::selection::SessionAffinityStore::new();

    let mut shuffle_calls = 0usize;
    let first = store.resolve_order(Some("s"), Some("m"), || {
        shuffle_calls += 1;
        vec![provider("a"), provider("b"), provider("c")]
    });
    assert_eq!(shuffle_calls, 1, "shuffle must be called exactly once");
    let first_ids: Vec<&str> = first.ordered.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(first_ids, vec!["a", "b", "c"]);
    assert_eq!(first.bound_provider_id.as_deref(), Some("a"));
    assert_eq!(
        store
            .lookup("s", "m")
            .as_ref()
            .map(|binding| binding.provider_id.as_str()),
        Some("a")
    );
    assert_eq!(
        store
            .lookup("s", "m")
            .as_ref()
            .map(|binding| binding.misses),
        Some(0)
    );

    let mut second_calls = 0usize;
    let second = store.resolve_order(Some("s"), Some("m"), || {
        second_calls += 1;
        vec![provider("c"), provider("b"), provider("a")]
    });
    assert_eq!(second_calls, 1, "shuffle must be called exactly once");
    let second_ids: Vec<&str> = second.ordered.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(
        second_ids,
        vec!["a", "c", "b"],
        "bound first, remaining order untouched"
    );
    assert_eq!(second.bound_provider_id.as_deref(), Some("a"));
}

/// AC-010: two threads resolving the same session and model agree on the first
/// provider under any interleaving, and the store holds that binding.
#[test]
fn session_affinity_concurrent_resolution_selects_one_first_provider() {
    let store: Arc<Mutex<super::selection::SessionAffinityStore>> =
        Arc::new(Mutex::new(super::selection::SessionAffinityStore::new()));
    let barrier = Arc::new(std::sync::Barrier::new(2));

    let worker = |base: Vec<GatewayUpstreamProvider>| {
        let store: Arc<Mutex<super::selection::SessionAffinityStore>> = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            let mut guard = store.lock().expect("session store lock");
            guard.resolve_order(Some("s"), Some("m"), || base)
        })
    };

    let first_handle = worker(vec![provider("a"), provider("b")]);
    let second_handle = worker(vec![provider("b"), provider("a")]);
    let first = first_handle.join().expect("first worker");
    let second = second_handle.join().expect("second worker");

    let first_id = first.ordered.first().map(|item| item.id.clone());
    let second_id = second.ordered.first().map(|item| item.id.clone());
    assert_eq!(
        first_id, second_id,
        "both concurrent resolutions must return the same first provider"
    );

    let mut guard = store.lock().expect("session store lock");
    let live = guard.lookup("s", "m").expect("a binding must exist");
    assert_eq!(
        Some(live.provider_id),
        first_id,
        "the store's live binding must be the agreed first provider"
    );
}

/// AC-008 at rule level: one miss is recorded, and the second consecutive miss
/// migrates the binding to the serving provider with a zero count.
#[test]
fn session_affinity_settle_records_one_miss_and_migrates_at_the_threshold() {
    assert_eq!(super::selection::SESSION_BINDING_MISS_THRESHOLD, 2);
    let mut store = super::selection::SessionAffinityStore::new();
    let bound = store.resolve_order(Some("s"), Some("m"), || vec![provider("a"), provider("b")]);
    assert_eq!(bound.bound_provider_id.as_deref(), Some("a"));

    store.settle("s", "m", true, Some("b"));
    let after_one = store
        .lookup("s", "m")
        .expect("binding must stay A after one miss");
    assert_eq!(after_one.provider_id.as_str(), "a");
    assert_eq!(after_one.misses, 1);

    store.settle("s", "m", true, Some("b"));
    let migrated = store
        .lookup("s", "m")
        .expect("binding must migrate at the threshold");
    assert_eq!(migrated.provider_id.as_str(), "b");
    assert_eq!(migrated.misses, 0);

    // The next resolve returns B first.
    let next = store.resolve_order(Some("s"), Some("m"), || vec![provider("a"), provider("b")]);
    let ids: Vec<&str> = next.ordered.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, vec!["b", "a"]);
    assert_eq!(next.bound_provider_id.as_deref(), Some("b"));
}

/// AC-009 at rule level: a bound success clears the miss count, and a single
/// later miss never migrates.
#[test]
fn session_affinity_settle_resets_misses_on_a_bound_success() {
    let mut store = super::selection::SessionAffinityStore::new();
    store.resolve_order(Some("s"), Some("m"), || vec![provider("a"), provider("b")]);

    store.settle("s", "m", true, Some("b"));
    assert_eq!(
        store
            .lookup("s", "m")
            .as_ref()
            .map(|binding| binding.misses),
        Some(1)
    );

    store.settle("s", "m", true, Some("a"));
    let cleared = store.lookup("s", "m").expect("binding must stay A");
    assert_eq!(cleared.provider_id.as_str(), "a");
    assert_eq!(
        cleared.misses, 0,
        "a bound success must reset the miss count"
    );

    store.settle("s", "m", true, Some("b"));
    let one_miss = store.lookup("s", "m").expect("binding must stay A");
    assert_eq!(one_miss.provider_id.as_str(), "a");
    assert_eq!(
        one_miss.misses, 1,
        "a single later miss must never migrate the binding"
    );
}

/// AC-007 at rule level: when the binding was not eligible, settle rebinds to
/// the serving provider immediately with a zero miss count.
#[test]
fn session_affinity_settle_rebinds_when_the_binding_was_not_eligible() {
    let mut store = super::selection::SessionAffinityStore::new();
    store.resolve_order(Some("s"), Some("m"), || vec![provider("a"), provider("b")]);

    store.settle("s", "m", false, Some("c"));
    let rebound = store
        .lookup("s", "m")
        .expect("rebind must replace the binding");
    assert_eq!(rebound.provider_id.as_str(), "c");
    assert_eq!(rebound.misses, 0);
}

/// AC-011 at rule level: a request without a terminal outcome changes nothing,
/// and settle never creates an absent binding.
#[test]
fn session_affinity_settle_ignores_a_request_without_a_terminal_outcome() {
    let mut store = super::selection::SessionAffinityStore::new();
    store.resolve_order(Some("s"), Some("m"), || vec![provider("a"), provider("b")]);
    store.settle("s", "m", true, Some("b"));
    assert_eq!(
        store
            .lookup("s", "m")
            .as_ref()
            .map(|binding| binding.misses),
        Some(1)
    );

    // A cancelled request (no terminal upstream outcome) is a no-op.
    store.settle("s", "m", true, None);
    let unchanged = store
        .lookup("s", "m")
        .expect("binding must survive a cancelled request");
    assert_eq!(unchanged.provider_id.as_str(), "a");
    assert_eq!(unchanged.misses, 1);

    // Settle on an absent binding creates nothing.
    let mut fresh = super::selection::SessionAffinityStore::new();
    fresh.settle("absent", "m", true, Some("a"));
    assert_eq!(fresh.lookup("absent", "m"), None);
    fresh.settle("absent", "m", false, Some("a"));
    assert_eq!(fresh.lookup("absent", "m"), None);
    assert_eq!(fresh.live_len(), 0);
}

/// AC-012: a binding idle for more than the timeout behaves as absent; exactly
/// the timeout of idleness is still live.
#[tokio::test(start_paused = true)]
async fn session_affinity_expired_binding_behaves_as_absent() {
    assert_eq!(
        super::selection::SESSION_BINDING_IDLE_TIMEOUT,
        std::time::Duration::from_secs(30 * 60)
    );

    // Boundary: exactly the timeout of idleness is still live. A separate store
    // keeps its lookup refresh from affecting the expiry case below.
    {
        let mut store = super::selection::SessionAffinityStore::new();
        let bound = store.resolve_order(Some("s-boundary"), Some("m"), || {
            vec![provider("a"), provider("b")]
        });
        assert_eq!(bound.bound_provider_id.as_deref(), Some("a"));
        tokio::time::advance(super::selection::SESSION_BINDING_IDLE_TIMEOUT).await;
        let live = store.lookup("s-boundary", "m");
        assert_eq!(
            live.as_ref().map(|binding| binding.provider_id.as_str()),
            Some("a"),
            "exactly the idle timeout must still be live"
        );
    }

    // Expired: timeout plus one second behaves as absent.
    let mut store = super::selection::SessionAffinityStore::new();
    let bound = store.resolve_order(Some("s"), Some("m"), || vec![provider("a"), provider("b")]);
    assert_eq!(bound.bound_provider_id.as_deref(), Some("a"));
    tokio::time::advance(
        super::selection::SESSION_BINDING_IDLE_TIMEOUT + std::time::Duration::from_secs(1),
    )
    .await;
    let mut shuffle_calls = 0usize;
    let next = store.resolve_order(Some("s"), Some("m"), || {
        shuffle_calls += 1;
        vec![provider("b"), provider("a")]
    });
    assert_eq!(shuffle_calls, 1);
    let ids: Vec<&str> = next.ordered.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["b", "a"],
        "an expired binding must not reorder the shuffled order"
    );
    assert_eq!(next.bound_provider_id.as_deref(), Some("b"));
    let live = store
        .lookup("s", "m")
        .expect("the new selection must bind B");
    assert_eq!(live.provider_id.as_str(), "b");
    assert_eq!(live.misses, 0);
}

/// AC-012: the table holds at most the capacity and evicts the least recently
/// used entry; a refreshed entry survives while the untouched one is gone.
#[test]
fn session_affinity_lru_evicts_the_least_recently_used_binding() {
    assert_eq!(super::selection::SESSION_BINDING_CAPACITY, 1024);
    let mut store = super::selection::SessionAffinityStore::new();

    for index in 0..super::selection::SESSION_BINDING_CAPACITY {
        let session = format!("s-{index:04}");
        let bound = store.resolve_order(Some(&session), Some("m"), || vec![provider("p")]);
        assert_eq!(bound.bound_provider_id.as_deref(), Some("p"));
    }
    assert_eq!(store.live_len(), super::selection::SESSION_BINDING_CAPACITY);

    // Refresh the oldest entry so it is no longer the least recently used.
    assert!(
        store.lookup("s-0000", "m").is_some(),
        "the oldest entry must be live before refresh"
    );

    // One more insert evicts exactly one entry.
    let evicting = store.resolve_order(Some("s-new"), Some("m"), || vec![provider("p")]);
    assert_eq!(evicting.bound_provider_id.as_deref(), Some("p"));
    assert_eq!(
        store.live_len(),
        super::selection::SESSION_BINDING_CAPACITY,
        "the table must hold at most the capacity"
    );

    assert!(
        store.lookup("s-0000", "m").is_some(),
        "the refreshed entry must still be live"
    );
    assert_eq!(
        store.lookup("s-0001", "m"),
        None,
        "the least recently used untouched entry must be evicted"
    );
    // The evicted entry behaves as absent: resolving it binds fresh.
    let rebound = store.resolve_order(Some("s-0001"), Some("m"), || {
        vec![provider("q"), provider("p")]
    });
    assert_eq!(rebound.bound_provider_id.as_deref(), Some("q"));
    assert_eq!(
        store
            .lookup("s-0001", "m")
            .as_ref()
            .map(|binding| binding.provider_id.as_str()),
        Some("q")
    );
    assert!(
        store.lookup("s-new", "m").is_some(),
        "the new entry must be live"
    );
}

/// AC-005 boundary, REQ-003: absent or blank sessions/models read and write
/// nothing; a request without a session never reuses a binding; a padded model
/// shares its trimmed form's single binding. Also asserts the process-global
/// accessor exists without depending on its state.
#[test]
fn session_affinity_ignores_models_and_headers_that_are_absent_or_blank() {
    let mut store = super::selection::SessionAffinityStore::new();

    for (session, model) in [
        (None, Some("m")),
        (Some("s"), None),
        (Some("s"), Some("")),
        (Some("s"), Some("   ")),
    ] {
        let mut shuffle_calls = 0usize;
        let order = store.resolve_order(session, model, || {
            shuffle_calls += 1;
            vec![provider("a"), provider("b")]
        });
        assert_eq!(
            shuffle_calls, 1,
            "shuffle must still be called exactly once"
        );
        let ids: Vec<&str> = order.ordered.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
        assert_eq!(
            order.bound_provider_id, None,
            "session={session:?} model={model:?} must not bind"
        );
    }
    assert_eq!(store.live_len(), 0, "blank requests must write nothing");

    // An empty shuffled list writes no binding and reports none.
    let empty = store.resolve_order(Some("s"), Some("m"), Vec::new);
    assert!(empty.ordered.is_empty());
    assert_eq!(empty.bound_provider_id, None);
    assert_eq!(store.live_len(), 0);

    // A request without a session header never reuses an existing binding.
    let bound = store.resolve_order(Some("s"), Some("m"), || vec![provider("a"), provider("b")]);
    assert_eq!(bound.bound_provider_id.as_deref(), Some("a"));
    let mut anonymous_calls = 0usize;
    let anonymous = store.resolve_order(None, Some("m"), || {
        anonymous_calls += 1;
        vec![provider("b"), provider("a")]
    });
    assert_eq!(anonymous_calls, 1);
    let anonymous_ids: Vec<&str> = anonymous
        .ordered
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(
        anonymous_ids,
        vec!["b", "a"],
        "a session-less request must keep the shuffled order"
    );
    assert_eq!(anonymous.bound_provider_id, None);

    // A whitespace-padded model resolves to the same single binding.
    let padded = store.resolve_order(Some("s"), Some("  m  "), || {
        vec![provider("b"), provider("a")]
    });
    let padded_ids: Vec<&str> = padded.ordered.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(padded_ids, vec!["a", "b"]);
    assert_eq!(padded.bound_provider_id.as_deref(), Some("a"));
    assert_eq!(
        store.live_len(),
        1,
        "a padded model must not create a second binding"
    );

    // The process-global store accessor exists; this test never depends on its
    // state, so it only locks and drops it.
    drop(
        super::selection::session_affinity()
            .lock()
            .expect("the process-global session affinity store must lock"),
    );
}

// ---------------------------------------------------------------------------
// Plan 20260920-gateway-session-affinity, Step 2 (RED): runtime sticky
// selection. Task-001 delivered the rule layer in `selection.rs`; the
// production wiring in `runtime_http.rs` does NOT exist yet:
// `handle_connection` still orders candidates with `shuffled_candidates` and
// never consults the binding store, and nothing settles a binding after the
// terminal outcome. The binding assertions below therefore fail (the store
// stays empty) while the preserved-semantics assertions already hold.
// Every runtime case holds the serialized `temp_home` lock and uses a unique
// session value containing the test name.
// ---------------------------------------------------------------------------

/// Send a complete request over a real loopback connection with custom inbound
/// headers. A NEW variant of `spawn_handle_connection` (which is left
/// untouched): the cancelled-request case needs a session header on the raw
/// handler path.
async fn spawn_handle_connection_with_headers(
    extra_headers: &[(&str, &str)],
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
    let mut client = TcpStream::connect(addr)
        .await
        .expect("connect relay loopback");
    let server = accept.await.expect("relay accept task");
    let handler = tokio::spawn(super::runtime_http::handle_connection(server));
    tokio::task::yield_now().await;
    let body = serde_json::to_vec(&json!({"model": "local", "stream": wants_stream}))
        .expect("encode relay request");
    let mut request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: Bearer local-key\r\ncontent-type: application/json\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (name, value) in extra_headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("connection: close\r\n\r\n");
    let mut frame = request.into_bytes();
    frame.extend_from_slice(&body);
    client
        .write_all(&frame)
        .await
        .expect("write complete relay request");
    client.flush().await.expect("flush relay request");
    (client, handler)
}

/// Read the mock's self-identifying `id` from a caller-visible response body.
fn session_response_id(text: &str) -> Option<String> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

/// Robust lock accessor for the process-global session affinity store.
/// Recovers from a poisoned mutex so each failing test reports its own
/// assertion instead of a cascading PoisonError.
fn affinity_lock() -> std::sync::MutexGuard<'static, super::selection::SessionAffinityStore> {
    super::selection::session_affinity()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// AC-001, AC-002, AC-003, AC-014 at runtime level: two providers A and B are
/// both eligible for model `local`. The first request of a session binds the
/// shuffled-first provider; every later request of the same session and model
/// is served by that same provider again. Each accepted spelling
/// (`x-session-affinity`, `session-id`, `x-opencode-session`, `X-Session-Id`)
/// establishes the binding the same way.
#[tokio::test]
async fn session_affinity_session_requests_reuse_the_bound_provider() {
    let home = temp_home("session-affinity-reuse");
    *affinity_lock() = super::selection::SessionAffinityStore::new();
    let port = free_port().await;
    let (a_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "a"}))).await;
    let (b_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "b"}))).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a_url,
        "sk-a",
        Some("remote-default"),
    ));
    config.providers.push(upstream_provider(
        "b",
        "Provider B",
        &b_url,
        "sk-b",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let mut failures: Vec<String> = Vec::new();
    let spellings: [(&str, &str, &str); 4] = [
        (
            "x-session-affinity",
            "session-affinity-reuse-affinity",
            "AC-003 x-session-affinity",
        ),
        ("session-id", "session-affinity-reuse-session-id", "AC-002 session-id"),
        (
            "x-opencode-session",
            "session-affinity-reuse-opencode",
            "AC-014 x-opencode-session",
        ),
        (
            "X-Session-Id",
            "session-affinity-reuse-x-session-id",
            "AC-003 X-Session-Id",
        ),
    ];
    for (header, session, label) in spellings {
        let (status, _, first_text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key"), (header, session)],
            Some(json!({"model": "local"})),
        )
        .await;
        if status != 200 {
            failures.push(format!(
                "{label}: first request status={status} body={first_text}"
            ));
            continue;
        }
        let first_id = session_response_id(&first_text);
        if first_id.is_none() {
            failures.push(format!(
                "{label}: first response carries no provider id: {first_text}"
            ));
            continue;
        }
        let mut stray = 0usize;
        for _ in 0..8 {
            let (status, _, text) = call_gateway(
                port,
                "POST",
                "/v1/chat/completions",
                &[("authorization", "Bearer local-key"), (header, session)],
                Some(json!({"model": "local"})),
            )
            .await;
            if status != 200 || session_response_id(&text) != first_id {
                stray += 1;
            }
        }
        if stray != 0 {
            failures.push(format!(
                "{label}: {stray}/8 follow-up requests left the bound provider {:?}",
                first_id.as_deref().unwrap_or("<none>")
            ));
        }
        let bound = affinity_lock().lookup(session, "local");
        match bound {
            Some(binding) if Some(binding.provider_id.as_str()) == first_id.as_deref() => {}
            other => failures.push(format!(
                "{label}: expected a live binding to {:?}, got {other:?}",
                first_id.as_deref().unwrap_or("<none>")
            )),
        }
    }

    super::runtime_http::stop_server().await.unwrap();
    assert!(
        failures.is_empty(),
        "session requests must reuse the bound provider:\n- {}",
        failures.join("\n- ")
    );
    drop(home);
}

/// AC-007 at runtime level: session S binds to whichever provider serves
/// request 1; that provider then stops being an eligible candidate, so request
/// 2 is served by the other provider with no attempt on the former one, the
/// binding is replaced, and request 3 attempts the new binding first again.
#[tokio::test]
async fn session_affinity_ineligible_binding_is_rebound_without_attempting_the_former_provider() {
    let home = temp_home("session-affinity-rebind");
    *affinity_lock() = super::selection::SessionAffinityStore::new();
    let session = "session-affinity-rebind-no-longer-eligible";
    let port = free_port().await;
    let (a_url, a_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "a"}))).await;
    let (b_url, b_log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "b"}))).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a_url,
        "sk-a",
        Some("remote-default"),
    ));
    config.providers.push(upstream_provider(
        "b",
        "Provider B",
        &b_url,
        "sk-b",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    // Request 1 binds S to whichever provider the shuffle placed first; both
    // initial bindings are handled symmetrically below.
    let (status, _, first_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 1 must be served: {first_text}");
    let first_id =
        session_response_id(&first_text).expect("request 1 must identify its provider");
    assert!(
        first_id == "a" || first_id == "b",
        "request 1 must be served by a known provider: {first_text}"
    );
    let (former_id, other_id) = if first_id == "a" { ("a", "b") } else { ("b", "a") };
    let (former_log, other_log) = if first_id == "a" {
        (&a_log, &b_log)
    } else {
        (&b_log, &a_log)
    };
    let former_calls = former_log.lock().expect("former log").len();

    // The bound provider stops being eligible; the other one stays eligible.
    let mut narrowed = super::storage::read_config().expect("read relay config");
    for provider in narrowed.providers.iter_mut() {
        if provider.id == former_id {
            provider.enabled = false;
        }
    }
    super::storage::write_config(&narrowed).unwrap();

    // Request 2: the other provider serves and the former one sees no request.
    let before_request2 = default_usage_store()
        .all_records()
        .unwrap_or_default()
        .len();
    let (status, _, second_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 2 must be served: {second_text}");
    assert_eq!(
        session_response_id(&second_text).as_deref(),
        Some(other_id),
        "request 2 must be served by the remaining provider: {second_text}"
    );
    assert_eq!(
        former_log.lock().expect("former log").len(),
        former_calls,
        "the ineligible former binding must produce no attempt"
    );
    // Wait for request 2's usage row so the settle side-effect has landed.
    wait_for_usage_logs((before_request2 + 1) as u32).await;
    let rebound = affinity_lock()
        .lookup(session, "local")
        .expect("the binding must be replaced by the serving provider");
    assert_eq!(
        rebound.provider_id.as_str(),
        other_id,
        "the binding must be replaced with zero misses"
    );
    assert_eq!(rebound.misses, 0);
    let other_calls = other_log.lock().expect("other log").len();

    // Request 3: the replaced binding is attempted first again; the former
    // provider still sees no new request.
    let (status, _, third_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 3 must be served: {third_text}");
    assert_eq!(
        session_response_id(&third_text).as_deref(),
        Some(other_id),
        "request 3 must still be served by the rebound provider: {third_text}"
    );
    assert!(
        other_log.lock().expect("other log").len() > other_calls,
        "the rebound provider must be attempted again"
    );
    assert_eq!(
        former_log.lock().expect("former log").len(),
        former_calls,
        "the former provider must still see no new request"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-008 at runtime level: the binding is deterministic to A first (B's
/// mapping for `local` is disabled while request 1 binds). After B is
/// re-enabled and A answers 429, requests 2 and 3 still attempt A first with B
/// serving; after request 3 the binding is B, so request 4 attempts B first
/// and A's attempt count stays exactly 2.
#[tokio::test]
async fn session_affinity_migrates_after_two_consecutive_misses() {
    let home = temp_home("session-affinity-migrate");
    *affinity_lock() = super::selection::SessionAffinityStore::new();
    let session = "session-affinity-migrate-two-misses";
    let port = free_port().await;
    let (a200_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "a"}))).await;
    let (b_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "b"}))).await;

    // Phase 1: only A can serve `local`, so request 1 binds S to A.
    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a200_url,
        "sk-a",
        Some("remote-default"),
    ));
    let mut b = upstream_provider("b", "Provider B", &b_url, "sk-b", None);
    let mut disabled = mapping("local", "remote-b", None);
    disabled.enabled = false;
    b.mappings = vec![disabled];
    config.providers.push(b);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, first_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 1 must be served: {first_text}");
    assert_eq!(
        session_response_id(&first_text).as_deref(),
        Some("a"),
        "request 1 must bind S to A: {first_text}"
    );

    // Phase 2: B is re-enabled and A answers 429 (fast retry header) while B
    // answers 200. A runs on a dedicated counting mock so request 1 is not
    // part of the attempt count below.
    let (a429_url, a429_count) = spawn_header_sequence_mock(vec![
        HeaderReply::new(429, json!({"error": {"message": "slow down"}}))
            .header("retry-after-ms", "0"),
    ])
    .await;
    let mut limited = GatewayConfig::default();
    limited.port = port;
    limited.keys.push(key_named("k1", "local-key"));
    limited.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a429_url,
        "sk-a",
        Some("remote-default"),
    ));
    limited.providers.push(upstream_provider(
        "b",
        "Provider B",
        &b_url,
        "sk-b",
        Some("remote-default"),
    ));
    super::storage::write_config(&limited).unwrap();

    let before_loop = default_usage_store()
        .all_records()
        .unwrap_or_default()
        .len();
    for (request_no, expected_a) in [(2u32, 1usize), (3u32, 2usize)] {
        let (status, _, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[
                ("authorization", "Bearer local-key"),
                ("x-session-affinity", session),
            ],
            Some(json!({"model": "local"})),
        )
        .await;
        assert_eq!(status, 200, "request {request_no} must be served: {text}");
        assert_eq!(
            session_response_id(&text).as_deref(),
            Some("b"),
            "request {request_no} must be served by B: {text}"
        );
        assert_eq!(
            a429_count.load(Ordering::SeqCst),
            expected_a,
            "request {request_no} must still attempt A first"
        );
    }
    // Wait for requests 2 and 3 usage rows so the settle side-effect has landed.
    // Each request: A fails (429) + B succeeds → 2 rows per request, 4 total.
    wait_for_usage_logs((before_loop + 4) as u32).await;
    let migrated = affinity_lock()
        .lookup(session, "local")
        .expect("a binding must exist after two misses");
    assert_eq!(
        migrated.provider_id.as_str(),
        "b",
        "the second consecutive miss must migrate the binding to B"
    );
    assert_eq!(migrated.misses, 0);

    // Request 4 attempts B first and adds no further A request.
    let (status, _, fourth_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 4 must be served: {fourth_text}");
    assert_eq!(
        session_response_id(&fourth_text).as_deref(),
        Some("b"),
        "request 4 must be served by B: {fourth_text}"
    );
    assert_eq!(
        a429_count.load(Ordering::SeqCst),
        2,
        "request 4 must not add another A attempt (exactly 2 for requests 2 and 3)"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-009 at runtime level: bound to A deterministically, then A fails and B
/// serves (miss 1), A answers 200 (count resets), A fails and B serves again
/// (miss 1, so the binding stays A), and the next request attempts A first.
/// A's scripted reply sequence makes the per-request answers observable.
#[tokio::test]
async fn session_affinity_bound_success_resets_consecutive_misses() {
    let home = temp_home("session-affinity-reset");
    *affinity_lock() = super::selection::SessionAffinityStore::new();
    let session = "session-affinity-reset-on-bound-success";
    let port = free_port().await;
    let (a200_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "a"}))).await;
    let (b_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "b"}))).await;

    // Phase 1: only A can serve `local`, so request 1 binds S to A.
    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a200_url,
        "sk-a",
        Some("remote-default"),
    ));
    let mut b = upstream_provider("b", "Provider B", &b_url, "sk-b", None);
    let mut disabled = mapping("local", "remote-b", None);
    disabled.enabled = false;
    b.mappings = vec![disabled];
    config.providers.push(b);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, first_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 1 must be served: {first_text}");
    assert_eq!(
        session_response_id(&first_text).as_deref(),
        Some("a"),
        "request 1 must bind S to A: {first_text}"
    );

    // Phase 2: A's answers change per request (429, 200, 429, then 200); B
    // always answers 200. Both mocks count, so the observed provider and the
    // attempt counts are asserted together.
    let (a_seq_url, a_count) = spawn_header_sequence_mock(vec![
        HeaderReply::new(429, json!({"error": {"message": "slow down"}}))
            .header("retry-after-ms", "0"),
        HeaderReply::new(200, json!({"id": "a"})),
        HeaderReply::new(429, json!({"error": {"message": "slow down"}}))
            .header("retry-after-ms", "0"),
        HeaderReply::new(200, json!({"id": "a"})),
    ])
    .await;
    let (b_seq_url, b_count) =
        spawn_header_sequence_mock(vec![HeaderReply::new(200, json!({"id": "b"}))])
            .await;
    let mut scripted = GatewayConfig::default();
    scripted.port = port;
    scripted.keys.push(key_named("k1", "local-key"));
    scripted.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a_seq_url,
        "sk-a",
        Some("remote-default"),
    ));
    scripted.providers.push(upstream_provider(
        "b",
        "Provider B",
        &b_seq_url,
        "sk-b",
        Some("remote-default"),
    ));
    super::storage::write_config(&scripted).unwrap();

    let before_requests = default_usage_store()
        .all_records()
        .unwrap_or_default()
        .len();
    // Request 2: A fails (429), B serves — miss 1.
    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 2 must be served: {text}");
    assert_eq!(
        session_response_id(&text).as_deref(),
        Some("b"),
        "request 2 must be served by B after A answers 429: {text}"
    );
    assert_eq!(
        a_count.load(Ordering::SeqCst),
        1,
        "request 2 must attempt A first"
    );

    // Request 3: A answers 200 — the bound success resets the count.
    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 3 must be served: {text}");
    assert_eq!(
        session_response_id(&text).as_deref(),
        Some("a"),
        "request 3 must be served by A: {text}"
    );
    assert_eq!(
        a_count.load(Ordering::SeqCst),
        2,
        "request 3 must attempt A first"
    );

    // Request 4: A fails again, B serves — miss 1, so the binding stays A.
    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 4 must be served: {text}");
    assert_eq!(
        session_response_id(&text).as_deref(),
        Some("b"),
        "request 4 must be served by B after A answers 429: {text}"
    );
    assert_eq!(
        a_count.load(Ordering::SeqCst),
        3,
        "request 4 must attempt A first"
    );
    // Wait for requests 2-4 usage rows so the settle side-effect has landed.
    // Request 2: A fails + B serves (2 rows); request 3: A succeeds (1 row);
    // request 4: A fails + B serves (2 rows) → 5 new rows.
    wait_for_usage_logs((before_requests + 5) as u32).await;
    let still_bound = affinity_lock()
        .lookup(session, "local")
        .expect("the binding must still exist after a single later miss");
    assert_eq!(
        still_bound.provider_id.as_str(),
        "a",
        "one miss after a reset must not migrate the binding"
    );
    assert_eq!(still_bound.misses, 1);

    // Request 5: A is attempted first again.
    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 5 must be served: {text}");
    assert_eq!(
        session_response_id(&text).as_deref(),
        Some("a"),
        "request 5 must attempt A first and be served by it: {text}"
    );
    assert_eq!(
        a_count.load(Ordering::SeqCst),
        4,
        "A must be attempted first in requests 2, 3, 4 and 5"
    );
    assert_eq!(
        b_count.load(Ordering::SeqCst),
        2,
        "B must serve exactly requests 2 and 4"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-010 at runtime level: with exactly two eligible providers and no binding
/// yet, 8 concurrent requests for the same session and model must all be
/// served by the same provider; the concurrent insert must not let two
/// requests choose two providers.
#[tokio::test]
async fn session_affinity_concurrent_session_requests_select_one_provider() {
    let home = temp_home("session-affinity-concurrent");
    *affinity_lock() = super::selection::SessionAffinityStore::new();
    let session = "session-affinity-concurrent-one-provider";
    let port = free_port().await;
    let (a_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "a"}))).await;
    let (b_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "b"}))).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a_url,
        "sk-a",
        Some("remote-default"),
    ));
    config.providers.push(upstream_provider(
        "b",
        "Provider B",
        &b_url,
        "sk-b",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let futures: Vec<_> = (0..8)
        .map(|_| {
            let session = session.to_string();
            async move {
                call_gateway(
                    port,
                    "POST",
                    "/v1/chat/completions",
                    &[
                        ("authorization", "Bearer local-key"),
                        ("x-session-affinity", session.as_str()),
                    ],
                    Some(json!({"model": "local"})),
                )
                .await
            }
        })
        .collect();
    let results = futures_util::future::join_all(futures).await;
    assert_eq!(results.len(), 8);
    for (status, _, text) in &results {
        assert_eq!(*status, 200, "every concurrent request must be served: {text}");
    }
    let ids: Vec<Option<String>> = results
        .iter()
        .map(|(_, _, text)| session_response_id(text))
        .collect();
    let first = ids[0].clone();
    assert!(
        first.as_deref() == Some("a") || first.as_deref() == Some("b"),
        "every response must identify its provider: {ids:?}"
    );
    assert!(
        ids.iter().all(|id| *id == first),
        "every concurrent response must be served by the same provider: {ids:?}"
    );
    let bound = affinity_lock()
        .lookup(session, "local")
        .expect("the concurrent insert must leave exactly one binding");
    assert_eq!(
        Some(bound.provider_id.as_str()),
        first.as_deref(),
        "the stored binding must be the agreed provider"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// AC-011 at runtime level: session S is bound to A deterministically; a
/// request driven through the raw loopback handler is cancelled while A holds
/// it. Two follow-ups (A 429, B 200) must both attempt A first (A grows by
/// exactly 2) before the binding migrates to B on the second one — if the
/// cancellation had counted as a miss, A would only be attempted once.
#[tokio::test]
async fn session_affinity_cancelled_request_does_not_count_as_a_miss() {
    let home = temp_home("session-affinity-cancel");
    *affinity_lock() = super::selection::SessionAffinityStore::new();
    let session = "session-affinity-cancel-no-miss";
    let port = free_port().await;
    let (a200_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "a"}))).await;
    let (b_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "b"}))).await;

    // Phase 1: only A can serve `local`, so request 1 binds S to A.
    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a200_url,
        "sk-a",
        Some("remote-default"),
    ));
    let mut b = upstream_provider("b", "Provider B", &b_url, "sk-b", None);
    let mut disabled = mapping("local", "remote-b", None);
    disabled.enabled = false;
    b.mappings = vec![disabled];
    config.providers.push(b);
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, first_text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", session),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "request 1 must be served: {first_text}");
    assert_eq!(
        session_response_id(&first_text).as_deref(),
        Some("a"),
        "request 1 must bind S to A: {first_text}"
    );

    // Phase 2: only A is eligible and its upstream holds the connection. The
    // binding is A so A is attempted first, which is why the held upstream is
    // A. The client disconnects while A is held, so the request is cancelled.
    let held_listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind held upstream");
    let held_url = format!("http://{}", held_listener.local_addr().expect("held addr"));
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let held = tokio::spawn(async move {
        let (mut stream, _) = held_listener.accept().await.expect("accept held upstream");
        super::runtime_http::read_http_request(&mut stream)
            .await
            .expect("read held upstream request");
        let _ = entered_tx.send(());
        let _ = release_rx.await;
        let body = br#"{"id":"late"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.write_all(body).await;
    });
    let mut held_config = GatewayConfig::default();
    held_config.port = port;
    held_config.keys.push(key_named("k1", "local-key"));
    held_config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &held_url,
        "sk-a",
        Some("remote-default"),
    ));
    super::storage::write_config(&held_config).unwrap();

    let before_cancel = default_usage_store()
        .all_records()
        .unwrap_or_default()
        .len();
    let (client, mut handler) =
        spawn_handle_connection_with_headers(&[("x-session-affinity", session)], false).await;
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
    let timeout_result = tokio::time::timeout(std::time::Duration::from_millis(500), &mut handler)
        .await;
    let join_result = timeout_result.expect("the handler must exit after the disconnect");
    let inner = join_result.expect("handle_connection must not join-err for a cancelled request");
    assert!(inner.is_ok(), "handle_connection must return Ok(()) for a cancelled request: {inner:?}");
    let _ = release_tx.send(());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), held).await;

    // Phase 3: A answers 429 while B answers 200. Both follow-ups must attempt
    // A first; the second one migrates the binding to B.
    let (a429_url, a429_count) = spawn_header_sequence_mock(vec![
        HeaderReply::new(429, json!({"error": {"message": "slow down"}}))
            .header("retry-after-ms", "0"),
    ])
    .await;
    let mut limited = GatewayConfig::default();
    limited.port = port;
    limited.keys.push(key_named("k1", "local-key"));
    limited.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a429_url,
        "sk-a",
        Some("remote-default"),
    ));
    limited.providers.push(upstream_provider(
        "b",
        "Provider B",
        &b_url,
        "sk-b",
        Some("remote-default"),
    ));
    super::storage::write_config(&limited).unwrap();

    let mut failures: Vec<String> = Vec::new();
    let after_cancel = affinity_lock().lookup(session, "local");
    match after_cancel {
        Some(binding) if binding.provider_id.as_str() == "a" && binding.misses == 0 => {}
        other => failures.push(format!(
            "the cancelled request must leave binding A with 0 misses, got {other:?}"
        )),
    }
    for request_no in [2u32, 3u32] {
        let (status, _, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[
                ("authorization", "Bearer local-key"),
                ("x-session-affinity", session),
            ],
            Some(json!({"model": "local"})),
        )
        .await;
        assert_eq!(status, 200, "request {request_no} must be served: {text}");
        assert_eq!(
            session_response_id(&text).as_deref(),
            Some("b"),
            "request {request_no} must be served by B: {text}"
        );
    }
    if a429_count.load(Ordering::SeqCst) != 2 {
        failures.push(format!(
            "both follow-ups must attempt A first (A attempts {} != 2); a counted cancellation would migrate after the first one",
            a429_count.load(Ordering::SeqCst)
        ));
    }
    // Wait for the two follow-up requests' usage rows so the settle
    // side-effects have landed before the second store lookup.
    // Each follow-up: A fails (429) + B succeeds → 2 rows, 4 total.
    wait_for_usage_logs((before_cancel + 4) as u32).await;
    match affinity_lock().lookup(session, "local") {
        Some(binding) if binding.provider_id.as_str() == "b" => {}
        other => failures.push(format!(
            "the second consecutive miss must migrate the binding to B, got {other:?}"
        )),
    }

    super::runtime_http::stop_server().await.unwrap();
    assert!(
        failures.is_empty(),
        "a cancelled request must not count as a miss:\n- {}",
        failures.join("\n- ")
    );
    drop(home);
}

/// AC-013 at runtime level: requests carrying a known session header keep the
/// existing failure semantics — (a) a first attempt answering 429 switches
/// without counting, (b) a first attempt answering 5xx switches and counts,
/// (c) a first attempt failing on the network switches and counts, (d) a
/// failure after the response body started streaming terminates with one SSE
/// error fragment and never switches. All mocks use `retry-after-ms: 0` so
/// retry-driven phases stay fast.
#[tokio::test]
async fn session_affinity_session_header_failure_paths_keep_existing_semantics() {
    let home = temp_home("session-affinity-failure-paths");
    *affinity_lock() = super::selection::SessionAffinityStore::new();
    let port = free_port().await;
    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let mut failures: Vec<String> = Vec::new();

    // (a)/(b)/(c): deterministic two-phase approach per switch phase.
    //
    // Phase A (priming): only `flaky` is eligible for model `local` (give it
    // `Some("remote-default")`; give `steady` no default model and a disabled
    // mapping). The flaky mock answers 200 with {"id": "flaky"}. One request
    // with the session header -> assert 200 served by flaky. This writes the
    // binding deterministically at selection time.
    //
    // Phase B (observed request): rewrite config so BOTH providers are eligible
    // (both with default_model = Some("remote-default")). The flaky mock fails
    // from its second request on. Exactly ONE request with the same session
    // header: assert 200 served by steady, flaky attempted exactly once,
    // steady attempted exactly once, usage rows match the per-phase error
    // semantics, health assertions hold, and the binding has exactly one miss
    // (below migration threshold).
    let switch_phases: [(u16, &str, &str, &str); 3] = [
        (429, "transient", "session-affinity-failure-429", "429"),
        (500, "boom", "session-affinity-failure-5xx", "5xx"),
        (0, "network error", "session-affinity-failure-drop", "network"),
    ];
    for (flaky_status, flaky_error, session, label) in switch_phases {
        // --- Phase A: prime the binding with flaky as sole candidate ---
        let request_count = Arc::new(AtomicUsize::new(0));
        let rc = request_count.clone();
        let (flaky_url, flaky_log) = spawn_mock_upstream(move |_| {
            if rc.fetch_add(1, Ordering::SeqCst) == 0 {
                MockReply::Json(200, json!({"id": "flaky"}))
            } else {
                match flaky_status {
                    429 => MockReply::Json(429, json!({"error": {"message": "transient"}})),
                    500 => MockReply::Json(500, json!({"error": {"message": "boom"}})),
                    _ => MockReply::Drop,
                }
            }
        })
        .await;
        let (steady_url, _steady_log_priming) =
            spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "steady"}))).await;

        let mut priming = GatewayConfig::default();
        priming.port = port;
        priming.keys.push(key_named("k1", "local-key"));
        priming.providers.push(upstream_provider(
            "flaky",
            "Flaky Provider",
            &flaky_url,
            "sk-flaky",
            Some("remote-default"),
        ));
        priming.providers.push(upstream_provider(
            "steady",
            "Steady Provider",
            &steady_url,
            "sk-steady",
            None, // no default_model -> not eligible for any model
        ));
        super::storage::write_config(&priming).unwrap();

        let flaky_before_priming = flaky_log.lock().expect("flaky log").len();
        let (status, _, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[
                ("authorization", "Bearer local-key"),
                ("x-session-affinity", session),
            ],
            Some(json!({"model": "local"})),
        )
        .await;
        let flaky_delta_priming =
            flaky_log.lock().expect("flaky log").len() - flaky_before_priming;
        assert_eq!(
            status, 200,
            "{label} priming: must serve the caller: {text}"
        );
        assert_eq!(
            session_response_id(&text).as_deref(),
            Some("flaky"),
            "{label} priming: flaky must serve (sole candidate): {text}"
        );
        assert_eq!(
            flaky_delta_priming, 1,
            "{label} priming: flaky must be attempted exactly once"
        );

        // --- Phase B: rewrite config so both are eligible, observe one request ---
        let mut both_eligible = GatewayConfig::default();
        both_eligible.port = port;
        both_eligible.keys.push(key_named("k1", "local-key"));
        both_eligible.providers.push(upstream_provider(
            "flaky",
            "Flaky Provider",
            &flaky_url,
            "sk-flaky",
            Some("remote-default"),
        ));
        both_eligible.providers.push(upstream_provider(
            "steady",
            "Steady Provider",
            &steady_url,
            "sk-steady",
            Some("remote-default"),
        ));
        super::storage::write_config(&both_eligible).unwrap();

        let before_rows = default_usage_store()
            .all_records()
            .unwrap_or_default()
            .len();
        let flaky_before = flaky_log.lock().expect("flaky log").len();
        let (status, _, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[
                ("authorization", "Bearer local-key"),
                ("x-session-affinity", session),
            ],
            Some(json!({"model": "local"})),
        )
        .await;
        let flaky_delta = flaky_log.lock().expect("flaky log").len() - flaky_before;
        assert_eq!(
            status, 200,
            "{label}: the switch must serve the caller: {text}"
        );
        assert_eq!(
            session_response_id(&text).as_deref(),
            Some("steady"),
            "{label}: the caller must see the steady provider: {text}"
        );
        assert_eq!(
            flaky_delta, 1,
            "{label}: the flaky provider must be attempted exactly once in the observed request"
        );

        // Usage rows for the observed request: one non-terminal failure
        // attempt plus the terminal success.
        // The observed request already wrote its rows; wait until they are
        // visible and take the two newest (requests run sequentially).
        let records = wait_for_usage_logs((before_rows + 2) as u32).await;
        let fresh: Vec<&UsageLogRecord> = records.iter().take(2).collect();
        assert_eq!(
            fresh.len(),
            2,
            "{label}: the observed request must write exactly two rows"
        );
        let terminal = fresh
            .iter()
            .find(|record| record.terminal)
            .expect("one terminal row");
        assert_eq!(terminal.provider_id.as_str(), "steady");
        assert_eq!(terminal.result, UsageResult::Success);
        assert_eq!(terminal.status, 200);
        let attempt = fresh
            .iter()
            .find(|record| !record.terminal)
            .expect("one non-terminal attempt row");
        assert_eq!(attempt.provider_id.as_str(), "flaky");
        assert_eq!(attempt.result, UsageResult::Failure);
        if flaky_status == 0 {
            assert_eq!(attempt.status, 0, "{label}: a network failure has no HTTP status");
            assert!(
                attempt
                    .error_message
                    .as_deref()
                    .unwrap_or("")
                    .contains(flaky_error),
                "{label}: the transport failure records its description: {:?}",
                attempt.error_message
            );
        } else {
            assert_eq!(
                attempt.status, flaky_status,
                "{label}: the attempt keeps the upstream status"
            );
            assert_eq!(
                attempt.error_message.as_deref(),
                Some(flaky_error),
                "{label}: the upstream error.message is recorded"
            );
        }

        // RequestHealth settlement: 429 never counts, 5xx and network failures
        // count exactly once (exactly one flaky-first occurrence happened).
        let stored = super::storage::read_config().expect("read relay config");
        let flaky_stored = stored
            .providers
            .iter()
            .find(|provider| provider.id == "flaky")
            .expect("flaky provider stored");
        if flaky_status == 429 {
            assert_eq!(
                flaky_stored.consecutive_failures, 0,
                "{label}: 429 must not count toward health"
            );
            assert!(
                !flaky_stored.auto_disabled,
                "{label}: 429 must not disable"
            );
        } else {
            assert_eq!(
                flaky_stored.consecutive_failures, 1,
                "{label}: the failure must count exactly once"
            );
            assert!(
                !flaky_stored.auto_disabled,
                "{label}: a single failure must not disable"
            );
        }

        // The binding for this session and model must exist, must point at
        // `flaky` (written by priming), and must have exactly one miss
        // (below the migration threshold of two consecutive misses).
        match affinity_lock().lookup(session, "local") {
            Some(binding) => {
                assert_eq!(
                    binding.provider_id, "flaky",
                    "{label}: the binding must still point at flaky after one miss"
                );
                assert_eq!(
                    binding.misses, 1,
                    "{label}: exactly one miss (below migration threshold)"
                );
            }
            None => failures.push(format!(
                "{label}: a request carrying a session header must leave a binding"
            )),
        }
    }

    // (d): a failure after the response body started streaming terminates the
    // stream with one SSE error fragment, never switches, and counts once.
    {
        let session = "session-affinity-failure-mid-stream";
        let partial = "data: {\"id\":\"partial-solo\"}\n\n";
        let (solo_url, solo_log) = spawn_mock_upstream(move |_| {
            MockReply::PartialStream(partial.to_string(), partial.len() + 500)
        })
        .await;
        let mut phase = GatewayConfig::default();
        phase.port = port;
        phase.keys.push(key_named("k1", "local-key"));
        phase.providers.push(upstream_provider(
            "solo",
            "Solo Provider",
            &solo_url,
            "sk-solo",
            Some("remote-default"),
        ));
        super::storage::write_config(&phase).unwrap();

        let before_rows = default_usage_store()
            .all_records()
            .unwrap_or_default()
            .len();
        let (status, content_type, text) = call_gateway(
            port,
            "POST",
            "/v1/chat/completions",
            &[("authorization", "Bearer local-key"), ("x-session-affinity", session)],
            Some(json!({"model": "local", "stream": true})),
        )
        .await;
        assert_eq!(
            status, 200,
            "the stream headers are already written: {text}"
        );
        assert!(
            content_type.contains("text/event-stream"),
            "mid-stream failure keeps the SSE transport: {content_type}"
        );
        assert!(
            text.contains("partial-solo"),
            "the first provider bytes reach the caller: {text}"
        );
        assert!(
            text.contains("upstream_stream_error"),
            "exactly one standalone error fragment closes the stream: {text}"
        );
        assert!(
            !text.contains("data: [DONE]"),
            "an abnormal stream must not send [DONE]: {text}"
        );
        assert_eq!(
            solo_log.lock().expect("solo log").len(),
            1,
            "the single candidate is attempted exactly once"
        );

        let records = wait_for_usage_logs((before_rows + 1) as u32).await;
        let row = records.first().expect("the mid-stream failure writes one row");
        assert!(row.terminal);
        assert_eq!(row.result, UsageResult::Failure);
        assert_eq!(row.status, 502);
        assert_eq!(row.provider_id.as_str(), "solo");
        assert!(
            row.error_message
                .as_deref()
                .unwrap_or("")
                .contains("stream failed after first byte"),
            "the mid-stream failure records its stream description: {:?}",
            row.error_message
        );

        let stored = super::storage::read_config().expect("read relay config");
        let solo_stored = stored
            .providers
            .iter()
            .find(|provider| provider.id == "solo")
            .expect("solo provider stored");
        assert_eq!(
            solo_stored.consecutive_failures, 1,
            "the mid-stream failure counts exactly once"
        );

        match affinity_lock().lookup(session, "local") {
            Some(_) => {}
            None => failures.push(
                "mid-stream: a request carrying a session header must leave a binding"
                    .to_string(),
            ),
        }
    }

    super::runtime_http::stop_server().await.unwrap();
    assert!(
        failures.is_empty(),
        "session-header failure paths must settle bindings:\n- {}",
        failures.join("\n- ")
    );
    drop(home);
}

/// AC-005 boundary at runtime level: (a) a request with no known session
/// header leaves no live bindings; (b) a request carrying a known session
/// header but a whitespace-only `model` (served via `default_model`) also
/// leaves none; (c) a no-candidate request carrying a known session header
/// still answers 502 with the standard `all_providers_unavailable` envelope
/// and leaves no binding. Never asserts which provider a request picked.
#[tokio::test]
async fn session_affinity_headerless_and_blank_model_requests_write_no_binding() {
    let home = temp_home("session-affinity-boundary");
    *affinity_lock() = super::selection::SessionAffinityStore::new();
    let port = free_port().await;
    let (a_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "a"}))).await;

    let mut config = GatewayConfig::default();
    config.port = port;
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a_url,
        "sk-a",
        Some("remote-default"),
    ));
    super::storage::write_config(&config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let live_bindings = || affinity_lock().live_len();

    // (a) No known session header: served, but nothing is read or written.
    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[("authorization", "Bearer local-key")],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "a header-less request must still be served: {text}");
    assert_eq!(
        live_bindings(),
        0,
        "a header-less request must write no binding"
    );

    // (b) Known session header but a whitespace-only model: served through the
    // default-model fallback, but the blank model is not a binding key.
    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", "session-affinity-boundary-blank"),
        ],
        Some(json!({"model": "   "})),
    )
    .await;
    assert_eq!(
        status, 200,
        "a blank-model request must still resolve via default_model: {text}"
    );
    assert_eq!(
        live_bindings(),
        0,
        "a whitespace-only model must write no binding"
    );

    // (c) Known session header but no candidate: the existing 502 path with
    // the standard envelope, and still no binding.
    //
    // Rewrite the config so provider `a` has NO default model and no enabled
    // mapping for the requested model. Without the fallback, `no-such-model`
    // has zero eligible candidates, producing the 502 path. The server
    // re-reads the config per connection, so no restart is needed.
    let mut no_fallback_config = GatewayConfig::default();
    no_fallback_config.port = port;
    no_fallback_config.keys.push(key_named("k1", "local-key"));
    no_fallback_config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a_url,
        "sk-a",
        None, // no default_model -> no fallback resolution
    ));
    super::storage::write_config(&no_fallback_config).unwrap();

    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", "session-affinity-boundary-unknown"),
        ],
        Some(json!({"model": "no-such-model"})),
    )
    .await;
    assert_eq!(status, 502, "an unknown model must be 502: {text}");
    let body = assert_standard_error_envelope(&text);
    assert_eq!(body["error"]["code"], "all_providers_unavailable");
    assert_eq!(
        live_bindings(),
        0,
        "a no-candidate request must write no binding"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}

/// Characterization test for terminal-outcome semantics (REQ-005): `settle`
/// treats the provider holding the request's terminal outcome as the "served"
/// provider. For an exhausted request where the binding was an eligible
/// candidate, the binding keeps the bound provider and records at most one miss;
/// when the binding was not eligible (REQ-004), it rebinds to the terminal
/// outcome provider with zero misses.
#[tokio::test]
async fn session_affinity_exhausted_request_keeps_the_bound_provider_and_rebinds_only_when_ineligible()
{
    let home = temp_home("session-affinity-exhausted");
    *affinity_lock() = super::selection::SessionAffinityStore::new();
    let port = free_port().await;

    // --- Bind phase: only provider `a` is a candidate for `local`. ---
    let (a_bind_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "a"}))).await;
    let (b_bind_url, _) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "b"}))).await;

    let mut bind_config = config_with_key(port);
    bind_config.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a_bind_url,
        "sk-a",
        Some("remote-default"),
    ));
    // b is not eligible: no default_model and no mapping for `local`.
    bind_config
        .providers
        .push(upstream_provider("b", "Provider B", &b_bind_url, "sk-b", None));
    super::storage::write_config(&bind_config).unwrap();
    super::runtime_http::start_server().await.unwrap();

    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", "exhausted-test"),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(status, 200, "bind phase must succeed: {text}");
    assert_eq!(
        session_response_id(&text).as_deref(),
        Some("a"),
        "bind phase must bind to provider A: {text}"
    );

    // --- Sub-case A: bound provider `a` is eligible; both `a` and `b` fail. ---
    let (a500_url, a500_log) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;
    let (b500_url, b500_log) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;

    let mut sub_a = config_with_key(port);
    sub_a.providers.push(upstream_provider(
        "a",
        "Provider A",
        &a500_url,
        "sk-a",
        Some("remote-default"),
    ));
    sub_a.providers.push(upstream_provider(
        "b",
        "Provider B",
        &b500_url,
        "sk-b",
        Some("remote-default"),
    ));
    super::storage::write_config(&sub_a).unwrap();

    let before_rows = default_usage_store()
        .all_records()
        .unwrap_or_default()
        .len();
    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", "exhausted-test"),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(
        status, 502,
        "exhausted request must return 502: {text}"
    );
    // The binding must keep `a` (terminal-outcome semantics: the bound
    // provider stays bound when it was an eligible candidate, regardless of
    // which provider held the terminal outcome).
    // Misses is 0 when `a` held the terminal outcome, 1 when another
    // provider did — both values are allowed.
    wait_for_usage_logs((before_rows + 1) as u32).await;
    match affinity_lock().lookup("exhausted-test", "local") {
        Some(binding) => {
            assert_eq!(
                binding.provider_id, "a",
                "binding must keep provider A (terminal-outcome semantics)"
            );
            assert!(
                binding.misses <= 1,
                "at most one miss when bound provider was eligible, got {}",
                binding.misses
            );
        }
        None => panic!("exhausted request with session header must leave a binding"),
    }
    // Both mocks must have been attempted (proving the request exhausted).
    assert!(
        a500_log.lock().expect("a500 log").len() >= 1,
        "provider A must be attempted at least once"
    );
    assert!(
        b500_log.lock().expect("b500 log").len() >= 1,
        "provider B must be attempted at least once (exhausted, not no-candidate)"
    );

    // --- Sub-case B: binding ineligible (no candidates for `a`); `b` fails. ---
    let (b500b_url, b500b_log) =
        spawn_mock_upstream(|_| MockReply::Json(500, json!({"error": {"message": "boom"}}))).await;

    let mut sub_b = config_with_key(port);
    // a is not eligible: no default_model and no mapping for `local`.
    sub_b
        .providers
        .push(upstream_provider("a", "Provider A", "http://127.0.0.1:1", "sk-a", None));
    sub_b.providers.push(upstream_provider(
        "b",
        "Provider B",
        &b500b_url,
        "sk-b",
        Some("remote-default"),
    ));
    super::storage::write_config(&sub_b).unwrap();

    let a_before = a500_log.lock().expect("a500 log").len();
    let (status, _, text) = call_gateway(
        port,
        "POST",
        "/v1/chat/completions",
        &[
            ("authorization", "Bearer local-key"),
            ("x-session-affinity", "exhausted-test"),
        ],
        Some(json!({"model": "local"})),
    )
    .await;
    assert_eq!(
        status, 502,
        "ineligible-binding exhausted request must return 502: {text}"
    );
    // Provider A must not have been attempted (REQ-004: no attempt for
    // the former bound provider when it is not an eligible candidate).
    assert_eq!(
        a500_log.lock().expect("a500 log").len(),
        a_before,
        "ineligible former binding must produce no attempt"
    );
    // The binding is now `b` with zero misses (rebind to terminal outcome
    // provider when bound provider was ineligible).
    wait_for_usage_logs((before_rows + 2) as u32).await;
    match affinity_lock().lookup("exhausted-test", "local") {
        Some(binding) => {
            assert_eq!(
                binding.provider_id, "b",
                "binding must be rebound to provider B (terminal outcome held B)"
            );
            assert_eq!(
                binding.misses, 0,
                "ineligible rebind must have zero misses"
            );
        }
        None => panic!("ineligible-binding exhausted request must leave a rebound binding"),
    }
    assert!(
        b500b_log.lock().expect("b500b log").len() >= 1,
        "provider B must be attempted at least once"
    );

    super::runtime_http::stop_server().await.unwrap();
    drop(home);
}
