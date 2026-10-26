use super::http;
use super::types::*;
use crate::tunnel::{TunnelMode, TunnelRuntimeStatus};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MAX_CONNECTIONS: usize = 64;
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
const TRANSFER_UPDATE_INTERVAL: Duration = Duration::from_millis(250);
pub(crate) const DEFAULT_LOGIN_FAILURE_DELAY: Duration = Duration::from_millis(500);
pub(crate) const MAX_SHARE_PASSWORD_CHARS: usize = 128;
pub(crate) const PUBLIC_TUNNEL_EVENT: &str = "file-sharing-public-updated";

pub(crate) fn ensure_public_dependency(installed: bool) -> Result<(), String> {
    if installed {
        return Ok(());
    }
    Err(format!(
        "public sharing requires cloudflared, but it was not found. Install it with `{}` and try again.",
        crate::tunnel::recommended_command_for(TunnelMode::Cloudflare)
    ))
}

pub(crate) fn new_public_tunnel_id() -> String {
    format!(
        "{}{}",
        crate::tunnel::EPHEMERAL_TUNNEL_ID_PREFIX,
        Uuid::new_v4().simple()
    )
}

pub(crate) fn public_url(tunnel_url: &str, token: &str) -> String {
    format!("{}/s/{token}/", tunnel_url.trim_end_matches('/'))
}

pub(crate) fn public_state_from_status(status: &str) -> FileSharingPublicState {
    match status {
        "starting" => FileSharingPublicState::Starting,
        "running" => FileSharingPublicState::Running,
        "stopped" => FileSharingPublicState::Disabled,
        _ => FileSharingPublicState::Error,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PublicAccessState {
    pub(crate) state: FileSharingPublicState,
    pub(crate) tunnel_id: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicSnapshotFields {
    pub(crate) tunnel_id: Option<String>,
    pub(crate) state: FileSharingPublicState,
    pub(crate) share_url: Option<String>,
    pub(crate) error: Option<String>,
}

impl PublicAccessState {
    pub(crate) fn begin(&mut self, tunnel_id: String) -> Option<String> {
        let previous = self.tunnel_id.take();
        self.state = FileSharingPublicState::Starting;
        self.error = None;
        self.tunnel_id = Some(tunnel_id);
        previous
    }

    pub(crate) fn apply_tunnel_status(&mut self, status: &TunnelRuntimeStatus) -> bool {
        if self.tunnel_id.as_deref() != Some(status.id.as_str()) {
            return false;
        }
        self.state = public_state_from_status(&status.state);
        match self.state {
            FileSharingPublicState::Error => {
                self.error = status
                    .error
                    .clone()
                    .or_else(|| Some("public tunnel exited unexpectedly".to_string()));
            }
            FileSharingPublicState::Disabled => {
                self.tunnel_id = None;
                self.error = None;
            }
            FileSharingPublicState::Starting | FileSharingPublicState::Running => {
                self.error = None;
            }
        }
        true
    }

    pub(crate) fn disable(&mut self) -> Option<String> {
        self.state = FileSharingPublicState::Disabled;
        self.error = None;
        self.tunnel_id.take()
    }

    pub(crate) fn mark_failed(&mut self, error: String) {
        self.state = FileSharingPublicState::Error;
        self.error = Some(error);
    }

    pub(crate) fn snapshot_fields(
        &self,
        token: &str,
        status: Option<&TunnelRuntimeStatus>,
    ) -> PublicSnapshotFields {
        let Some(tunnel_id) = self.tunnel_id.as_deref() else {
            return PublicSnapshotFields {
                tunnel_id: None,
                state: FileSharingPublicState::Disabled,
                share_url: None,
                error: None,
            };
        };
        let status = status.filter(|status| status.id == tunnel_id);
        let state = status
            .map(|status| public_state_from_status(&status.state))
            .unwrap_or(self.state);
        let share_url = if state == FileSharingPublicState::Running {
            status
                .and_then(|status| status.public_url.as_deref())
                .map(|url| public_url(url, token))
        } else {
            None
        };
        let error = if state == FileSharingPublicState::Error {
            status
                .and_then(|status| status.error.clone())
                .or_else(|| self.error.clone())
        } else {
            None
        };
        PublicSnapshotFields {
            tunnel_id: Some(tunnel_id.to_string()),
            state,
            share_url,
            error,
        }
    }
}

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
    pub connections: Arc<Semaphore>,
    pub header_read_timeout: Duration,
    pub password_hash: Option<[u8; 32]>,
    pub session_secret: String,
    pub login_failure_delay: Duration,
    pub(crate) transfer_updates: Option<TransferUpdates>,
}

pub(crate) struct TransferUpdates {
    app: tauri::AppHandle,
    last_emitted: Mutex<Option<Instant>>,
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
    public: PublicAccessState,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            snapshot: FileSharingSnapshot::default(),
            session: None,
            active: None,
            public: PublicAccessState::default(),
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

pub(crate) fn validate_share_password(password: &str) -> Result<(), String> {
    let characters = password.chars().count();
    if characters == 0 {
        return Err("access password must not be empty".to_string());
    }
    if characters > MAX_SHARE_PASSWORD_CHARS {
        return Err(format!(
            "access password must be at most {MAX_SHARE_PASSWORD_CHARS} characters"
        ));
    }
    if !password.chars().any(|character| !character.is_whitespace()) {
        return Err("access password must contain a non-whitespace character".to_string());
    }
    Ok(())
}

pub(crate) fn hash_share_password(password: &str) -> [u8; 32] {
    let digest = Sha256::digest(password.as_bytes());
    let mut hash = [0_u8; 32];
    hash.copy_from_slice(&digest);
    hash
}

pub(crate) fn prepare_share_password(password: Option<&str>) -> Result<Option<[u8; 32]>, String> {
    match password {
        Some(password) => {
            validate_share_password(password)?;
            Ok(Some(hash_share_password(password)))
        }
        None => Ok(None),
    }
}

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

pub(crate) fn safe_login_next(next: Option<&str>, token: &str) -> String {
    let fallback = format!("/s/{token}/");
    let Some(next) = next else {
        return fallback;
    };
    if next.is_empty()
        || !next.starts_with('/')
        || next.starts_with("//")
        || next.contains('#')
        || next.contains('\\')
        || !next.chars().all(|character| character.is_ascii_graphic())
    {
        return fallback;
    }
    if next.starts_with(&fallback) {
        next.to_string()
    } else {
        fallback
    }
}

pub(crate) fn session_is_authenticated(
    session: &SharedSession,
    cookie_header: Option<&str>,
) -> bool {
    if session.password_hash.is_none() {
        return true;
    }
    let Some(cookie_header) = cookie_header else {
        return false;
    };
    let value = cookie_header.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        (name.trim() == "onespace_share").then_some(value.trim())
    });
    let Some(value) = value else {
        return false;
    };
    constant_time_eq(value.as_bytes(), session.session_secret.as_bytes())
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
    snapshot.password_protected = session.password_hash.is_some();
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

fn with_public_fields(
    mut snapshot: FileSharingSnapshot,
    public: &PublicAccessState,
    token: &str,
    status: Option<&TunnelRuntimeStatus>,
) -> FileSharingSnapshot {
    let fields = public.snapshot_fields(token, status);
    snapshot.public_tunnel_id = fields.tunnel_id;
    snapshot.public_state = fields.state;
    snapshot.public_share_url = fields.share_url;
    snapshot.public_error = fields.error;
    snapshot
}

fn clear_public_fields(snapshot: &mut FileSharingSnapshot) {
    snapshot.public_tunnel_id = None;
    snapshot.public_state = FileSharingPublicState::Disabled;
    snapshot.public_share_url = None;
    snapshot.public_error = None;
}

fn refresh_public_state(public: &mut PublicAccessState) -> Option<TunnelRuntimeStatus> {
    let status = public
        .tunnel_id
        .as_deref()
        .map(crate::tunnel::supervisor::get_status);
    if let Some(status) = status.as_ref() {
        public.apply_tunnel_status(status);
    }
    status
}

fn public_action_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
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
        let dropped = if records.len() == 200 {
            records
                .iter()
                .position(|record| record.state != FileSharingTransferState::InProgress)
                .map(|index| records.remove(index))
        } else {
            None
        };
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
        if dropped.is_some() {
            if let Ok(mut summary) = session.summary.lock() {
                summary.dropped_transfer_records += 1;
            }
        }
    }
    emit_transfer_update(session, false);
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
    emit_transfer_update(session, true);
}

pub(crate) fn update_transfer_progress(session: &SharedSession, id: &str, bytes_sent: u64) {
    if let Ok(mut records) = session.transfers.lock() {
        if let Some(record) = records.iter_mut().find(|record| record.id == id) {
            if record.state == FileSharingTransferState::InProgress {
                record.bytes_sent = bytes_sent;
            }
        }
    }
    emit_transfer_update(session, false);
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

pub(crate) fn should_emit_transfer_update(
    previous: Option<Instant>,
    now: Instant,
    terminal: bool,
) -> bool {
    terminal || previous.is_none_or(|last| now.duration_since(last) >= TRANSFER_UPDATE_INTERVAL)
}

fn emit_transfer_update(session: &SharedSession, terminal: bool) {
    let Some(updates) = &session.transfer_updates else {
        return;
    };
    let now = Instant::now();
    if let Ok(mut previous) = updates.last_emitted.lock() {
        if should_emit_transfer_update(*previous, now, terminal) {
            *previous = Some(now);
            emit_update(&updates.app, "transfer");
        }
    }
}

#[cfg(test)]
pub(crate) fn test_session(
    files: Vec<SharedFile>,
    max_connections: usize,
    header_read_timeout: Duration,
) -> SharedSession {
    Arc::new(Session {
        token: "test-token".to_string(),
        files,
        transfers: Mutex::new(Vec::new()),
        summary: Mutex::new(FileSharingSummary::default()),
        cancellation: CancellationToken::new(),
        connections: Arc::new(Semaphore::new(max_connections)),
        header_read_timeout,
        password_hash: None,
        session_secret: "test-session-secret".to_string(),
        login_failure_delay: DEFAULT_LOGIN_FAILURE_DELAY,
        transfer_updates: None,
    })
}

#[cfg(test)]
pub(crate) fn test_password_session(
    password: &str,
    login_failure_delay: Duration,
) -> SharedSession {
    Arc::new(Session {
        token: "test-token".to_string(),
        files: Vec::new(),
        transfers: Mutex::new(Vec::new()),
        summary: Mutex::new(FileSharingSummary::default()),
        cancellation: CancellationToken::new(),
        connections: Arc::new(Semaphore::new(64)),
        header_read_timeout: Duration::from_secs(1),
        password_hash: Some(hash_share_password(password)),
        session_secret: "test-secret".to_string(),
        login_failure_delay,
        transfer_updates: None,
    })
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
    if input.public_access {
        ensure_public_dependency(crate::tunnel::find_binary(TunnelMode::Cloudflare).is_some())?;
    }
    let password_hash = prepare_share_password(input.password.as_deref())?;
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
    let mut secret_bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut secret_bytes);
    let session_secret = URL_SAFE_NO_PAD.encode(secret_bytes);
    let session = Arc::new(Session {
        token: token.clone(),
        files,
        transfers: Mutex::new(Vec::new()),
        summary: Mutex::new(FileSharingSummary::default()),
        cancellation: CancellationToken::new(),
        connections: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        header_read_timeout: HEADER_READ_TIMEOUT,
        password_hash,
        session_secret,
        login_failure_delay: DEFAULT_LOGIN_FAILURE_DELAY,
        transfer_updates: Some(TransferUpdates {
            app: app.clone(),
            last_emitted: Mutex::new(None),
        }),
    });
    let (shutdown, mut shutdown_rx) = oneshot::channel();
    let handler_session = session.clone();
    let listener_app = app.clone();
    let task = tokio::spawn(async move {
        if let ListenerExit::AcceptError(error) =
            run_listener(listener, handler_session.clone(), &mut shutdown_rx).await
        {
            if !handler_session.cancellation.is_cancelled() {
                handle_unexpected_listener_exit(&listener_app, &handler_session, error);
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
        password_protected: session.password_hash.is_some(),
        public_tunnel_id: None,
        public_state: FileSharingPublicState::Disabled,
        public_share_url: None,
        public_error: None,
    };
    let mut snapshot = snapshot_for(&session, snapshot);
    let mut public = PublicAccessState::default();
    if input.public_access {
        let tunnel_id = new_public_tunnel_id();
        let _ = public.begin(tunnel_id.clone());
        let target_url = format!("http://{address}:{port}");
        if let Err(error) = crate::tunnel::start_ephemeral_cloudflare_tunnel(
            tunnel_id,
            target_url,
            PUBLIC_TUNNEL_EVENT.to_string(),
            Some(app.clone()),
        )
        .await
        {
            session.cancellation.cancel();
            let _ = shutdown.send(());
            let _ = task.await;
            return Err(format!("failed to start public tunnel: {error}"));
        }
    }
    snapshot = with_public_fields(snapshot, &public, &session.token, None);
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
    runtime.public = public;
    drop(runtime);
    emit_update(&app, "session");
    Ok(snapshot)
}

pub(crate) enum ListenerExit {
    Stopped,
    AcceptError(String),
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) async fn run_listener(
    listener: TcpListener,
    session: SharedSession,
    shutdown: &mut oneshot::Receiver<()>,
) -> ListenerExit {
    let mut connections = tokio::task::JoinSet::new();
    loop {
        while connections.try_join_next().is_some() {}
        tokio::select! {
            _ = &mut *shutdown => break,
            _ = session.cancellation.cancelled() => break,
            accepted = listener.accept() => {
                let (stream, client_address) = match accepted {
                    Ok(connection) => connection,
                    Err(error) => {
                        connections.abort_all();
                        while connections.join_next().await.is_some() {}
                        return ListenerExit::AcceptError(error.to_string());
                    }
                };
                let Ok(permit) = session.connections.clone().try_acquire_owned() else {
                    drop(stream);
                    continue;
                };
                let connection_session = session.clone();
                let service_session = connection_session.clone();
                connections.spawn(async move {
                    let service = service_fn(move |request| {
                        http::handle(request, service_session.clone(), client_address.ip().to_string())
                    });
                    let mut builder = hyper::server::conn::http1::Builder::new();
                    builder
                        .timer(TokioTimer::new())
                        .header_read_timeout(connection_session.header_read_timeout);
                    let connection = builder.serve_connection(TokioIo::new(stream), service);
                    tokio::pin!(connection);
                    tokio::select! {
                        _ = &mut connection => {},
                        _ = connection_session.cancellation.cancelled() => {},
                    }
                    drop(permit);
                });
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    ListenerExit::Stopped
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn unexpected_exit_snapshot(
    mut snapshot: FileSharingSnapshot,
    session: &SharedSession,
    error: String,
) -> FileSharingSnapshot {
    session.cancellation.cancel();
    cancel_in_progress_transfers(session);
    snapshot = snapshot_for(session, snapshot);
    snapshot.running = false;
    snapshot.share_url = None;
    snapshot.stopped_at = Some(now_ms());
    snapshot.last_error = Some(error);
    clear_public_fields(&mut snapshot);
    snapshot
}

fn handle_unexpected_listener_exit(app: &tauri::AppHandle, session: &SharedSession, error: String) {
    let mut public_tunnel_id = None;
    if let Ok(mut runtime) = state().lock() {
        if runtime
            .session
            .as_ref()
            .is_some_and(|active_session| Arc::ptr_eq(active_session, session))
        {
            public_tunnel_id = runtime.public.disable();
            runtime.public = PublicAccessState::default();
            runtime.snapshot = unexpected_exit_snapshot(runtime.snapshot.clone(), session, error);
            runtime.active = None;
        }
    }
    if let Some(tunnel_id) = public_tunnel_id {
        crate::tunnel::kill_tunnel(&tunnel_id);
    }
    emit_update(app, "session");
}

pub(crate) fn status() -> Result<FileSharingSnapshot, String> {
    let mut runtime = state()
        .lock()
        .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
    let Some(session) = runtime.session.clone() else {
        return Ok(runtime.snapshot.clone());
    };
    let status = refresh_public_state(&mut runtime.public);
    let snapshot = with_public_fields(
        snapshot_for(&session, runtime.snapshot.clone()),
        &runtime.public,
        &session.token,
        status.as_ref(),
    );
    runtime.snapshot = snapshot.clone();
    Ok(snapshot)
}

pub(crate) async fn stop(app: tauri::AppHandle) -> Result<FileSharingSnapshot, String> {
    let (active, session, mut snapshot, mut public) = {
        let mut runtime = state()
            .lock()
            .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
        (
            runtime.active.take(),
            runtime.session.take(),
            runtime.snapshot.clone(),
            std::mem::take(&mut runtime.public),
        )
    };
    if let Some(tunnel_id) = public.disable() {
        let _ = crate::tunnel::supervisor::stop_tunnel(&tunnel_id, Some(app.clone())).await;
    }
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
    clear_public_fields(&mut snapshot);
    let mut runtime = state()
        .lock()
        .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
    runtime.snapshot = snapshot.clone();
    runtime.public = PublicAccessState::default();
    drop(runtime);
    emit_update(&app, "session");
    Ok(snapshot)
}

pub(crate) async fn start_public(
    app: tauri::AppHandle,
) -> Result<FileSharingSnapshot, String> {
    let _guard = public_action_lock().lock().await;
    ensure_public_dependency(crate::tunnel::find_binary(TunnelMode::Cloudflare).is_some())?;
    let (session, target_url, tunnel_id, previous) = {
        let mut runtime = state()
            .lock()
            .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
        let session = runtime
            .session
            .clone()
            .ok_or_else(|| "file sharing is not running".to_string())?;
        let target_url = match (runtime.snapshot.address.as_deref(), runtime.snapshot.port) {
            (Some(address), Some(port)) => format!("http://{address}:{port}"),
            _ => return Err("file sharing listener address is unavailable".to_string()),
        };
        let tunnel_id = new_public_tunnel_id();
        let previous = runtime.public.begin(tunnel_id.clone());
        (session, target_url, tunnel_id, previous)
    };
    if let Some(previous) = previous.as_deref() {
        crate::tunnel::kill_tunnel(previous);
    }
    if let Err(error) = crate::tunnel::start_ephemeral_cloudflare_tunnel(
        tunnel_id.clone(),
        target_url,
        PUBLIC_TUNNEL_EVENT.to_string(),
        Some(app.clone()),
    )
    .await
    {
        let message = format!("failed to start public tunnel: {error}");
        let recorded = {
            let mut runtime = state()
                .lock()
                .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
            let active = runtime
                .session
                .as_ref()
                .is_some_and(|active_session| Arc::ptr_eq(active_session, &session))
                && runtime.public.tunnel_id.as_deref() == Some(tunnel_id.as_str());
            if active {
                runtime.public.mark_failed(message.clone());
                let snapshot = with_public_fields(
                    snapshot_for(&session, runtime.snapshot.clone()),
                    &runtime.public,
                    &session.token,
                    None,
                );
                runtime.snapshot = snapshot;
            }
            active
        };
        if recorded {
            emit_update(&app, "session");
        }
        return Err(message);
    }
    let snapshot = {
        let mut runtime = state()
            .lock()
            .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
        let active = runtime
            .session
            .as_ref()
            .is_some_and(|active_session| Arc::ptr_eq(active_session, &session))
            && runtime.public.tunnel_id.as_deref() == Some(tunnel_id.as_str());
        if !active {
            None
        } else {
            let snapshot = with_public_fields(
                snapshot_for(&session, runtime.snapshot.clone()),
                &runtime.public,
                &session.token,
                None,
            );
            runtime.snapshot = snapshot.clone();
            Some(snapshot)
        }
    };
    let Some(snapshot) = snapshot else {
        crate::tunnel::kill_tunnel(&tunnel_id);
        return Err("file sharing is not running".to_string());
    };
    emit_update(&app, "session");
    Ok(snapshot)
}

pub(crate) async fn stop_public(
    app: tauri::AppHandle,
) -> Result<FileSharingSnapshot, String> {
    let _guard = public_action_lock().lock().await;
    let (stopped, was_public, snapshot) = {
        let mut runtime = state()
            .lock()
            .map_err(|_| "file sharing runtime state is unavailable".to_string())?;
        let session = runtime
            .session
            .clone()
            .ok_or_else(|| "file sharing is not running".to_string())?;
        let was_public = runtime.public.state != FileSharingPublicState::Disabled
            || runtime.public.tunnel_id.is_some();
        let stopped = runtime.public.disable();
        let snapshot = with_public_fields(
            snapshot_for(&session, runtime.snapshot.clone()),
            &runtime.public,
            &session.token,
            None,
        );
        runtime.snapshot = snapshot.clone();
        (stopped, was_public, snapshot)
    };
    if let Some(tunnel_id) = stopped.as_deref() {
        let _ = crate::tunnel::supervisor::stop_tunnel(tunnel_id, Some(app.clone())).await;
    }
    if was_public {
        emit_update(&app, "session");
    }
    Ok(snapshot)
}

pub(crate) fn request_shutdown() {
    let mut public_tunnel_id = None;
    if let Ok(mut runtime) = state().lock() {
        public_tunnel_id = runtime.public.disable();
        runtime.public = PublicAccessState::default();
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
        clear_public_fields(&mut runtime.snapshot);
    }
    if let Some(tunnel_id) = public_tunnel_id {
        crate::tunnel::kill_tunnel(&tunnel_id);
    }
}
