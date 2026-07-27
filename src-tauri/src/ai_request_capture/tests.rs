use super::{
    config, database_path_in, recover_from_dir, runtime, storage::CaptureStore, types::*,
    AiRequestCaptureConfig, CAPTURE_BODY_LIMIT_BYTES,
};
use bytes::Bytes;
use futures_util::{stream, StreamExt};
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::Frame;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::fs;
use std::net::TcpListener as StdTcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use uuid::Uuid;

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("onespace-capture-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temporary test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn start(id: &str, started_at: i64, method: &str) -> CaptureStart {
    CaptureStart {
        id: id.to_string(),
        started_at,
        http_version: "HTTP/1.1".to_string(),
        method: method.to_string(),
        request_path_and_query: "/v1/messages?stream=true".to_string(),
        upstream_url: "https://api.example.test/prefix/v1/messages?stream=true".to_string(),
        request_headers: vec![AiRequestCaptureHeader {
            name: "authorization".to_string(),
            values: vec![
                "Bearer plain-secret".to_string(),
                "second-value".to_string(),
            ],
        }],
        request_body: CapturedBody::from_bytes(vec![0, 0xff, 1], 3),
        provider: Some("openai".to_string()),
        model: Some("gpt-test".to_string()),
    }
}

fn finish(completed_at: i64, state: CaptureState) -> CaptureFinish {
    CaptureFinish {
        completed_at,
        state,
        response_status: Some(200),
        response_headers: vec![AiRequestCaptureHeader {
            name: "set-cookie".to_string(),
            values: vec!["session=plain-secret".to_string()],
        }],
        response_body: CapturedBody::from_bytes(b"response body".to_vec(), 13),
        error: None,
    }
}

fn header_values(headers: &[AiRequestCaptureHeader], name: &str) -> Vec<String> {
    headers
        .iter()
        .find(|header| header.name == name)
        .map(|header| header.values.clone())
        .unwrap_or_default()
}

fn proxy_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn free_loopback_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
    let port = listener.local_addr().expect("read listener address").port();
    drop(listener);
    port
}

#[derive(Clone, Debug, PartialEq)]
struct ObservedRequest {
    method: String,
    path_and_query: String,
    authorization: Option<String>,
    accept_encoding: Option<String>,
    forwarded_hop_header: Option<String>,
    host: Option<String>,
    body: Vec<u8>,
}

async fn start_mock_upstream(
    observed: Arc<Mutex<Option<ObservedRequest>>>,
    response_body: Bytes,
) -> (u16, oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock upstream");
    let port = listener.local_addr().expect("mock listener address").port();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    let observed = Arc::clone(&observed);
                    let response_body = response_body.clone();
                    tokio::spawn(async move {
                        let service = service_fn(move |request: Request<hyper::body::Incoming>| {
                            let observed = Arc::clone(&observed);
                            let response_body = response_body.clone();
                            async move {
                                let (parts, body) = request.into_parts();
                                let body = body.collect().await.expect("read mock request").to_bytes();
                                *observed.lock().expect("lock observed request") = Some(ObservedRequest {
                                     method: parts.method.to_string(),
                                     path_and_query: parts.uri.path_and_query().expect("origin-form URI").as_str().to_string(),
                                     authorization: parts.headers.get("authorization").and_then(|value| value.to_str().ok()).map(str::to_string),
                                     accept_encoding: parts.headers.get("accept-encoding").and_then(|value| value.to_str().ok()).map(str::to_string),
                                     forwarded_hop_header: parts.headers.get("x-remove-from-forward").and_then(|value| value.to_str().ok()).map(str::to_string),
                                     host: parts.headers.get("host").and_then(|value| value.to_str().ok()).map(str::to_string),
                                     body: body.to_vec(),
                                 });
                                 Ok::<_, hyper::Error>(Response::builder()
                                     .status(StatusCode::IM_A_TEAPOT)
                                     .header("x-upstream-secret", "plain-response-secret")
                                     .header("connection", "x-remove-from-client")
                                     .header("x-remove-from-client", "must-not-reach-client")
                                     .body(Full::new(response_body))
                                    .expect("build mock response"))
                            }
                        });
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(TokioIo::new(stream), service)
                            .await;
                    });
                }
            }
        }
    });
    (port, shutdown_tx)
}

async fn wait_for_completed_capture(dir: &Path) -> AiRequestCaptureDetail {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(capture) = CaptureStore::open(database_path_in(dir))
                .expect("open capture store")
                .list(CaptureListQuery::default())
                .expect("list captures")
                .items
                .pop()
            {
                let detail = CaptureStore::open(database_path_in(dir))
                    .expect("open capture store")
                    .get(&capture.id)
                    .expect("load capture")
                    .expect("capture exists");
                if detail.state == CaptureState::Completed {
                    return detail;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("capture completes")
}

mod config_storage {
    use super::*;

    #[test]
    fn config_defaults_validate_and_persist_in_the_local_capture_directory() {
        let dir = TestDir::new();
        let config = AiRequestCaptureConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.port, 17688);
        assert!(config.upstream_base_url.is_empty());
        assert!(config::validation_errors(&config).is_empty());

        let custom = AiRequestCaptureConfig {
            enabled: true,
            port: 19001,
            upstream_base_url: "https://api.example.test/prefix".to_string(),
        };
        config::write_config_in(dir.path(), &custom).expect("write config atomically");

        assert_eq!(
            config::read_config_in(dir.path()).expect("read persisted config"),
            custom
        );
        assert!(dir
            .path()
            .join("data/ai-request-capture/config.json")
            .exists());
    }

    #[test]
    fn config_rejects_invalid_ports_urls_and_loopback_cycles() {
        let invalid = [
            AiRequestCaptureConfig {
                enabled: true,
                port: 0,
                upstream_base_url: "https://api.example.test".to_string(),
            },
            AiRequestCaptureConfig {
                enabled: true,
                port: 17688,
                upstream_base_url: "ftp://api.example.test".to_string(),
            },
            AiRequestCaptureConfig {
                enabled: true,
                port: 17688,
                upstream_base_url: "https:///missing-host".to_string(),
            },
            AiRequestCaptureConfig {
                enabled: true,
                port: 17688,
                upstream_base_url: "https://api.example.test/path?query=yes".to_string(),
            },
            AiRequestCaptureConfig {
                enabled: true,
                port: 17688,
                upstream_base_url: "https://api.example.test/path#fragment".to_string(),
            },
            AiRequestCaptureConfig {
                enabled: true,
                port: 17688,
                upstream_base_url: "http://localhost:17688".to_string(),
            },
            AiRequestCaptureConfig {
                enabled: true,
                port: 17688,
                upstream_base_url: "http://127.0.0.1:17688".to_string(),
            },
            AiRequestCaptureConfig {
                enabled: true,
                port: 17688,
                upstream_base_url: "http://[::1]:17688".to_string(),
            },
        ];

        for config in invalid {
            assert!(!config::validation_errors(&config).is_empty(), "{config:?}");
        }
    }

    #[test]
    fn storage_migrates_with_pragmas_and_keeps_plain_headers_and_blob_bodies() {
        let dir = TestDir::new();
        let store = CaptureStore::open(dir.path().join("captures.sqlite3")).expect("open store");

        assert_eq!(store.user_version().expect("schema version"), 2);
        assert!(store
            .has_index("captures_started_at_id_idx")
            .expect("started index"));
        assert!(store.has_index("captures_state_idx").expect("state index"));

        store
            .begin(start("capture-1", 100, "POST"))
            .expect("begin capture");
        store
            .finish("capture-1", finish(130, CaptureState::Completed))
            .expect("finish capture");

        let record = store
            .get("capture-1")
            .expect("load capture")
            .expect("capture exists");
        assert_eq!(record.request_headers[0].values[0], "Bearer plain-secret");
        assert_eq!(record.request_headers[0].values[1], "second-value");
        assert_eq!(record.request_body.data, vec![0, 0xff, 1]);
        assert_eq!(record.response_headers[0].values[0], "session=plain-secret");
        assert_eq!(record.duration_ms, Some(30));
    }

    #[test]
    fn storage_caps_samples_but_preserves_actual_byte_counts() {
        let dir = TestDir::new();
        let store = CaptureStore::open(dir.path().join("captures.sqlite3")).expect("open store");
        let oversized = vec![7_u8; CAPTURE_BODY_LIMIT_BYTES + 1];
        let mut started = start("large", 100, "POST");
        started.request_body =
            CapturedBody::from_bytes(oversized, (CAPTURE_BODY_LIMIT_BYTES + 99) as u64);

        store.begin(started).expect("begin capture");
        let record = store
            .get("large")
            .expect("load capture")
            .expect("capture exists");
        assert_eq!(record.request_body.data.len(), CAPTURE_BODY_LIMIT_BYTES);
        assert_eq!(
            record.request_body.captured_bytes,
            CAPTURE_BODY_LIMIT_BYTES as u64
        );
        assert_eq!(
            record.request_body.total_bytes,
            (CAPTURE_BODY_LIMIT_BYTES + 99) as u64
        );
        assert!(record.request_body.truncated);
    }

    #[test]
    fn storage_filters_with_stable_pagination_and_excludes_bodies_from_list_rows() {
        let dir = TestDir::new();
        let store = CaptureStore::open(dir.path().join("captures.sqlite3")).expect("open store");
        for id in ["a", "b", "c"] {
            store.begin(start(id, 100, "POST")).expect("begin capture");
            store
                .finish(id, finish(130, CaptureState::Completed))
                .expect("finish capture");
        }

        let first = store
            .list(CaptureListQuery {
                method: Some("POST".to_string()),
                page: 1,
                page_size: 2,
                ..Default::default()
            })
            .expect("first page");
        let second = store
            .list(CaptureListQuery {
                method: Some("POST".to_string()),
                page: 2,
                page_size: 2,
                ..Default::default()
            })
            .expect("second page");

        assert_eq!(first.total, 3);
        assert_eq!(
            first
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "b"]
        );
        assert_eq!(
            second
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a"]
        );
        assert!(first.items.iter().all(|item| item.request_body.is_none()));
    }

    #[test]
    fn clear_keeps_in_progress_captures() {
        let dir = TestDir::new();
        let store = CaptureStore::open(dir.path().join("captures.sqlite3")).expect("open store");
        store
            .begin(start("active", 100, "POST"))
            .expect("begin active capture");
        store
            .begin(start("finished", 110, "POST"))
            .expect("begin finished capture");
        store
            .finish("finished", finish(120, CaptureState::Completed))
            .expect("finish capture");

        assert_eq!(store.clear().expect("clear finished captures"), 1);
        assert!(store.get("active").expect("load active capture").is_some());
        assert!(store
            .get("finished")
            .expect("load finished capture")
            .is_none());
    }

    #[test]
    fn recovery_marks_in_progress_and_removes_records_older_than_seven_days() {
        let dir = TestDir::new();
        let store = CaptureStore::open(dir.path().join("captures.sqlite3")).expect("open store");
        let now = 10 * 24 * 60 * 60 * 1_000;
        store
            .begin(start("active", now - 1, "GET"))
            .expect("begin active capture");
        store
            .begin(start("old", now - 8 * 24 * 60 * 60 * 1_000, "GET"))
            .expect("begin old capture");

        let result = store
            .recover_interrupted_and_cleanup(now)
            .expect("recover storage");
        assert_eq!(result.interrupted, 2);
        assert_eq!(result.deleted, 1);
        assert_eq!(
            store
                .get("active")
                .expect("get active")
                .expect("active exists")
                .state,
            CaptureState::Interrupted
        );
        assert!(store.get("old").expect("get old").is_none());
    }

    #[test]
    fn recovery_failure_is_reported_in_status_without_blocking_callers() {
        let dir = TestDir::new();
        let status = recover_from_dir(dir.path(), true);

        assert!(!status.running);
        assert!(status.last_error.is_some());
    }
}

mod basic_proxy {
    use super::*;

    #[tokio::test]
    async fn forwards_regular_request_with_prefixed_path_query_and_sensitive_headers() {
        let _guard = proxy_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = TestDir::new();
        let observed = Arc::new(Mutex::new(None));
        let (upstream_port, upstream_shutdown) = start_mock_upstream(
            Arc::clone(&observed),
            Bytes::from_static(b"upstream failure payload"),
        )
        .await;
        let config = AiRequestCaptureConfig {
            enabled: true,
            port: free_loopback_port(),
            upstream_base_url: format!("http://127.0.0.1:{upstream_port}/api/v1/"),
        };

        let status = runtime::start_in(dir.path(), config.clone()).await;
        assert!(status.running, "{status:?}");
        assert_eq!(status.listen_address, "127.0.0.1");

        let response = reqwest::Client::new()
            .patch(format!(
                "http://127.0.0.1:{}/messages?model=test%2Fmodel",
                config.port
            ))
            .header("authorization", "Bearer plain-request-secret")
            .body(r#"{"input":"plain request body"}"#)
            .send()
            .await
            .expect("proxy response");
        assert_eq!(response.status(), StatusCode::IM_A_TEAPOT);
        assert_eq!(
            response.text().await.expect("response body"),
            "upstream failure payload"
        );

        assert_eq!(
            observed.lock().expect("read observed request").clone(),
            Some(ObservedRequest {
                method: "PATCH".to_string(),
                path_and_query: "/api/v1/messages?model=test%2Fmodel".to_string(),
                authorization: Some("Bearer plain-request-secret".to_string()),
                accept_encoding: Some("identity".to_string()),
                forwarded_hop_header: None,
                host: Some(format!("127.0.0.1:{upstream_port}")),
                body: br#"{"input":"plain request body"}"#.to_vec(),
            })
        );

        let detail = wait_for_completed_capture(dir.path()).await;
        assert_eq!(detail.response_status, Some(418));
        assert_eq!(
            header_values(&detail.request_headers, "authorization"),
            vec!["Bearer plain-request-secret"]
        );
        assert_eq!(
            detail.request_body.data,
            br#"{"input":"plain request body"}"#
        );
        assert_eq!(detail.response_body.data, b"upstream failure payload");
        assert_eq!(
            header_values(&detail.response_headers, "x-upstream-secret"),
            vec!["plain-response-secret"]
        );
        assert_eq!(
            header_values(&detail.response_headers, "x-remove-from-client"),
            vec!["must-not-reach-client"]
        );

        let stopped = runtime::stop().await;
        assert!(!stopped.running);
        let _ = upstream_shutdown.send(());
    }

    #[tokio::test]
    async fn records_upstream_connection_failure_and_returns_bad_gateway() {
        let _guard = proxy_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = TestDir::new();
        let config = AiRequestCaptureConfig {
            enabled: true,
            port: free_loopback_port(),
            upstream_base_url: format!("http://127.0.0.1:{}", free_loopback_port()),
        };
        assert!(runtime::start_in(dir.path(), config.clone()).await.running);

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/unavailable", config.port))
            .body("plain request")
            .send()
            .await
            .expect("proxy error response");
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

        let capture = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Some(capture) = CaptureStore::open(database_path_in(dir.path()))
                    .expect("open capture store")
                    .list(CaptureListQuery::default())
                    .expect("list capture")
                    .items
                    .pop()
                {
                    if capture.state == CaptureState::UpstreamError {
                        return capture;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("upstream error capture completes");
        assert_eq!(capture.response_status, Some(502));
        assert!(!runtime::stop().await.running);
    }

    #[tokio::test]
    async fn rejects_connect_and_websocket_upgrade_requests() {
        let _guard = proxy_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = TestDir::new();
        let config = AiRequestCaptureConfig {
            enabled: true,
            port: free_loopback_port(),
            upstream_base_url: format!("http://127.0.0.1:{}", free_loopback_port()),
        };
        assert!(runtime::start_in(dir.path(), config.clone()).await.running);
        let client = reqwest::Client::new();

        let connect = client
            .request(
                reqwest::Method::CONNECT,
                format!("http://127.0.0.1:{}/tunnel", config.port),
            )
            .send()
            .await
            .expect("CONNECT rejection response");
        assert_eq!(connect.status(), StatusCode::METHOD_NOT_ALLOWED);

        let upgrade = client
            .get(format!("http://127.0.0.1:{}/socket", config.port))
            .header("connection", "Upgrade")
            .header("upgrade", "websocket")
            .send()
            .await
            .expect("Upgrade rejection response");
        assert_eq!(upgrade.status(), StatusCode::BAD_REQUEST);
        assert!(!runtime::stop().await.running);
    }

    #[tokio::test]
    async fn rejects_loopback_cycles_and_reports_port_conflicts_without_running() {
        let _guard = proxy_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = TestDir::new();
        let loopback_port = free_loopback_port();
        let loopback = runtime::start_in(
            dir.path(),
            AiRequestCaptureConfig {
                enabled: true,
                port: loopback_port,
                upstream_base_url: format!("http://127.0.0.1:{loopback_port}"),
            },
        )
        .await;
        assert!(!loopback.running);
        assert!(loopback.last_error.is_some());

        let occupied = StdTcpListener::bind("127.0.0.1:0").expect("occupy loopback port");
        let conflict_port = occupied.local_addr().expect("occupied address").port();
        let conflict = runtime::start_in(
            dir.path(),
            AiRequestCaptureConfig {
                enabled: true,
                port: conflict_port,
                upstream_base_url: "http://example.test".to_string(),
            },
        )
        .await;
        assert!(!conflict.running);
        assert!(conflict.last_error.is_some());
        drop(occupied);
    }

    #[tokio::test]
    async fn start_stop_and_restart_are_idempotent() {
        let _guard = proxy_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = TestDir::new();
        let first = AiRequestCaptureConfig {
            enabled: true,
            port: free_loopback_port(),
            upstream_base_url: format!("http://127.0.0.1:{}", free_loopback_port()),
        };
        let first_status = runtime::start_in(dir.path(), first.clone()).await;
        assert!(first_status.running);
        let repeated = runtime::start_in(dir.path(), first).await;
        assert!(repeated.running);
        assert_eq!(repeated.port, first_status.port);

        let restart = AiRequestCaptureConfig {
            enabled: true,
            port: free_loopback_port(),
            upstream_base_url: format!("http://127.0.0.1:{}", free_loopback_port()),
        };
        let restarted = runtime::start_in(dir.path(), restart.clone()).await;
        assert!(restarted.running);
        assert_eq!(restarted.port, restart.port);
        assert!(!runtime::stop().await.running);
        assert!(!runtime::stop().await.running);
    }
}

mod streaming_fidelity {
    use super::*;

    #[tokio::test]
    async fn forwards_the_first_sse_chunk_before_upstream_completion_and_persists_the_full_capture()
    {
        let _guard = proxy_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = TestDir::new();
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind SSE upstream");
        let upstream_port = listener.local_addr().expect("read upstream port").port();
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let (first_sent_tx, first_sent_rx) = oneshot::channel::<()>();
        let release_rx = Arc::new(Mutex::new(Some(release_rx)));
        let first_sent_tx = Arc::new(Mutex::new(Some(first_sent_tx)));
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        tokio::spawn(async move {
            tokio::select! {
                _ = &mut shutdown_rx => {}
                accepted = listener.accept() => {
                    let (stream, _) = accepted.expect("accept SSE client");
                    let release_rx = Arc::clone(&release_rx);
                    let first_sent_tx = Arc::clone(&first_sent_tx);
                    let service = service_fn(move |_request: Request<hyper::body::Incoming>| {
                        let first_sent_tx = first_sent_tx
                            .lock()
                            .expect("lock first chunk signal")
                            .take();
                        let release_rx = release_rx.lock().expect("lock release receiver").take();
                        async move {
                            let stream = stream::unfold(
                                (0_u8, first_sent_tx, release_rx),
                                |(step, first_sent_tx, release_rx)| async move {
                                    match step {
                                        0 => {
                                            first_sent_tx
                                                .expect("send first chunk signal")
                                                .send(())
                                                .expect("signal first chunk");
                                            Some((
                                                Ok::<_, std::convert::Infallible>(Frame::data(
                                                    Bytes::from_static(b"data: first\n\n"),
                                                )),
                                                (1, None, release_rx),
                                            ))
                                        }
                                        1 => {
                                            release_rx
                                                .expect("wait for test release")
                                                .await
                                                .expect("release SSE response");
                                            Some((
                                                Ok(Frame::data(Bytes::from_static(
                                                    b"data: final\n\n",
                                                ))),
                                                (2, None, None),
                                            ))
                                        }
                                        _ => None,
                                    }
                                },
                            );
                            Ok::<_, hyper::Error>(Response::builder()
                                .status(StatusCode::OK)
                                .header("content-type", "text/event-stream")
                                .body(BodyExt::boxed(StreamBody::new(stream)))
                                .expect("build SSE response"))
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                }
            }
        });

        let config = AiRequestCaptureConfig {
            enabled: true,
            port: free_loopback_port(),
            upstream_base_url: format!("http://127.0.0.1:{upstream_port}"),
        };
        assert!(runtime::start_in(dir.path(), config.clone()).await.running);

        let response = match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            reqwest::Client::new()
                .get(format!("http://127.0.0.1:{}/stream", config.port))
                .send(),
        )
        .await
        {
            Ok(response) => response.expect("send SSE request"),
            Err(_) => {
                let _ = release_tx.send(());
                panic!("proxy did not return SSE headers before upstream completion");
            }
        };
        first_sent_rx.await.expect("upstream sent first SSE chunk");
        let mut chunks = response.bytes_stream();
        assert_eq!(
            chunks
                .next()
                .await
                .expect("first SSE body result")
                .expect("first SSE body bytes"),
            Bytes::from_static(b"data: first\n\n")
        );

        let capture = CaptureStore::open(database_path_in(dir.path()))
            .expect("open capture store")
            .list(CaptureListQuery::default())
            .expect("list capture")
            .items
            .pop()
            .expect("in-progress capture");
        assert_eq!(capture.state, CaptureState::InProgress);

        release_tx.send(()).expect("release final SSE chunk");
        assert_eq!(
            chunks
                .next()
                .await
                .expect("final SSE body result")
                .expect("final SSE body bytes"),
            Bytes::from_static(b"data: final\n\n")
        );
        assert!(chunks.next().await.is_none());

        let capture = wait_for_completed_capture(dir.path()).await;
        assert_eq!(
            capture.response_body.data,
            b"data: first\n\ndata: final\n\n"
        );
        assert_eq!(capture.response_body.total_bytes, 26);
        assert!(!capture.response_body.truncated);
        assert!(!runtime::stop().await.running);
        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn tees_chunked_large_bodies_without_forwarding_hop_by_hop_headers() {
        let _guard = proxy_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = TestDir::new();
        let observed = Arc::new(Mutex::new(None));
        let body = vec![0xFF; CAPTURE_BODY_LIMIT_BYTES + 257];
        let (upstream_port, upstream_shutdown) =
            start_mock_upstream(Arc::clone(&observed), Bytes::from(body.clone())).await;
        let config = AiRequestCaptureConfig {
            enabled: true,
            port: free_loopback_port(),
            upstream_base_url: format!("http://127.0.0.1:{upstream_port}"),
        };
        assert!(runtime::start_in(dir.path(), config.clone()).await.running);
        let midpoint = body.len() / 2;
        let request_body = reqwest::Body::wrap_stream(stream::iter(vec![
            Ok::<_, std::io::Error>(Bytes::copy_from_slice(&body[..midpoint])),
            Ok(Bytes::copy_from_slice(&body[midpoint..])),
        ]));

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/large", config.port))
            .header("connection", "x-remove-from-forward")
            .header("x-remove-from-forward", "remove-me")
            .header("host", "client.example.test")
            .body(request_body)
            .send()
            .await
            .expect("send large chunked request");
        assert!(response.headers().get("x-remove-from-client").is_none());
        let response_body = response.bytes().await.expect("read large response");
        assert_eq!(response_body, body);

        let observed = observed
            .lock()
            .expect("read observed request")
            .clone()
            .expect("upstream received request");
        assert_eq!(observed.body, body);
        assert_eq!(observed.accept_encoding.as_deref(), Some("identity"));
        assert_eq!(observed.forwarded_hop_header, None);
        let expected_upstream_host = format!("127.0.0.1:{upstream_port}");
        assert_eq!(
            observed.host.as_deref(),
            Some(expected_upstream_host.as_str())
        );

        let capture = wait_for_completed_capture(dir.path()).await;
        assert_eq!(
            capture.request_body.captured_bytes,
            CAPTURE_BODY_LIMIT_BYTES as u64
        );
        assert_eq!(capture.request_body.total_bytes, body.len() as u64);
        assert!(capture.request_body.truncated);
        assert_eq!(capture.request_body.data.len(), CAPTURE_BODY_LIMIT_BYTES);
        assert_eq!(
            capture.response_body.captured_bytes,
            CAPTURE_BODY_LIMIT_BYTES as u64
        );
        assert_eq!(capture.response_body.total_bytes, body.len() as u64);
        assert!(capture.response_body.truncated);
        assert_eq!(capture.response_body.data.len(), CAPTURE_BODY_LIMIT_BYTES);
        assert!(capture.response_body.data.iter().all(|byte| *byte == 0xFF));
        assert_eq!(
            header_values(&capture.request_headers, "x-remove-from-forward"),
            vec!["remove-me"]
        );
        assert_eq!(
            header_values(&capture.response_headers, "x-remove-from-client"),
            vec!["must-not-reach-client"]
        );
        assert!(!runtime::stop().await.running);
        let _ = upstream_shutdown.send(());
    }
}

mod export_enrichment {
    use super::*;
    use crate::ai_request_capture::{enrichment, export};

    fn headers(name: &str, value: &str) -> Vec<AiRequestCaptureHeader> {
        vec![AiRequestCaptureHeader {
            name: name.to_string(),
            values: vec![value.to_string()],
        }]
    }

    #[test]
    fn extracts_provider_model_and_tokens_from_openai_anthropic_and_gemini_json_and_sse() {
        let fixtures = [
            (
                "https://api.openai.com/v1/chat/completions",
                br#"{"model":"gpt-4.1"}"#.as_slice(),
                br#"{"model":"gpt-4.1","usage":{"prompt_tokens":3,"completion_tokens":5,"total_tokens":8}}"#.as_slice(),
                "openai",
                "gpt-4.1",
                (3, 5, 8),
            ),
            (
                "https://api.openai.com/v1/chat/completions",
                br#"{"model":"gpt-4.1-mini"}"#.as_slice(),
                b"data: {\"model\":\"gpt-4.1-mini\",\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":4,\"total_tokens\":6}}\n\ndata: [DONE]\n\n",
                "openai",
                "gpt-4.1-mini",
                (2, 4, 6),
            ),
            (
                "https://api.anthropic.com/v1/messages",
                br#"{"model":"claude-sonnet-4"}"#.as_slice(),
                br#"{"model":"claude-sonnet-4","usage":{"input_tokens":7,"output_tokens":11}}"#.as_slice(),
                "anthropic",
                "claude-sonnet-4",
                (7, 11, 18),
            ),
            (
                "https://api.anthropic.com/v1/messages",
                br#"{"model":"claude-haiku-4"}"#.as_slice(),
                b"event: message_start\ndata: {\"message\":{\"model\":\"claude-haiku-4\",\"usage\":{\"input_tokens\":9}}}\n\nevent: message_delta\ndata: {\"usage\":{\"output_tokens\":12}}\n\n",
                "anthropic",
                "claude-haiku-4",
                (9, 12, 21),
            ),
            (
                "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent",
                br#"{"contents":[]}"#.as_slice(),
                br#"{"modelVersion":"gemini-2.5-pro","usageMetadata":{"promptTokenCount":13,"candidatesTokenCount":17,"totalTokenCount":30}}"#.as_slice(),
                "gemini",
                "gemini-2.5-pro",
                (13, 17, 30),
            ),
            (
                "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent",
                br#"{"contents":[]}"#.as_slice(),
                b"data: {\"modelVersion\":\"gemini-2.5-flash\",\"usageMetadata\":{\"promptTokenCount\":19,\"candidatesTokenCount\":23,\"totalTokenCount\":42}}\n\n",
                "gemini",
                "gemini-2.5-flash",
                (19, 23, 42),
            ),
        ];

        for (url, request, response, provider, model, tokens) in fixtures {
            let enriched = enrichment::enrich(url, request, response);
            assert_eq!(enriched.provider.as_deref(), Some(provider), "{url}");
            assert_eq!(enriched.model.as_deref(), Some(model), "{url}");
            assert_eq!(
                (
                    enriched.input_tokens,
                    enriched.output_tokens,
                    enriched.total_tokens
                ),
                (Some(tokens.0), Some(tokens.1), Some(tokens.2)),
                "{url}"
            );
        }
    }

    #[test]
    fn ignores_unknown_invalid_and_truncated_ai_samples() {
        for (url, request, response) in [
            (
                "https://example.test/v1/messages",
                b"{}".as_slice(),
                b"{}".as_slice(),
            ),
            (
                "https://api.openai.com/v1/chat/completions",
                b"{invalid".as_slice(),
                b"data: {invalid\n\n".as_slice(),
            ),
            (
                "https://api.anthropic.com/v1/messages",
                br#"{\"model\":\"claude"#.as_slice(),
                br#"{\"usage\":{\"input_tokens\":1"#.as_slice(),
            ),
        ] {
            let enriched = enrichment::enrich(url, request, response);
            if url.contains("example.test") {
                assert!(enriched.provider.is_none());
            }
            assert!(enriched.input_tokens.is_none());
            assert!(enriched.output_tokens.is_none());
            assert!(enriched.total_tokens.is_none());
        }
    }

    #[test]
    fn represents_valid_text_as_text_and_binary_as_base64() {
        let text = enrichment::body_representation(
            &headers("content-type", "application/json"),
            &CapturedBody::from_bytes(br#"{"key":"value"}"#.to_vec(), 15),
        );
        assert_eq!(text.data, r#"{"key":"value"}"#);
        assert_eq!(text.encoding, None);

        let binary = enrichment::body_representation(
            &headers("content-type", "application/octet-stream"),
            &CapturedBody::from_bytes(vec![0, 0xff, 1], 3),
        );
        assert_eq!(binary.data, "AP8B");
        assert_eq!(binary.encoding.as_deref(), Some("base64"));
    }

    #[test]
    fn filters_paginates_and_searches_request_and_response_bodies_in_sqlite() {
        let dir = TestDir::new();
        let store = CaptureStore::open(dir.path().join("captures.sqlite3")).expect("open store");
        for (id, state, model, request, response) in [
            (
                "first",
                CaptureState::Completed,
                "gpt-4.1",
                b"request needle".as_slice(),
                b"ok".as_slice(),
            ),
            (
                "second",
                CaptureState::UpstreamError,
                "gpt-4.1-mini",
                b"no".as_slice(),
                b"response needle".as_slice(),
            ),
            (
                "third",
                CaptureState::Completed,
                "claude-sonnet",
                b"no".as_slice(),
                b"no".as_slice(),
            ),
        ] {
            let mut started = start(id, 100, "POST");
            started.model = Some(model.to_string());
            started.request_body = CapturedBody::from_bytes(request.to_vec(), request.len() as u64);
            store.begin(started).expect("begin capture");
            let mut completed = finish(130, state);
            completed.response_body =
                CapturedBody::from_bytes(response.to_vec(), response.len() as u64);
            store.finish(id, completed).expect("finish capture");
        }

        let searched = store
            .list(CaptureListQuery {
                search: Some("needle".to_string()),
                page: 1,
                page_size: 1,
                ..Default::default()
            })
            .expect("search bodies");
        assert_eq!(searched.total, 2);
        assert_eq!(searched.items.len(), 1);

        let filtered = store
            .list(CaptureListQuery {
                states: vec![CaptureState::Completed],
                model: Some("gpt-4.1".to_string()),
                page: 1,
                page_size: 10,
                ..Default::default()
            })
            .expect("filter state and model");
        assert_eq!(
            filtered
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["first"]
        );
    }

    #[test]
    fn creates_har_for_filtered_finished_records_with_plain_secrets_binary_body_and_completeness() {
        let dir = TestDir::new();
        let store = CaptureStore::open(dir.path().join("captures.sqlite3")).expect("open store");
        let mut completed = start("finished", 100, "POST");
        completed.request_headers = headers("authorization", "Bearer plain-secret");
        completed.request_body = CapturedBody::from_bytes(vec![0, 0xff], 5);
        store.begin(completed).expect("begin finished");
        let mut result = finish(130, CaptureState::ResponseTransferError);
        result.response_headers = headers("set-cookie", "session=plain-secret");
        result.response_body = CapturedBody::from_bytes(b"reply".to_vec(), 5);
        result.error = Some("upstream stream failed".to_string());
        store.finish("finished", result).expect("finish record");
        store
            .begin(start("active", 200, "GET"))
            .expect("begin active");

        let har = export::har_document(
            &store
                .finished_for_export(CaptureListQuery {
                    method: Some("POST".to_string()),
                    ..Default::default()
                })
                .expect("load finished records"),
        )
        .expect("build HAR");
        let entry = &har["log"]["entries"][0];
        assert_eq!(har["log"]["version"], "1.2");
        assert_eq!(har["log"]["entries"].as_array().expect("entries").len(), 1);
        assert_eq!(
            entry["request"]["headers"][0]["value"],
            "Bearer plain-secret"
        );
        assert_eq!(
            entry["response"]["headers"][0]["value"],
            "session=plain-secret"
        );
        assert_eq!(entry["request"]["postData"]["text"], "AP8=");
        assert_eq!(entry["request"]["postData"]["encoding"], "base64");
        assert_eq!(entry["_onespace"]["request"]["truncated"], true);
        assert!(entry["comment"]
            .as_str()
            .expect("comment")
            .contains("response_transfer_error"));
    }

    #[test]
    fn generates_safe_curl_for_text_binary_empty_and_incomplete_requests() {
        let mut text = start("text", 100, "POST");
        text.upstream_url = "https://api.example.test/v1/messages?name=O'Reilly".to_string();
        text.request_headers = vec![
            AiRequestCaptureHeader {
                name: "authorization".to_string(),
                values: vec!["Bearer real-secret".to_string()],
            },
            AiRequestCaptureHeader {
                name: "connection".to_string(),
                values: vec!["x-remove".to_string()],
            },
            AiRequestCaptureHeader {
                name: "x-remove".to_string(),
                values: vec!["skip".to_string()],
            },
            AiRequestCaptureHeader {
                name: "host".to_string(),
                values: vec!["proxy.example.test".to_string()],
            },
            AiRequestCaptureHeader {
                name: "content-length".to_string(),
                values: vec!["999".to_string()],
            },
        ];
        text.request_body = CapturedBody::from_bytes(b"O'Reilly\n".to_vec(), 9);
        let text = export::curl_command(&detail_from_start(text, CaptureState::Completed));
        assert!(text.complete);
        assert!(text.command.contains("Bearer real-secret"));
        assert!(text.command.contains("O'\\''Reilly"));
        assert!(!text.command.contains("x-remove: skip"));
        assert!(!text.command.contains("host: proxy.example.test"));
        assert!(!text.command.contains("content-length: 999"));

        let mut binary = start("binary", 100, "PUT");
        binary.request_body = CapturedBody::from_bytes(vec![0, 0xff, b'\n'], 3);
        let binary = export::curl_command(&detail_from_start(binary, CaptureState::Completed));
        assert!(binary.complete);
        assert!(binary.command.starts_with("printf '%b'"));
        assert!(binary.command.contains("\\000\\377\\012"));
        assert!(binary.command.contains("--data-binary @-"));

        let mut empty_start = start("empty", 100, "GET");
        empty_start.request_body = CapturedBody::from_bytes(Vec::new(), 0);
        let empty = export::curl_command(&detail_from_start(empty_start, CaptureState::Completed));
        assert!(!empty.command.contains("--data-binary"));

        let mut incomplete = start("incomplete", 100, "POST");
        incomplete.request_body = CapturedBody::from_bytes(b"partial".to_vec(), 99);
        let incomplete = export::curl_command(&detail_from_start(
            incomplete,
            CaptureState::ResponseTransferError,
        ));
        assert!(!incomplete.complete);
        assert!(incomplete.warning.is_some());
        assert!(incomplete.command.starts_with("# WARNING:"));
    }

    fn detail_from_start(start: CaptureStart, state: CaptureState) -> AiRequestCaptureDetail {
        AiRequestCaptureDetail {
            id: start.id,
            started_at: start.started_at,
            completed_at: Some(start.started_at + 1),
            state,
            http_version: start.http_version,
            method: start.method,
            request_path_and_query: start.request_path_and_query,
            upstream_url: start.upstream_url,
            request_headers: start.request_headers,
            request_body: start.request_body,
            response_status: Some(200),
            response_headers: Vec::new(),
            response_body: CapturedBody::from_bytes(Vec::new(), 0),
            duration_ms: Some(1),
            error: None,
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
        }
    }
}

mod integration_acceptance {
    use super::*;
    use crate::ai_request_capture::export;

    #[tokio::test]
    async fn integration_acceptance() {
        let _guard = proxy_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = TestDir::new();
        let observed = Arc::new(Mutex::new(None));
        let (upstream_port, upstream_shutdown) =
            start_mock_upstream(Arc::clone(&observed), Bytes::from_static(br#"{"ok":true}"#)).await;
        let config = AiRequestCaptureConfig {
            enabled: true,
            port: free_loopback_port(),
            upstream_base_url: format!("http://127.0.0.1:{upstream_port}/v1"),
        };

        config::write_config_in(dir.path(), &config).expect("persist local capture config");
        assert_eq!(
            config::read_config_in(dir.path()).expect("restore capture config"),
            config
        );
        assert!(runtime::start_in(dir.path(), config.clone()).await.running);

        let response = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{}/messages?stream=false",
                config.port
            ))
            .header("authorization", "Bearer test-secret")
            .header("connection", "x-remove-from-forward")
            .header("x-remove-from-forward", "remove-me")
            .body(r#"{"model":"gpt-test","input":"hello"}"#)
            .send()
            .await
            .expect("send local proxy request");
        assert_eq!(response.status(), StatusCode::IM_A_TEAPOT);
        assert_eq!(
            response.text().await.expect("read proxy response"),
            r#"{"ok":true}"#
        );

        let upstream = observed
            .lock()
            .expect("read observed upstream request")
            .clone()
            .expect("upstream received request");
        assert_eq!(upstream.path_and_query, "/v1/messages?stream=false");
        assert_eq!(
            upstream.authorization.as_deref(),
            Some("Bearer test-secret")
        );
        assert_eq!(upstream.forwarded_hop_header, None);

        let capture = wait_for_completed_capture(dir.path()).await;
        assert_eq!(
            capture.request_body.data,
            br#"{"model":"gpt-test","input":"hello"}"#
        );
        assert_eq!(capture.response_body.data, br#"{"ok":true}"#);
        let har = export::har_document(&[capture.clone()]).expect("export HAR");
        assert!(har["log"]["entries"][0]["request"]["headers"]
            .as_array()
            .expect("HAR request headers")
            .iter()
            .any(|header| header["name"] == "authorization"
                && header["value"] == "Bearer test-secret"));
        assert!(export::curl_command(&capture)
            .command
            .contains("Bearer test-secret"));

        assert!(!runtime::stop().await.running);
        let _ = upstream_shutdown.send(());
    }
}
