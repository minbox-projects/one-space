//! Step 2 failing behavior tests for REQ-001 (serialized configuration
//! read-modify-write) and REQ-002 (read caching and no-op write suppression).
//!
//! These tests observe only the public/module-private entry points — the relay
//! settlement path (`attempt_non_streaming`), the upsert command, the
//! configuration read/write boundary and the serialized write primitive — using
//! raw encrypted fixtures and mock upstreams. They must compile against the
//! current entry points and fail behaviorally: concurrent writers lose marks,
//! a no-op settlement rewrites the encrypted file, the legacy migration races
//! concurrent writes, and every attempt re-decrypts.
//!
//! All concurrency tests use the process-wide [`super::temp_home`] helper and
//! therefore run serially under the shared `HOME` lock, because the thread-local
//! `isolated_temp_home` guard does not cross spawned tasks/threads.

use super::{
    key_named, mapping, pool_key, pool_provider, spawn_mock_upstream, temp_home,
    upstream_provider, write_raw_gateway_config, MockReply,
};
use crate::ai_gateway::runtime_http::attempt_non_streaming;
use crate::ai_gateway::storage::{config_path, read_config, write_config};
use crate::ai_gateway::{
    ai_gateway_upsert_provider, GatewayConfig, GatewayUpstreamProvider, KeyFailureKind,
    GATEWAY_CONFIG_SCHEMA_VERSION,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

/// A one-row mapping value used by every provider fixture here.
fn mapping_value() -> Value {
    serde_json::to_value(mapping("local-model", "remote-model", None)).expect("mapping value")
}

/// A provider with an ordered key pool of `key_count` enabled keys and one
/// `local-model -> remote-model` mapping.
fn multi_key_provider(
    id: &str,
    base_url: &str,
    key_count: usize,
    mappings: Vec<Value>,
) -> GatewayUpstreamProvider {
    let keys: Vec<Value> = (1..=key_count)
        .map(|index| {
            pool_key(
                &format!("key-{index}"),
                &format!("Key {index}"),
                &format!("sk-{index}"),
                true,
            )
        })
        .collect();
    let mut provider: GatewayUpstreamProvider =
        serde_json::from_value(pool_provider(id, "Shared Provider", base_url, keys, mappings))
            .expect("multi-key provider fixture must deserialize");
    provider.default_model = Some("remote-model".to_string());
    provider
}

/// Enable exactly `index` of the provider's keys and clear all runtime state on
/// its clone, so a single relay attempt settles exactly that key. The persisted
/// configuration still carries the full pool, which is what each settlement
/// writes against.
fn snapshot_for_key(provider: &GatewayUpstreamProvider, index: usize) -> GatewayUpstreamProvider {
    let mut snapshot = provider.clone();
    for (position, key) in snapshot.keys.iter_mut().enumerate() {
        key.enabled = position == index;
        key.auto_marked = false;
        key.failure_kind = None;
        key.marked_at = None;
        key.reason = None;
    }
    snapshot
}

/// A mock upstream that answers every request with `status`/`value` after an
/// async delay, so concurrently launched attempts all read the persisted
/// configuration before any of their settlements writes it back.
async fn spawn_delayed_json_mock(status: u16, value: Value, delay_ms: u64) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind delayed mock");
    let addr = listener.local_addr().expect("delayed mock addr");
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let value = value.clone();
            tauri::async_runtime::spawn(async move {
                let Ok(_request) =
                    crate::ai_gateway::runtime_http::read_http_request(&mut stream).await
                else {
                    return;
                };
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                let body = serde_json::to_vec(&value).unwrap_or_default();
                let header = format!(
                    "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len(),
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(&body).await;
            });
        }
    });
    format!("http://{}", addr)
}

/// Force a distinct modification time so any metadata-keyed read cache must
/// invalidate between rounds.
fn set_file_mtime(path: &Path, secs_since_epoch: u64) {
    let file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open for mtime");
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs_since_epoch))
        .expect("set mtime");
}

/// Seed the process-wide temp home with one shared provider carrying the given
/// ordered key pool and a local relay key.
fn seed_shared_provider(provider: &GatewayUpstreamProvider) -> GatewayConfig {
    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(provider.clone());
    write_config(&config).expect("seed shared config");
    config
}

/// AC-001: eight threads concurrently mark a distinct key each of one provider
/// through the relay settlement path (a key-scoped 401). All eight marks must
/// persist with their own failure kind and marking time.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn ac001_eight_concurrent_key_marks_all_persist() {
    let _home = temp_home("ac001-concurrent-key-marks");
    let mock = spawn_delayed_json_mock(
        401,
        json!({"error": {"message": "invalid api key"}}),
        250,
    )
    .await;
    let key_count = 8;
    let base = multi_key_provider("shared", &mock, key_count, vec![mapping_value()]);
    let body = Arc::new(serde_json::to_vec(&json!({"model": "local-model"})).expect("body"));

    for round in 0..3 {
        seed_shared_provider(&base);
        set_file_mtime(&config_path().expect("config path"), 1_700_000_000 + round * 60);

        let barrier = Arc::new(tokio::sync::Barrier::new(key_count));
        let mut handles = Vec::new();
        for index in 0..key_count {
            let snapshot = Arc::new(snapshot_for_key(&base, index));
            let body = body.clone();
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                let mut attempts = Vec::new();
                let _ = attempt_non_streaming(
                    std::slice::from_ref(&*snapshot),
                    "/v1/chat/completions",
                    &body,
                    Some("local-model"),
                    &HashMap::new(),
                    false,
                    None,
                    &mut attempts,
                )
                .await;
            }));
        }
        for handle in handles {
            handle.await.expect("attempt task must not panic");
        }

        let stored = read_config().expect("configuration must stay readable");
        let provider = stored
            .providers
            .iter()
            .find(|provider| provider.id == "shared")
            .expect("shared provider must persist");
        let marks: Vec<String> = provider
            .keys
            .iter()
            .map(|key| format!("{}={}", key.id, key.auto_marked))
            .collect();
        for key in &provider.keys {
            assert!(
                key.auto_marked,
                "round {round}: every concurrently marked key must persist; got {marks:?}"
            );
            assert_eq!(
                key.failure_kind,
                Some(KeyFailureKind::Authentication),
                "round {round}: key {} must carry its own failure kind",
                key.id
            );
            assert!(
                key.marked_at.is_some(),
                "round {round}: key {} must carry its marking time",
                key.id
            );
        }
    }
}

/// AC-002: a key mark and a mapping-health settlement running concurrently both
/// persist. One request settles a key-scoped 401 while another settles a
/// retryable 500 on the same provider.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn ac002_concurrent_key_mark_and_mapping_health_both_persist() {
    let _home = temp_home("ac002-key-mark-and-health");
    let body = Arc::new(serde_json::to_vec(&json!({"model": "local-model"})).expect("body"));

    for round in 0..3 {
        let auth_arrived = Arc::new(tokio::sync::Notify::new());
        let auth_release = Arc::new(tokio::sync::Notify::new());
        let fail_arrived = Arc::new(tokio::sync::Notify::new());
        let fail_release = Arc::new(tokio::sync::Notify::new());
        let (auth_url, _log) = {
            let arrived = auth_arrived.clone();
            let release = auth_release.clone();
            spawn_mock_upstream(move |_| {
                MockReply::Withhold(
                    arrived.clone(),
                    release.clone(),
                    401,
                    json!({"error": {"message": "denied"}}),
                )
            })
            .await
        };
        let (fail_url, _log) = {
            let arrived = fail_arrived.clone();
            let release = fail_release.clone();
            spawn_mock_upstream(move |_| {
                MockReply::Withhold(
                    arrived.clone(),
                    release.clone(),
                    500,
                    json!({"error": {"message": "server busy"}}),
                )
            })
            .await
        };
        let base = multi_key_provider("shared", &auth_url, 1, vec![mapping_value()]);
        seed_shared_provider(&base);
        set_file_mtime(&config_path().expect("config path"), 1_700_000_500 + round * 60);

        let mut auth_snapshot = base.clone();
        auth_snapshot.base_url = auth_url.clone();
        let mut fail_snapshot = base.clone();
        fail_snapshot.base_url = fail_url.clone();

        let auth_body = body.clone();
        let auth_task = tokio::spawn(async move {
            let mut attempts = Vec::new();
            let _ = attempt_non_streaming(
                std::slice::from_ref(&auth_snapshot),
                "/v1/chat/completions",
                &auth_body,
                Some("local-model"),
                &HashMap::new(),
                false,
                None,
                &mut attempts,
            )
            .await;
        });
        let fail_body = body.clone();
        let fail_task = tokio::spawn(async move {
            let mut attempts = Vec::new();
            let _ = attempt_non_streaming(
                std::slice::from_ref(&fail_snapshot),
                "/v1/chat/completions",
                &fail_body,
                Some("local-model"),
                &HashMap::new(),
                false,
                None,
                &mut attempts,
            )
            .await;
        });

        // Both settlements have read the persisted configuration and are held
        // at their upstreams; release them together so their read-modify-write
        // windows overlap.
        let _ = tokio::join!(auth_arrived.notified(), fail_arrived.notified());
        auth_release.notify_one();
        fail_release.notify_one();
        auth_task.await.expect("auth attempt task");
        fail_task.await.expect("failure attempt task");

        let stored = read_config().expect("configuration must stay readable");
        let provider = stored
            .providers
            .iter()
            .find(|provider| provider.id == "shared")
            .expect("shared provider must persist");
        assert!(
            provider.keys[0].auto_marked,
            "round {round}: the concurrent key mark must persist"
        );
        assert_eq!(
            provider.keys[0].failure_kind,
            Some(KeyFailureKind::Authentication),
            "round {round}: the key mark must keep its failure kind"
        );
        assert!(
            provider.mappings[0].consecutive_failures >= 1,
            "round {round}: the concurrent mapping-health settlement must persist"
        );
        assert!(
            provider.mappings[0].last_error_at.is_some(),
            "round {round}: the mapping-health settlement must persist its error time"
        );
    }
}

/// AC-003: several threads repeatedly write key marks for distinct keys; after
/// every writer finishes, every subsequent read must decrypt, parse and return a
/// complete configuration. The shared temp-path write path must never publish a
/// partial file.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn ac003_repeated_concurrent_key_writes_leave_a_readable_configuration() {
    let _home = temp_home("ac003-repeated-concurrent-writes");
    let mock = spawn_delayed_json_mock(
        401,
        json!({"error": {"message": "denied"}}),
        5,
    )
    .await;
    let key_count = 48;
    let filler_count = 64;
    let base = multi_key_provider("shared", &mock, key_count, vec![mapping_value()]);

    let mut seed = GatewayConfig::default();
    seed.keys.push(key_named("k1", "local-key"));
    seed.providers.push(base.clone());
    for filler in 0..filler_count {
        let mut provider = upstream_provider(
            &format!("filler-{filler}"),
            "Filler",
            "https://filler.example.invalid/v1",
            "sk",
            None,
        );
        provider.name = "F".repeat(16_000);
        seed.providers.push(provider);
    }
    write_config(&seed).expect("seed large config");
    set_file_mtime(&config_path().expect("config path"), 1_700_001_000);

    let tasks = 8;
    let per_task = key_count / tasks;
    let barrier = Arc::new(tokio::sync::Barrier::new(tasks));
    let mut handles = Vec::new();
    for task in 0..tasks {
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            for key_index in (task * per_task)..((task + 1) * per_task) {
                let latest = read_config().expect("read latest configuration before marking");
                let Some(provider) = latest
                    .providers
                    .iter()
                    .find(|provider| provider.id == "shared")
                    .cloned()
                else {
                    continue;
                };
                let snapshot = snapshot_for_key(&provider, key_index);
                let body = serde_json::to_vec(&json!({"model": "local-model"})).expect("body");
                let mut attempts = Vec::new();
                let _ = attempt_non_streaming(
                    std::slice::from_ref(&snapshot),
                    "/v1/chat/completions",
                    &body,
                    Some("local-model"),
                    &HashMap::new(),
                    false,
                    None,
                    &mut attempts,
                )
                .await;
            }
        }));
    }
    for handle in handles {
        handle.await.expect("key-mark writer task must not panic");
    }

    for read_index in 0..5 {
        let stored = read_config()
            .unwrap_or_else(|error| panic!("read {read_index} must decrypt and parse: {error}"));
        assert!(
            stored.providers.iter().any(|provider| provider.id == "shared"),
            "read {read_index}: the shared provider must be present"
        );
        let shared = stored
            .providers
            .iter()
            .find(|provider| provider.id == "shared")
            .expect("shared provider");
        assert_eq!(
            shared.keys.len(),
            key_count,
            "read {read_index}: the complete key pool must survive"
        );
        let unmarked: Vec<&str> = shared
            .keys
            .iter()
            .filter(|key| !key.auto_marked)
            .map(|key| key.id.as_str())
            .collect();
        assert!(
            unmarked.is_empty(),
            "read {read_index}: every writer's distinct key mark must survive repeated concurrent writes; unmarked: {unmarked:?}"
        );
        for filler in 0..filler_count {
            assert!(
                stored
                    .providers
                    .iter()
                    .any(|provider| provider.id == format!("filler-{filler}")),
                "read {read_index}: filler-{filler} must be present"
            );
        }
    }
}

/// AC-004: a user edit through `ai_gateway_upsert_provider` racing a request
/// settlement on the same provider persists both the edited fields and the key
/// mark.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac004_user_edit_and_request_settlement_do_not_overwrite_each_other() {
    let _home = temp_home("ac004-upsert-racing-settlement");
    let body = Arc::new(serde_json::to_vec(&json!({"model": "local-model"})).expect("body"));

    for round in 0..3 {
        let arrived = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let (withhold_url, _log) = {
            let arrived = arrived.clone();
            let release = release.clone();
            spawn_mock_upstream(move |_| {
                MockReply::Withhold(
                    arrived.clone(),
                    release.clone(),
                    401,
                    json!({"error": {"message": "denied"}}),
                )
            })
            .await
        };
        let base = multi_key_provider("shared", &withhold_url, 1, vec![mapping_value()]);
        seed_shared_provider(&base);
        set_file_mtime(
            &config_path().expect("config path"),
            1_700_002_000 + round * 60,
        );

        let snapshot = base.clone();
        let settlement_body = body.clone();
        let settlement_task = tokio::spawn(async move {
            let mut attempts = Vec::new();
            let _ = attempt_non_streaming(
                std::slice::from_ref(&snapshot),
                "/v1/chat/completions",
                &settlement_body,
                Some("local-model"),
                &HashMap::new(),
                false,
                None,
                &mut attempts,
            )
            .await;
        });

        // The settlement has read the persisted configuration and is now held
        // at the upstream; release it together with the user edit so their
        // read-modify-write windows overlap.
        arrived.notified().await;
        let mut edited = base.clone();
        edited.name = "Edited Provider".to_string();
        let edit_task =
            tokio::task::spawn_blocking(move || ai_gateway_upsert_provider(edited, None));
        release.notify_one();
        let _ = edit_task.await.expect("upsert task must not panic");
        settlement_task.await.expect("settlement task must not panic");

        let stored = read_config().expect("configuration must stay readable");
        let provider = stored
            .providers
            .iter()
            .find(|provider| provider.id == "shared")
            .expect("shared provider must persist");
        assert_eq!(
            provider.name, "Edited Provider",
            "round {round}: the user edit must survive the concurrent settlement"
        );
        assert!(
            provider.keys[0].auto_marked,
            "round {round}: the key mark must survive the concurrent user edit"
        );
    }
}

/// AC-005: a legacy-schema configuration read while other configuration writes
/// run concurrently must end at the current schema version with readable
/// content, and a subsequent read must perform no further rewrite.
#[test]
fn ac005_legacy_migration_stays_atomic_and_one_shot_under_concurrent_writes() {
    let _home = temp_home("ac005-legacy-migration-concurrent");
    let readers = 12;
    let writers = 4;
    let filler_count = 40;

    for round in 0..6 {
        let mut providers = vec![json!({
            "id": "legacy",
            "name": "Legacy Provider",
            "base_url": "https://legacy.example.com/v1",
            "api_key": "sk-legacy",
            "protocol": "chat_completions",
            "mappings": [
                {"local_model": "local-model", "upstream_model": "remote-model", "enabled": true}
            ]
        })];
        for filler in 0..filler_count {
            providers.push(json!({
                "id": format!("legacy-filler-{filler}"),
                "name": "F".repeat(8_000),
                "base_url": "https://legacy-filler.example.invalid/v1",
                "api_key": "sk-legacy-filler",
                "protocol": "chat_completions",
                "mappings": [
                    {
                        "local_model": format!("local-{filler}"),
                        "upstream_model": format!("remote-{filler}"),
                        "enabled": true
                    }
                ]
            }));
        }
        let legacy = json!({ "enabled": true, "providers": providers });
        write_raw_gateway_config(&legacy);
        set_file_mtime(
            &config_path().expect("config path"),
            1_700_003_000 + round * 60,
        );

        let barrier = Arc::new(std::sync::Barrier::new(readers + writers));
        let mut handles = Vec::new();
        for _ in 0..readers {
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..2 {
                    let _ = read_config();
                }
            }));
        }
        for writer in 0..writers {
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let provider_id = format!("writer-{round}-{writer}");
                // A user command save is an unserialized read-modify-write on
                // the current code, so it races both the lazy migration and
                // the other saves.
                let _ = ai_gateway_upsert_provider(
                    upstream_provider(
                        &provider_id,
                        "Writer Provider",
                        "https://writer.example.invalid/v1",
                        "sk-writer",
                        None,
                    ),
                    None,
                );
            }));
        }
        for handle in handles {
            handle.join().expect("migration worker must not panic");
        }

        let stored = read_config().expect("migration must end readable");
        assert_eq!(
            stored.schema_version, GATEWAY_CONFIG_SCHEMA_VERSION,
            "round {round}: the legacy file must end at the current schema version"
        );
        let ids: Vec<String> = stored
            .providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect();
        for writer in 0..writers {
            let provider_id = format!("writer-{round}-{writer}");
            assert!(
                stored.providers.iter().any(|p| p.id == provider_id),
                "round {round}: {provider_id} must survive the concurrent migration; got {ids:?}"
            );
        }
        for filler in 0..filler_count {
            assert!(
                stored
                    .providers
                    .iter()
                    .any(|p| p.id == format!("legacy-filler-{filler}")),
                "round {round}: legacy-filler-{filler} must survive the concurrent migration"
            );
        }

        let stable = fs::read(config_path().expect("config path")).expect("config bytes");
        let _ = read_config().expect("second read must succeed");
        assert_eq!(
            fs::read(config_path().expect("config path")).expect("config bytes after"),
            stable,
            "round {round}: a subsequent read must not rewrite the file"
        );
    }
}

/// AC-008: a warmed read cache serves two equal configurations without
/// decrypting again, and the next read observes content written directly on
/// disk once its metadata changes (no invalidation helper is called).
#[test]
fn ac008_warm_cache_is_metadata_driven_and_skips_redecryption() {
    let _home = temp_home("ac008-cache-coherence");
    let path = config_path().expect("config path");
    let initial = json!({
        "schema_version": GATEWAY_CONFIG_SCHEMA_VERSION,
        "enabled": true,
        "providers": [{
            "id": "cached",
            "name": "Original",
            "base_url": "https://cached.example.com/v1",
            "protocol": "chat_completions",
            "keys": [],
            "mappings": []
        }],
        "model_prices": []
    });
    write_raw_gateway_config(&initial);
    let original_mtime = fs::metadata(&path)
        .expect("config metadata")
        .modified()
        .expect("config mtime");

    let first = read_config().expect("first read must warm the cache");
    let second = read_config().expect("second read must succeed");
    assert_eq!(
        serde_json::to_value(&first).expect("first config"),
        serde_json::to_value(&second).expect("second config"),
        "two reads with no intervening write must return equal configurations"
    );

    // Prove the second read did not decrypt the configuration: replace the
    // master password on disk. A cached read still returns the value; a
    // re-decrypting read fails.
    let key_path = crate::crypto::get_local_key_path().expect("local key path");
    let original_password =
        crate::crypto::get_or_init_master_password().expect("master password");
    fs::write(&key_path, "ac008-different-password").expect("replace master password");
    let cached = read_config().expect(
        "a warm read must serve the cached configuration without decrypting the changed file",
    );
    assert_eq!(
        cached.providers[0].name, "Original",
        "a warm read must return the cached configuration"
    );
    fs::write(&key_path, &original_password).expect("restore master password");

    // Rewrite the file directly on disk with new content and a changed
    // modification time: the next read must return the new content.
    let mut rewritten = initial.clone();
    rewritten["providers"][0]["name"] = json!("Rewritten");
    let password = crate::crypto::get_or_init_master_password().expect("master password");
    let encrypted = crate::crypto::encrypt(&rewritten.to_string(), &password).expect("encrypt");
    fs::write(&path, encrypted).expect("rewrite raw config directly");
    let new_mtime = original_mtime + Duration::from_secs(7200);
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open rewritten config")
        .set_modified(new_mtime)
        .expect("change config mtime");

    let updated = read_config().expect("the metadata change must invalidate the cache");
    assert_eq!(
        updated.providers[0].name, "Rewritten",
        "the next read must return the directly rewritten content"
    );
}

/// AC-009: a successful request resolving through a mapping row with zero
/// consecutive failures and no recorded last error settles health, and the
/// encrypted configuration file bytes stay unchanged because the settlement
/// produces no actual state change.
#[tokio::test]
async fn ac009_noop_success_settlement_does_not_rewrite_the_file() {
    let _home = temp_home("ac009-noop-settlement");
    let (upstream_url, _log) =
        spawn_mock_upstream(|_| MockReply::Json(200, json!({"id": "ok", "choices": []}))).await;
    let mut provider = upstream_provider(
        "shared",
        "Shared Provider",
        &upstream_url,
        "sk",
        Some("remote-model"),
    );
    provider.mappings = vec![mapping("local-model", "remote-model", None)];
    let mut config = GatewayConfig::default();
    config.keys.push(key_named("k1", "local-key"));
    config.providers.push(provider.clone());
    write_config(&config).expect("seed config");
    let before = fs::read(config_path().expect("config path")).expect("config bytes before");

    let body = serde_json::to_vec(&json!({"model": "local-model"})).expect("body");
    let mut attempts = Vec::new();
    let response = attempt_non_streaming(
        std::slice::from_ref(&provider),
        "/v1/chat/completions",
        &body,
        Some("local-model"),
        &HashMap::new(),
        false,
        None,
        &mut attempts,
    )
    .await;
    assert_eq!(response.status, 200, "the mapped request must succeed");

    let after = fs::read(config_path().expect("config path")).expect("config bytes after");
    assert!(
        before == after,
        "a settlement that changes nothing must not rewrite the encrypted file (before {} bytes, after {} bytes)",
        before.len(),
        after.len()
    );
    let stored = read_config().expect("configuration must stay readable");
    let row = &stored
        .providers
        .iter()
        .find(|provider| provider.id == "shared")
        .expect("shared provider")
        .mappings[0];
    assert_eq!(row.consecutive_failures, 0);
    assert_eq!(row.last_error_at, None);
}
