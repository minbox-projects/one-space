use super::http::{
    listing_document, parse_range, safe_header_filename, DownloadStream, ParsedRange,
};
use super::runtime::{
    begin_transfer, finish_transfer, is_private_ipv4, networks_from, run_listener,
    should_emit_transfer_update, unexpected_exit_snapshot, SharedFile, SharedSession,
};
use super::types::{FileSharingSnapshot, FileSharingTransferState};
use futures_util::StreamExt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

#[test]
fn filters_and_sorts_private_ipv4_networks() {
    let networks = networks_from(vec![
        ("z".to_string(), IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))),
        ("a".to_string(), IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2))),
        ("loopback".to_string(), IpAddr::V4(Ipv4Addr::LOCALHOST)),
        ("public".to_string(), IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))),
        ("v6".to_string(), IpAddr::V6(Ipv6Addr::LOCALHOST)),
        (
            "duplicate".to_string(),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
        ),
    ]);
    assert_eq!(
        networks
            .iter()
            .map(|network| network.address.as_str())
            .collect::<Vec<_>>(),
        vec!["10.0.0.2", "192.168.1.2"]
    );
    assert!(is_private_ipv4(Ipv4Addr::new(172, 16, 0, 1)));
    assert!(!is_private_ipv4(Ipv4Addr::new(172, 32, 0, 1)));
}

#[test]
fn parses_single_ranges_and_rejects_invalid_ranges() {
    assert_eq!(parse_range(Some("bytes=2-4"), 10), ParsedRange::Bytes(2, 4));
    assert_eq!(parse_range(Some("bytes=7-"), 10), ParsedRange::Bytes(7, 9));
    assert_eq!(parse_range(Some("bytes=-3"), 10), ParsedRange::Bytes(7, 9));
    assert_eq!(parse_range(Some("bytes=1-2,4-5"), 10), ParsedRange::Full);
    assert_eq!(parse_range(Some("bytes=20-30"), 10), ParsedRange::Invalid);
}

#[test]
fn strips_control_characters_from_download_headers() {
    assert_eq!(
        safe_header_filename("report\r\nX-Test: bad.txt"),
        "reportX-Test: bad.txt"
    );
    assert_eq!(safe_header_filename("文件.txt"), "__.txt");
}

fn test_session() -> SharedSession {
    super::runtime::test_session(Vec::new(), 64, Duration::from_secs(1))
}

fn test_file(path: PathBuf) -> SharedFile {
    SharedFile {
        id: "file-1".to_string(),
        name: "download.bin".to_string(),
        path,
        size: 128 * 1024,
        modified_at: 0,
    }
}

#[tokio::test]
async fn cancellation_stops_active_download_and_records_cancelled() {
    let path = std::env::temp_dir().join(format!(
        "onespace-file-sharing-{}.bin",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&path, vec![7_u8; 128 * 1024]).unwrap();
    let session = test_session();
    let file = test_file(path.clone());
    let transfer_id = begin_transfer(&session, &file, "127.0.0.1".to_string(), file.size);
    let handle = tokio::fs::File::open(&path).await.unwrap();
    let mut stream = DownloadStream::new(handle, file.size, session.clone(), transfer_id);

    let first = stream
        .next()
        .await
        .expect("first chunk")
        .expect("read chunk");
    assert!(first
        .data_ref()
        .map(|data| !data.is_empty())
        .unwrap_or(false));
    session.cancellation.cancel();
    assert!(stream.next().await.is_none());

    let records = session.transfers.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, FileSharingTransferState::Cancelled);
    assert!(records[0].bytes_sent > 0 && records[0].bytes_sent < file.size);
    drop(records);
    let summary = session.summary.lock().unwrap();
    assert_eq!(summary.cancelled_transfers, 1);
    assert_eq!(summary.completed_transfers, 0);
    assert_eq!(summary.active_transfers, 0);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn completed_download_is_not_recorded_as_cancelled() {
    let path = std::env::temp_dir().join(format!(
        "onespace-file-sharing-{}.bin",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&path, vec![9_u8; 8 * 1024]).unwrap();
    let session = test_session();
    let file = test_file(path.clone());
    let transfer_id = begin_transfer(&session, &file, "127.0.0.1".to_string(), 8 * 1024);
    let handle = tokio::fs::File::open(&path).await.unwrap();
    let mut stream = DownloadStream::new(handle, 8 * 1024, session.clone(), transfer_id);
    while stream.next().await.is_some() {}

    let records = session.transfers.lock().unwrap();
    assert_eq!(records[0].state, FileSharingTransferState::Completed);
    assert_eq!(records[0].bytes_sent, 8 * 1024);
    drop(records);
    let summary = session.summary.lock().unwrap();
    assert_eq!(summary.completed_transfers, 1);
    assert_eq!(summary.cancelled_transfers, 0);
    let _ = std::fs::remove_file(path);
}

#[test]
fn listing_localizes_file_count_and_total_size() {
    let session = super::runtime::test_session(
        vec![
            test_file(PathBuf::from("/tmp/a.bin")),
            SharedFile {
                id: "file-2".to_string(),
                name: "b.bin".to_string(),
                path: PathBuf::from("/tmp/b.bin"),
                size: 1024,
                modified_at: 0,
            },
        ],
        64,
        Duration::from_secs(1),
    );
    let english = listing_document(&session, Some("en-US,en;q=0.9"));
    let chinese = listing_document(&session, Some("zh-CN,zh;q=0.9"));
    let english_with_chinese_fallback = listing_document(&session, Some("en-US,zh;q=0.9"));
    assert!(english.contains("2 files, 129.0 KB"));
    assert!(chinese.contains("2 个文件，共 129.0 KB"));
    assert!(chinese.contains("lang=\"zh\""));
    assert!(english_with_chinese_fallback.contains("lang=\"en\""));
}

#[test]
fn terminal_records_are_evicted_before_active_transfers() {
    let session = test_session();
    let file = test_file(PathBuf::from("/tmp/download.bin"));
    for _ in 0..200 {
        let id = begin_transfer(&session, &file, "127.0.0.1".to_string(), 1);
        finish_transfer(&session, &id, FileSharingTransferState::Completed, 1, None);
    }
    let active = begin_transfer(&session, &file, "127.0.0.1".to_string(), 1);
    for _ in 0..10 {
        let id = begin_transfer(&session, &file, "127.0.0.1".to_string(), 1);
        finish_transfer(&session, &id, FileSharingTransferState::Completed, 1, None);
    }
    finish_transfer(
        &session,
        &active,
        FileSharingTransferState::Completed,
        1,
        None,
    );
    let summary = session.summary.lock().unwrap();
    assert_eq!(summary.active_transfers, 0);
    assert_eq!(summary.completed_transfers, 211);
    assert_eq!(summary.bytes_sent, 211);
    assert_eq!(summary.dropped_transfer_records, 11);
}

#[test]
fn unexpected_listener_exit_cancels_active_transfers_and_preserves_error() {
    let session = test_session();
    let file = test_file(PathBuf::from("/tmp/download.bin"));
    let id = begin_transfer(&session, &file, "127.0.0.1".to_string(), 1);
    let snapshot = unexpected_exit_snapshot(
        FileSharingSnapshot {
            running: true,
            share_url: Some("http://test".to_string()),
            ..FileSharingSnapshot::default()
        },
        &session,
        "accept failed".to_string(),
    );
    assert!(!snapshot.running);
    assert!(snapshot.share_url.is_none());
    assert_eq!(snapshot.last_error.as_deref(), Some("accept failed"));
    assert!(session.cancellation.is_cancelled());
    assert_eq!(
        snapshot
            .transfers
            .iter()
            .find(|record| record.id == id)
            .unwrap()
            .state,
        FileSharingTransferState::Cancelled
    );
}

#[test]
fn transfer_updates_are_throttled_but_terminal_events_are_immediate() {
    let now = std::time::Instant::now();
    assert!(should_emit_transfer_update(None, now, false));
    assert!(!should_emit_transfer_update(
        Some(now),
        now + Duration::from_millis(249),
        false
    ));
    assert!(should_emit_transfer_update(
        Some(now),
        now + Duration::from_millis(250),
        false
    ));
    assert!(should_emit_transfer_update(Some(now), now, true));
}

async fn start_loopback_session(
    session: SharedSession,
) -> (
    std::net::SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = run_listener(listener, session, &mut shutdown_rx).await;
    });
    (address, shutdown_tx, task)
}

async fn request_once(address: std::net::SocketAddr, request: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test]
async fn loopback_http_preserves_head_headers_and_security_headers() {
    let session = test_session();
    let (address, shutdown, task) = start_loopback_session(session).await;
    let head = request_once(
        address,
        "HEAD /s/test-token/ HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(head.starts_with("HTTP/1.1 200"));
    assert!(head.contains("content-length:"));
    assert!(head.contains("cache-control: no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));
    assert!(head.ends_with("\r\n\r\n"));
    let missing = request_once(
        address,
        "GET /missing HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(missing.starts_with("HTTP/1.1 404"));
    assert!(missing.contains("cache-control: no-store"));
    assert!(missing.contains("referrer-policy: no-referrer"));
    assert!(missing.contains("x-content-type-options: nosniff"));
    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn loopback_missing_file_records_failed_transfer_and_stop_closes_existing_connection() {
    let missing = std::env::temp_dir().join(format!(
        "onespace-file-sharing-missing-{}",
        uuid::Uuid::new_v4()
    ));
    let file = test_file(missing);
    let session = super::runtime::test_session(vec![file], 64, Duration::from_secs(1));
    let (address, shutdown, task) = start_loopback_session(session.clone()).await;
    let missing_response = request_once(
        address,
        "GET /s/test-token/files/file-1 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(missing_response.starts_with("HTTP/1.1 404"));
    let records = session.transfers.lock().unwrap();
    assert_eq!(records[0].state, FileSharingTransferState::Failed);
    assert_eq!(
        records[0].error.as_deref(),
        Some("shared file is unavailable")
    );
    drop(records);

    let mut keep_alive = tokio::net::TcpStream::connect(address).await.unwrap();
    keep_alive
        .write_all(b"GET /s/test-token/ HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut buffer = [0_u8; 4096];
    assert!(keep_alive.read(&mut buffer).await.unwrap() > 0);
    session.cancellation.cancel();
    let mut remaining = Vec::new();
    let _ = tokio::time::timeout(
        Duration::from_secs(1),
        keep_alive.read_to_end(&mut remaining),
    )
    .await
    .unwrap()
    .unwrap();
    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn loopback_connection_limit_and_header_timeout_release_resources() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let session = super::runtime::test_session(Vec::new(), 1, Duration::from_millis(25));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task_session = session.clone();
    let listener_task =
        tokio::spawn(async move { run_listener(listener, task_session, &mut shutdown_rx).await });

    let mut first = tokio::net::TcpStream::connect(address).await.unwrap();
    let mut excess = tokio::net::TcpStream::connect(address).await.unwrap();
    excess
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut byte = [0_u8; 1];
    let excess_closed = tokio::time::timeout(Duration::from_secs(1), excess.read(&mut byte))
        .await
        .unwrap();
    assert!(
        matches!(excess_closed, Ok(0))
            || matches!(excess_closed, Err(ref error) if error.kind() == std::io::ErrorKind::ConnectionReset)
    );
    first
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n")
        .await
        .unwrap();
    let first_closed = tokio::time::timeout(Duration::from_secs(1), first.read(&mut byte))
        .await
        .unwrap();
    assert!(
        matches!(first_closed, Ok(0))
            || matches!(first_closed, Err(ref error) if error.kind() == std::io::ErrorKind::ConnectionReset)
    );
    let _ = shutdown_tx.send(());
    let _ = listener_task.await;
}
