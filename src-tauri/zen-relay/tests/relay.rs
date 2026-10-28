use onespace_zen_relay as relay;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const TOKEN: &str = "test-sidecar-token";

fn spawn_sidecar() -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_onespace-zen-relay"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sidecar");
    let stdin = child.stdin.as_mut().expect("stdin");
    writeln!(stdin, "{TOKEN}").expect("write token");
    stdin.flush().expect("flush token");
    child
}

fn read_ready_line(child: &mut Child) -> Result<u16, String> {
    let stdout = child.stdout.take().ok_or_else(|| "missing stdout".to_string())?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = tx.send(line);
    });
    let line = rx
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| "timed out waiting for READY".to_string())?;
    let line = line.trim_end_matches('\n').trim_end_matches('\r');
    let port_text = line
        .strip_prefix("READY ")
        .ok_or_else(|| format!("unexpected startup line: {line}"))?;
    port_text
        .parse::<u16>()
        .map_err(|_| format!("invalid port: {port_text}"))
}

fn send_http(port: u16, request: &str) -> Result<Option<u16>, String> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_secs(2),
    )
    .map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;

    let mut response = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buf[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let text = String::from_utf8_lossy(&response);
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok());
    Ok(status)
}

#[test]
fn max_request_body_bytes_is_64_mib() {
    assert_eq!(relay::MAX_REQUEST_BODY_BYTES, 64 * 1024 * 1024);
}

#[test]
fn join_upstream_url_handles_v1_and_query() {
    assert_eq!(
        relay::join_upstream_url("https://opencode.ai/zen/v1", "/chat/completions?x=1").unwrap(),
        "https://opencode.ai/zen/v1/chat/completions?x=1"
    );
    assert_eq!(
        relay::join_upstream_url("https://opencode.ai/zen/v1", "/v1/chat/completions?x=1").unwrap(),
        "https://opencode.ai/zen/v1/chat/completions?x=1"
    );
    assert_eq!(
        relay::join_upstream_url("https://opencode.ai/zen", "/chat/completions").unwrap(),
        "https://opencode.ai/zen/chat/completions"
    );
    assert!(relay::join_upstream_url("https://opencode.ai/zen/v1", "").is_err());
}

#[test]
fn validate_upstream_base_allowlist() {
    for base in [
        "https://opencode.ai/zen",
        "https://opencode.ai/zen/v1",
        "https://opencode.ai/zen/go/v1",
    ] {
        assert!(relay::validate_upstream_base(base).is_ok(), "{base}");
    }
    for base in [
        "http://opencode.ai/zen",
        "https://opencode.ai/evil",
        "https://evil.example.com/zen",
        "https://opencode.ai.evil.example.com/zen",
        "https://opencode.ai:8443/zen",
        "https://user:pass@opencode.ai/zen",
        "",
    ] {
        assert!(relay::validate_upstream_base(base).is_err(), "{base}");
    }
}

#[test]
fn forwardable_request_headers() {
    for name in [
        "host",
        "content-length",
        "connection",
        "keep-alive",
        "proxy-connection",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        "accept-encoding",
        "x-onespace-sidecar-token",
        "x-onespace-upstream-base",
        "HOST",
        "X-Onespace-Sidecar-Token",
    ] {
        assert!(!relay::is_forwardable_request_header(name), "{name}");
    }
    for name in [
        "authorization",
        "content-type",
        "accept",
        "user-agent",
        "x-opencode-session-id",
    ] {
        assert!(relay::is_forwardable_request_header(name), "{name}");
    }
}

#[test]
fn forwardable_response_headers() {
    for name in [
        "content-length",
        "transfer-encoding",
        "connection",
        "keep-alive",
        "te",
        "trailer",
        "upgrade",
    ] {
        assert!(!relay::is_forwardable_response_header(name), "{name}");
    }
    for name in ["content-type", "content-encoding", "cache-control", "x-request-id"] {
        assert!(relay::is_forwardable_response_header(name), "{name}");
    }
}

#[test]
fn token_exact_match_only() {
    assert!(relay::token_matches("secret", Some("secret")));
    assert!(!relay::token_matches("secret", None));
    assert!(!relay::token_matches("secret", Some("")));
    assert!(!relay::token_matches("secret", Some("secretx")));
    assert!(!relay::token_matches("secret", Some("secre")));
    assert!(!relay::token_matches("secret", Some("other")));
}

#[test]
fn sidecar_startup_ready_line() {
    let mut child = spawn_sidecar();
    let port = match read_ready_line(&mut child) {
        Ok(port) => port,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{error}");
        }
    };
    assert!(port > 0);
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn sidecar_auth_requires_token() {
    let mut child = spawn_sidecar();
    let port = match read_ready_line(&mut child) {
        Ok(port) => port,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{error}");
        }
    };

    let no_token = "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    assert_eq!(send_http(port, no_token).unwrap(), Some(401));

    let wrong_token = "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nx-onespace-sidecar-token: wrong\r\nConnection: close\r\n\r\n";
    assert_eq!(send_http(port, wrong_token).unwrap(), Some(401));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn sidecar_validates_upstream_base() {
    let mut child = spawn_sidecar();
    let port = match read_ready_line(&mut child) {
        Ok(port) => port,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{error}");
        }
    };

    let evil = "GET / HTTP/1.1\\r\\nHost: 127.0.0.1\\r\\nx-onespace-sidecar-token: test-sidecar-token\\r\\nx-onespace-upstream-base: https://evil.example.com/zen/v1\\r\\nConnection: close\\r\\n\\r\\n";
    assert_eq!(send_http(port, evil).unwrap(), Some(400));

    let allowed = "POST /v1/chat/completions HTTP/1.1\\r\\nHost: 127.0.0.1\\r\\nx-onespace-sidecar-token: test-sidecar-token\\r\\nx-onespace-upstream-base: https://opencode.ai/zen/v1\\r\\nContent-Length: 67108865\\r\\nConnection: close\\r\\n\\r\\n";
    assert_eq!(send_http(port, allowed).unwrap(), Some(413));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn sidecar_body_limit_returns_413() {
    let mut child = spawn_sidecar();
    let port = match read_ready_line(&mut child) {
        Ok(port) => port,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{error}");
        }
    };

    let request = "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 67108865\r\nx-onespace-sidecar-token: test-sidecar-token\r\nx-onespace-upstream-base: https://opencode.ai/zen/v1\r\nConnection: close\r\n\r\n";
    assert_eq!(send_http(port, request).unwrap(), Some(413));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn sidecar_stops_when_stdin_closes() {
    let mut child = spawn_sidecar();
    let _port = match read_ready_line(&mut child) {
        Ok(port) => port,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{error}");
        }
    };

    drop(child.stdin.take());

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if started.elapsed() > Duration::from_secs(10) {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("sidecar did not exit after stdin closed");
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{error}");
            }
        }
    }
}

#[test]
fn sidecar_does_not_log_secrets() {
    let mut child = spawn_sidecar();
    let port = match read_ready_line(&mut child) {
        Ok(port) => port,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{error}");
        }
    };

    let stderr = child.stderr.take().expect("stderr");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = stderr;
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        let _ = tx.send(text);
    });

    let request = concat!(
        "POST /v1/chat/completions HTTP/1.1\\r\\n",
        "Host: 127.0.0.1\\r\\n",
        "Authorization: Bearer secret-SENTINEL-value\\r\\n",
        "Content-Type: application/json\\r\\n",
        "x-onespace-sidecar-token: test-sidecar-token\\r\\n",
        "x-onespace-upstream-base: https://opencode.ai/zen/v1\\r\\n",
        "Content-Length: 67108865\\r\\n",
        "Connection: close\\r\\n\\r\\n"
    );
    let _ = send_http(port, request);

    let _ = child.kill();
    let _ = child.wait();

    let stderr_text = rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_default();
    assert!(!stderr_text.contains("test-sidecar-token"));
    assert!(!stderr_text.contains("secret-SENTINEL-value"));
    assert!(!stderr_text.contains("Bearer"));
    assert!(!stderr_text.contains("secret body value"));
}
