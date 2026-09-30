use crate::ssh_tunnels::{
    accept_error_is_periodic_tick, apply_probe_outcome, bridge_streams, emit_connect_failed,
    emit_tunnels_updated, ensure_local_target_reachable, handle_dynamic_client, load_records,
    open_authenticated_session, open_authenticated_session_kinded, resolve_ssh_config_from_record,
    run_supervision, run_two_step_probe, runtime_manager, runtime_view, serve_dynamic_listener,
    sleep_respecting_stop, start_local_runtime, transport_round_trip, tunnel_failure_message_input,
    tunnel_summary, update_record_connection_success, update_record_error, update_runtime_state,
    with_session_connect_timeout, AppSupervisorObserver, FailureKind, PreSpawnConnectFailure,
    ResolvedSshConfig, RunningTunnel, RuntimeOutcome, RuntimeState, SessionPool,
    SshTunnelFailureEvent, SshTunnelForwardMode, SshTunnelRecord, SshTunnelRuntimeView,
    SshTunnelStatus, StartupResult, StartupSuccess, LOCAL_BIND_HOST, PROBE_INTERVAL,
    REMOTE_BIND_HOST, SSH_CONNECT_TIMEOUT, SSH_TUNNEL_CONNECT_FAILED_EVENT,
};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub(in crate::ssh_tunnels) fn start_remote_runtime(
    app: AppHandle,
    record: SshTunnelRecord,
    resolved: ResolvedSshConfig,
    state: Arc<Mutex<RuntimeState>>,
    stop: Arc<AtomicBool>,
    active_clients: Arc<AtomicUsize>,
    startup: mpsc::Sender<StartupResult>,
) -> RuntimeOutcome {
    let target_host = match record.forward.target_host.clone() {
        Some(host) => host,
        None => {
            let message = "Missing target host".to_string();
            let _ = startup.send(StartupResult::Failed(message.clone()));
            return RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Config,
                message,
            };
        }
    };
    let target_port = match record.forward.target_port {
        Some(port) => port,
        None => {
            let message = "Missing target port".to_string();
            let _ = startup.send(StartupResult::Failed(message.clone()));
            return RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Config,
                message,
            };
        }
    };
    if let Err(error) = ensure_local_target_reachable(&target_host, target_port) {
        let _ = startup.send(StartupResult::Failed(error.clone()));
        return RuntimeOutcome::FailedAtStartup {
            kind: FailureKind::Target,
            message: error,
        };
    }
    let session = match open_authenticated_session_kinded(&resolved) {
        Ok(session) => session,
        Err((kind, message)) => {
            let _ = startup.send(StartupResult::Failed(message.clone()));
            return RuntimeOutcome::FailedAtStartup { kind, message };
        }
    };
    let remote_port = match record.forward.remote_port {
        Some(port) => port,
        None => {
            let message = "Missing remote port".to_string();
            let _ = startup.send(StartupResult::Failed(message.clone()));
            return RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Config,
                message,
            };
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
            let message = format!(
                "Failed to reserve remote port {}:{}: {}",
                remote_host, remote_port, error
            );
            let _ = startup.send(StartupResult::Failed(message.clone()));
            return RuntimeOutcome::FailedAtStartup {
                kind: FailureKind::Port,
                message,
            };
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

    let mut last_probe = Instant::now();
    let mut consecutive_transport_failures: u32 = 0;

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
            Err(error) => {
                let io_error = io::Error::from(error);
                if accept_error_is_periodic_tick(&io_error) {
                    if last_probe.elapsed() >= PROBE_INTERVAL {
                        last_probe = Instant::now();
                        let outcome =
                            run_two_step_probe(|| transport_round_trip(&session), || Ok(()));
                        let transport_message = match &outcome {
                            Err((_kind, message)) => Some(message.clone()),
                            Ok(()) => None,
                        };
                        let previous_error =
                            state.lock().ok().and_then(|guard| guard.last_error.clone());
                        let should_reconnect = apply_probe_outcome(
                            &state,
                            &mut consecutive_transport_failures,
                            outcome,
                        );
                        let current_error =
                            state.lock().ok().and_then(|guard| guard.last_error.clone());
                        if previous_error != current_error {
                            emit_tunnels_updated(&app);
                        }
                        if should_reconnect {
                            return if stop.load(Ordering::Relaxed) {
                                RuntimeOutcome::Stopped
                            } else {
                                RuntimeOutcome::DroppedAfterConnected {
                                    kind: FailureKind::Transport,
                                    message: transport_message.unwrap_or_else(|| {
                                        "SSH transport probe failed".to_string()
                                    }),
                                }
                            };
                        }
                    }
                    continue;
                }
                let message = io_error.to_string();
                return if stop.load(Ordering::Relaxed) {
                    RuntimeOutcome::Stopped
                } else {
                    RuntimeOutcome::DroppedAfterConnected {
                        kind: FailureKind::Transport,
                        message,
                    }
                };
            }
        }
    }

    RuntimeOutcome::Stopped
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
        let mut startup_tx = Some(startup_tx);
        let mut first_attempt = true;

        let app_for_runtime = app_for_thread.clone();
        let record_for_runtime = record_for_thread.clone();
        let resolved_for_runtime = resolved_for_thread.clone();
        let state_for_runtime = state_for_thread.clone();
        let stop_for_runtime = stop_for_thread.clone();
        let active_for_runtime = active_for_thread.clone();

        let next_outcome = move || -> RuntimeOutcome {
            let tx = startup_tx.take().unwrap_or_else(|| mpsc::channel().0);

            if !first_attempt {
                let grace_start = Instant::now();
                while active_for_runtime.load(Ordering::Relaxed) > 0
                    && grace_start.elapsed() < Duration::from_secs(3)
                    && !stop_for_runtime.load(Ordering::Relaxed)
                {
                    thread::sleep(Duration::from_millis(100));
                }
                if stop_for_runtime.load(Ordering::Relaxed) {
                    return RuntimeOutcome::Stopped;
                }
            }
            first_attempt = false;

            match record_for_runtime.forward.mode {
                SshTunnelForwardMode::Local => start_local_runtime(
                    app_for_runtime.clone(),
                    record_for_runtime.clone(),
                    resolved_for_runtime.clone(),
                    state_for_runtime.clone(),
                    stop_for_runtime.clone(),
                    active_for_runtime.clone(),
                    tx,
                ),
                SshTunnelForwardMode::Remote => start_remote_runtime(
                    app_for_runtime.clone(),
                    record_for_runtime.clone(),
                    resolved_for_runtime.clone(),
                    state_for_runtime.clone(),
                    stop_for_runtime.clone(),
                    active_for_runtime.clone(),
                    tx,
                ),
                SshTunnelForwardMode::Dynamic => serve_dynamic_listener(
                    app_for_runtime.clone(),
                    record_for_runtime.clone(),
                    resolved_for_runtime.clone(),
                    state_for_runtime.clone(),
                    stop_for_runtime.clone(),
                    active_for_runtime.clone(),
                    tx,
                ),
            }
        };

        let stop_for_sleep = stop_for_thread.clone();
        let sleeper = move |delay: Duration| sleep_respecting_stop(&stop_for_sleep, delay);
        let mut observer = AppSupervisorObserver::new(&app_for_thread, &record_for_thread);

        run_supervision(
            &state_for_thread,
            record_for_thread.auto_reconnect,
            &stop_for_thread,
            next_outcome,
            sleeper,
            &mut observer,
        );
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
    let resolved = match resolve_connect_target(&record) {
        Ok(resolved) => resolved,
        Err(failure) => {
            log::warn!(
                "SSH tunnel {} connect failed before spawn ({:?}): {}",
                record.id,
                failure.kind,
                failure.message
            );
            let input = tunnel_failure_message_input(&record, "auto-connect", &failure.message);
            let _ = crate::messages::create_message_with_app(&app, input);
            if emit_failure_event {
                let _ = app.emit(
                    SSH_TUNNEL_CONNECT_FAILED_EVENT,
                    SshTunnelFailureEvent {
                        id: record.id.clone(),
                        name: record.name.clone(),
                        error: failure.message.clone(),
                        auto_connect: record.auto_connect,
                    },
                );
            }
            return Err(failure.message);
        }
    };
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

/// Resolves the SSH configuration before spawning a runtime. A failure is
/// terminal for this connect attempt: the error is persisted and returned
/// without spawning or retrying.
pub(in crate::ssh_tunnels) fn resolve_connect_target(
    record: &SshTunnelRecord,
) -> Result<ResolvedSshConfig, PreSpawnConnectFailure> {
    match resolve_ssh_config_from_record(record) {
        Ok(resolved) => Ok(resolved),
        Err(message) => {
            let _ = update_record_error(&record.id, &message);
            Err(PreSpawnConnectFailure {
                kind: FailureKind::Config,
                message,
            })
        }
    }
}
