use crate::ssh_tunnels::{
    emit_tunnels_updated, update_record_error, RuntimeState, SshTunnelRecord, SshTunnelStatus,
    RECONNECT_INITIAL_BACKOFF, SUPERVISOR_RETRY_JITTER_RATIO, SUPERVISOR_RETRY_MAX_BASE_DELAY,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;

/// Classification of a tunnel runtime failure used to decide whether the
/// supervisor may retry automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ssh_tunnels) enum FailureKind {
    Transport,
    Auth,
    HostKey,
    Config,
    Port,
    Target,
}

/// Only connectivity-style failures are retried automatically; authentication,
/// host-key and configuration problems require user action.
pub(in crate::ssh_tunnels) fn is_retryable(kind: FailureKind) -> bool {
    matches!(
        kind,
        FailureKind::Transport | FailureKind::Port | FailureKind::Target
    )
}

/// Why a runtime function returned to the supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ssh_tunnels) enum RuntimeOutcome {
    /// The runtime exited because the user asked it to stop.
    Stopped,
    /// The runtime never reached the connected state.
    FailedAtStartup { kind: FailureKind, message: String },
    /// The runtime was connected and then lost the connection.
    DroppedAfterConnected { kind: FailureKind, message: String },
}

/// A pre-spawn `resolve_ssh_config_from_record` failure.
#[derive(Debug)]
pub(in crate::ssh_tunnels) struct PreSpawnConnectFailure {
    pub kind: FailureKind,
    pub message: String,
}

/// Exponential backoff with bounded jitter.
///
/// `base = min(2s * 2^attempt, 48s)` and `delay = base * (1 + jitter_fraction)`.
/// The caller passes `jitter_fraction` in `[0, 0.25]`, so the delay never
/// exceeds 60 seconds and stays exact when `jitter_fraction` is zero.
pub(in crate::ssh_tunnels) fn next_retry_delay(attempt: u32, jitter_fraction: f64) -> Duration {
    let exponent = attempt.min(31);
    let base = RECONNECT_INITIAL_BACKOFF
        .checked_mul(1u32 << exponent)
        .unwrap_or(SUPERVISOR_RETRY_MAX_BASE_DELAY);
    let base = base.min(SUPERVISOR_RETRY_MAX_BASE_DELAY);
    base.mul_f64(1.0 + jitter_fraction)
}

/// Side effects emitted by the supervisor loop. Kept abstract so the loop can
/// be exercised without a Tauri app or SSH.
pub(in crate::ssh_tunnels) trait SupervisorObserver {
    fn record_failure(&mut self, category: &str, kind: FailureKind, message: &str);
    fn state_changed(&mut self);
}

/// Production observer: persists the runtime error, records one deduplicated
/// message and emits `ssh-tunnels-updated`.
pub(in crate::ssh_tunnels) struct AppSupervisorObserver {
    app: AppHandle,
    record: SshTunnelRecord,
}

impl AppSupervisorObserver {
    pub(in crate::ssh_tunnels) fn new(app: &AppHandle, record: &SshTunnelRecord) -> Self {
        Self {
            app: app.clone(),
            record: record.clone(),
        }
    }
}

impl SupervisorObserver for AppSupervisorObserver {
    fn record_failure(&mut self, category: &str, _kind: FailureKind, message: &str) {
        let _ = update_record_error(&self.record.id, message);
        let input = tunnel_failure_message_input(&self.record, category, message);
        let _ = crate::messages::create_message_with_app(&self.app, input);
    }

    fn state_changed(&mut self) {
        emit_tunnels_updated(&self.app);
    }
}

/// Builds the message input mirroring `types_state::record_tunnel_failure`, so
/// supervisor failures deduplicate on the same key.
pub(in crate::ssh_tunnels) fn tunnel_failure_message_input(
    record: &SshTunnelRecord,
    category: &str,
    error: &str,
) -> crate::messages::MessageCreateInput {
    let title = match category {
        "auto-reconnect" => crate::messages::localized(
            "SSH 隧道自动重连失败",
            "SSH tunnel auto-reconnect failed",
        ),
        _ => crate::messages::localized(
            "SSH 隧道自动连接失败",
            "SSH tunnel auto-connect failed",
        ),
    };
    crate::messages::MessageCreateInput {
        source: "ssh_tunnels".to_string(),
        category: category.to_string(),
        severity: "error".to_string(),
        title,
        summary: Some(format!("{}: {}", record.name, error)),
        detail: Some(error.to_string()),
        dedupe_key: Some(format!("ssh-tunnels:{}:{}", category, record.id)),
        target: Some(crate::messages::MessageTarget {
            tab: "ssh-tunnels".to_string(),
            section: None,
            entity_id: Some(record.id.clone()),
        }),
        metadata: Some(serde_json::json!({
            "tunnel_id": record.id,
            "tunnel_name": record.name,
            "auto_connect": record.auto_connect,
            "auto_reconnect": record.auto_reconnect,
            "category": category,
        })),
    }
}

/// Applies a status change without holding the state lock while the observer
/// emits (the production observer re-locks the state through snapshot_state).
fn apply_status(
    state: &Arc<Mutex<RuntimeState>>,
    status: SshTunnelStatus,
    observer: &mut impl SupervisorObserver,
) {
    if let Ok(mut guard) = state.lock() {
        guard.status = status;
    }
    observer.state_changed();
}

/// Persistent supervision loop.
///
/// The caller has already set `Connecting`; the loop only changes the status
/// once it has an outcome. Retryable failures keep the loop alive, a
/// `DroppedAfterConnected` resets the backoff, and terminal failures or a
/// disabled `auto_reconnect` end in `Error`.
pub(in crate::ssh_tunnels) fn run_supervision<F, S, O>(
    state: &Arc<Mutex<RuntimeState>>,
    auto_reconnect: bool,
    stop: &Arc<AtomicBool>,
    mut next_outcome: F,
    mut sleep: S,
    observer: &mut O,
) where
    F: FnMut() -> RuntimeOutcome,
    S: FnMut(Duration) -> bool,
    O: SupervisorObserver,
{
    let mut attempt: u32 = 0;

    loop {
        if stop.load(Ordering::Relaxed) {
            apply_status(state, SshTunnelStatus::Disconnected, observer);
            return;
        }

        let (kind, message, category) = match next_outcome() {
            RuntimeOutcome::Stopped => {
                apply_status(state, SshTunnelStatus::Disconnected, observer);
                return;
            }
            RuntimeOutcome::FailedAtStartup { kind, message } => (kind, message, "auto-connect"),
            RuntimeOutcome::DroppedAfterConnected { kind, message } => {
                (kind, message, "auto-reconnect")
            }
        };

        if let Ok(mut guard) = state.lock() {
            guard.last_error = Some(message.clone());
        }
        observer.record_failure(category, kind, &message);

        if !auto_reconnect || !is_retryable(kind) {
            apply_status(state, SshTunnelStatus::Error, observer);
            return;
        }

        if category == "auto-reconnect" {
            attempt = 0;
        }
        let jitter_fraction = rand::random::<f64>() * SUPERVISOR_RETRY_JITTER_RATIO;
        let delay = next_retry_delay(attempt, jitter_fraction);
        attempt += 1;

        apply_status(state, SshTunnelStatus::Reconnecting, observer);

        if !sleep(delay) {
            apply_status(state, SshTunnelStatus::Disconnected, observer);
            return;
        }
    }
}
