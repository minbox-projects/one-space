use crate::ai_gateway::GatewaySettings;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

/// A ready sidecar endpoint, the per-process token required to call it, and the
/// lease that keeps its relay process alive until the response body completes.
#[derive(Clone)]
pub(in crate::ai_gateway) struct StartedSidecar {
    pub(in crate::ai_gateway) endpoint_base: String,
    pub(in crate::ai_gateway) token: String,
    _lease: Option<SidecarLease>,
}

/// One admitted response's hold on a specific relay child. The last lease of a
/// retiring child terminates it; the current child stays alive between
/// requests.
struct SidecarLease {
    child_id: u64,
    live: Arc<AtomicUsize>,
}

impl SidecarLease {
    fn acquire(child_id: u64, live: Arc<AtomicUsize>) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self { child_id, live }
    }
}

impl Clone for SidecarLease {
    fn clone(&self) -> Self {
        Self::acquire(self.child_id, Arc::clone(&self.live))
    }
}

impl Drop for SidecarLease {
    fn drop(&mut self) {
        if self.live.fetch_sub(1, Ordering::SeqCst) == 1 {
            release_child(self.child_id);
        }
    }
}

const READINESS_TIMEOUT_SECS: u64 = 10;
const RELAY_BINARY_NAME: &str = "onespace-zen-relay";
const RELAY_BINARY_ENV: &str = "ONESPACE_ZEN_RELAY_BIN";

#[derive(Clone, PartialEq, Eq)]
struct SidecarIdentity {
    generation: u64,
    proxy_url: Option<String>,
    timeout_secs: u32,
}

struct Managed {
    child: Child,
    _stdin: ChildStdin,
    token: String,
    port: u16,
    identity: SidecarIdentity,
    child_id: u64,
    live: Arc<AtomicUsize>,
}

impl Managed {
    fn started(&self) -> StartedSidecar {
        StartedSidecar {
            endpoint_base: format!("http://127.0.0.1:{}", self.port),
            token: self.token.clone(),
            _lease: Some(SidecarLease::acquire(self.child_id, Arc::clone(&self.live))),
        }
    }

    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

struct ManagerState {
    current: Option<Managed>,
    retiring: Vec<Managed>,
}

fn manager() -> &'static Mutex<ManagerState> {
    static MANAGER: OnceLock<Mutex<ManagerState>> = OnceLock::new();
    MANAGER.get_or_init(|| {
        Mutex::new(ManagerState {
            current: None,
            retiring: Vec::new(),
        })
    })
}

fn lock_manager() -> MutexGuard<'static, ManagerState> {
    manager().lock().unwrap_or_else(|error| error.into_inner())
}

/// The explicit upstream proxy for the current global snapshot: the full proxy
/// URL including credentials when enabled, or `None` for direct transport.
fn explicit_proxy_url(snapshot: &crate::proxy::ProxySnapshot) -> Option<String> {
    if !snapshot.is_enabled() {
        return None;
    }
    crate::proxy::proxy_env_vars(snapshot)
        .into_iter()
        .find_map(|(key, value)| (key == "HTTP_PROXY").then_some(value))
}

/// Start the sidecar if it is not already running with a matching identity and
/// return its loopback endpoint and response lease. The blocking process
/// handshake runs off the async runtime.
pub(in crate::ai_gateway) async fn ensure_started(
    settings: &GatewaySettings,
) -> Result<StartedSidecar, String> {
    #[cfg(test)]
    {
        if let Some(test) = test_endpoint_snapshot() {
            return Ok(test);
        }
    }
    let snapshot = crate::ai_gateway::current_proxy_snapshot();
    let proxy_url = explicit_proxy_url(&snapshot);
    if snapshot.is_enabled() && proxy_url.is_none() {
        return Err("global proxy configuration is not usable for the zen sidecar".to_string());
    }
    let timeout_secs = settings.read_timeout_secs;
    let identity = SidecarIdentity {
        generation: snapshot.generation,
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
    if let Some(existing) = guard.current.as_mut() {
        if existing.identity == identity && existing.is_running() {
            return Ok(existing.started());
        }
    }
    if let Some(mut old) = guard.current.take() {
        if old.live.load(Ordering::SeqCst) == 0 {
            terminate(&mut old);
        } else {
            guard.retiring.push(old);
        }
    }
    let managed = spawn_sidecar(identity, proxy_url, timeout_secs)?;
    let started = managed.started();
    guard.current = Some(managed);
    Ok(started)
}

/// Terminate one retiring child once its last admitted lease has been released.
/// The current child is never terminated by a lease release.
fn release_child(child_id: u64) {
    let mut guard = lock_manager();
    if guard
        .current
        .as_ref()
        .is_some_and(|managed| managed.child_id == child_id)
    {
        return;
    }
    if let Some(index) = guard
        .retiring
        .iter()
        .position(|managed| {
            managed.child_id == child_id && managed.live.load(Ordering::SeqCst) == 0
        })
    {
        let mut managed = guard.retiring.remove(index);
        terminate(&mut managed);
    }
}

/// Synchronously terminate every owned current and retiring sidecar. Idempotent
/// and safe to call from either gateway shutdown seam.
pub(in crate::ai_gateway) fn stop() {
    let mut guard = lock_manager();
    if let Some(mut managed) = guard.current.take() {
        terminate(&mut managed);
    }
    for mut managed in guard.retiring.drain(..) {
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
    static NEXT_CHILD_ID: AtomicU64 = AtomicU64::new(1);
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
        child_id: NEXT_CHILD_ID.fetch_add(1, Ordering::SeqCst),
        live: Arc::new(AtomicUsize::new(0)),
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
        _lease: None,
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
        .current
        .as_mut()
        .map(Managed::is_running)
        .unwrap_or(false)
}


#[cfg(unix)]
#[cfg(test)]
mod proxy_switch_named {
    use super::{clear_test_endpoint, ensure_started, is_running, stop};
    use crate::ai_gateway::GatewaySettings;
    use crate::proxy::test_support::{HttpProxyRecorder, SERIAL_GUARD};
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    const RELAY_BIN_ENV: &str = "ONESPACE_ZEN_RELAY_BIN";
    const LOG_DIR_ENV: &str = "ONESPACE_ZEN_SIDECAR_LOG_DIR";

    struct EnvRestore {
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvRestore {
        fn set(values: &[(&'static str, OsString)]) -> Self {
            let saved = values
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            for (key, value) in values {
                std::env::set_var(key, value);
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

    fn port_of(url: &str) -> u16 {
        url.trim_start_matches("http://")
            .split(':')
            .nth(1)
            .expect("recorder URL has a port")
            .parse()
            .expect("recorder URL port is numeric")
    }

    fn proxy_config(url: &str) -> crate::config::ProxyConfig {
        crate::config::ProxyConfig {
            proxy_enabled: true,
            proxy_type: "http".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: port_of(url),
            proxy_username: None,
            proxy_password: None,
            check_interval: 15,
        }
    }

    fn events(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("events"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn start_pids(dir: &Path) -> Vec<u32> {
        events(dir)
            .iter()
            .filter_map(|line| line.strip_prefix("start "))
            .filter_map(|pid| pid.trim().parse().ok())
            .collect()
    }

    async fn wait_for_start_pids(dir: &Path, expected: usize) -> Vec<u32> {
        for _ in 0..200 {
            let pids = start_pids(dir);
            if pids.len() >= expected {
                return pids;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!(
            "expected {expected} sidecar start records, saw {:?}",
            events(dir)
        );
    }

    fn pid_alive(pid: u32) -> bool {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    async fn wait_until_dead(pid: u32) {
        for _ in 0..200 {
            if !pid_alive(pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("sidecar pid {pid} must exit");
    }

    /// REQ-004 / AC-007: a proxy generation change starts a new relay child,
    /// keeps the retiring child alive while a response lease holds it, reuses the
    /// current child for an equivalent save, and stops every owned generation.
    #[tokio::test(flavor = "current_thread")]
    async fn zen_sidecar_generation_lifecycle() {
        let _serial = SERIAL_GUARD.lock().unwrap_or_else(|error| error.into_inner());
        let _home = crate::lock_test_home_env();

        let dir = std::env::temp_dir().join(format!(
            "onespace-zen-generation-lifecycle-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create sidecar lifecycle directory");
        let script = dir.join("relay.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\n\
             dir=\"$ONESPACE_ZEN_SIDECAR_LOG_DIR\"\n\
             pid=$$\n\
             echo \"start $pid\" >> \"$dir/events\"\n\
             trap 'echo \"exit $pid\" >> \"$dir/events\"; exit 0' TERM INT HUP\n\
             read token\n\
             echo \"ready $pid\" >> \"$dir/events\"\n\
             if [ -f \"$dir/ports\" ]; then\n\
               count=$(wc -l < \"$dir/ports\" | tr -d ' ')\n\
             else\n\
               count=0\n\
             fi\n\
             echo x >> \"$dir/ports\"\n\
             port=$((41000 + count))\n\
             echo \"READY $port\"\n\
             while read line; do :; done\n\
             echo \"exit $pid\" >> \"$dir/events\"\n",
        )
        .expect("write sidecar lifecycle script");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("make sidecar lifecycle script executable");

        let _env = EnvRestore::set(&[
            (RELAY_BIN_ENV, script.into_os_string()),
            (LOG_DIR_ENV, dir.clone().into_os_string()),
        ]);

        clear_test_endpoint();
        stop();

        let proxy_a = HttpProxyRecorder::new("A").await;
        let proxy_b = HttpProxyRecorder::new("B").await;
        let manager = crate::proxy::PROXY_MANAGER
            .get_or_init(|| Arc::new(crate::proxy::ProxyManager::new()));

        manager.publish(
            manager
                .prepare(Some(proxy_config(&proxy_a.url())))
                .expect("proxy A prepares"),
        );
        let settings = GatewaySettings::default();
        let started_a = ensure_started(&settings)
            .await
            .expect("generation A sidecar starts");
        let pids = wait_for_start_pids(&dir, 1).await;
        let pid_a = pids[0];
        assert!(pid_alive(pid_a), "generation A child must be running");

        manager.publish(
            manager
                .prepare(Some(proxy_config(&proxy_b.url())))
                .expect("proxy B prepares"),
        );
        let started_b = ensure_started(&settings)
            .await
            .expect("generation B sidecar starts");
        assert_ne!(
            started_a.endpoint_base, started_b.endpoint_base,
            "a new proxy generation must own a distinct relay endpoint"
        );
        let pids = wait_for_start_pids(&dir, 2).await;
        assert_eq!(
            pids.len(),
            2,
            "two generations must spawn exactly two children, saw {:?}",
            events(&dir)
        );
        let pid_b = pids[1];
        assert_ne!(pid_a, pid_b, "both children must have distinct pids");
        assert!(
            pid_alive(pid_a) && pid_alive(pid_b),
            "both the leasing child and the current child must stay alive"
        );
        let ready_records: Vec<String> = events(&dir)
            .into_iter()
            .filter(|line| line.starts_with("ready "))
            .collect();
        assert_eq!(
            ready_records,
            vec![format!("ready {pid_a}"), format!("ready {pid_b}")],
            "each child must read its token and report readiness"
        );

        // An equivalent save keeps the same generation and must reuse the child.
        manager.publish(
            manager
                .prepare(Some(proxy_config(&proxy_b.url())))
                .expect("equivalent proxy B prepares"),
        );
        let started_b_again = ensure_started(&settings)
            .await
            .expect("equivalent save reuses the current sidecar");
        assert_eq!(
            started_b_again.endpoint_base, started_b.endpoint_base,
            "an equivalent save must reuse the same relay child"
        );
        assert_eq!(
            start_pids(&dir).len(),
            2,
            "an equivalent save must not spawn another child, saw {:?}",
            events(&dir)
        );
        drop(started_b_again);

        // Releasing the last generation A lease retires that child only.
        drop(started_a);
        wait_until_dead(pid_a).await;
        assert!(!pid_alive(pid_a), "the retired generation A child must exit");
        assert!(
            pid_alive(pid_b),
            "the current generation B child must survive generation A's lease release"
        );

        stop();
        wait_until_dead(pid_b).await;
        assert!(!pid_alive(pid_b), "the current generation B child must exit");
        assert!(!is_running(), "no owned sidecar may remain after stop");

        drop(started_b);
        drop(_env);
        stop();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
