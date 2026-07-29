use super::http::{parse_range, safe_header_filename, DownloadStream, ParsedRange};
use super::runtime::{
    begin_transfer, is_private_ipv4, networks_from, Session, SharedFile, SharedSession,
};
use super::types::{FileSharingSummary, FileSharingTransferState};
use futures_util::StreamExt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

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
    Arc::new(Session {
        token: "test-token".to_string(),
        files: Vec::new(),
        transfers: Mutex::new(Vec::new()),
        summary: Mutex::new(FileSharingSummary::default()),
        cancellation: CancellationToken::new(),
    })
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
