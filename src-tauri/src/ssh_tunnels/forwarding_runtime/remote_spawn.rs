use crate::ssh_tunnels::{
    bridge_streams, emit_connect_failed, emit_tunnels_updated, ensure_local_target_reachable,
    handle_dynamic_client, load_records, open_authenticated_session, prepare_session_for_reuse,
    record_tunnel_failure, resolve_ssh_config_from_record, runtime_manager, runtime_view,
    serve_dynamic_listener, sleep_respecting_stop, start_local_runtime, tunnel_summary,
    update_record_connection_success, update_record_error, update_runtime_state,
    with_session_connect_timeout, ResolvedSshConfig, RunningTunnel, RuntimeState, SessionPool,
    SshTunnelForwardMode, SshTunnelRecord, SshTunnelRuntimeView, SshTunnelStatus, StartupResult,
    StartupSuccess, LOCAL_BIND_HOST, RECONNECT_HEALTH_CHECK_INTERVAL, RECONNECT_INITIAL_BACKOFF,
    RECONNECT_MAX_BACKOFF, REMOTE_BIND_HOST, SSH_CONNECT_TIMEOUT,
};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self};
use std::time::{Duration, Instant};
use tauri::AppHandle;

pub(in crate::ssh_tunnels) fn start_remote_runtime(
    app: AppHandle,
    record: SshTunnelRecord,
    resolved: ResolvedSshConfig,
    state: Arc<Mutex<RuntimeState>>,
    stop: Arc<AtomicBool>,
    active_clients: Arc<AtomicUsize>,
    startup: mpsc::Sender<StartupResult>,
) {
    let target_host = match record.forward.target_host.clone() {
        Some(host) => host,
        None => {
            let _ = startup.send(StartupResult::Failed("Missing target host".to_string()));
            return;
        }
    };
    let target_port = match record.forward.target_port {
        Some(port) => port,
        None => {
            let _ = startup.send(StartupResult::Failed("Missing target port".to_string()));
            return;
        }
    };
    if let Err(error) = ensure_local_target_reachable(&target_host, target_port) {
        let _ = startup.send(StartupResult::Failed(error));
        return;
    }
    let session = match open_authenticated_session(&resolved) {
        Ok(session) => session,
        Err(error) => {
            let _ = startup.send(StartupResult::Failed(error));
            return;
        }
    };
    let remote_port = match record.forward.remote_port {
        Some(port) => port,
        None => {
            let _ = startup.send(StartupResult::Failed("Missing remote port".to_string()));
            return;
        }
    };
    let remote_host = record
        .forward
        .remote_bind_host
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(REMOTE_BIND_HOST)
        .to_string();
    let (mut listener, bound_port) = match with_session_connect_timeout(&session, |session| {
        session
            .channel_forward_listen(remote_port, Some(&remote_host), Some(16))
            .map_err(|e| e.to_string())
    }) {
        Ok(result) => result,
        Err(error) => {
            let _ = startup.send(StartupResult::Failed(format!(
                "Failed to reserve remote port {}:{}: {}",
                remote_host, remote_port, error
            )));
            return;
        }
    };

    {
        let mut state_guard = state.lock().expect("runtime state poisoned");
        state_guard.status = SshTunnelStatus::Connected;
        state_guard.resolved_server_host = Some(format!("{}:{}", resolved.host, resolved.port));
        state_guard.listening_addr = Some(format!("{}:{}", remote_host, bound_port));
        state_guard.last_error = None;
    }
    let _ = update_record_connection_success(&record.id);
    emit_tunnels_updated(&app);
    let _ = startup.send(StartupResult::Connected(StartupSuccess {
        listening_addr: Some(format!("{}:{}", remote_host, bound_port)),
        resolved_server_host: format!("{}:{}", resolved.host, resolved.port),
    }));

    let mut last_health_check = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok(channel) => {
                let stop_for_client = stop.clone();
                let app_for_client = app.clone();
                let tunnel_id = record.id.clone();
                let active_clients_for_worker = active_clients.clone();
                let target_host_for_worker = target_host.clone();
                active_clients_for_worker.fetch_add(1, Ordering::Relaxed);
                thread::spawn(move || {
                    let result = (|| -> Result<(), String> {
                        let addr = format!("{}:{}", target_host_for_worker, target_port);
                        let socket_addr = addr
                            .parse::<SocketAddr>()
                            .ok()
                            .or_else(|| {
                                (target_host_for_worker.as_str(), target_port)
                                    .to_socket_addrs()
                                    .ok()
                                    .and_then(|mut addrs| addrs.next())
                            })
                            .ok_or_else(|| format!("Could not resolve local target {}", addr))?;
                        let socket = TcpStream::connect_timeout(&socket_addr, SSH_CONNECT_TIMEOUT)
                            .map_err(|e| {
                                format!("Failed to connect to local target {}: {}", addr, e)
                            })?;
                        bridge_streams(socket, channel, stop_for_client)
                    })();
                    active_clients_for_worker.fetch_sub(1, Ordering::Relaxed);
                    if let Err(error) = result {
                        let _ = update_record_error(&tunnel_id, &error);
                        let _ = update_runtime_state(&app_for_client, &tunnel_id, |state| {
                            state.last_error = Some(error.clone());
                        });
                    } else {
                        emit_tunnels_updated(&app_for_client);
                    }
                });
            }
            Err(error) if error.to_string().to_lowercase().contains("timed out") => {
                if last_health_check.elapsed() >= RECONNECT_HEALTH_CHECK_INTERVAL {
                    last_health_check = Instant::now();
                    if let Err(e) = prepare_session_for_reuse(&session) {
                        let error_msg = format!("SSH session health check failed: {}", e);
                        let _ = update_record_error(&record.id, &error_msg);
                        record_tunnel_failure(&app, &record, &error_msg, "health-check");
                        let _ = update_runtime_state(&app, &record.id, |state| {
                            state.status = SshTunnelStatus::Error;
                            state.last_error = Some(error_msg);
                        });
                        return;
                    }
                }
                continue;
            }
            Err(error) => {
                let err = error.to_string();
                let _ = update_record_error(&record.id, &err);
                let _ = update_runtime_state(&app, &record.id, |state| {
                    state.status = if stop.load(Ordering::Relaxed) {
                        SshTunnelStatus::Disconnected
                    } else {
                        SshTunnelStatus::Error
                    };
                    state.last_error = Some(err.clone());
                });
                return;
            }
        }
    }

    let _ = update_runtime_state(&app, &record.id, |state| {
        state.status = SshTunnelStatus::Disconnected;
    });
}

pub(in crate::ssh_tunnels) fn spawn_runtime_thread(
    app: AppHandle,
    record: SshTunnelRecord,
    resolved: ResolvedSshConfig,
) -> Result<(RunningTunnel, Result<StartupSuccess, String>), String> {
    let state = Arc::new(Mutex::new(RuntimeState {
        status: SshTunnelStatus::Connecting,
        mode: record.forward.mode.clone(),
        summary: tunnel_summary(&record.forward),
        resolved_server_host: None,
        listening_addr: None,
        last_error: None,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let active_clients = Arc::new(AtomicUsize::new(0));
    let (startup_tx, startup_rx) = mpsc::channel::<StartupResult>();
    let state_for_thread = state.clone();
    let stop_for_thread = stop.clone();
    let active_for_thread = active_clients.clone();
    let record_for_thread = record.clone();
    let app_for_thread = app.clone();
    let resolved_for_thread = resolved.clone();

    let join = thread::spawn(move || {
        let mut first_attempt = true;
        let mut backoff = RECONNECT_INITIAL_BACKOFF;
        let mut startup_tx = Some(startup_tx);

        loop {
            let tx = startup_tx.take().unwrap_or_else(|| mpsc::channel().0);

            if !first_attempt {
                let grace_start = Instant::now();
                while active_for_thread.load(Ordering::Relaxed) > 0
                    && grace_start.elapsed() < Duration::from_secs(3)
                    && !stop_for_thread.load(Ordering::Relaxed)
                {
                    thread::sleep(Duration::from_millis(100));
                }
                if stop_for_thread.load(Ordering::Relaxed) {
                    break;
                }
                {
                    let mut g = state_for_thread.lock().ok();
                    if let Some(ref mut g) = g {
                        g.status = SshTunnelStatus::Reconnecting;
                        g.last_error = None;
                    }
                }
                emit_tunnels_updated(&app_for_thread);
            }
            first_attempt = false;

            match record_for_thread.forward.mode {
                SshTunnelForwardMode::Local => start_local_runtime(
                    app_for_thread.clone(),
                    record_for_thread.clone(),
                    resolved_for_thread.clone(),
                    state_for_thread.clone(),
                    stop_for_thread.clone(),
                    active_for_thread.clone(),
                    tx,
                ),
                SshTunnelForwardMode::Remote => start_remote_runtime(
                    app_for_thread.clone(),
                    record_for_thread.clone(),
                    resolved_for_thread.clone(),
                    state_for_thread.clone(),
                    stop_for_thread.clone(),
                    active_for_thread.clone(),
                    tx,
                ),
                SshTunnelForwardMode::Dynamic => serve_dynamic_listener(
                    app_for_thread.clone(),
                    record_for_thread.clone(),
                    resolved_for_thread.clone(),
                    state_for_thread.clone(),
                    stop_for_thread.clone(),
                    active_for_thread.clone(),
                    tx,
                ),
            }

            if stop_for_thread.load(Ordering::Relaxed) {
                break;
            }

            let was_connected_before = state_for_thread
                .lock()
                .ok()
                .map_or(false, |g| g.status == SshTunnelStatus::Connected);

            let should_reconnect = record_for_thread.auto_reconnect
                && state_for_thread
                    .lock()
                    .ok()
                    .map_or(false, |g| g.status == SshTunnelStatus::Error);

            if !should_reconnect {
                break;
            }

            if was_connected_before {
                backoff = RECONNECT_INITIAL_BACKOFF;
            }

            if !sleep_respecting_stop(&stop_for_thread, backoff) {
                break;
            }
            backoff = (backoff * 2).min(RECONNECT_MAX_BACKOFF);
        }
    });

    let tunnel = RunningTunnel {
        stop,
        active_clients,
        state: state.clone(),
        join: Some(join),
    };

    match startup_rx.recv_timeout(Duration::from_secs(20)) {
        Ok(StartupResult::Connected(startup)) => {
            let mut state_guard = state.lock().map_err(|e| e.to_string())?;
            state_guard.status = SshTunnelStatus::Connected;
            state_guard.resolved_server_host = Some(startup.resolved_server_host.clone());
            state_guard.listening_addr = startup.listening_addr.clone();
            drop(state_guard);
            Ok((tunnel, Ok(startup)))
        }
        Ok(StartupResult::Failed(error)) => {
            // Thread keeps running with reconnect loop; do NOT stop it.
            Ok((tunnel, Err(error)))
        }
        Err(_) => {
            // Thread may still be connecting; do NOT stop it.
            Ok((
                tunnel,
                Err("Timed out while establishing the SSH tunnel".to_string()),
            ))
        }
    }
}

pub(in crate::ssh_tunnels) fn probe_dynamic_via_temp_proxy(
    resolved: ResolvedSshConfig,
    target_host: String,
    target_port: u16,
) -> Result<(), String> {
    let listener = TcpListener::bind((LOCAL_BIND_HOST, 0))
        .map_err(|e| format!("Failed to start temporary SOCKS5 probe: {}", e))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = stop.clone();
    let initial_session = open_authenticated_session(&resolved)?;
    let session_pool = Arc::new(SessionPool::with_initial_session(resolved, initial_session));
    let session_pool_for_thread = session_pool.clone();
    let handle = thread::spawn(move || -> Result<(), String> {
        let (socket, _) = listener.accept().map_err(|e| e.to_string())?;
        handle_dynamic_client(socket, session_pool_for_thread, stop_for_thread)
    });

    let mut client = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        SSH_CONNECT_TIMEOUT,
    )
    .map_err(|e| format!("Failed to connect to temporary SOCKS5 probe: {}", e))?;
    client.set_read_timeout(Some(SSH_CONNECT_TIMEOUT)).ok();
    client.set_write_timeout(Some(SSH_CONNECT_TIMEOUT)).ok();
    client.write_all(&[5, 1, 0]).map_err(|e| e.to_string())?;
    let mut hello = [0u8; 2];
    client.read_exact(&mut hello).map_err(|e| e.to_string())?;
    if hello != [5, 0] {
        stop.store(true, Ordering::Relaxed);
        let _ = handle.join();
        return Err("The temporary SOCKS5 probe failed during negotiation".to_string());
    }
    let host_bytes = target_host.as_bytes();
    let mut request = vec![5, 1, 0, 3, host_bytes.len() as u8];
    request.extend_from_slice(host_bytes);
    request.extend_from_slice(&target_port.to_be_bytes());
    client.write_all(&request).map_err(|e| e.to_string())?;
    let mut response = [0u8; 10];
    client
        .read_exact(&mut response)
        .map_err(|e| e.to_string())?;
    stop.store(true, Ordering::Relaxed);
    let thread_result = handle
        .join()
        .map_err(|_| "Dynamic probe thread panicked".to_string())?;
    if response[1] != 0 {
        return Err(format!(
            "The SOCKS5 proxy could not reach {}:{} (reply code {}).",
            target_host, target_port, response[1]
        ));
    }
    thread_result
}

pub(in crate::ssh_tunnels) fn disconnect_runtime(id: &str) -> Result<(), String> {
    let maybe_running = runtime_manager()
        .lock()
        .map_err(|e| e.to_string())?
        .remove(id);

    if let Some(mut running) = maybe_running {
        running.stop.store(true, Ordering::Relaxed);
        if let Some(join) = running.join.take() {
            let _ = join.join();
        }
    }

    Ok(())
}

pub(in crate::ssh_tunnels) fn connect_internal(
    app: AppHandle,
    id: String,
    emit_failure_event: bool,
) -> Result<SshTunnelRuntimeView, String> {
    let record = load_records()?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| "Tunnel not found".to_string())?;

    let _ = disconnect_runtime(&id);
    let resolved = resolve_ssh_config_from_record(&record)?;
    let (running, startup_result) = spawn_runtime_thread(app.clone(), record.clone(), resolved)?;

    let view = runtime_view(&record, Some(&running));
    runtime_manager()
        .lock()
        .map_err(|e| e.to_string())?
        .insert(record.id.clone(), running);
    emit_tunnels_updated(&app);

    match startup_result {
        Ok(_) => Ok(view),
        Err(error) => {
            let _ = update_record_error(&record.id, &error);
            if emit_failure_event {
                emit_connect_failed(&app, &record, &error);
            }
            Err(error)
        }
    }
}
