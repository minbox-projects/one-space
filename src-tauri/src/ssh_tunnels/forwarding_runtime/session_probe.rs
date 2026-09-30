use crate::ssh_tunnels::{
    emit_tunnels_updated, open_authenticated_session, probe_dynamic_via_temp_proxy, FailureKind,
    ResolvedSshConfig, RuntimeState, SessionPool, SshTunnelForwardConfig, SshTunnelForwardMode,
    LOCAL_BIND_HOST, PROBE_TIMEOUT, REMOTE_BIND_HOST, SSH_CONNECT_TIMEOUT, SSH_IO_TIMEOUT,
    SSH_KEEPALIVE_INTERVAL_SECS,
};
use ssh2::Session;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;

pub(in crate::ssh_tunnels) fn session_timeout_ms(timeout: Duration) -> u32 {
    timeout.as_millis().min(u128::from(u32::MAX)) as u32
}

pub(in crate::ssh_tunnels) fn set_session_timeout(session: &Session, timeout: Duration) {
    session.set_timeout(session_timeout_ms(timeout));
}

pub(in crate::ssh_tunnels) fn prepare_session_for_reuse(session: &Session) -> Result<(), String> {
    if !session.authenticated() {
        return Err("SSH session is not authenticated".to_string());
    }
    session.set_blocking(true);
    set_session_timeout(session, SSH_IO_TIMEOUT);
    session.set_keepalive(true, SSH_KEEPALIVE_INTERVAL_SECS);
    session
        .keepalive_send()
        .map(|_| ())
        .map_err(|e| format!("SSH keepalive failed: {}", e))
}

pub(in crate::ssh_tunnels) fn with_session_connect_timeout<T>(
    session: &Session,
    operation: impl FnOnce(&Session) -> Result<T, String>,
) -> Result<T, String> {
    set_session_timeout(session, SSH_CONNECT_TIMEOUT);
    let result = operation(session);
    set_session_timeout(session, SSH_IO_TIMEOUT);
    result
}

pub(in crate::ssh_tunnels) fn with_session_probe_timeout<T>(
    session: &Session,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    set_session_timeout(session, PROBE_TIMEOUT);
    let result = operation();
    set_session_timeout(session, SSH_IO_TIMEOUT);
    result
}

/// Step one of the round-trip probe: opens and closes an SSH session channel so
/// the server must answer the channel-open request within the probe timeout.
pub(in crate::ssh_tunnels) fn transport_round_trip(session: &Session) -> Result<(), String> {
    with_session_probe_timeout(session, || {
        let mut channel = session.channel_session().map_err(|e| e.to_string())?;
        let _ = channel.close();
        Ok(())
    })
}

/// Step two of the local round-trip probe: opens and closes a `direct-tcpip`
/// channel to the forwarding target through the session.
pub(in crate::ssh_tunnels) fn probe_local_target(
    session: &Session,
    target_host: &str,
    target_port: u16,
) -> Result<(), String> {
    with_session_probe_timeout(session, || {
        let mut channel = session
            .channel_direct_tcpip(target_host, target_port, None)
            .map_err(|e| e.to_string())?;
        let _ = channel.close();
        Ok(())
    })
}

/// Runs the two probe steps in order. A step-one failure is a transport
/// failure and the target step is never invoked; a step-two failure after a
/// successful step one is a target failure.
pub(in crate::ssh_tunnels) fn run_two_step_probe(
    transport: impl FnOnce() -> Result<(), String>,
    target: impl FnOnce() -> Result<(), String>,
) -> Result<(), (FailureKind, String)> {
    transport().map_err(|message| (FailureKind::Transport, message))?;
    target().map_err(|message| (FailureKind::Target, message))?;
    Ok(())
}

/// Folds one probe outcome into the runtime state. A success clears the
/// counter and the recorded error; a target failure only records the error; a
/// transport failure increments the counter and reports whether the runtime
/// must reconnect (two consecutive failures).
pub(in crate::ssh_tunnels) fn apply_probe_outcome(
    state: &Arc<Mutex<RuntimeState>>,
    consecutive_transport_failures: &mut u32,
    outcome: Result<(), (FailureKind, String)>,
) -> bool {
    match outcome {
        Ok(()) => {
            *consecutive_transport_failures = 0;
            if let Ok(mut guard) = state.lock() {
                guard.last_error = None;
            }
            false
        }
        Err((FailureKind::Target, message)) => {
            if let Ok(mut guard) = state.lock() {
                guard.last_error = Some(message);
            }
            false
        }
        Err((_kind, message)) => {
            *consecutive_transport_failures += 1;
            if let Ok(mut guard) = state.lock() {
                guard.last_error = Some(message);
            }
            *consecutive_transport_failures >= 2
        }
    }
}

/// A `TimedOut` or `WouldBlock` accept error is the periodic non-blocking tick
/// of a bounded accept call; any other kind is a real listener error. The
/// message text is deliberately not inspected.
pub(in crate::ssh_tunnels) fn accept_error_is_periodic_tick(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

/// Result of one probe tick driven from a runtime accept loop.
pub(in crate::ssh_tunnels) enum ProbeTickResult {
    /// The tunnel stays connected.
    Continue,
    /// The transport failed twice consecutively; reconnect with this message.
    Reconnect { message: String },
}

/// Runs one pooled-session probe tick: acquires a session, runs the two-step
/// probe and folds the outcome into the state, releasing a live session back to
/// the pool and dropping a session whose transport round trip failed. Emits
/// `ssh-tunnels-updated` only when the recorded error changes.
pub(in crate::ssh_tunnels) fn run_pool_probe_tick<T>(
    app: &AppHandle,
    session_pool: &SessionPool,
    state: &Arc<Mutex<RuntimeState>>,
    consecutive_transport_failures: &mut u32,
    target: T,
) -> ProbeTickResult
where
    T: FnOnce(&Session) -> Result<(), String>,
{
    let previous_error = state.lock().ok().and_then(|guard| guard.last_error.clone());

    let outcome = match session_pool.acquire() {
        Ok(session) => {
            let outcome =
                run_two_step_probe(|| transport_round_trip(&session), || target(&session));
            if matches!(&outcome, Err((FailureKind::Transport, _))) {
                let _ = session.disconnect(None, "Probe transport round trip failed", None);
            } else {
                session_pool.release(session);
            }
            outcome
        }
        Err(message) => Err((FailureKind::Transport, message)),
    };

    let reconnect_message = match &outcome {
        Err((_kind, message)) => Some(message.clone()),
        Ok(()) => None,
    };
    let should_reconnect = apply_probe_outcome(state, consecutive_transport_failures, outcome);

    let current_error = state.lock().ok().and_then(|guard| guard.last_error.clone());
    if previous_error != current_error {
        emit_tunnels_updated(app);
    }

    if should_reconnect {
        ProbeTickResult::Reconnect {
            message: reconnect_message.unwrap_or_else(|| "SSH transport probe failed".to_string()),
        }
    } else {
        ProbeTickResult::Continue
    }
}

pub(in crate::ssh_tunnels) fn open_direct_tcpip_channel(
    session: &Session,
    target_host: &str,
    target_port: u16,
) -> Result<ssh2::Channel, String> {
    with_session_connect_timeout(session, |session| {
        session
            .channel_direct_tcpip(target_host, target_port, None)
            .map_err(|e| e.to_string())
    })
}

pub(in crate::ssh_tunnels) fn port_conflict_details(port: u16) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.lines().nth(1)?.trim().to_string();
        if line.is_empty() {
            None
        } else {
            Some(line)
        }
    }
    #[cfg(target_os = "windows")]
    {
        let netstat = Command::new("netstat")
            .args(["-ano", "-p", "tcp"])
            .output()
            .ok()?;
        if !netstat.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&netstat.stdout);
        let pid = stdout
            .lines()
            .find(|line| line.contains(&format!(":{port}")) && line.contains("LISTENING"))
            .and_then(|line| line.split_whitespace().last())
            .map(|value| value.to_string())?;
        let tasklist = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .ok()?;
        let proc_name = if tasklist.status.success() {
            String::from_utf8_lossy(&tasklist.stdout)
                .lines()
                .skip(3)
                .find_map(|line| line.split_whitespace().next())
                .unwrap_or("unknown")
                .to_string()
        } else {
            "unknown".to_string()
        };
        Some(format!("PID {} ({})", pid, proc_name))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = port;
        None
    }
}

pub(in crate::ssh_tunnels) fn ensure_local_port_available(port: u16) -> Result<(), String> {
    match TcpListener::bind((LOCAL_BIND_HOST, port)) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(error) => {
            let details = port_conflict_details(port)
                .map(|details| format!(" Occupied by {}.", details))
                .unwrap_or_default();
            Err(format!(
                "Local port {} is unavailable: {}.{}",
                port, error, details
            ))
        }
    }
}

pub(in crate::ssh_tunnels) fn ensure_local_target_reachable(
    host: &str,
    port: u16,
) -> Result<(), String> {
    let addr = format!("{}:{}", host, port);
    let socket_addr = addr
        .parse::<SocketAddr>()
        .ok()
        .or_else(|| {
            (host, port)
                .to_socket_addrs()
                .ok()
                .and_then(|mut addrs| addrs.next())
        })
        .ok_or_else(|| format!("Could not resolve target {}", addr))?;
    TcpStream::connect_timeout(&socket_addr, SSH_CONNECT_TIMEOUT)
        .map(|stream| {
            drop(stream);
        })
        .map_err(|error| format!("Target service {} is unreachable: {}", addr, error))
}

pub(in crate::ssh_tunnels) fn probe_forward(
    forward: &SshTunnelForwardConfig,
    resolved: &ResolvedSshConfig,
) -> Result<String, String> {
    let result = match forward.mode {
        SshTunnelForwardMode::Local => {
            let local_port = forward
                .local_port
                .ok_or_else(|| "Missing local port".to_string())?;
            ensure_local_port_available(local_port)?;
            let session = open_authenticated_session(resolved)?;
            let target_host = forward
                .target_host
                .as_deref()
                .ok_or_else(|| "Missing target host".to_string())?;
            let target_port = forward
                .target_port
                .ok_or_else(|| "Missing target port".to_string())?;
            let mut channel = open_direct_tcpip_channel(&session, target_host, target_port)
                .map_err(|e| {
                    format!(
                        "Target {}:{} is unreachable: {}",
                        target_host, target_port, e
                    )
                })?;
            let _ = channel.close();
            let _ = session.disconnect(None, "Probe completed", None);
            Ok(format!(
                "SSH login succeeded and remote target {}:{} is reachable.",
                target_host, target_port
            ))
        }
        SshTunnelForwardMode::Remote => {
            let target_host = forward
                .target_host
                .as_deref()
                .ok_or_else(|| "Missing target host".to_string())?;
            let target_port = forward
                .target_port
                .ok_or_else(|| "Missing target port".to_string())?;
            ensure_local_target_reachable(target_host, target_port)?;
            let session = open_authenticated_session(resolved)?;
            let remote_port = forward
                .remote_port
                .ok_or_else(|| "Missing remote port".to_string())?;
            let remote_host = forward
                .remote_bind_host
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(REMOTE_BIND_HOST);
            let (_listener, bound_port) = with_session_connect_timeout(&session, |session| {
                session
                    .channel_forward_listen(remote_port, Some(remote_host), Some(16))
                    .map_err(|e| {
                        format!(
                            "Failed to reserve remote port {}:{}: {}",
                            remote_host, remote_port, e
                        )
                    })
            })?;
            let _ = session.disconnect(None, "Probe completed", None);
            Ok(format!(
                "SSH login succeeded, remote port {}:{} is available, and local target {}:{} is reachable.",
                remote_host, bound_port, target_host, target_port
            ))
        }
        SshTunnelForwardMode::Dynamic => {
            let local_port = forward
                .local_port
                .ok_or_else(|| "Missing local port".to_string())?;
            ensure_local_port_available(local_port)?;
            let session = open_authenticated_session(resolved)?;
            let _ = session.disconnect(None, "Probe completed", None);
            if let (Some(host), Some(port)) = (
                forward
                    .dynamic_probe_host
                    .as_deref()
                    .filter(|value| !value.trim().is_empty()),
                forward.dynamic_probe_port,
            ) {
                probe_dynamic_via_temp_proxy(resolved.clone(), host.to_string(), port)?;
                Ok(format!(
                    "SOCKS5 proxy probe to {}:{} succeeded.",
                    host, port
                ))
            } else {
                Ok("SSH login succeeded and the SOCKS5 proxy can be started.".to_string())
            }
        }
    };
    result
}
