fn start_local_runtime(
    app: AppHandle,
    record: SshTunnelRecord,
    resolved: ResolvedSshConfig,
    state: Arc<Mutex<RuntimeState>>,
    stop: Arc<AtomicBool>,
    active_clients: Arc<AtomicUsize>,
    startup: mpsc::Sender<StartupResult>,
) {
    let summary = tunnel_summary(&record.forward);
    let local_port = match record.forward.local_port {
        Some(port) => port,
        None => {
            let _ = startup.send(StartupResult::Failed("Missing local port".to_string()));
            return;
        }
    };
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

    if let Err(error) = ensure_local_port_available(local_port) {
        let _ = startup.send(StartupResult::Failed(error));
        return;
    }

    let initial_session = match open_authenticated_session(&resolved) {
        Ok(session) => session,
        Err(error) => {
            let _ = startup.send(StartupResult::Failed(error));
            return;
        }
    };

    if let Err(error) = open_direct_tcpip_channel(&initial_session, &target_host, target_port)
        .and_then(|mut channel| {
            let _ = channel.close();
            Ok(())
        })
    {
        let _ = startup.send(StartupResult::Failed(error));
        return;
    }

    let session_pool = Arc::new(SessionPool::with_initial_session(
        resolved.clone(),
        initial_session,
    ));

    let listener = match bind_local_listener(local_port) {
        Ok(listener) => listener,
        Err(error) => {
            let _ = startup.send(StartupResult::Failed(error));
            return;
        }
    };

    {
        let mut state_guard = state.lock().expect("runtime state poisoned");
        state_guard.status = SshTunnelStatus::Connected;
        state_guard.summary = summary.clone();
        state_guard.resolved_server_host = Some(format!("{}:{}", resolved.host, resolved.port));
        state_guard.listening_addr = Some(format!("{}:{}", LOCAL_BIND_HOST, local_port));
        state_guard.last_error = None;
    }
    let _ = update_record_connection_success(&record.id);
    emit_tunnels_updated(&app);
    let _ = startup.send(StartupResult::Connected(StartupSuccess {
        listening_addr: Some(format!("{}:{}", LOCAL_BIND_HOST, local_port)),
        resolved_server_host: format!("{}:{}", resolved.host, resolved.port),
    }));

    let mut last_health_check = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((socket, _peer)) => {
                let stop_for_client = stop.clone();
                let app_for_client = app.clone();
                let tunnel_id = record.id.clone();
                let target_host_for_client = target_host.clone();
                let session_pool_for_client = session_pool.clone();
                active_clients.fetch_add(1, Ordering::Relaxed);
                let active_clients_for_worker = active_clients.clone();
                thread::spawn(move || {
                    let result = (|| -> Result<(), String> {
                        let session = session_pool_for_client.acquire()?;
                        let channel = open_direct_tcpip_channel(
                            &session,
                            &target_host_for_client,
                            target_port,
                        )?;
                        let result = bridge_streams_dedicated_session(
                            socket,
                            &session,
                            channel,
                            stop_for_client,
                        );
                        if result.is_ok() {
                            session_pool_for_client.release(session);
                        }
                        result
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
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if last_health_check.elapsed() >= RECONNECT_HEALTH_CHECK_INTERVAL {
                    last_health_check = Instant::now();
                    if let Err(e) = session_pool.health_check() {
                        let error_msg = format!("SSH session health check failed: {}", e);
                        let _ = update_record_error(&record.id, &error_msg);
                        record_tunnel_failure(&app, &record, &error_msg, "health-check");
                        let _ = update_runtime_state(&app, &record.id, |s| {
                            s.status = SshTunnelStatus::Error;
                            s.last_error = Some(error_msg);
                        });
                        return;
                    }
                }
                thread::sleep(Duration::from_millis(150));
            }
            Err(error) => {
                let _ = startup.send(StartupResult::Failed(error.to_string()));
                let _ = update_record_error(&record.id, &error.to_string());
                let _ = update_runtime_state(&app, &record.id, |state| {
                    state.status = SshTunnelStatus::Error;
                    state.last_error = Some(error.to_string());
                });
                return;
            }
        }
    }

    let _ = update_runtime_state(&app, &record.id, |state| {
        state.status = SshTunnelStatus::Disconnected;
    });
}

fn read_socks_address(socket: &mut TcpStream) -> Result<(String, u16), String> {
    let mut header = [0u8; 4];
    socket.read_exact(&mut header).map_err(|e| e.to_string())?;
    if header[0] != 5 {
        return Err("Invalid SOCKS5 request version".to_string());
    }
    if header[1] != 1 {
        return Err("Only SOCKS5 CONNECT is supported".to_string());
    }
    let host = match header[3] {
        1 => {
            let mut addr = [0u8; 4];
            socket.read_exact(&mut addr).map_err(|e| e.to_string())?;
            std::net::Ipv4Addr::from(addr).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            socket.read_exact(&mut len).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; len[0] as usize];
            socket.read_exact(&mut buf).map_err(|e| e.to_string())?;
            String::from_utf8(buf).map_err(|e| e.to_string())?
        }
        4 => {
            let mut addr = [0u8; 16];
            socket.read_exact(&mut addr).map_err(|e| e.to_string())?;
            std::net::Ipv6Addr::from(addr).to_string()
        }
        _ => return Err("Unsupported SOCKS5 address type".to_string()),
    };
    let mut port_buf = [0u8; 2];
    socket
        .read_exact(&mut port_buf)
        .map_err(|e| e.to_string())?;
    Ok((host, u16::from_be_bytes(port_buf)))
}

fn write_socks_success(socket: &mut TcpStream) -> Result<(), String> {
    socket
        .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
        .map_err(|e| e.to_string())
}

fn write_socks_error(socket: &mut TcpStream, code: u8) {
    let _ = socket.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]);
}

fn handle_dynamic_client(
    socket: TcpStream,
    session_pool: Arc<SessionPool>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    socket.set_read_timeout(Some(SSH_IO_TIMEOUT)).ok();
    socket.set_write_timeout(Some(SSH_IO_TIMEOUT)).ok();
    let mut socket = socket;
    let mut hello = [0u8; 2];
    socket.read_exact(&mut hello).map_err(|e| e.to_string())?;
    if hello[0] != 5 {
        return Err("Invalid SOCKS5 greeting".to_string());
    }
    let mut methods = vec![0u8; hello[1] as usize];
    socket.read_exact(&mut methods).map_err(|e| e.to_string())?;
    if !methods.contains(&0) {
        socket.write_all(&[5, 0xff]).map_err(|e| e.to_string())?;
        return Err("SOCKS5 client does not support no-auth mode".to_string());
    }
    socket.write_all(&[5, 0]).map_err(|e| e.to_string())?;
    let (target_host, target_port) = read_socks_address(&mut socket)?;
    let session = session_pool.acquire()?;
    match open_direct_tcpip_channel(&session, &target_host, target_port) {
        Ok(channel) => {
            write_socks_success(&mut socket)?;
            let result = bridge_streams_dedicated_session(socket, &session, channel, stop);
            if result.is_ok() {
                session_pool.release(session);
            }
            result
        }
        Err(error) => {
            write_socks_error(&mut socket, 5);
            Err(format!(
                "SOCKS probe target {}:{} is unreachable: {}",
                target_host, target_port, error
            ))
        }
    }
}

fn serve_dynamic_listener(
    app: AppHandle,
    record: SshTunnelRecord,
    resolved: ResolvedSshConfig,
    state: Arc<Mutex<RuntimeState>>,
    stop: Arc<AtomicBool>,
    active_clients: Arc<AtomicUsize>,
    startup: mpsc::Sender<StartupResult>,
) {
    let local_port = match record.forward.local_port {
        Some(port) => port,
        None => {
            let _ = startup.send(StartupResult::Failed("Missing local port".to_string()));
            return;
        }
    };
    if let Err(error) = ensure_local_port_available(local_port) {
        let _ = startup.send(StartupResult::Failed(error));
        return;
    }
    let initial_session = match open_authenticated_session(&resolved) {
        Ok(session) => session,
        Err(error) => {
            let _ = startup.send(StartupResult::Failed(error));
            return;
        }
    };
    let session_pool = Arc::new(SessionPool::with_initial_session(
        resolved.clone(),
        initial_session,
    ));
    let listener = match bind_local_listener(local_port) {
        Ok(listener) => listener,
        Err(error) => {
            let _ = startup.send(StartupResult::Failed(error));
            return;
        }
    };

    {
        let mut state_guard = state.lock().expect("runtime state poisoned");
        state_guard.status = SshTunnelStatus::Connected;
        state_guard.resolved_server_host = Some(format!("{}:{}", resolved.host, resolved.port));
        state_guard.listening_addr = Some(format!("{}:{}", LOCAL_BIND_HOST, local_port));
        state_guard.last_error = None;
    }
    let _ = update_record_connection_success(&record.id);
    emit_tunnels_updated(&app);
    let _ = startup.send(StartupResult::Connected(StartupSuccess {
        listening_addr: Some(format!("{}:{}", LOCAL_BIND_HOST, local_port)),
        resolved_server_host: format!("{}:{}", resolved.host, resolved.port),
    }));

    let mut last_health_check = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((socket, _peer)) => {
                let stop_for_client = stop.clone();
                let app_for_client = app.clone();
                let tunnel_id = record.id.clone();
                let session_pool_for_client = session_pool.clone();
                let active_clients_for_worker = active_clients.clone();
                active_clients_for_worker.fetch_add(1, Ordering::Relaxed);
                thread::spawn(move || {
                    let result =
                        handle_dynamic_client(socket, session_pool_for_client, stop_for_client);
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
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if last_health_check.elapsed() >= RECONNECT_HEALTH_CHECK_INTERVAL {
                    last_health_check = Instant::now();
                    if let Err(e) = session_pool.health_check() {
                        let error_msg = format!("SSH session health check failed: {}", e);
                        let _ = update_record_error(&record.id, &error_msg);
                        record_tunnel_failure(&app, &record, &error_msg, "health-check");
                        let _ = update_runtime_state(&app, &record.id, |s| {
                            s.status = SshTunnelStatus::Error;
                            s.last_error = Some(error_msg);
                        });
                        return;
                    }
                }
                thread::sleep(Duration::from_millis(150));
            }
            Err(error) => {
                let _ = update_record_error(&record.id, &error.to_string());
                let _ = update_runtime_state(&app, &record.id, |state| {
                    state.status = SshTunnelStatus::Error;
                    state.last_error = Some(error.to_string());
                });
                return;
            }
        }
    }

    let _ = update_runtime_state(&app, &record.id, |state| {
        state.status = SshTunnelStatus::Disconnected;
    });
}
