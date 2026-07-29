use super::http;
use super::types::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use rand::RngCore;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Emitter;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct SharedFile {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified_at: i64,
}

pub(crate) type SharedSession = Arc<Session>;

pub(crate) struct Session {
    pub token: String,
    pub files: Vec<SharedFile>,
    pub transfers: Mutex<Vec<FileSharingTransfer>>,
    pub summary: Mutex<FileSharingSummary>,
    pub cancellation: CancellationToken,
}

struct ActiveRuntime {
    shutdown: oneshot::Sender<()>,
    cancellation: CancellationToken,
    task: JoinHandle<()>,
}

struct RuntimeState {
    snapshot: FileSharingSnapshot,
    session: Option<SharedSession>,
    active: Option<ActiveRuntime>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            snapshot: FileSharingSnapshot::default(),
            session: None,
            active: None,
        }
    }
}

fn state() -> &'static Mutex<RuntimeState> {
    static STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(RuntimeState::default()))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn is_private_ipv4(address: Ipv4Addr) -> bool {
    address.is_private()
        && !address.is_loopback()
        && !address.is_unspecified()
        && !address.is_multicast()
}

pub(crate) fn networks_from<I>(interfaces: I) -> Vec<FileSharingNetwork>
where
    I: IntoIterator<Item = (String, IpAddr)>,
{
    let mut seen = HashSet::new();
    let mut networks = interfaces
        .into_iter()
        .filter_map(|(interface_name, address)| match address {
            IpAddr::V4(address) if is_private_ipv4(address) && seen.insert(address) => {
                Some(FileSharingNetwork {
                    id: format!("{interface_name}:{address}"),
                    interface_name,
                    address: address.to_string(),
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    networks.sort_by(|left, right| {
        (&left.interface_name, &left.address).cmp(&(&right.interface_name, &right.address))
    });
    networks
}

fn discover_networks() -> Result<Vec<FileSharingNetwork>, String> {
    let interfaces = if_addrs::get_if_addrs()
        .map_err(|error| format!("failed to enumerate network interfaces: {error}"))?;
    Ok(networks_from(
        interfaces
            .into_iter()
            .map(|interface| (interface.name, interface.addr.ip())),
    ))
}

fn snapshot_for(session: &SharedSession, mut snapshot: FileSharingSnapshot) -> FileSharingSnapshot {
    snapshot.files = session
        .files
        .iter()
        .map(|file| FileSharingFile {
            id: file.id.clone(),
            name: file.name.clone(),
            source_path: file.path.display().to_string(),
            size: file.size,
            modified_at: file.modified_at,
        })
        .collect();
    snapshot.transfers = session
        .transfers
        .lock()
        .map(|records| records.clone())
        .unwrap_or_default();
    snapshot.summary = session
        .summary
        .lock()
        .map(|summary| summary.clone())
        .unwrap_or_default();
    snapshot
}

pub(crate) fn begin_transfer(
    session: &SharedSession,
    file: &SharedFile,
    client_address: String,
    response_bytes: u64,
) -> String {
    let timestamp = now_ms();
    let id = Uuid::new_v4().to_string();
    if let Ok(mut summary) = session.summary.lock() {
        summary.active_transfers += 1;
    }
    if let Ok(mut records) = session.transfers.lock() {
        let dropped = records.len() == 200;
        if records.len() == 200 {
            records.remove(0);
        }
        records.push(FileSharingTransfer {
            id: id.clone(),
            file_id: file.id.clone(),
            file_name: file.name.clone(),
            client_address,
            state: FileSharingTransferState::InProgress,
            started_at: timestamp,
            finished_at: None,
            bytes_sent: 0,
            response_bytes,
            error: None,
        });
        if dropped {
            if let Ok(mut summary) = session.summary.lock() {
                summary.dropped_transfer_records += 1;
            }
        }
    }
    id
}

pub(crate) fn finish_transfer(
    session: &SharedSession,
    id: &str,
    transfer_state: FileSharingTransferState,
    bytes_sent: u64,
    error: Option<String>,
) {
    let mut finished = false;
    if let Ok(mut records) = session.transfers.lock() {
        if let Some(record) = records.iter_mut().find(|record| record.id == id) {
            if record.state == FileSharingTransferState::InProgress {
                record.state = transfer_state.clone();
                record.finished_at = Some(now_ms());
                record.bytes_sent = bytes_sent;
                record.error = error;
                finished = true;
            }
        }
    }
    if !finished {
        return;
    }
    if let Ok(mut summary) = session.summary.lock() {
        summary.active_transfers = summary.active_transfers.saturating_sub(1);
        match transfer_state {
            FileSharingTransferState::Completed => summary.completed_transfers += 1,
            FileSharingTransferState::Cancelled => summary.cancelled_transfers += 1,
            FileSharingTransferState::Failed | FileSharingTransferState::ClientDisconnected => {
                summary.failed_transfers += 1
            }
            FileSharingTransferState::InProgress => {}
        }
        summary.bytes_sent += bytes_sent;
    }
}

pub(crate) fn update_transfer_progress(session: &SharedSession, id: &str, bytes_sent: u64) {
    if let Ok(mut records) = session.transfers.lock() {
        if let Some(record) = records.iter_mut().find(|record| record.id == id) {
            if record.state == FileSharingTransferState::InProgress {
                record.bytes_sent = bytes_sent;
            }
        }
    }
}

pub(crate) fn cancel_in_progress_transfers(session: &SharedSession) {
    let ids = session
        .transfers
        .lock()
        .map(|records| {
            records
                .iter()
                .filter(|record| record.state == FileSharingTransferState::InProgress)
                .map(|record| record.id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for id in ids {
        let bytes_sent = session
            .transfers
            .lock()
            .ok()
            .and_then(|records| {
                records
                    .iter()
                    .find(|record| record.id == id)
                    .map(|record| record.bytes_sent)
            })
            .unwrap_or_default();
        finish_transfer(
            session,
            &id,
            FileSharingTransferState::Cancelled,
            bytes_sent,
            None,
        );
    }
}

fn emit_update(app: &tauri::AppHandle, kind: &str) {
    let _ = app.emit("file-sharing-updated", serde_json::json!({ "kind": kind }));
}

fn canonical_files(paths: &[String]) -> Result<Vec<SharedFile>, String> {
    if paths.is_empty() {
        return Err("select at least one file".to_string());
    }
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for source in paths {
        let path =
            std::fs::canonicalize(source).map_err(|_| format!("invalid file path: {source}"))?;
        if !seen.insert(path.clone()) {
            continue;
        }
        let metadata = std::fs::metadata(&path)
            .map_err(|_| format!("cannot read file: {}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!("not a regular file: {}", path.display()));
        }
        std::fs::File::open(&path).map_err(|_| format!("cannot read file: {}", path.display()))?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("invalid file name: {}", path.display()))?
            .to_string();
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or_default();
        files.push(SharedFile {
            id: Uuid::new_v4().simple().to_string(),
            name,
            path,
            size: metadata.len(),
            modified_at,
        });
    }
    if files.is_empty() {
        return Err("select at least one file".to_string());
    }
    Ok(files)
}

pub(crate) fn networks() -> Result<Vec<FileSharingNetwork>, String> {
    discover_networks()
}

pub(crate) async fn start(
    app: tauri::AppHandle,
    input: FileSharingStartInput,
) -> Result<FileSharingSnapshot, String> {
    if state()
        .lock()
        .map_err(|_| "file sharing runtime state is unavailable".to_string())?
        .active
        .is_some()
    {
        return Err("file sharing is already running".to_string());
    }
    let network = discover_networks()?
        .into_iter()
        .find(|network| network.id == input.network_id)
        .ok_or_else(|| "selected network is no longer available".to_string())?;
    let address: Ipv4Addr = network
        .address
        .parse()
        .map_err(|_| "selected network is invalid".to_string())?;
    let files = canonical_files(&input.paths)?;
    let listener = TcpListener::bind((address, 0))
        .await
        .map_err(|error| format!("failed to bind {}: {error}", network.address))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("failed to read listener address: {error}"))?
        .port();
    let mut token_bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);
    let token = URL_SAFE_NO_PAD.encode(token_bytes);
    let session = Arc::new(Session {
        token: token.clone(),
        files,
        transfers: Mutex::new(Vec::new()),
        summary: Mutex::new(FileSharingSummary::default()),
        cancellation: CancellationToken::new(),
    });
    let (shutdown, mut shutdown_rx) = oneshot::channel();
    let handler_session = session.clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                _ = handler_session.cancellation.cancelled() => break,
                accepted = listener.accept() => {
                    let Ok((stream, client_address)) = accepted else { break };
                    let session = handler_session.clone();
                    tokio::spawn(async move {
                        let service = service_fn(move |request| http::handle(request, session.clone(), client_address.ip().to_string()));
                        let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
                    });
                }
            }
        }
    });
    let snapshot = FileSharingSnapshot {
        running: true,
        session_id: Some(Uuid::new_v4().to_string()),
        address: Some(network.address),
        port: Some(port),
        share_url: Some(format!("http://{}:{port}/s/{token}/", address)),
        started_at: Some(now_ms()),
        stopped_at: None,
        files: Vec::new(),
        transfers: Vec::new(),
        summary: FileSharingSummary::default(),
        last_error: None,
    };
    let snapshot = snapshot_for(&session, snapshot);
    let mut runtime = state()
        .lock()
        .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
    runtime.snapshot = snapshot.clone();
    runtime.session = Some(session.clone());
    runtime.active = Some(ActiveRuntime {
        shutdown,
        cancellation: session.cancellation.clone(),
        task,
    });
    drop(runtime);
    emit_update(&app, "session");
    Ok(snapshot)
}

pub(crate) fn status() -> Result<FileSharingSnapshot, String> {
    let runtime = state()
        .lock()
        .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
    Ok(runtime
        .session
        .as_ref()
        .map(|session| snapshot_for(session, runtime.snapshot.clone()))
        .unwrap_or_else(|| runtime.snapshot.clone()))
}

pub(crate) async fn stop(app: tauri::AppHandle) -> Result<FileSharingSnapshot, String> {
    let (active, session, mut snapshot) = {
        let mut runtime = state()
            .lock()
            .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
        (
            runtime.active.take(),
            runtime.session.take(),
            runtime.snapshot.clone(),
        )
    };
    if let Some(active) = active {
        active.cancellation.cancel();
        let _ = active.shutdown.send(());
        let _ = active.task.await;
    }
    if let Some(session) = session {
        cancel_in_progress_transfers(&session);
        snapshot = snapshot_for(&session, snapshot);
    }
    snapshot.running = false;
    snapshot.share_url = None;
    snapshot.stopped_at = Some(now_ms());
    let mut runtime = state()
        .lock()
        .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
    runtime.snapshot = snapshot.clone();
    drop(runtime);
    emit_update(&app, "session");
    Ok(snapshot)
}

pub(crate) fn request_shutdown() {
    if let Ok(mut runtime) = state().lock() {
        if let Some(active) = runtime.active.take() {
            active.cancellation.cancel();
            let _ = active.shutdown.send(());
            active.task.abort();
        }
        if let Some(session) = runtime.session.as_ref() {
            cancel_in_progress_transfers(session);
        }
        runtime.session = None;
        runtime.snapshot.running = false;
        runtime.snapshot.share_url = None;
        runtime.snapshot.stopped_at = Some(now_ms());
    }
}
