use crate::ssh_tunnels::{
    set_session_timeout, LOCAL_BIND_HOST, RECONNECT_BACKOFF_STEP, SSH_IO_RETRY_BACKOFF,
    SSH_IO_TIMEOUT,
};
use ssh2::Session;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self};
use std::time::{Duration, Instant};

pub(in crate::ssh_tunnels) fn bind_local_listener(port: u16) -> Result<TcpListener, String> {
    let listener = TcpListener::bind((LOCAL_BIND_HOST, port))
        .map_err(|e| format!("Failed to bind local port {}: {}", port, e))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("Failed to set non-blocking listener on {}: {}", port, e))?;
    Ok(listener)
}

pub(in crate::ssh_tunnels) fn is_retryable_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

pub(in crate::ssh_tunnels) fn wait_before_io_retry(stop: &Arc<AtomicBool>) -> bool {
    if stop.load(Ordering::Relaxed) {
        return false;
    }
    thread::sleep(SSH_IO_RETRY_BACKOFF);
    !stop.load(Ordering::Relaxed)
}

pub(in crate::ssh_tunnels) fn sleep_respecting_stop(
    stop: &Arc<AtomicBool>,
    duration: Duration,
) -> bool {
    let deadline = Instant::now() + duration;
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let step = remaining.min(RECONNECT_BACKOFF_STEP);
        if step.is_zero() {
            break;
        }
        thread::sleep(step);
    }
    !stop.load(Ordering::Relaxed)
}

pub(in crate::ssh_tunnels) fn write_all_channel(
    channel: &mut ssh2::Channel,
    stop: &Arc<AtomicBool>,
    data: &[u8],
) -> Result<(), String> {
    let mut offset = 0;
    while offset < data.len() {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match channel.write(&data[offset..]) {
            Ok(0) => return Err("SSH channel closed while forwarding data".to_string()),
            Ok(written) => offset += written,
            Err(error) if is_retryable_io_error(&error) => {
                if !wait_before_io_retry(stop) {
                    return Ok(());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

pub(in crate::ssh_tunnels) fn write_all_socket(
    stream: &mut TcpStream,
    stop: &Arc<AtomicBool>,
    data: &[u8],
) -> Result<(), String> {
    let mut offset = 0;
    while offset < data.len() {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match stream.write(&data[offset..]) {
            Ok(0) => return Err("Local socket closed while forwarding data".to_string()),
            Ok(written) => offset += written,
            Err(error) if is_retryable_io_error(&error) => {
                if !wait_before_io_retry(stop) {
                    return Ok(());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

pub(in crate::ssh_tunnels) fn bridge_streams(
    socket: TcpStream,
    channel: ssh2::Channel,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    socket.set_read_timeout(Some(SSH_IO_TIMEOUT)).ok();
    socket.set_write_timeout(Some(SSH_IO_TIMEOUT)).ok();
    let socket_read = socket
        .try_clone()
        .map_err(|e| format!("Failed to clone local socket: {}", e))?;
    let mut socket_write = socket;
    let mut channel_write = channel.clone();
    let mut channel_close = channel.clone();
    let mut channel_read = channel;
    let stop_a = stop.clone();
    let stop_b = stop.clone();

    let a = thread::spawn(move || -> Result<(), String> {
        let mut socket_read = socket_read;
        let mut buf = [0u8; 8192];
        loop {
            if stop_a.load(Ordering::Relaxed) {
                let _ = channel_write.send_eof();
                return Ok(());
            }
            match socket_read.read(&mut buf) {
                Ok(0) => {
                    let _ = channel_write.send_eof();
                    let _ = channel_write.flush();
                    return Ok(());
                }
                Ok(read) => write_all_channel(&mut channel_write, &stop_a, &buf[..read])?,
                Err(error) if is_retryable_io_error(&error) => {
                    if !wait_before_io_retry(&stop_a) {
                        let _ = channel_write.send_eof();
                        return Ok(());
                    }
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    });

    let b = thread::spawn(move || -> Result<(), String> {
        let mut buf = [0u8; 8192];
        loop {
            if stop_b.load(Ordering::Relaxed) {
                let _ = socket_write.shutdown(Shutdown::Both);
                return Ok(());
            }
            match channel_read.read(&mut buf) {
                Ok(0) => {
                    let _ = socket_write.shutdown(Shutdown::Write);
                    return Ok(());
                }
                Ok(read) => write_all_socket(&mut socket_write, &stop_b, &buf[..read])?,
                Err(error) if is_retryable_io_error(&error) => {
                    if !wait_before_io_retry(&stop_b) {
                        let _ = socket_write.shutdown(Shutdown::Both);
                        return Ok(());
                    }
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    });

    let res_a = a
        .join()
        .map_err(|_| "Forwarding thread panicked".to_string())?;
    let res_b = b
        .join()
        .map_err(|_| "Forwarding thread panicked".to_string())?;
    let _ = channel_close.close();
    let _ = channel_close.wait_close();
    res_a?;
    res_b?;
    Ok(())
}

pub(in crate::ssh_tunnels) fn drain_written_prefix(buffer: &mut Vec<u8>, written: usize) {
    if written >= buffer.len() {
        buffer.clear();
    } else {
        buffer.drain(..written);
    }
}

pub(in crate::ssh_tunnels) fn bridge_streams_dedicated_session(
    mut socket: TcpStream,
    session: &Session,
    mut channel: ssh2::Channel,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    socket.set_nodelay(true).ok();
    socket
        .set_nonblocking(true)
        .map_err(|e| format!("Failed to set local socket non-blocking: {}", e))?;
    session.set_blocking(false);

    let result = (|| -> Result<(), String> {
        let mut socket_to_channel = Vec::with_capacity(8192);
        let mut channel_to_socket = Vec::with_capacity(8192);
        let mut socket_read_eof = false;
        let mut channel_read_eof = false;
        let mut buf = [0u8; 8192];

        loop {
            if stop.load(Ordering::Relaxed) {
                let _ = channel.send_eof();
                let _ = socket.shutdown(Shutdown::Both);
                return Ok(());
            }

            let mut progressed = false;

            if !socket_read_eof && socket_to_channel.is_empty() {
                match socket.read(&mut buf) {
                    Ok(0) => {
                        socket_read_eof = true;
                        let _ = channel.send_eof();
                        progressed = true;
                    }
                    Ok(read) => {
                        socket_to_channel.extend_from_slice(&buf[..read]);
                        progressed = true;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error.to_string()),
                }
            }

            while !socket_to_channel.is_empty() {
                match channel.write(&socket_to_channel) {
                    Ok(0) => return Err("SSH channel closed while forwarding data".to_string()),
                    Ok(written) => {
                        drain_written_prefix(&mut socket_to_channel, written);
                        progressed = true;
                    }
                    Err(error) if is_retryable_io_error(&error) => break,
                    Err(error) => return Err(error.to_string()),
                }
            }

            if !channel_read_eof && channel_to_socket.is_empty() {
                match channel.read(&mut buf) {
                    Ok(0) => {
                        channel_read_eof = true;
                        let _ = socket.shutdown(Shutdown::Write);
                        progressed = true;
                    }
                    Ok(read) => {
                        channel_to_socket.extend_from_slice(&buf[..read]);
                        progressed = true;
                    }
                    Err(error) if is_retryable_io_error(&error) => {}
                    Err(error) => return Err(error.to_string()),
                }
            }

            while !channel_to_socket.is_empty() {
                match socket.write(&channel_to_socket) {
                    Ok(0) => return Err("Local socket closed while forwarding data".to_string()),
                    Ok(written) => {
                        drain_written_prefix(&mut channel_to_socket, written);
                        progressed = true;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error.to_string()),
                }
            }

            if socket_read_eof
                && channel_read_eof
                && socket_to_channel.is_empty()
                && channel_to_socket.is_empty()
            {
                return Ok(());
            }

            if !progressed {
                thread::sleep(SSH_IO_RETRY_BACKOFF);
            }
        }
    })();

    session.set_blocking(true);
    set_session_timeout(session, SSH_IO_TIMEOUT);
    let _ = channel.close();
    let _ = channel.wait_close();
    result
}
