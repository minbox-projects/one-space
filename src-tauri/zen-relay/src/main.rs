use std::collections::HashSet;
use std::convert::Infallible;
use std::io::{BufRead as _, Read as _, Write as _};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use futures_util::TryStreamExt;
use http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use http_body::Frame;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full, LengthLimitError, Limited, StreamBody};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use onespace_zen_relay::{
    is_forwardable_request_header, is_forwardable_response_header, join_upstream_url, token_matches,
    validate_upstream_base, DEFAULT_TIMEOUT_SECS, MAX_REQUEST_BODY_BYTES, SIDECAR_TOKEN_HEADER,
    TLS_CIPHER_LIST, TLS_CURVES, UPSTREAM_BASE_HEADER,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpListener, TcpStream};

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type RespBody = UnsyncBoxBody<Bytes, BoxError>;

const HEADER_ARRIVAL_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_HEADER_BYTES: usize = 64 * 1024;
const LOOPBACK_NO_PROXY: &str = "localhost,127.0.0.1,::1";

#[derive(Clone)]
struct AppState {
    token: Arc<String>,
    client: wreq::Client,
}

struct Args {
    proxy: Option<String>,
    timeout_secs: u64,
}

fn main() {
    if let Err(message) = run() {
        println!("ERROR {message}");
        let _ = std::io::stdout().flush();
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(std::env::args().skip(1))?;
    let token = read_token()?;
    if token.is_empty() {
        return Err("missing sidecar token".to_string());
    }
    spawn_stdin_watchdog();

    let client = build_client(&args)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| "failed to start async runtime".to_string())?;

    runtime.block_on(async move {
        let listener = TcpListener::bind(("127.0.0.1", 0u16))
            .await
            .map_err(|_| "failed to bind 127.0.0.1".to_string())?;
        let port = listener
            .local_addr()
            .map_err(|_| "failed to read loopback address".to_string())?
            .port();
        println!("READY {port}");
        std::io::stdout()
            .flush()
            .map_err(|_| "failed to flush startup line".to_string())?;

        serve(listener, AppState { token: Arc::new(token), client }).await;
        Ok(())
    })
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut proxy = None;
    let mut timeout_secs = DEFAULT_TIMEOUT_SECS;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--proxy" => {
                let value = args.next().ok_or_else(|| "missing value for --proxy".to_string())?;
                if value.is_empty() {
                    return Err("missing value for --proxy".to_string());
                }
                proxy = Some(value);
            }
            "--timeout-secs" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --timeout-secs".to_string())?;
                let parsed = value
                    .parse::<u64>()
                    .map_err(|_| "invalid value for --timeout-secs".to_string())?;
                if parsed == 0 {
                    return Err("invalid value for --timeout-secs".to_string());
                }
                timeout_secs = parsed;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Args { proxy, timeout_secs })
}

fn read_token() -> Result<String, String> {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|_| "failed to read sidecar token".to_string())?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn spawn_stdin_watchdog() {
    std::thread::spawn(|| {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 256];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => std::process::exit(0),
                Ok(_) => continue,
                Err(_) => std::process::exit(0),
            }
        }
    });
}

fn build_client(args: &Args) -> Result<wreq::Client, String> {
    let tls = wreq::tls::TlsOptions::builder()
        .cipher_list(TLS_CIPHER_LIST)
        .curves_list(TLS_CURVES)
        .build();
    let mut builder = wreq::Client::builder()
        .tls_options(tls)
        .read_timeout(Duration::from_secs(args.timeout_secs))
        .connect_timeout(Duration::from_secs(args.timeout_secs));

    if let Some(proxy) = args.proxy.as_deref() {
        let proxy = wreq::Proxy::all(proxy)
            .map_err(|_| "invalid --proxy value".to_string())?
            .no_proxy(wreq::NoProxy::from_string(LOOPBACK_NO_PROXY));
        builder = builder.proxy(proxy);
    } else {
        builder = builder.no_proxy();
    }

    builder
        .build()
        .map_err(|_| "failed to initialize HTTP client".to_string())
}

async fn serve(listener: TcpListener, state: AppState) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(_) => continue,
        };
        let state = state.clone();
        tokio::spawn(async move {
            let _ = stream.set_nodelay(true);
            let mut stream = stream;
            let mut prefix = Vec::with_capacity(8192);
            let mut buf = [0u8; 8192];

            let outcome = tokio::time::timeout(HEADER_ARRIVAL_TIMEOUT, async {
                loop {
                    if has_header_terminator(&prefix) || prefix.len() >= MAX_HEADER_BYTES {
                        return;
                    }
                    match stream.read(&mut buf).await {
                        Ok(0) => return,
                        Ok(n) => prefix.extend_from_slice(&buf[..n]),
                        Err(_) => return,
                    }
                }
            })
            .await;

            if outcome.is_err() {
                let status = malformed_request_status(&prefix);
                let _ = write_raw_status(&mut stream, status).await;
                return;
            }

            let io = BufferedIo { prefix, pos: 0, inner: stream };
            let service = service_fn(move |request| {
                let state = state.clone();
                handle(request, state)
            });
            let _ = http1::Builder::new()
                .serve_connection(TokioIo::new(io), service)
                .await;
        });
    }
}

async fn handle(request: Request<Incoming>, state: AppState) -> Result<Response<RespBody>, Infallible> {
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    let provided_token = request
        .headers()
        .get(SIDECAR_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    if !token_matches(&state.token, provided_token) {
        log_request(&method, &path, StatusCode::UNAUTHORIZED);
        return Ok(json_error(StatusCode::UNAUTHORIZED, "unauthorized"));
    }

    let upstream_base = match request
        .headers()
        .get(UPSTREAM_BASE_HEADER)
        .and_then(|value| value.to_str().ok())
    {
        Some(base) if validate_upstream_base(base).is_ok() => base.to_string(),
        _ => {
            log_request(&method, &path, StatusCode::BAD_REQUEST);
            return Ok(json_error(StatusCode::BAD_REQUEST, "invalid upstream base"));
        }
    };

    let declared_length = request
        .headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if declared_length.is_some_and(|length| length > MAX_REQUEST_BODY_BYTES as u64) {
        log_request(&method, &path, StatusCode::PAYLOAD_TOO_LARGE);
        return Ok(json_error(StatusCode::PAYLOAD_TOO_LARGE, "request body too large"));
    }

    let (parts, body) = request.into_parts();
    let body_bytes = match Limited::new(body, MAX_REQUEST_BODY_BYTES).collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(error) => {
            let status = if error.downcast_ref::<LengthLimitError>().is_some() {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::BAD_REQUEST
            };
            let message = if status == StatusCode::PAYLOAD_TOO_LARGE {
                "request body too large"
            } else {
                "invalid request body"
            };
            log_request(&method, &path, status);
            return Ok(json_error(status, message));
        }
    };

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("");
    let url = match join_upstream_url(&upstream_base, path_and_query) {
        Ok(url) => url,
        Err(_) => {
            log_request(&method, &path, StatusCode::BAD_REQUEST);
            return Ok(json_error(StatusCode::BAD_REQUEST, "invalid request path"));
        }
    };

    let connection_headers = connection_tokens(&parts.headers);
    let mut forward_headers = HeaderMap::new();
    for (name, value) in parts.headers.iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if !is_forwardable_request_header(&lower) || connection_headers.contains(&lower) {
            continue;
        }
        forward_headers.append(name.clone(), value.clone());
    }

    let upstream = match state
        .client
        .request(parts.method, url)
        .headers(forward_headers)
        .body(body_bytes)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            log_request(&method, &path, StatusCode::BAD_GATEWAY);
            return Ok(json_error(StatusCode::BAD_GATEWAY, "upstream request failed"));
        }
    };

    let status = upstream.status();
    log_request(&method, &path, status);

    let connection_headers = connection_tokens(upstream.headers());
    let mut response_headers = HeaderMap::new();
    for (name, value) in upstream.headers().iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if !is_forwardable_response_header(&lower) || connection_headers.contains(&lower) {
            continue;
        }
        response_headers.append(name.clone(), value.clone());
    }

    let stream = upstream
        .bytes_stream()
        .map_ok(Frame::data)
        .map_err(|error| -> BoxError { Box::new(error) });
    let body: RespBody = StreamBody::new(stream).boxed_unsync();

    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = response_headers;
    Ok(response)
}

fn connection_tokens(headers: &HeaderMap) -> HashSet<String> {
    let mut tokens = HashSet::new();
    for value in headers.get_all(http::header::CONNECTION).iter() {
        if let Ok(text) = value.to_str() {
            for token in text.split(',') {
                let token = token.trim();
                if !token.is_empty() {
                    tokens.insert(token.to_ascii_lowercase());
                }
            }
        }
    }
    tokens
}

fn has_header_terminator(prefix: &[u8]) -> bool {
    prefix.windows(4).any(|window| window == b"\r\n\r\n")
}

fn malformed_request_status(prefix: &[u8]) -> u16 {
    let text = String::from_utf8_lossy(prefix).to_ascii_lowercase();
    if let Some(index) = text.find("content-length") {
        let after = &text[index + "content-length".len()..];
        let after = after.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(length) = digits.parse::<u64>() {
            if length > MAX_REQUEST_BODY_BYTES as u64 {
                return 413;
            }
        }
    }
    400
}

async fn write_raw_status(stream: &mut TcpStream, status: u16) -> std::io::Result<()> {
    let (reason, message) = match status {
        413 => ("Payload Too Large", "request body too large"),
        _ => ("Bad Request", "invalid request"),
    };
    let body = format!("{{\"error\":\"{message}\"}}");
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

fn json_error(status: StatusCode, message: &'static str) -> Response<RespBody> {
    let body = format!("{{\"error\":\"{message}\"}}");
    let mut response = Response::new(full_body(Bytes::from(body)));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(http::header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

fn full_body(bytes: Bytes) -> RespBody {
    Full::new(bytes)
        .map_err(|never: Infallible| -> BoxError { match never {} })
        .boxed_unsync()
}

fn log_request(method: &Method, path: &str, status: StatusCode) {
    eprintln!("{method} {path} {}", status.as_u16());
}

struct BufferedIo {
    prefix: Vec<u8>,
    pos: usize,
    inner: TcpStream,
}

impl AsyncRead for BufferedIo {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.pos < this.prefix.len() {
            let end = (this.pos + buf.remaining()).min(this.prefix.len());
            let chunk = &this.prefix[this.pos..end];
            buf.put_slice(chunk);
            this.pos = end;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for BufferedIo {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod proxy_switch_named {
    use super::{build_client, Args};
    use std::ffi::OsString;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const PROXY_ENV_KEYS: [&str; 6] = [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ];
    const NO_PROXY_ENV_KEYS: [&str; 2] = ["NO_PROXY", "no_proxy"];
    const TARGET_URL: &str = "http://egress.example.test/";

    /// Minimal local HTTP proxy recorder: records the request target of every
    /// accepted connection and answers with a fixed 200 response.
    struct Recorder {
        url: String,
        request_lines: Arc<Mutex<Vec<String>>>,
        stop: Arc<AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl Recorder {
        fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind local relay recorder");
            listener
                .set_nonblocking(true)
                .expect("set recorder nonblocking");
            let url = format!(
                "http://{}",
                listener.local_addr().expect("recorder address")
            );
            let request_lines = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let thread_lines = Arc::clone(&request_lines);
            let thread_stop = Arc::clone(&stop);
            let handle = std::thread::spawn(move || {
                while !thread_stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => record_connection(stream, &thread_lines),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                url,
                request_lines,
                stop,
                handle: Some(handle),
            }
        }

        fn observed(&self) -> Vec<String> {
            self.request_lines
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone()
        }
    }

    impl Drop for Recorder {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn record_connection(mut stream: TcpStream, lines: &Arc<Mutex<Vec<String>>>) {
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let reader_stream = match stream.try_clone() {
            Ok(clone) => clone,
            Err(_) => return,
        };
        let mut reader = BufReader::new(reader_stream);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            return;
        }
        lines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(request_line.trim().to_string());
        loop {
            let mut header = String::new();
            match reader.read_line(&mut header) {
                Ok(0) => break,
                Ok(_) if header == "\r\n" || header == "\n" => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        );
    }

    struct EnvRestore {
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvRestore {
        fn apply(proxy_url: &str) -> Self {
            let mut saved = Vec::new();
            for key in PROXY_ENV_KEYS {
                saved.push((key, std::env::var_os(key)));
                std::env::set_var(key, proxy_url);
            }
            for key in NO_PROXY_ENV_KEYS {
                saved.push((key, std::env::var_os(key)));
                std::env::remove_var(key);
            }
            Self { saved }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in self.saved.drain(..) {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build relay test runtime")
            .block_on(future)
    }

    /// REQ-002 / AC-002: the independent relay uses only the explicit
    /// snapshot-derived proxy. Inherited proxy environment variables are ignored
    /// when the relay is started in direct mode, and a malformed explicit proxy
    /// is rejected instead of silently falling back.
    #[test]
    fn zen_relay_egress_uses_explicit_proxy_and_ignores_inherited_env() {
        let recorder = Recorder::new();
        let _env = EnvRestore::apply(&recorder.url);

        let explicit = block_on(async {
            let client = build_client(&Args {
                proxy: Some(recorder.url.clone()),
                timeout_secs: 3,
            })
            .expect("an explicit proxy must build a client");
            client.get(TARGET_URL).send().await
        })
        .expect("the explicit proxy request must complete");
        assert_eq!(explicit.status().as_u16(), 200);

        let observed = recorder.observed();
        assert_eq!(
            observed.len(),
            1,
            "the explicit proxy must observe exactly one request, saw {observed:?}"
        );
        assert!(
            observed[0].contains("egress.example.test"),
            "the explicit proxy must observe the configured upstream target, saw {observed:?}"
        );

        let direct = block_on(async {
            let client = build_client(&Args {
                proxy: None,
                timeout_secs: 2,
            })
            .expect("direct mode must build a client");
            client.get(TARGET_URL).send().await
        });
        assert!(
            direct.is_err(),
            "direct mode must not reach a non-loopback host through the inherited proxy environment"
        );
        assert_eq!(
            recorder.observed().len(),
            1,
            "direct mode must ignore inherited proxy environment variables"
        );

        let malformed = build_client(&Args {
            proxy: Some("http://[::1".to_string()),
            timeout_secs: 1,
        });
        assert!(
            malformed.is_err(),
            "a malformed explicit proxy must be rejected"
        );
    }
}
