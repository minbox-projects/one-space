//! Step 3 behavior tests for the OpenCode Zen sidecar gateway relay.
//!
//! These tests are expected to be RED until `crate::ai_gateway::zen_sidecar`
//! and the sidecar-aware forwarding hooks exist.

use super::{
    call_gateway, free_port, key_named, mapping, spawn_mock_upstream, temp_home, upstream_provider,
    MockReply,
};
use crate::ai_gateway::types_config::TransportStrategy;
use crate::ai_gateway::{forwarding, runtime_http, GatewaySettings, GatewayUpstreamProvider};
use serde_json::json;
use std::sync::Mutex;

static GLOBAL: Mutex<()> = Mutex::new(());

fn provider_sidecar(id: &str) -> GatewayUpstreamProvider {
    let mut p = upstream_provider(id, "OpenCode Zen Free", "https://opencode.ai/zen/v1", "sk-provider", Some("remote-model"));
    p.attempt_key = "sk-provider".to_string();
    p.transport = TransportStrategy::OpencodeZenSidecar;
    p.mappings.push(mapping("local-model", "remote-model", None));
    p
}

#[tokio::test]
async fn gateway_relays_through_sidecar_with_identity_headers() {
    let _g = GLOBAL.lock().unwrap();
    let home = temp_home("zen-sidecar-relay");
    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();

    let (sidecar_url, sidecar_log) = spawn_mock_upstream(|captured| {
        MockReply::JsonWithHeaders(200, json!({"ok": true}), vec![("x-custom".to_string(), "sidecar".to_string())])
    })
    .await;
    crate::ai_gateway::zen_sidecar::set_test_endpoint(&sidecar_url, "gateway-token");

    let provider = provider_sidecar("p");
    let mut client_headers = std::collections::HashMap::new();
    client_headers.insert("x-opencode-session".to_string(), "ses_x".to_string());
    client_headers.insert("x-session-affinity".to_string(), "ses_x".to_string());

    let settings = GatewaySettings::default();
    let resp = forwarding::forward_non_streaming_with_protocol(
        &provider,
        "/v1/chat/completions?trace=1",
        br#"{"model":"local-model","keep":true}"#,
        "remote-model",
        &client_headers,
        crate::ai_gateway::UpstreamProtocol::ChatCompletions,
        &settings,
    )
    .await
    .expect("forward succeeds");

    assert_eq!(resp.status, 200);
    let captured = sidecar_log.lock().unwrap();
    let c = captured.last().expect("sidecar captured");
    assert_eq!(c.method, "POST");
    assert_eq!(c.path, "/v1/chat/completions?trace=1");
    let body: serde_json::Value = serde_json::from_slice(&c.body).unwrap();
    assert_eq!(body["model"], "remote-model");
    assert_eq!(body["keep"], true);
    // identity headers
    let get = |n: &str| c.headers.get(n).map(String::as_str).unwrap_or("");
    assert_eq!(get("authorization"), "Bearer sk-provider");
    assert_eq!(get("x-opencode-session"), "ses_x");
    assert_eq!(get("x-session-affinity"), "ses_x");
    assert_eq!(get("x-onespace-sidecar-token"), "gateway-token");
    assert_eq!(get("x-onespace-upstream-base"), "https://opencode.ai/zen/v1");

    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();
    drop(home);
}

#[tokio::test]
async fn client_supplied_sidecar_headers_never_override() {
    let _g = GLOBAL.lock().unwrap();
    let home = temp_home("zen-sidecar-override");
    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();

    let (sidecar_url, sidecar_log) = spawn_mock_upstream(|_| MockReply::Json(200, json!({"ok": true}))).await;
    crate::ai_gateway::zen_sidecar::set_test_endpoint(&sidecar_url, "gateway-token");

    let provider = provider_sidecar("p");
    let mut client_headers = std::collections::HashMap::new();
    client_headers.insert("x-onespace-sidecar-token".to_string(), "evil".to_string());
    client_headers.insert("x-onespace-upstream-base".to_string(), "https://evil.example.com/zen/v1".to_string());

    let settings = GatewaySettings::default();
    let _ = forwarding::forward_non_streaming_with_protocol(
        &provider,
        "/v1/chat/completions",
        br#"{"model":"local-model"}"#,
        "remote-model",
        &client_headers,
        crate::ai_gateway::UpstreamProtocol::ChatCompletions,
        &settings,
    )
    .await;

    let captured = sidecar_log.lock().unwrap();
    let c = captured.last().unwrap();
    let get = |n: &str| c.headers.get(n).map(String::as_str).unwrap_or("");
    assert_eq!(get("x-onespace-sidecar-token"), "gateway-token");
    assert_eq!(get("x-onespace-upstream-base"), "https://opencode.ai/zen/v1");

    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();
    drop(home);
}

#[tokio::test]
async fn streaming_sidecar_response_is_incremental() {
    let _g = GLOBAL.lock().unwrap();
    let home = temp_home("zen-sidecar-stream");
    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();

    // raw TCP chunked SSE mock
    use tokio::net::TcpListener;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sidecar_url = format!("http://{}", addr);
    let notify = std::sync::Arc::new(tokio::sync::Notify::new());
    let notify_srv = notify.clone();
    tauri::async_runtime::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // read request
        let _ = crate::ai_gateway::runtime_http::read_http_request(&mut stream).await;
        let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n";
        let _ = stream.write_all(headers.as_bytes()).await;
        let _ = stream.flush().await;
        let chunk1 = "data: one\n\n";
        let _ = stream.write_all(format!("{:x}\r\n{}\r\n", chunk1.len(), chunk1).as_bytes()).await;
        let _ = stream.flush().await;
        // wait for test to read first chunk
        notify_srv.notified().await;
        let chunk2 = "data: two\n\n";
        let _ = stream.write_all(format!("{:x}\r\n{}\r\n", chunk2.len(), chunk2).as_bytes()).await;
        let _ = stream.write_all(b"0\r\n\r\n").await;
        let _ = stream.flush().await;
    });

    crate::ai_gateway::zen_sidecar::set_test_endpoint(&sidecar_url, "gateway-token");

    let provider = provider_sidecar("p");
    let client_headers = std::collections::HashMap::new();
    let settings = GatewaySettings::default();
    let resp = forwarding::open_streaming_response_with_protocol(
        &provider,
        "/v1/chat/completions",
        br#"{"model":"local-model","stream":true}"#,
        "remote-model",
        &client_headers,
        crate::ai_gateway::UpstreamProtocol::ChatCompletions,
        &settings,
    )
    .await
    .expect("open stream");
    assert_eq!(resp.status().as_u16(), 200);

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut acc = Vec::new();
    let first = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Some(chunk) = stream.next().await {
            let c = chunk.unwrap();
            acc.extend_from_slice(&c);
            if String::from_utf8_lossy(&acc).contains("data: one") {
                break;
            }
        }
    })
    .await;
    assert!(first.is_ok(), "first chunk must arrive while second withheld");
    let text = String::from_utf8_lossy(&acc);
    assert!(text.contains("data: one"));

    // notify mock to send second chunk
    notify.notify_one();

    // read until data: two
    let mut acc2 = acc.clone();
    let second = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !String::from_utf8_lossy(&acc2).contains("data: two") {
            if let Some(chunk) = stream.next().await {
                let c = chunk.unwrap();
                acc2.extend_from_slice(&c);
            } else {
                break;
            }
        }
    })
    .await;
    assert!(second.is_ok());
    assert!(String::from_utf8_lossy(&acc2).contains("data: two"));

    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();
    drop(home);
}

#[tokio::test]
async fn sidecar_429_is_relayed_unchanged() {
    let _g = GLOBAL.lock().unwrap();
    let home = temp_home("zen-sidecar-429");
    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();

    let (sidecar_url, _log) = spawn_mock_upstream(|_| {
        MockReply::JsonWithHeaders(
            429,
            json!({"error":{"type":"FreeUsageLimitError","message":"limit"}}),
            vec![("x-custom".to_string(), "sidecar".to_string())],
        )
    })
    .await;
    crate::ai_gateway::zen_sidecar::set_test_endpoint(&sidecar_url, "gateway-token");

    let provider = provider_sidecar("p");
    let client_headers = std::collections::HashMap::new();
    let settings = GatewaySettings::default();
    let resp = forwarding::forward_non_streaming_with_protocol(
        &provider,
        "/v1/chat/completions",
        br#"{"model":"local-model"}"#,
        "remote-model",
        &client_headers,
        crate::ai_gateway::UpstreamProtocol::ChatCompletions,
        &settings,
    )
    .await
    .expect("forward");

    assert_eq!(resp.status, 429);
    assert_eq!(resp.headers.get("x-custom").unwrap().to_str().unwrap(), "sidecar");
    let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(body["error"]["type"], "FreeUsageLimitError");

    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();
    drop(home);
}

#[tokio::test]
async fn missing_binary_fails_with_paths_and_never_goes_direct() {
    let _g = GLOBAL.lock().unwrap();
    let home = temp_home("zen-sidecar-missing");
    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();

    let prev = std::env::var("ONESPACE_ZEN_RELAY_BIN").ok();
    let missing = home.path.join("no-such-binary");
    std::env::set_var("ONESPACE_ZEN_RELAY_BIN", &missing);

    // ensure_started should fail naming the path
    let settings = GatewaySettings::default();
    let provider = provider_sidecar("p");
    let mut client_headers = std::collections::HashMap::new();
    let res = forwarding::forward_non_streaming_with_protocol(
        &provider,
        "/v1/chat/completions",
        br#"{"model":"local-model"}"#,
        "remote-model",
        &client_headers,
        crate::ai_gateway::UpstreamProtocol::ChatCompletions,
        &settings,
    )
    .await;
    let err = match res {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    };
    assert!(err.contains(missing.to_str().unwrap()));
    assert!(err.to_lowercase().contains("sidecar"));

    // direct upstream mock must not be hit
    // (no mock started)

    // restore env
    match prev {
        Some(v) => std::env::set_var("ONESPACE_ZEN_RELAY_BIN", v),
        None => std::env::remove_var("ONESPACE_ZEN_RELAY_BIN"),
    }
    crate::ai_gateway::zen_sidecar::stop();
    drop(home);
}

#[tokio::test]
async fn sidecar_failure_still_serves_a_direct_pool_candidate() {
    let _g = GLOBAL.lock().unwrap();
    let home = temp_home("zen-sidecar-fallback");
    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();

    let prev = std::env::var("ONESPACE_ZEN_RELAY_BIN").ok();
    let missing = home.path.join("no-such-binary");
    std::env::set_var("ONESPACE_ZEN_RELAY_BIN", &missing);

    let (direct_url, direct_log) = spawn_mock_upstream(|_| MockReply::Json(200, json!({"id":"b"}))).await;

    let mut provider_a = provider_sidecar("a");
    provider_a.id = "a".to_string();
    let mut provider_b = upstream_provider("b", "Direct", &direct_url, "sk-b", Some("remote-model"));
    provider_b.mappings.push(mapping("local-model", "remote-model", None));

    let settings = GatewaySettings::default();
    let client_headers = std::collections::HashMap::new();
    // attempt_non_streaming with both ordered
    let mut attempts = Vec::new();
    let response = runtime_http::attempt_non_streaming(
        &[provider_a, provider_b],
        "/v1/chat/completions",
        br#"{"model":"local-model"}"#,
        Some("local-model"),
        &client_headers,
        false,
        None,
        &mut attempts,
    )
    .await;
    assert_eq!(response.status, 200, "the direct candidate must serve");

    let captured = direct_log.lock().unwrap();
    assert!(!captured.is_empty(), "direct provider B should receive request");

    match prev {
        Some(v) => std::env::set_var("ONESPACE_ZEN_RELAY_BIN", v),
        None => std::env::remove_var("ONESPACE_ZEN_RELAY_BIN"),
    }
    crate::ai_gateway::zen_sidecar::stop();
    drop(home);
}

#[tokio::test]
async fn manager_spawns_restarts_and_stops() {
    let _g = GLOBAL.lock().unwrap();
    let home = temp_home("zen-sidecar-manager");
    crate::ai_gateway::zen_sidecar::clear_test_endpoint();
    crate::ai_gateway::zen_sidecar::stop();

    // write an executable shell script
    let script = home.path.join("relay.sh");
    let pidfile = home.path.join("relay.pid");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nread line\nprintf 'READY 1\\n'\nprintf '%s\\n' $$ > {}\nsleep 60\n",
            pidfile.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let prev = std::env::var("ONESPACE_ZEN_RELAY_BIN").ok();
    std::env::set_var("ONESPACE_ZEN_RELAY_BIN", &script);

    let settings = GatewaySettings::default();
    let _started = crate::ai_gateway::zen_sidecar::ensure_started(&settings).await.expect("start");
    assert!(crate::ai_gateway::zen_sidecar::is_running());

    // poll pid file up to 2s
    let mut pid1 = String::new();
    for _ in 0..20 {
        if let Ok(s) = std::fs::read_to_string(&pidfile) {
            let t = s.trim().to_string();
            if !t.is_empty() {
                pid1 = t;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(!pid1.is_empty());

    // kill pid1
    let _ = std::process::Command::new("kill").arg(&pid1).status();
    // wait until is_running false or 2s
    for _ in 0..20 {
        if !crate::ai_gateway::zen_sidecar::is_running() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let _started2 = crate::ai_gateway::zen_sidecar::ensure_started(&settings).await.expect("restart");
    assert!(crate::ai_gateway::zen_sidecar::is_running());

    // poll pid file until pid != pid1 up to 2s
    let mut pid2 = String::new();
    for _ in 0..20 {
        if let Ok(s) = std::fs::read_to_string(&pidfile) {
            let t = s.trim().to_string();
            if !t.is_empty() && t != pid1 {
                pid2 = t;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(!pid2.is_empty());
    assert_ne!(pid2, pid1);

    crate::ai_gateway::zen_sidecar::stop();
    // assert pid2 gone via kill -0 polling
    for _ in 0..20 {
        let status = std::process::Command::new("kill").arg("-0").arg(&pid2).status();
        if status.map(|s| !s.success()).unwrap_or(true) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let status = std::process::Command::new("kill").arg("-0").arg(&pid2).status();
    assert!(status.map(|s| !s.success()).unwrap_or(true));
    assert!(!crate::ai_gateway::zen_sidecar::is_running());

    match prev {
        Some(v) => std::env::set_var("ONESPACE_ZEN_RELAY_BIN", v),
        None => std::env::remove_var("ONESPACE_ZEN_RELAY_BIN"),
    }
    drop(home);
}
