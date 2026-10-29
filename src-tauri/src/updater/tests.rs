use super::*;
use crate::config::ProxyConfig;
use crate::proxy::test_support::HttpProxyRecorder;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri_plugin_updater::Update;

const TEST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDEzMzlGOThEMzM2MzlFOEIKUldTTG5tTXpqZms1RTdmL1pzSVJGR1N4MnBocklYUTJzeVlSdktKb1JaZzJpUkFuakpDVytjbkYK";

fn proxy_config(port: u16) -> ProxyConfig {
    ProxyConfig {
        proxy_enabled: true,
        proxy_type: "http".to_string(),
        proxy_host: "127.0.0.1".to_string(),
        proxy_port: port,
        proxy_username: Some(crate::proxy::test_support::SAFE_FIXTURE_USER.to_string()),
        proxy_password: Some(crate::proxy::test_support::SAFE_FIXTURE_PASS.to_string()),
        check_interval: 15,
    }
}

fn recorder_port(recorder: &HttpProxyRecorder) -> u16 {
    recorder
        .url()
        .rsplit(':')
        .next()
        .expect("recorder URL has a port")
        .parse()
        .expect("recorder URL port is numeric")
}

fn publish_recorder(recorder: &HttpProxyRecorder) {
    let manager = crate::proxy::init_proxy_manager();
    manager.publish(
        manager
            .prepare(Some(proxy_config(recorder_port(recorder))))
            .expect("prepare recorder proxy"),
    );
}

fn publish_direct() {
    let manager = crate::proxy::init_proxy_manager();
    manager.publish(manager.prepare(None).expect("prepare direct transport"));
}

fn invalid_signature() -> String {
    let public_key_text = String::from_utf8(STANDARD.decode(TEST_PUBKEY).unwrap())
        .expect("fixture public key text is UTF-8");
    let public_key_line = public_key_text
        .lines()
        .nth(1)
        .expect("fixture public key has a key line");
    let public_key = STANDARD
        .decode(public_key_line)
        .expect("fixture public key line is base64");

    let mut signature_blob = Vec::with_capacity(74);
    signature_blob.extend_from_slice(&[0x45, 0x64]);
    signature_blob.extend_from_slice(&public_key[2..10]);
    signature_blob.extend_from_slice(&[0u8; 64]);

    let signature_text = format!(
        "untrusted comment: SAFE_FIXTURE_invalid_signature\n{}\ntrusted comment: SAFE_FIXTURE_invalid_trusted_comment\n{}\n",
        STANDARD.encode(signature_blob),
        STANDARD.encode([0u8; 64]),
    );
    STANDARD.encode(signature_text.as_bytes())
}

fn release_manifest(artifact_url: &str, signature: &str) -> String {
    let mut platforms = serde_json::Map::new();
    platforms.insert(
        TEST_UPDATER_TARGET.to_string(),
        serde_json::json!({
            "url": artifact_url,
            "signature": signature,
        }),
    );
    serde_json::json!({
        "version": "9.9.9",
        "notes": "SAFE_FIXTURE_release_notes",
        "pub_date": "2026-01-02T03:04:05Z",
        "platforms": platforms,
    })
    .to_string()
}

fn mock_updater_app() -> tauri::App<tauri::test::MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".to_string(),
        serde_json::json!({
            "pubkey": TEST_PUBKEY,
            "endpoints": ["http://update.example.test/latest.json"],
            "dangerousInsecureTransportProtocol": true,
        }),
    );
    mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .expect("build mock updater app")
}

async fn download_update(
    update: &Update,
) -> (Result<Vec<u8>, String>, Vec<AppUpdaterDownloadEvent>) {
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let started = std::sync::Arc::new(AtomicBool::new(false));
    let chunk_events = events.clone();
    let chunk_started = started.clone();
    let finished_events = events.clone();
    let result = update
        .download(
            move |chunk_length, content_length| {
                let mut events = chunk_events
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if !chunk_started.swap(true, Ordering::SeqCst) {
                    events.push(AppUpdaterDownloadEvent::Started { content_length });
                }
                events.push(AppUpdaterDownloadEvent::Progress { chunk_length });
            },
            move || {
                finished_events
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .push(AppUpdaterDownloadEvent::Finished);
            },
        )
        .await
        .map_err(|error| error.to_string());
    let events = events
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    (result, events)
}

#[tokio::test]
async fn updater_transport_closure_follows_current_proxy() {
    let _serial = crate::proxy::test_support::SERIAL_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let proxy_a = HttpProxyRecorder::new(r#"{"fixture":"A"}"#).await;
    let proxy_b = HttpProxyRecorder::new(r#"{"fixture":"B"}"#).await;
    let _home =
        crate::proxy::test_support::with_global_proxy(Some(proxy_config(recorder_port(&proxy_a))));

    let configure = |builder| configure_current_transport(builder);
    let client = configure(reqwest::Client::builder())
        .build()
        .expect("build client through proxy A");
    let response = client
        .get("http://update.example.test/latest.json")
        .send()
        .await
        .expect("request through proxy A");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        proxy_a.targets(),
        vec!["http://update.example.test/latest.json".to_string()]
    );
    assert!(proxy_b.targets().is_empty());

    publish_recorder(&proxy_b);
    let client = configure(reqwest::Client::builder())
        .build()
        .expect("build client through proxy B");
    let response = client
        .get("http://update.example.test/latest.json")
        .send()
        .await
        .expect("request through proxy B");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(proxy_a.hits(), 1);
    assert_eq!(
        proxy_b.targets(),
        vec!["http://update.example.test/latest.json".to_string()]
    );

    publish_direct();
}

#[tokio::test]
async fn updater_check_and_download_follow_proxy_and_keep_progress() {
    let _serial = crate::proxy::test_support::SERIAL_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let signature = invalid_signature();
    let manifest = release_manifest("http://update.example.test/artifact", &signature);
    let proxy_a = HttpProxyRecorder::new(&manifest).await;
    let proxy_b = HttpProxyRecorder::new("SAFE_FIXTURE_artifact_bytes").await;
    let _home =
        crate::proxy::test_support::with_global_proxy(Some(proxy_config(recorder_port(&proxy_a))));

    let app = mock_updater_app();
    let updater = test_check_builder(
        app.handle(),
        url::Url::parse("http://update.example.test/latest.json").unwrap(),
        TEST_PUBKEY,
    )
    .expect("build the test updater");
    let update = updater
        .check()
        .await
        .expect("check through proxy A")
        .expect("an update is available");
    assert_eq!(
        proxy_a.targets(),
        vec!["http://update.example.test/latest.json".to_string()]
    );
    assert!(proxy_b.targets().is_empty());

    let metadata = test_metadata_from_update(&update);
    assert_eq!(metadata.version, "9.9.9");
    assert_eq!(metadata.current_version, update.current_version);
    assert_eq!(metadata.id, stable_update_id(&update));
    assert_eq!(metadata.date.as_deref(), Some("2026-01-02T03:04:05+00:00"));
    assert_eq!(metadata.body.as_deref(), Some("SAFE_FIXTURE_release_notes"));

    let state = AppUpdaterState::new();
    state.replace_all_with(metadata.id.clone(), update.clone());
    let download_target = state
        .update_for(&metadata.id)
        .expect("the checked update is owned by the state");
    publish_recorder(&proxy_b);

    let (result, events) = download_update(&download_target).await;
    assert!(
        result.is_err(),
        "the official verification must reject the invalid signature"
    );
    assert_eq!(proxy_a.hits(), 1);
    assert_eq!(
        proxy_b.targets(),
        vec!["http://update.example.test/artifact".to_string()]
    );
    assert!(events.len() >= 3);
    assert!(matches!(
        events.first(),
        Some(AppUpdaterDownloadEvent::Started { .. })
    ));
    assert!(matches!(
        events.last(),
        Some(AppUpdaterDownloadEvent::Finished)
    ));
    assert!(events[1..events.len() - 1]
        .iter()
        .all(|event| matches!(event, AppUpdaterDownloadEvent::Progress { .. })));
    assert!(events.iter().any(|event| matches!(
        event,
        AppUpdaterDownloadEvent::Started {
            content_length: Some(length)
        } if *length == "SAFE_FIXTURE_artifact_bytes".len() as u64
    )));
    assert!(state.verified_download_for(&metadata.id).is_none());

    let verified = b"SAFE_FIXTURE_verified_bytes".to_vec();
    state.store_verified_download(&metadata.id, verified.clone());
    let proxy_c = HttpProxyRecorder::new("unused").await;
    publish_recorder(&proxy_c);

    let (stored_update, stored_bytes) = state
        .verified_download_for(&metadata.id)
        .expect("verified bytes survive another proxy switch");
    assert_eq!(stored_bytes, verified);
    assert_eq!(stable_update_id(&stored_update), metadata.id);

    let installed = std::sync::Arc::new(AtomicBool::new(false));
    let installed_flag = installed.clone();
    let expected_id = metadata.id.clone();
    test_install_verified_with_mock(&state, &metadata.id, move |update, bytes| {
        assert_eq!(stable_update_id(update), expected_id);
        assert_eq!(bytes, b"SAFE_FIXTURE_verified_bytes".as_slice());
        installed_flag.store(true, Ordering::SeqCst);
        Ok(())
    })
    .expect("mocked installation receives the verified bytes");
    assert!(installed.load(Ordering::SeqCst));

    app_updater_shutdown();
    publish_direct();
}

#[tokio::test]
async fn updater_shutdown_clears_owned_state() {
    let _serial = crate::proxy::test_support::SERIAL_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let manifest = release_manifest("http://update.example.test/artifact", &invalid_signature());
    let proxy = HttpProxyRecorder::new(&manifest).await;
    let _home =
        crate::proxy::test_support::with_global_proxy(Some(proxy_config(recorder_port(&proxy))));

    let app = mock_updater_app();
    let updater = test_check_builder(
        app.handle(),
        url::Url::parse("http://update.example.test/latest.json").unwrap(),
        TEST_PUBKEY,
    )
    .expect("build the test updater");
    let update = updater
        .check()
        .await
        .expect("check for the shutdown fixture")
        .expect("an update is available");
    let id = stable_update_id(&update);
    let state = AppUpdaterState::new();
    state.replace_all_with(id.clone(), update);
    state.store_verified_download(&id, b"SAFE_FIXTURE_shutdown_bytes".to_vec());
    assert!(state.verified_download_for(&id).is_some());

    app_updater_shutdown();
    assert!(state.verified_download_for(&id).is_none());

    app_updater_shutdown();
    assert!(state.verified_download_for(&id).is_none());

    publish_direct();
}
