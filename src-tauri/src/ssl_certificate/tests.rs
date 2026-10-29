use super::inspector::check_domain_match;
use super::nginx_template::generate_nginx_config;
use super::types::{SslOrderRequest, SslWatchItemInput};
use super::watchlist::{add_watch_item, load_watchlist, remove_watch_item};
use crate::config::ProxyConfig;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

#[derive(Clone, Debug)]
pub(crate) struct FakeAcmeRequest {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl FakeAcmeRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub(crate) struct FakeAcmeState {
    finalized: AtomicBool,
    cert_pem: String,
}

impl FakeAcmeState {
    pub fn new(cert_pem: String) -> Arc<Self> {
        Arc::new(Self {
            finalized: AtomicBool::new(false),
            cert_pem,
        })
    }
}

struct FakeAcmeResponse {
    status: u16,
    reason: &'static str,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl FakeAcmeResponse {
    fn new(status: u16, reason: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            reason,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }
}

pub(crate) struct FakeAcmeProxy {
    addr: SocketAddr,
    records: Arc<Mutex<Vec<FakeAcmeRequest>>>,
    _shutdown: oneshot::Sender<()>,
    _task: tokio::task::JoinHandle<()>,
}

impl FakeAcmeProxy {
    pub async fn new(cert_pem: String) -> Self {
        Self::new_with_state(FakeAcmeState::new(cert_pem)).await
    }

    pub async fn new_with_state(state: Arc<FakeAcmeState>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let records = Arc::new(Mutex::new(Vec::new()));
        let rec = records.clone();
        let (tx, mut rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, _)) => {
                                let rec = rec.clone();
                                let state = state.clone();
                                tokio::spawn(async move {
                                    handle_acme_connection(stream, rec, state).await;
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
            _shutdown: tx,
            _task: task,
        }
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn requests(&self) -> Vec<FakeAcmeRequest> {
        self.records
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn hits(&self) -> usize {
        self.records
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len()
    }
}

pub(crate) fn http_proxy_config(proxy: &FakeAcmeProxy) -> ProxyConfig {
    ProxyConfig {
        proxy_enabled: true,
        proxy_type: "http".to_string(),
        proxy_host: "127.0.0.1".to_string(),
        proxy_port: proxy.port(),
        proxy_username: Some(crate::proxy::test_support::SAFE_FIXTURE_USER.to_string()),
        proxy_password: Some(crate::proxy::test_support::SAFE_FIXTURE_PASS.to_string()),
        check_interval: 15,
    }
}

pub(crate) fn publish_proxy(proxy: &FakeAcmeProxy) {
    let manager = crate::proxy::init_proxy_manager();
    manager.publish(manager.prepare(Some(http_proxy_config(proxy))).unwrap());
}

pub(crate) fn request_path(target: &str) -> String {
    let rest = target
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(target);
    let path = match rest.find('/') {
        Some(index) => &rest[index..],
        None => "/",
    };
    path.split('?').next().unwrap_or("/").to_string()
}

pub(crate) fn recorded_operations(proxy: &FakeAcmeProxy) -> Vec<(String, String)> {
    proxy
        .requests()
        .into_iter()
        .map(|request| (request.method, request_path(&request.target)))
        .collect()
}

async fn handle_acme_connection(
    mut stream: TcpStream,
    records: Arc<Mutex<Vec<FakeAcmeRequest>>>,
    state: Arc<FakeAcmeState>,
) {
    let Some(request) = read_acme_request(&mut stream).await else {
        return;
    };
    let response = fake_acme_response(&request, &state);
    records
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(request);
    write_fake_response(&mut stream, response).await;
}

async fn read_acme_request(stream: &mut TcpStream) -> Option<FakeAcmeRequest> {
    let mut buffer = Vec::new();
    let header_end = loop {
        if let Some(position) = find_bytes(&buffer, b"\r\n\r\n") {
            break position + 4;
        }
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > 1_048_576 {
            return None;
        }
    };

    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let content_length = header_text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find_map(|(key, value)| {
            if key.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    while buffer.len() < header_end + content_length {
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    let mut lines = header_text.lines();
    let first_line = lines.next().unwrap_or_default();
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let headers = lines
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect();
    let body_end = (header_end + content_length).min(buffer.len());
    Some(FakeAcmeRequest {
        method,
        target,
        headers,
        body: buffer[header_end..body_end].to_vec(),
    })
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn fake_acme_response(request: &FakeAcmeRequest, state: &FakeAcmeState) -> FakeAcmeResponse {
    let path = request_path(&request.target);
    let nonce =
        |response: FakeAcmeResponse| response.header("Replay-Nonce", "SAFE_FIXTURE_replay_nonce");

    match (request.method.as_str(), path.as_str()) {
        ("GET", "/directory") => nonce(FakeAcmeResponse::new(
            200,
            "OK",
            r#"{"newNonce":"http://acme.example.test/new-nonce","newAccount":"http://acme.example.test/new-account","newOrder":"http://acme.example.test/new-order","meta":{}}"#,
        )),
        ("HEAD", "/new-nonce") => nonce(FakeAcmeResponse::new(200, "OK", Vec::new())),
        ("POST", "/new-account") => nonce(
            FakeAcmeResponse::new(201, "Created", "{}")
                .header("Location", "http://acme.example.test/account/1"),
        ),
        ("POST", "/new-order") => nonce(
            FakeAcmeResponse::new(201, "Created", fixture_order_json("pending", None))
                .header("Location", "http://acme.example.test/order"),
        ),
        ("GET" | "POST", "/authz/1") => nonce(FakeAcmeResponse::new(
            200,
            "OK",
            fixture_authorization_json(),
        )),
        ("GET" | "POST", "/challenge/1") => {
            nonce(FakeAcmeResponse::new(200, "OK", fixture_challenge_json()))
        }
        ("GET" | "POST", "/order") => {
            let body = if state.finalized.load(Ordering::SeqCst) {
                fixture_order_json("valid", Some("http://acme.example.test/cert"))
            } else {
                fixture_order_json("ready", None)
            };
            nonce(FakeAcmeResponse::new(200, "OK", body))
        }
        ("POST", "/finalize") => {
            state.finalized.store(true, Ordering::SeqCst);
            nonce(FakeAcmeResponse::new(
                200,
                "OK",
                fixture_order_json("processing", None),
            ))
        }
        ("GET" | "POST", "/cert") => nonce(
            FakeAcmeResponse::new(200, "OK", state.cert_pem.clone())
                .header("Content-Type", "application/pem-certificate-chain"),
        ),
        ("GET" | "POST", "/submit") => nonce(
            FakeAcmeResponse::new(200, "OK", "SAFE_FIXTURE_ok")
                .header("X-Fixture-Response", "SAFE_FIXTURE_response"),
        ),
        _ => FakeAcmeResponse::new(404, "Not Found", "SAFE_FIXTURE_missing")
            .header("X-Fixture-Response", "SAFE_FIXTURE_missing"),
    }
}

fn fixture_order_json(status: &str, certificate: Option<&str>) -> String {
    match certificate {
        Some(certificate) => format!(
            r#"{{"status":"{}","authorizations":["http://acme.example.test/authz/1"],"finalize":"http://acme.example.test/finalize","certificate":"{}"}}"#,
            status, certificate
        ),
        None => format!(
            r#"{{"status":"{}","authorizations":["http://acme.example.test/authz/1"],"finalize":"http://acme.example.test/finalize"}}"#,
            status
        ),
    }
}

fn fixture_authorization_json() -> String {
    r#"{"identifier":{"type":"dns","value":"acme.example.test"},"status":"pending","challenges":[{"type":"dns-01","url":"http://acme.example.test/challenge/1","token":"SAFE_FIXTURE_token","status":"pending"}]}"#
        .to_string()
}

fn fixture_challenge_json() -> String {
    r#"{"type":"dns-01","url":"http://acme.example.test/challenge/1","token":"SAFE_FIXTURE_token","status":"processing"}"#
        .to_string()
}

async fn write_fake_response(stream: &mut TcpStream, response: FakeAcmeResponse) {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nConnection: close\r\nContent-Length: {}\r\n",
        response.status,
        response.reason,
        response.body.len()
    );
    for (name, value) in response.headers {
        head.push_str(&format!("{}: {}\r\n", name, value));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(&response.body).await;
    let _ = stream.shutdown().await;
}

fn fixture_certificate_pem() -> String {
    let mut params = rcgen::CertificateParams::new(vec!["acme.example.test".to_string()]).unwrap();
    let mut distinguished_name = rcgen::DistinguishedName::new();
    distinguished_name.push(rcgen::DnType::CommonName, "SAFE_FIXTURE Test CA");
    params.distinguished_name = distinguished_name;
    let key_pair = rcgen::KeyPair::generate().unwrap();
    params.self_signed(&key_pair).unwrap().pem()
}

#[tokio::test]
async fn acme_account_and_order_follow_proxy_switch() {
    let _serial = crate::proxy::test_support::SERIAL_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let state = FakeAcmeState::new(fixture_certificate_pem());
    let proxy_a = FakeAcmeProxy::new_with_state(state.clone()).await;
    let proxy_b = FakeAcmeProxy::new_with_state(state).await;
    let _home = crate::proxy::test_support::with_global_proxy(Some(http_proxy_config(&proxy_a)));
    let _directory = super::acme::set_test_directory_url("http://acme.example.test/directory");

    let created = super::acme::create_acme_order(SslOrderRequest {
        domain: "acme.example.test".to_string(),
        san_domains: Vec::new(),
        email: None,
        use_staging: false,
        key_type: "ecc_p256".to_string(),
        cloudflare_api_token: None,
    })
    .await
    .expect("create the ACME order through proxy A");

    assert_eq!(created.status, "pending_challenge");
    assert_eq!(created.challenges.len(), 1);
    assert_eq!(
        created.challenges[0].record_name,
        "_acme-challenge.acme.example.test"
    );
    assert!(!created.challenges[0].record_value.is_empty());
    assert_eq!(
        recorded_operations(&proxy_a),
        vec![
            ("GET".to_string(), "/directory".to_string()),
            ("HEAD".to_string(), "/new-nonce".to_string()),
            ("POST".to_string(), "/new-account".to_string()),
            ("HEAD".to_string(), "/new-nonce".to_string()),
            ("POST".to_string(), "/new-order".to_string()),
            ("POST".to_string(), "/authz/1".to_string()),
        ]
    );
    assert_eq!(proxy_b.hits(), 0);

    let a_hits_after_creation = proxy_a.hits();
    publish_proxy(&proxy_b);

    let issued = super::acme::verify_and_finalize_order(&created.order_id)
        .await
        .expect("verify and finalize the ACME order through proxy B");

    assert_eq!(
        recorded_operations(&proxy_b),
        vec![
            ("GET".to_string(), "/directory".to_string()),
            ("HEAD".to_string(), "/new-nonce".to_string()),
            ("POST".to_string(), "/order".to_string()),
            ("POST".to_string(), "/authz/1".to_string()),
            ("POST".to_string(), "/challenge/1".to_string()),
            ("POST".to_string(), "/order".to_string()),
            ("POST".to_string(), "/finalize".to_string()),
            ("POST".to_string(), "/order".to_string()),
            ("POST".to_string(), "/cert".to_string()),
        ]
    );
    assert_eq!(proxy_a.hits(), a_hits_after_creation);
    assert_eq!(issued.primary_domain, "acme.example.test");
    assert_eq!(issued.issuer, "SAFE_FIXTURE Test CA");
    assert!(issued.cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(issued.fullchain_pem.contains("BEGIN CERTIFICATE"));
    assert!(issued.expires_at > issued.issued_at);
    assert!(proxy_b
        .requests()
        .iter()
        .any(|request| request.target == "http://acme.example.test/order"));

    publish_direct();
}

pub(crate) fn publish_direct() {
    let manager = crate::proxy::init_proxy_manager();
    manager.publish(manager.prepare(None).unwrap());
}

#[test]
fn test_domain_matching_rules() {
    let sans = vec![
        "example.com".to_string(),
        "*.example.com".to_string(),
        "admin.mycorp.org".to_string(),
    ];

    // Exact matches
    assert!(check_domain_match("example.com", &sans, None));
    assert!(check_domain_match("admin.mycorp.org", &sans, None));

    // Wildcard matches
    assert!(check_domain_match("api.example.com", &sans, None));
    assert!(check_domain_match("web.example.com", &sans, None));

    // Mismatches
    assert!(!check_domain_match("other.com", &sans, None));
    assert!(!check_domain_match("nested.api.example.com", &sans, None));
    assert!(!check_domain_match("mycorp.org", &sans, None));

    // Fallback to CN
    assert!(check_domain_match("cn-only.net", &[], Some("cn-only.net")));
}

#[test]
fn test_nginx_template_generation() {
    let config = generate_nginx_config(
        "example.com",
        &["www.example.com".to_string(), "*.example.com".to_string()],
        Some("/var/ssl"),
    );

    assert!(config.contains("server_name example.com www.example.com *.example.com;"));
    assert!(config.contains("ssl_certificate /var/ssl/example.com/fullchain.pem;"));
    assert!(config.contains("ssl_certificate_key /var/ssl/example.com/privkey.pem;"));
    assert!(config.contains("return 301 https://$host$request_uri;"));
    assert!(config.contains("listen 443 ssl http2;"));
}

#[test]
fn test_watchlist_operations() {
    // Add item
    let input = SslWatchItemInput {
        domain: "test.domain.local".to_string(),
        port: 443,
        alias: Some("Local Test".to_string()),
        alert_days_threshold: 15,
    };

    let item = add_watch_item(input).expect("add watch item");
    assert_eq!(item.domain, "test.domain.local");
    assert_eq!(item.alert_days_threshold, 15);

    // List and check existence
    let list = load_watchlist().expect("load watchlist");
    assert!(list.iter().any(|i| i.id == item.id));

    // Remove
    remove_watch_item(&item.id).expect("remove watch item");
    let after_list = load_watchlist().expect("load watchlist after remove");
    assert!(!after_list.iter().any(|i| i.id == item.id));
}

#[tokio::test]
async fn test_inspect_huanweiyun() {
    let res = super::inspector::inspect_ssl("huanweiyun.com", 443).await;
    match res {
        Ok(check) => {
            assert_eq!(check.domain, "huanweiyun.com");
            assert!(check.handshake_success);
            assert_eq!(check.tls_version.as_deref(), Some("TLSv1.3"));
        }
        Err(e) => {
            println!("Network check skipped or offline: {}", e);
        }
    }
}

#[test]
fn test_ssl_config_normalization() {
    use super::config::{is_valid_hh_mm, normalize_config, SslConfig};

    assert!(is_valid_hh_mm("00:00"));
    assert!(is_valid_hh_mm("09:30"));
    assert!(is_valid_hh_mm("23:59"));
    assert!(!is_valid_hh_mm("24:00"));
    assert!(!is_valid_hh_mm("12:60"));
    assert!(!is_valid_hh_mm("9:30"));
    assert!(!is_valid_hh_mm("invalid"));

    let config = SslConfig {
        auto_check_enabled: true,
        check_times: vec![
            "18:00".to_string(),
            "09:00".to_string(),
            "18:00".to_string(),
            "invalid".to_string(),
        ],
        alert_days_threshold: 0,
    };
    let normalized = normalize_config(config);
    assert_eq!(
        normalized.check_times,
        vec!["09:00".to_string(), "18:00".to_string()]
    );
    assert_eq!(normalized.alert_days_threshold, 30);
}
