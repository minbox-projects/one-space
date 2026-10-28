use crate::ai_gateway::GatewaySettings;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

/// A ready sidecar endpoint and the per-process token required to call it.
pub(in crate::ai_gateway) struct StartedSidecar {
    pub(in crate::ai_gateway) endpoint_base: String,
    pub(in crate::ai_gateway) token: String,
}

const READINESS_TIMEOUT_SECS: u64 = 10;
const RELAY_BINARY_NAME: &str = "onespace-zen-relay";
const RELAY_BINARY_ENV: &str = "ONESPACE_ZEN_RELAY_BIN";

#[derive(Clone, PartialEq, Eq)]
struct SidecarIdentity {
    proxy_url: Option<String>,
    timeout_secs: u32,
}

struct Managed {
    child: Child,
    _stdin: ChildStdin,
    token: String,
    port: u16,
    identity: SidecarIdentity,
}

impl Managed {
    fn started(&self) -> StartedSidecar {
        StartedSidecar {
            endpoint_base: format!("http://127.0.0.1:{}", self.port),
            token: self.token.clone(),
        }
    }

    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

fn manager() -> &'static Mutex<Option<Managed>> {
    static MANAGER: OnceLock<Mutex<Option<Managed>>> = OnceLock::new();
    MANAGER.get_or_init(|| Mutex::new(None))
}

fn lock_manager() -> MutexGuard<'static, Option<Managed>> {
    manager().lock().unwrap_or_else(|error| error.into_inner())
}

/// Start the sidecar if it is not already running with a matching identity and
/// return its loopback endpoint. The blocking process handshake runs off the
/// async runtime.
pub(in crate::ai_gateway) async fn ensure_started(
    settings: &GatewaySettings,
) -> Result<StartedSidecar, String> {
    #[cfg(test)]
    {
        if let Some(test) = test_endpoint_snapshot() {
            return Ok(test);
        }
    }
    let proxy_url = normalize_proxy(settings.proxy_url.as_deref());
    let timeout_secs = settings.read_timeout_secs;
    let identity = SidecarIdentity {
        proxy_url: proxy_url.clone(),
        timeout_secs,
    };
    tokio::task::spawn_blocking(move || ensure_started_blocking(identity, proxy_url, timeout_secs))
        .await
        .map_err(|error| format!("zen sidecar startup task failed: {error}"))?
}

fn ensure_started_blocking(
    identity: SidecarIdentity,
    proxy_url: Option<String>,
    timeout_secs: u32,
) -> Result<StartedSidecar, String> {
    let mut guard = lock_manager();
    if let Some(existing) = guard.as_mut() {
        if existing.identity == identity && existing.is_running() {
            return Ok(existing.started());
        }
    }
    if let Some(mut stale) = guard.take() {
        terminate(&mut stale);
    }
    let managed = spawn_sidecar(identity, proxy_url, timeout_secs)?;
    let started = managed.started();
    *guard = Some(managed);
    Ok(started)
}

/// Synchronously terminate the owned sidecar (if any). Idempotent and safe to
/// call from either gateway shutdown seam.
pub(in crate::ai_gateway) fn stop() {
    let mut guard = lock_manager();
    if let Some(mut managed) = guard.take() {
        terminate(&mut managed);
    }
}

fn terminate(managed: &mut Managed) {
    let _ = managed.child.kill();
    let _ = managed.child.wait();
}

fn spawn_sidecar(
    identity: SidecarIdentity,
    proxy_url: Option<String>,
    timeout_secs: u32,
) -> Result<Managed, String> {
    let binary = locate_relay_binary()?;
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());

    let mut command = Command::new(&binary);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(proxy) = proxy_url.as_deref() {
        command.arg("--proxy").arg(proxy);
    }
    command.arg("--timeout-secs").arg(timeout_secs.to_string());

    let mut child = command.spawn().map_err(|error| {
        format!(
            "failed to start zen sidecar at {}: {error}",
            binary.display()
        )
    })?;

    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            terminate_raw(&mut child);
            return Err(format!(
                "zen sidecar at {} did not expose stdin",
                binary.display()
            ));
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_raw(&mut child);
            return Err(format!(
                "zen sidecar at {} did not expose stdout",
                binary.display()
            ));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            terminate_raw(&mut child);
            return Err(format!(
                "zen sidecar at {} did not expose stderr",
                binary.display()
            ));
        }
    };

    // The token arrives as the first stdin line; keeping stdin open for the
    // child's lifetime is part of the handshake and its shutdown detection.
    if writeln!(stdin, "{token}")
        .and_then(|_| stdin.flush())
        .is_err()
    {
        terminate_raw(&mut child);
        return Err(format!(
            "zen sidecar at {} closed stdin before receiving its token",
            binary.display()
        ));
    }

    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            log::debug!("zen sidecar stderr: {line}");
        }
    });

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u16, String>>();
    thread::spawn(move || read_ready_line(stdout, ready_tx));

    let readiness = match ready_rx.recv_timeout(Duration::from_secs(READINESS_TIMEOUT_SECS)) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "zen sidecar at {} did not report readiness within {READINESS_TIMEOUT_SECS}s",
            binary.display()
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(format!(
            "zen sidecar at {} exited before reporting readiness",
            binary.display()
        )),
    };
    let port = match readiness {
        Ok(port) => port,
        Err(message) => {
            terminate_raw(&mut child);
            return Err(format!(
                "zen sidecar at {} failed to start: {message}",
                binary.display()
            ));
        }
    };

    Ok(Managed {
        child,
        _stdin: stdin,
        token,
        port,
        identity,
    })
}

fn terminate_raw(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Read stdout until the child reports `READY <port>` or `ERROR <message>`.
/// Lines after the first protocol line (if any) are drained at debug level.
fn read_ready_line(
    stdout: std::process::ChildStdout,
    ready_tx: mpsc::Sender<Result<u16, String>>,
) {
    let reader = BufReader::new(stdout);
    let mut reported = false;
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim_end();
        if !reported {
            if let Some(port) = line.strip_prefix("READY ") {
                reported = true;
                let parsed = port
                    .trim()
                    .parse::<u16>()
                    .map_err(|_| format!("invalid READY port: {line}"));
                let _ = ready_tx.send(parsed);
                continue;
            }
            if let Some(message) = line.strip_prefix("ERROR ") {
                reported = true;
                let _ = ready_tx.send(Err(message.to_string()));
                continue;
            }
        }
        log::debug!("zen sidecar stdout: {line}");
    }
    if !reported {
        let _ = ready_tx.send(Err("stdout closed before readiness".to_string()));
    }
}

fn normalize_proxy(proxy_url: Option<&str>) -> Option<String> {
    proxy_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn relay_binary_file_name() -> String {
    if cfg!(windows) {
        format!("{RELAY_BINARY_NAME}.exe")
    } else {
        RELAY_BINARY_NAME.to_string()
    }
}

fn target_triple() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

fn locate_relay_binary() -> Result<PathBuf, String> {
    if let Some(explicit) = std::env::var_os(RELAY_BINARY_ENV) {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "zen sidecar binary from {RELAY_BINARY_ENV} was not found: {}",
            path.display()
        ));
    }

    let file_name = relay_binary_file_name();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join(&file_name));
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("zen-relay/target/debug").join(&file_name));
    candidates.push(manifest.join("zen-relay/target/release").join(&file_name));
    if let Some(triple) = target_triple() {
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        candidates.push(
            manifest
                .join("binaries")
                .join(format!("{RELAY_BINARY_NAME}-{triple}{suffix}")),
        );
    }

    if let Some(found) = candidates.iter().find(|candidate| candidate.is_file()) {
        return Ok(found.clone());
    }

    let expected = candidates
        .iter()
        .map(|candidate| candidate.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "zen sidecar binary was not found; expected one of: {expected}"
    ))
}

#[cfg(test)]
struct TestEndpoint {
    endpoint_base: String,
    token: String,
}

#[cfg(test)]
fn test_endpoint_slot() -> &'static Mutex<Option<TestEndpoint>> {
    static TEST_ENDPOINT: OnceLock<Mutex<Option<TestEndpoint>>> = OnceLock::new();
    TEST_ENDPOINT.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
fn test_endpoint_snapshot() -> Option<StartedSidecar> {
    let slot = test_endpoint_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    slot.as_ref().map(|test| StartedSidecar {
        endpoint_base: test.endpoint_base.clone(),
        token: test.token.clone(),
    })
}

/// Point forwarding at a mock endpoint instead of spawning the relay binary.
#[cfg(test)]
pub(in crate::ai_gateway) fn set_test_endpoint(endpoint_base: &str, token: &str) {
    let mut slot = test_endpoint_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *slot = Some(TestEndpoint {
        endpoint_base: endpoint_base.to_string(),
        token: token.to_string(),
    });
}

#[cfg(test)]
pub(in crate::ai_gateway) fn clear_test_endpoint() {
    let mut slot = test_endpoint_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *slot = None;
}

#[cfg(test)]
pub(in crate::ai_gateway) fn is_running() -> bool {
    if test_endpoint_snapshot().is_some() {
        return true;
    }
    let mut guard = lock_manager();
    guard
        .as_mut()
        .map(Managed::is_running)
        .unwrap_or(false)
}
