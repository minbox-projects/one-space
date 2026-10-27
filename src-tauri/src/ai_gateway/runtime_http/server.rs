use super::*;
use super::events::{capture_app_handle, captured_app_handle};

pub(in crate::ai_gateway) struct RunningServer {
    pub(in crate::ai_gateway) port: u16,
    pub(in crate::ai_gateway) shutdown: Option<oneshot::Sender<()>>,
    /// Monotonic identity of this listener task. An exiting task retires the
    /// slot only while it still holds this generation, so it can never clear a
    /// replacement listener.
    pub(in crate::ai_gateway) generation: u64,
}

/// Process-monotonic listener generation; never reused within a process.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

pub(in crate::ai_gateway) static RUNNING_SERVER: OnceLock<Mutex<Option<RunningServer>>> =
    OnceLock::new();

pub(in crate::ai_gateway) fn state_lock() -> &'static Mutex<Option<RunningServer>> {
    RUNNING_SERVER.get_or_init(|| Mutex::new(None))
}

pub(in crate::ai_gateway) fn status_from_config(
    config: &GatewayConfig,
    running: bool,
) -> GatewayStatus {
    GatewayStatus {
        running,
        enabled: config.enabled,
        port: config.port,
        local_base_url: local_base_url(config.port),
        provider_count: config.providers.len(),
        auto_disabled_count: config
            .providers
            .iter()
            .flat_map(|provider| provider.mappings.iter())
            .filter(|mapping| mapping.auto_disabled)
            .count(),
        key_count: config.keys.iter().filter(|key| key.enabled).count(),
        default_key_id: config.default_key_id.clone(),
    }
}

/// Bind the configured loopback port and spawn the listener.
///
/// On bind failure the port configuration is left untouched and the error is
/// actionable: it names the port and the underlying cause. It never falls back
/// to another port.
pub(in crate::ai_gateway) async fn start_server(
    app: Option<tauri::AppHandle>,
) -> Result<GatewayStatus, String> {
    // Store the handle before the early-return path so a restart always
    // refreshes (or clears) the process-wide slot (REQ-005).
    capture_app_handle(app);
    let config = read_config_shared()?;
    let mut guard = state_lock().lock().await;
    if let Some(running) = guard.as_ref() {
        if running.port == config.port {
            return Ok(status_from_config(&config, true));
        }
        if let Some(mut running) = guard.take() {
            if let Some(tx) = running.shutdown.take() {
                let _ = tx.send(());
            }
        }
    }
    let bind_host = if config.settings.bind_address.trim().is_empty() {
        "127.0.0.1"
    } else {
        config.settings.bind_address.trim()
    };
    let listener = TcpListener::bind((bind_host, config.port))
        .await
        .map_err(|e| {
            format!(
                "failed to bind AI Gateway port {} on {}: {e}",
                config.port, bind_host
            )
        })?;
    let (tx, rx) = oneshot::channel();
    let port = config.port;
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::SeqCst);
    tauri::async_runtime::spawn(run_server(listener, rx, generation));
    *guard = Some(RunningServer {
        port,
        shutdown: Some(tx),
        generation,
    });
    Ok(status_from_config(&config, true))
}

pub(in crate::ai_gateway) async fn stop_server() -> Result<GatewayStatus, String> {
    let config = read_config_shared();
    let mut guard = state_lock().lock().await;
    let running_port = guard.as_ref().map(|running| running.port);
    if let Some(mut running) = guard.take() {
        if let Some(tx) = running.shutdown.take() {
            let _ = tx.send(());
        }
    }
    match config {
        Ok(config) => Ok(status_from_config(&config, false)),
        // Stopping the listener must not depend on a readable configuration:
        // an unreadable config still stops the server and reports a
        // best-effort status for the port that was running.
        Err(_) => Ok(GatewayStatus {
            running: false,
            enabled: false,
            port: running_port.unwrap_or(0),
            local_base_url: running_port.map(local_base_url).unwrap_or_default(),
            provider_count: 0,
            auto_disabled_count: 0,
            key_count: 0,
            default_key_id: None,
        }),
    }
}

/// Synchronous best-effort stop for the shutdown path: signal the running
/// listener (if one is registered) and clear the slot. The listener's exit path
/// then sees a slot that no longer holds its generation and publishes nothing.
/// Idempotent and safe when nothing runs or the lock is momentarily contended.
pub(in crate::ai_gateway) fn stop_server_now() {
    if let Ok(mut guard) = state_lock().try_lock() {
        if let Some(mut running) = guard.take() {
            if let Some(tx) = running.shutdown.take() {
                let _ = tx.send(());
            }
        }
    }
}

pub(in crate::ai_gateway) fn server_status() -> Result<GatewayStatus, String> {
    let config = read_config_shared()?;
    let running = state_lock()
        .try_lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false);
    Ok(status_from_config(&config, running))
}

pub(in crate::ai_gateway) async fn autostart(
    app: Option<tauri::AppHandle>,
) -> Result<GatewayStatus, String> {
    let config = read_config_shared()?;
    if config.enabled {
        start_server(app).await
    } else {
        Ok(status_from_config(&config, false))
    }
}

pub(in crate::ai_gateway) async fn run_server(
    listener: TcpListener,
    mut shutdown: oneshot::Receiver<()>,
    generation: u64,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _)) => {
                        tauri::async_runtime::spawn(async move {
                            let _ = handle_connection(stream).await;
                        });
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            _ = fault_future() => break,
        }
    }
    // Release the listening socket before publishing the stopped transition, so
    // a same-port restart that observes `running == false` can bind immediately.
    drop(listener);
    retire_listener(generation).await;
}

/// Retire this listener's slot after `run_server` exits, but only while the slot
/// still holds this exact generation: an old exiting task must never clear a
/// replacement listener. A genuine transition publishes the stopped status so a
/// faulted listener reads as not running and can be restarted on the same port.
///
/// Retirement runs in async context and awaits the state lock instead of
/// `try_lock`, so it can never be silently skipped while a concurrent
/// `start_server`/`stop_server` holds the lock (for example across
/// `start_server`'s `bind().await`). The lock guard is scoped so it is released
/// before the stopped-status event is emitted, and no other await is taken while
/// it is held, so waiting on the lock cannot deadlock.
async fn retire_listener(generation: u64) {
    #[cfg(test)]
    RETIRE_EPOCH.fetch_add(1, Ordering::SeqCst);
    let retired = {
        let mut guard = state_lock().lock().await;
        match guard.as_ref() {
            Some(running) if running.generation == generation => {
                *guard = None;
                true
            }
            _ => false,
        }
    };
    if retired {
        if let Some(handle) = captured_app_handle() {
            let _ = handle.emit("ai-gateway-status-update", ());
        }
    }
}

/// A future that never resolves in production; the fault-injection branch only
/// exists under `cfg(test)`.
#[cfg(not(test))]
async fn fault_future() {
    std::future::pending::<()>().await;
}

#[cfg(test)]
async fn fault_future() {
    fault_notify().notified().await;
}

#[cfg(test)]
fn fault_notify() -> &'static tokio::sync::Notify {
    static FAULT_NOTIFY: OnceLock<tokio::sync::Notify> = OnceLock::new();
    FAULT_NOTIFY.get_or_init(|| tokio::sync::Notify::new())
}

/// Test seam: force the next accept loop to exit as an abnormal (non-shutdown)
/// failure so a fault/restart race can be driven deterministically.
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::ai_gateway) fn force_next_accept_failure() {
    fault_notify().notify_one();
}

/// Monotonic count of listener tasks that have entered retirement. Test seam: a
/// test that holds the state lock can poll this until it advances, proving a
/// retirement is genuinely in flight (and therefore blocked awaiting the lock)
/// before it releases the lock — making the lock-contention retirement path
/// deterministic without production effect.
#[cfg(test)]
static RETIRE_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Test seam: read [`RETIRE_EPOCH`]. See its documentation for the deterministic
/// contention pattern.
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::ai_gateway) fn retire_listener_epoch() -> u64 {
    RETIRE_EPOCH.load(Ordering::SeqCst)
}
