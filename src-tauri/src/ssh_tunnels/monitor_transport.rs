//! Narrow SSH adapter for server monitoring.
//!
//! Monitoring reuses the existing SSH configuration resolution, host-key
//! verification and authentication paths. It owns only its own command session:
//! no tunnel record, terminal or SFTP session is created, started or modified.

use super::{
    authenticate_session, bridge_streams_dedicated_session, open_authenticated_session_kinded,
    resolve_ssh_config_from_input, set_session_timeout, verify_host_key, FailureKind,
    SshTunnelAuthKind, SshTunnelCustomInput, SshTunnelForwardConfig, SshTunnelForwardMode,
    SshTunnelProbeDraftInput, SshTunnelSourceKind, SSH_CONNECT_TIMEOUT, SSH_IO_TIMEOUT,
};
use crate::server_monitor::transport::{MonitorConnection, MonitorConnector};
use crate::server_monitor::types::{
    CollectorOutput, MonitorAuth, MonitorConnectRequest, MonitorSource, MonitorTransportError,
    MonitorTransportErrorKind, ServerMonitorAuthKind, ServerMonitorSourceKind,
    ServerMonitorTunnelRecord,
};
use ssh2::{ExtendedData, Session};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Connects monitor collections through the project's existing SSH semantics.
pub(crate) struct SshMonitorConnector;

impl SshMonitorConnector {
    pub(crate) fn new() -> Self {
        Self
    }

    fn connect_via_tunnel(
        &self,
        request: &MonitorConnectRequest,
        tunnel: &ServerMonitorTunnelRecord,
    ) -> Result<Box<dyn MonitorConnection>, MonitorTransportError> {
        // 1. Resolve and connect to the Jump Host
        let jump_input = tunnel_to_input(tunnel)?;
        let jump_config = resolve_ssh_config_from_input(&jump_input)
            .map_err(|message| failure(MonitorTransportErrorKind::Config, message))?;
        let jump_session = open_authenticated_session_kinded(&jump_config)
            .map_err(|(kind, message)| {
                failure(
                    map_failure_kind(kind),
                    format!("Jump host connection failed: {message}"),
                )
            })?;
        jump_session.set_keepalive(false, 0);
        jump_session.set_timeout(30000);

        // 2. Resolve Target Server configuration
        let target_input = request_to_input(request)?;
        let target_config = resolve_ssh_config_from_input(&target_input)
            .map_err(|message| failure(MonitorTransportErrorKind::Config, message))?;

        // 3. Open direct-tcpip channel on jump_session to target internal IP:port
        let channel = jump_session
            .channel_direct_tcpip(&target_config.host, target_config.port, None)
            .map_err(|error| {
                failure(
                    MonitorTransportErrorKind::Transport,
                    format!(
                        "Failed to open tunnel channel to {}:{}: {error}",
                        target_config.host, target_config.port
                    ),
                )
            })?;

        // 4. Create local TCP bridge pair on 127.0.0.1
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|error| failure(MonitorTransportErrorKind::Transport, error.to_string()))?;
        let addr = listener
            .local_addr()
            .map_err(|error| failure(MonitorTransportErrorKind::Transport, error.to_string()))?;
        let client_stream = std::net::TcpStream::connect_timeout(&addr, SSH_CONNECT_TIMEOUT)
            .map_err(|error| failure(MonitorTransportErrorKind::Transport, error.to_string()))?;
        let (server_stream, _) = listener
            .accept()
            .map_err(|error| failure(MonitorTransportErrorKind::Transport, error.to_string()))?;
        drop(listener);

        client_stream.set_nodelay(true).ok();
        client_stream
            .set_read_timeout(Some(SSH_CONNECT_TIMEOUT))
            .ok();
        client_stream
            .set_write_timeout(Some(SSH_CONNECT_TIMEOUT))
            .ok();

        // 5. Start background dedicated session bridge between local server_stream and tunnel channel
        let stop = Arc::new(AtomicBool::new(false));
        let bridge_stop = stop.clone();
        std::thread::spawn(move || {
            let _ = bridge_streams_dedicated_session(server_stream, &jump_session, channel, bridge_stop);
        });

        // 6. Connect and authenticate target session over bridged client stream
        let mut target_session = Session::new()
            .map_err(|error| failure(MonitorTransportErrorKind::Transport, error.to_string()))?;
        target_session.set_tcp_stream(client_stream);
        set_session_timeout(&target_session, SSH_CONNECT_TIMEOUT);
        target_session.handshake().map_err(|error| {
            failure(
                MonitorTransportErrorKind::Transport,
                format!("Target handshake failed: {error}"),
            )
        })?;
        verify_host_key(&target_session, &target_config).map_err(|error| {
            failure(
                MonitorTransportErrorKind::HostKey,
                format!("Target host key verification failed: {error}"),
            )
        })?;
        authenticate_session(&target_session, &target_config).map_err(|error| {
            failure(
                MonitorTransportErrorKind::Auth,
                format!("Target authentication failed: {error}"),
            )
        })?;
        set_session_timeout(&target_session, SSH_CONNECT_TIMEOUT);
        target_session.set_keepalive(false, 0);

        Ok(Box::new(SshMonitorConnection {
            session: target_session,
            _bridge_stop: Some(stop),
        }))
    }
}

impl MonitorConnector for SshMonitorConnector {
    fn connect(
        &self,
        request: &MonitorConnectRequest,
    ) -> Result<Box<dyn MonitorConnection>, MonitorTransportError> {
        if let Some(tunnel) = &request.tunnel {
            if tunnel.enabled {
                return self.connect_via_tunnel(request, tunnel);
            }
        }

        let input = request_to_input(request)?;
        let config = resolve_ssh_config_from_input(&input)
            .map_err(|message| failure(MonitorTransportErrorKind::Config, message))?;
        let session = open_authenticated_session_kinded(&config)
            .map_err(|(kind, message)| failure(map_failure_kind(kind), message))?;
        // Keepalives are driven explicitly by the monitor session so they can be
        // serialized with collection on this owned connection.
        session.set_keepalive(false, 0);
        session.set_timeout(SSH_IO_TIMEOUT.as_millis() as u32);
        Ok(Box::new(SshMonitorConnection {
            session,
            _bridge_stop: None,
        }))
    }
}

struct SshMonitorConnection {
    session: Session,
    _bridge_stop: Option<Arc<AtomicBool>>,
}

impl Drop for SshMonitorConnection {
    fn drop(&mut self) {
        self.close();
    }
}

impl MonitorConnection for SshMonitorConnection {
    fn run_collector(
        &mut self,
        script: &str,
        deadline: Duration,
    ) -> Result<CollectorOutput, MonitorTransportError> {
        let started = Instant::now();
        let remaining = remaining_deadline(started, deadline).unwrap_or(SSH_CONNECT_TIMEOUT);
        self.session.set_timeout(remaining.as_millis().max(1) as u32);
        let mut channel = self
            .session
            .channel_session()
            .map_err(|error| transport_error(error))?;
        channel
            .handle_extended_data(ExtendedData::Merge)
            .map_err(|error| transport_error(error))?;
        if let Err(error) = channel.exec("/bin/sh -s") {
            let _ = channel.close();
            return Err(transport_error(error));
        }
        if let Err(error) = channel.write_all(script.as_bytes()) {
            let _ = channel.close();
            return Err(transport_error_io(error));
        }
        if let Err(error) = channel.send_eof() {
            let _ = channel.close();
            return Err(transport_error(error));
        }

        let mut output = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let remaining = match remaining_deadline(started, deadline) {
                Some(remaining) => remaining,
                None => {
                    let _ = channel.close();
                    return Err(timeout_error());
                }
            };
            self.session
                .set_timeout(remaining.as_millis().max(1) as u32);
            match channel.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => output.extend_from_slice(&buffer[..read]),
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut =>
                {
                    if remaining_deadline(started, deadline).is_none() {
                        let _ = channel.close();
                        return Err(timeout_error());
                    }
                }
                Err(error) => {
                    let _ = channel.close();
                    return Err(transport_error_io(error));
                }
            }
        }

        let remaining = remaining_deadline(started, deadline).unwrap_or(Duration::from_millis(1));
        self.session
            .set_timeout(remaining.as_millis().max(1) as u32);
        if let Err(error) = channel.wait_close() {
            let _ = channel.close();
            return Err(transport_error(error));
        }
        let exit_code = channel.exit_status().ok();
        let _ = channel.close();
        self.session.set_timeout(SSH_IO_TIMEOUT.as_millis() as u32);

        Ok(CollectorOutput {
            stdout: String::from_utf8_lossy(&output).into_owned(),
            exit_code,
        })
    }

    fn keepalive(&mut self) -> Result<(), MonitorTransportError> {
        self.session
            .keepalive_send()
            .map(|_| ())
            .map_err(|error| transport_error(error))
    }

    fn close(&mut self) {
        let _ = self.session.disconnect(None, "", None);
        if let Some(stop) = &self._bridge_stop {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

fn tunnel_to_input(
    tunnel: &ServerMonitorTunnelRecord,
) -> Result<SshTunnelProbeDraftInput, MonitorTransportError> {
    match tunnel.source_kind {
        ServerMonitorSourceKind::SavedHost => {
            let alias = tunnel
                .saved_host_name
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    failure(
                        MonitorTransportErrorKind::Config,
                        "Tunnel saved SSH host alias is missing".to_string(),
                    )
                })?;
            Ok(SshTunnelProbeDraftInput {
                id: None,
                name: String::new(),
                group_id: None,
                source_kind: SshTunnelSourceKind::SavedHost,
                saved_host_name: Some(alias),
                custom: None,
                forward: default_forward(),
                auto_connect: false,
                auto_reconnect: false,
            })
        }
        ServerMonitorSourceKind::Custom => {
            let host = tunnel
                .host
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    failure(
                        MonitorTransportErrorKind::Config,
                        "Tunnel host is missing".to_string(),
                    )
                })?;
            let user = tunnel
                .user
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    failure(
                        MonitorTransportErrorKind::Config,
                        "Tunnel user is missing".to_string(),
                    )
                })?;
            let port = tunnel.port.unwrap_or(22);
            let (auth_kind, key_path) = match tunnel.auth_kind {
                Some(ServerMonitorAuthKind::Password) => (SshTunnelAuthKind::Password, None),
                Some(ServerMonitorAuthKind::Key) => {
                    (SshTunnelAuthKind::Key, tunnel.key_path.clone())
                }
                Some(ServerMonitorAuthKind::Agent) => {
                    return Err(failure(
                        MonitorTransportErrorKind::Config,
                        "Tunnel agent authentication requires a saved SSH host alias".to_string(),
                    ));
                }
                None => {
                    return Err(failure(
                        MonitorTransportErrorKind::Config,
                        "Tunnel authentication method is missing".to_string(),
                    ));
                }
            };
            Ok(SshTunnelProbeDraftInput {
                id: None,
                name: String::new(),
                group_id: None,
                source_kind: SshTunnelSourceKind::Custom,
                saved_host_name: None,
                custom: Some(SshTunnelCustomInput {
                    host,
                    port,
                    user,
                    auth_kind,
                    key_path,
                    password: tunnel.password.clone(),
                    preserve_password: Some(false),
                }),
                forward: default_forward(),
                auto_connect: false,
                auto_reconnect: false,
            })
        }
    }
}

fn request_to_input(
    request: &MonitorConnectRequest,
) -> Result<SshTunnelProbeDraftInput, MonitorTransportError> {
    match &request.source {
        MonitorSource::SavedHost { alias } => Ok(SshTunnelProbeDraftInput {
            id: None,
            name: String::new(),
            group_id: None,
            source_kind: SshTunnelSourceKind::SavedHost,
            saved_host_name: Some(alias.clone()),
            custom: None,
            forward: default_forward(),
            auto_connect: false,
            auto_reconnect: false,
        }),
        MonitorSource::Custom {
            host,
            port,
            user,
            auth,
        } => {
            let (auth_kind, key_path) = match auth {
                MonitorAuth::Password => (SshTunnelAuthKind::Password, None),
                MonitorAuth::Key { path } => (SshTunnelAuthKind::Key, Some(path.clone())),
                MonitorAuth::Agent => {
                    return Err(failure(
                        MonitorTransportErrorKind::Config,
                        "Agent authentication requires a saved SSH host alias".to_string(),
                    ))
                }
            };
            Ok(SshTunnelProbeDraftInput {
                id: None,
                name: String::new(),
                group_id: None,
                source_kind: SshTunnelSourceKind::Custom,
                saved_host_name: None,
                custom: Some(SshTunnelCustomInput {
                    host: host.clone(),
                    port: *port,
                    user: user.clone(),
                    auth_kind,
                    key_path,
                    password: request.password.clone(),
                    preserve_password: Some(false),
                }),
                forward: default_forward(),
                auto_connect: false,
                auto_reconnect: false,
            })
        }
    }
}

fn default_forward() -> SshTunnelForwardConfig {
    SshTunnelForwardConfig {
        mode: SshTunnelForwardMode::Local,
        local_bind_host: None,
        local_port: None,
        remote_bind_host: None,
        remote_port: None,
        target_host: None,
        target_port: None,
        dynamic_probe_host: None,
        dynamic_probe_port: None,
    }
}

fn remaining_deadline(started: Instant, deadline: Duration) -> Option<Duration> {
    deadline.checked_sub(started.elapsed())
}

fn map_failure_kind(kind: FailureKind) -> MonitorTransportErrorKind {
    match kind {
        FailureKind::Auth => MonitorTransportErrorKind::Auth,
        FailureKind::HostKey => MonitorTransportErrorKind::HostKey,
        FailureKind::Config => MonitorTransportErrorKind::Config,
        FailureKind::Transport | FailureKind::Port | FailureKind::Target => {
            MonitorTransportErrorKind::Transport
        }
    }
}

fn failure(kind: MonitorTransportErrorKind, message: String) -> MonitorTransportError {
    MonitorTransportError { kind, message }
}

fn timeout_error() -> MonitorTransportError {
    failure(
        MonitorTransportErrorKind::Timeout,
        "SSH collection exceeded its deadline".to_string(),
    )
}

fn transport_error(error: ssh2::Error) -> MonitorTransportError {
    let kind = if error.message().to_ascii_lowercase().contains("timeout") {
        MonitorTransportErrorKind::Timeout
    } else {
        MonitorTransportErrorKind::Transport
    };
    failure(kind, error.to_string())
}

fn transport_error_io(error: std::io::Error) -> MonitorTransportError {
    let kind = if error.kind() == std::io::ErrorKind::TimedOut {
        MonitorTransportErrorKind::Timeout
    } else {
        MonitorTransportErrorKind::Transport
    };
    failure(kind, error.to_string())
}
