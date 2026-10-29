use crate::config::{self, ProxyConfig};
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub const SAFE_FIXTURE_USER: &str = "SAFE_FIXTURE_proxy_user";
pub const SAFE_FIXTURE_PASS: &str = "SAFE_FIXTURE_proxy_pass";

/// Process-wide serialization for tests that publish a global proxy.
///
/// Acquiring the lock also resets the test-only thread-local current snapshot,
/// so a test never inherits a proxy published by an earlier test on the same
/// thread, and dropping the guard clears it again. Other threads are never
/// affected because the override is thread-local.
pub struct ProxyTestGuard {
    lock: Mutex<()>,
}

/// Held for the duration of a serialized proxy test. Dropping it clears the
/// thread-local current snapshot override.
pub struct ProxyTestLock<'a> {
    _guard: MutexGuard<'a, ()>,
}

impl Drop for ProxyTestLock<'_> {
    fn drop(&mut self) {
        crate::proxy::clear_test_current_snapshot();
    }
}

/// Poison recovery that preserves the existing
/// `SERIAL_GUARD.lock().unwrap_or_else(|error| error.into_inner())` shape.
pub struct ProxyTestPoisonError<'a> {
    guard: MutexGuard<'a, ()>,
}

impl<'a> ProxyTestPoisonError<'a> {
    pub fn into_inner(self) -> ProxyTestLock<'a> {
        ProxyTestLock {
            _guard: self.guard,
        }
    }
}

impl ProxyTestGuard {
    pub fn lock(&self) -> Result<ProxyTestLock<'_>, ProxyTestPoisonError<'_>> {
        match self.lock.lock() {
            Ok(guard) => {
                crate::proxy::clear_test_current_snapshot();
                Ok(ProxyTestLock { _guard: guard })
            }
            Err(error) => {
                crate::proxy::clear_test_current_snapshot();
                Err(ProxyTestPoisonError {
                    guard: error.into_inner(),
                })
            }
        }
    }
}

pub static SERIAL_GUARD: ProxyTestGuard = ProxyTestGuard { lock: Mutex::new(()) };

#[derive(Clone)]
pub struct HttpResponseConfig {
    pub status: u16,
    pub body: String,
    pub headers: Vec<(String, String)>,
    pub delay_ms: u64,
    pub redirect_to: Option<String>,
}

pub struct HttpProxyRecorder {
    addr: SocketAddr,
    records: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<String>>>,
    config: Arc<Mutex<HttpResponseConfig>>,
    on_hit: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
    _shutdown: oneshot::Sender<()>,
    _task: tokio::task::JoinHandle<()>,
}

impl HttpProxyRecorder {
    pub async fn new(body: &str) -> Self {
        Self::new_inner(HttpResponseConfig {
            status: 200,
            body: body.to_string(),
            headers: vec![],
            delay_ms: 0,
            redirect_to: None,
        }).await
    }

    pub async fn new_delayed(body: &str, delay_ms: u64) -> Self {
        Self::new_inner(HttpResponseConfig {
            status: 200,
            body: body.to_string(),
            headers: vec![],
            delay_ms,
            redirect_to: None,
        }).await
    }

    pub async fn new_redirect(location: &str) -> Self {
        Self::new_inner(HttpResponseConfig {
            status: 302,
            body: String::new(),
            headers: vec![],
            delay_ms: 0,
            redirect_to: Some(location.to_string()),
        }).await
    }

    pub async fn new_with_status(body: &str, status: u16) -> Self {
        Self::new_inner(HttpResponseConfig {
            status,
            body: body.to_string(),
            headers: vec![],
            delay_ms: 0,
            redirect_to: None,
        }).await
    }

    pub async fn new_full(config: HttpResponseConfig) -> Self {
        Self::new_inner(config).await
    }

    async fn new_inner(config: HttpResponseConfig) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let records = Arc::new(Mutex::new(Vec::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let config = Arc::new(Mutex::new(config));
        let on_hit: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>> = Arc::new(Mutex::new(None));
        let rec = records.clone();
        let reqs = requests.clone();
        let cfg = config.clone();
        let hit = on_hit.clone();
        let (tx, mut rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((mut stream, _)) => {
                                let rec = rec.clone();
                                let reqs = reqs.clone();
                                let cfg = cfg.clone();
                                let hit = hit.clone();
                                tokio::spawn(async move {
                                    let mut buf = vec![0u8; 8192];
                                    let n = stream.read(&mut buf).await.unwrap_or(0);
                                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                                    let first = req.lines().next().unwrap_or("").to_string();
                                    let target = if first.starts_with("CONNECT ") {
                                        first.split_whitespace().nth(1).unwrap_or("").to_string()
                                    } else {
                                        first.split_whitespace().nth(1).unwrap_or("").to_string()
                                    };
                                    rec.lock().unwrap().push(target.clone());
                                    reqs.lock().unwrap().push(req.clone());
                                    // on-hit callback
                                    let cb = hit.lock().unwrap().clone();
                                    if let Some(cb) = cb { cb(); }
                                    let snapshot = cfg.lock().unwrap().clone();
                                    if snapshot.delay_ms > 0 {
                                        tokio::time::sleep(std::time::Duration::from_millis(snapshot.delay_ms)).await;
                                    }
                                    if first.starts_with("CONNECT ") {
                                        let resp = "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n";
                                        let _ = stream.write_all(resp.as_bytes()).await;
                                    } else if let Some(loc) = snapshot.redirect_to {
                                        let resp = format!(
                                            "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\n\r\n",
                                            loc
                                        );
                                        let _ = stream.write_all(resp.as_bytes()).await;
                                    } else {
                                        let mut headers = String::new();
                                        for (k,v) in &snapshot.headers {
                                            headers.push_str(&format!("{}: {}\r\n", k, v));
                                        }
                                        let resp = format!(
                                            "HTTP/1.1 {} {}\r\n{}Content-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                            snapshot.status,
                                            if snapshot.status < 400 { "OK" } else { "ERR" },
                                            headers,
                                            snapshot.body.len(),
                                            snapshot.body
                                        );
                                        let _ = stream.write_all(resp.as_bytes()).await;
                                    }
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });
        Self {
            addr,
            records,
            requests,
            config,
            on_hit,
            _shutdown: tx,
            _task: task,
        }
    }

    pub fn set_on_hit(&self, cb: Arc<dyn Fn() + Send + Sync>) {
        *self.on_hit.lock().unwrap() = Some(cb);
    }

    pub fn set_response(&self, status: u16, body: String, headers: Vec<(String,String)>) {
        let mut c = self.config.lock().unwrap();
        c.status = status;
        c.body = body;
        c.headers = headers;
    }

    pub fn set_delay(&self, ms: u64) {
        self.config.lock().unwrap().delay_ms = ms;
    }

    pub fn set_redirect(&self, location: Option<String>) {
        self.config.lock().unwrap().redirect_to = location;
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn observed(&self) -> Vec<String> {
        self.records.lock().unwrap().clone()
    }

    pub fn targets(&self) -> Vec<String> {
        self.records.lock().unwrap().clone()
    }

    pub fn hits(&self) -> usize {
        self.records.lock().unwrap().len()
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

pub struct DirectUpstream {
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
    body: String,
    _shutdown: oneshot::Sender<()>,
    _task: tokio::task::JoinHandle<()>,
}

impl DirectUpstream {
    pub async fn new(body: &str) -> Self {
        use hyper::service::service_fn;
        use hyper::{Request, Response};
        use hyper_util::rt::TokioIo;
        use hyper_util::server::conn::auto::Builder as ConnBuilder;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let body = body.to_string();
        let hits_clone = hits.clone();
        let body_clone = body.clone();
        let (tx, mut rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, _)) => {
                                let hits = hits_clone.clone();
                                let body = body_clone.clone();
                                tokio::spawn(async move {
                                    hits.fetch_add(1, Ordering::SeqCst);
                                    let io = TokioIo::new(stream);
                                    let svc = service_fn(move |_req: Request<hyper::body::Incoming>| {
                                        let body = body.clone();
                                        async move {
                                            Ok::<_, io::Error>(Response::new(http_body_util::Full::new(bytes::Bytes::from(body))))
                                        }
                                    });
                                    let _ = ConnBuilder::new(hyper_util::rt::TokioExecutor::new())
                                        .serve_connection(io, svc)
                                        .await;
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });
        Self {
            addr,
            hits,
            body,
            _shutdown: tx,
            _task: task,
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

pub struct FakeSocks5 {
    addr: SocketAddr,
    target: Arc<Mutex<Option<String>>>,
    _shutdown: oneshot::Sender<()>,
    _task: tokio::task::JoinHandle<()>,
}

impl FakeSocks5 {
    pub async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let target = Arc::new(Mutex::new(None));
        let target_clone = target.clone();
        let (tx, mut rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((mut stream, _)) => {
                                let target = target_clone.clone();
                                tokio::spawn(async move {
                                    // greeting
                                    let mut buf = [0u8; 2];
                                    if stream.read_exact(&mut buf).await.is_err() { return; }
                                    // method count
                                    let nmethods = buf[1] as usize;
                                    let mut methods = vec![0u8; nmethods];
                                    if stream.read_exact(&mut methods).await.is_err() { return; }
                                    // reply no auth
                                    let _ = stream.write_all(&[0x05, 0x00]).await;
                                    // request header: VER CMD RSV ATYP
                                    let mut hdr = [0u8; 4];
                                    if stream.read_exact(&mut hdr).await.is_err() { return; }
                                    let atyp = hdr[3];
                                    let host = if atyp == 0x01 {
                                        let mut ip = [0u8; 4];
                                        if stream.read_exact(&mut ip).await.is_err() { return; }
                                        format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
                                    } else if atyp == 0x03 {
                                        let mut len = [0u8; 1];
                                        if stream.read_exact(&mut len).await.is_err() { return; }
                                        let mut domain = vec![0u8; len[0] as usize];
                                        if stream.read_exact(&mut domain).await.is_err() { return; }
                                        String::from_utf8_lossy(&domain).to_string()
                                    } else {
                                        return;
                                    };
                                    let mut port = [0u8; 2];
                                    if stream.read_exact(&mut port).await.is_err() { return; }
                                    let port = u16::from_be_bytes(port);
                                    *target.lock().unwrap() = Some(format!("{}:{}", host, port));
                                    // reply success
                                    let _ = stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0,0,0,0, 0,0]).await;
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });
        Self {
            addr,
            target,
            _shutdown: tx,
            _task: task,
        }
    }

    pub fn url(&self) -> String {
        format!("socks5://{}", self.addr)
    }

    pub fn target(&self) -> Option<String> {
        self.target.lock().unwrap().clone()
    }
}

pub struct GlobalProxyGuard {
    _home: crate::config::test_home::TestHomeGuard,
    _temp: PathBuf,
    saved_env: Vec<(String, Option<String>)>,
}

impl Drop for GlobalProxyGuard {
    fn drop(&mut self) {
        for (k, v) in &self.saved_env {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

pub fn with_global_proxy(config: Option<ProxyConfig>) -> GlobalProxyGuard {
    let temp = std::env::temp_dir().join(format!("onespace-proxy-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let home_guard = crate::config::test_home::TestHomeGuard::set(temp.clone());
    // also set env HOME
    let keys = ["HOME", "USERPROFILE"];
    let mut saved_env = Vec::new();
    for k in keys {
        saved_env.push((k.to_string(), std::env::var(k).ok()));
    }
    std::env::set_var("HOME", &temp);
    // init manager
    let mgr = crate::proxy::init_proxy_manager();
    if let Some(cfg) = config {
        let prepared = mgr.prepare(Some(cfg)).unwrap();
        mgr.publish(prepared);
    } else {
        let prepared = mgr.prepare(None).unwrap();
        mgr.publish(prepared);
    }
    GlobalProxyGuard {
        _home: home_guard,
        _temp: temp,
        saved_env,
    }
}
