use crate::ssh_tunnels::{
    clear_tunnel_desired, connect_internal_if_missing, desired_tunnel_ids, disconnect_runtime,
    emit_tunnels_updated, mark_tunnel_desired, runtime_manager, sleep_respecting_stop,
    update_record_error, RunningTunnel, RuntimeState, SshTunnelRecord, SshTunnelStatus,
    PROBE_INTERVAL, RECONNECT_BACKOFF_STEP, RECONNECT_INITIAL_BACKOFF,
    SUPERVISOR_RETRY_JITTER_RATIO, SUPERVISOR_RETRY_MAX_BASE_DELAY, TUNNEL_WATCHDOG_INTERVAL,
};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// Monotonic counter bumped on wake, window display or any other event that
/// should interrupt a backoff sleep or probe wait. Process memory only.
static RETRY_POKE_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Per-id start claims. A claim is only ever checked or inserted while holding
/// the runtime-manager lock first and this claims lock second, so a watchdog
/// restart and a manual connect can never both spawn a thread for one id.
static TUNNEL_START_CLAIMS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn tunnel_start_claims() -> &'static Mutex<HashSet<String>> {
    TUNNEL_START_CLAIMS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Claims the right to start `id`, returning `false` when the manager already
/// holds an instance or another start is in flight.
#[allow(dead_code)] // part of the start-claim interface exercised by the behavior tests
pub(in crate::ssh_tunnels) fn try_claim_tunnel_start(id: &str) -> bool {
    let manager = match runtime_manager().lock() {
        Ok(manager) => manager,
        Err(_) => return false,
    };
    if manager.contains_key(id) {
        return false;
    }
    match tunnel_start_claims().lock() {
        Ok(mut claims) => claims.insert(id.to_string()),
        Err(_) => false,
    }
}

pub(in crate::ssh_tunnels) fn release_tunnel_start_claim(id: &str) {
    if let Ok(mut claims) = tunnel_start_claims().lock() {
        claims.remove(id);
    }
}

#[allow(dead_code)] // part of the start-claim interface exercised by the behavior tests
pub(in crate::ssh_tunnels) fn tunnel_start_claim_held(id: &str) -> bool {
    tunnel_start_claims()
        .lock()
        .map(|claims| claims.contains(id))
        .unwrap_or(false)
}

/// Reserves `id` for a start: claims it, marks it desired and, when
/// `replace_existing` is set, removes and returns the running instance so the
/// caller can stop and join it outside the lock. Returns `Err` when a claim is
/// already held, or when an instance exists and `replace_existing` is false.
pub(in crate::ssh_tunnels) fn begin_tunnel_start(
    id: &str,
    replace_existing: bool,
) -> Result<Option<RunningTunnel>, String> {
    let mut manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    {
        let mut claims = tunnel_start_claims().lock().map_err(|e| e.to_string())?;
        if claims.contains(id) {
            return Err("A start is already in progress for this tunnel".to_string());
        }
        if manager.contains_key(id) && !replace_existing {
            return Err("Tunnel is already running".to_string());
        }
        claims.insert(id.to_string());
    }
    mark_tunnel_desired(id);
    Ok(if replace_existing { manager.remove(id) } else { None })
}

/// Releases the claim after the instance is in the manager (or the spawn
/// failed); the desired flag is left set so supervision continues.
pub(in crate::ssh_tunnels) fn finish_tunnel_start(id: &str) {
    release_tunnel_start_claim(id);
}

/// Releases the claim and clears the desired flag after a failed start so the
/// watchdog does not resurrect it.
pub(in crate::ssh_tunnels) fn abandon_tunnel_start(id: &str) {
    release_tunnel_start_claim(id);
    clear_tunnel_desired(id);
}

/// Stops a tunnel: clears the desired flag first so nothing restarts it, then
/// tears the runtime down.
pub(in crate::ssh_tunnels) fn disconnect_tunnel(id: &str) -> Result<(), String> {
    clear_tunnel_desired(id);
    disconnect_runtime(id)
}

/// Ids that should run but currently have no instance and no start claim.
pub(in crate::ssh_tunnels) fn watchdog_restart_candidates(
    desired: &HashSet<String>,
    busy: &HashSet<String>,
) -> Vec<String> {
    let mut candidates = desired.difference(busy).cloned().collect::<Vec<_>>();
    candidates.sort();
    candidates
}

pub(in crate::ssh_tunnels) fn bump_retry_poke() {
    RETRY_POKE_EPOCH.fetch_add(1, Ordering::SeqCst);
}

pub(in crate::ssh_tunnels) fn retry_poke_epoch() -> u64 {
    RETRY_POKE_EPOCH.load(Ordering::SeqCst)
}

/// Sleeps up to `duration`, waking early when the stop flag is set (returns
/// `false`) or when a retry poke arrives (returns `true`). Polls the poke epoch
/// at `RECONNECT_BACKOFF_STEP` granularity.
pub(in crate::ssh_tunnels) fn sleep_respecting_stop_and_poke(
    stop: &Arc<AtomicBool>,
    duration: Duration,
) -> bool {
    let start_epoch = retry_poke_epoch();
    let deadline = Instant::now() + duration;
    loop {
        if retry_poke_epoch() != start_epoch {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        let step = remaining.min(RECONNECT_BACKOFF_STEP);
        if !sleep_respecting_stop(stop, step) {
            return false;
        }
    }
}

/// A probe tick is due on the interval or immediately after a retry poke.
pub(in crate::ssh_tunnels) fn probe_tick_due(elapsed: Duration, poke_changed: bool) -> bool {
    elapsed >= PROBE_INTERVAL || poke_changed
}

/// Periodic watchdog: restarts a desired tunnel whose runtime thread ended
/// unexpectedly, never racing a live or in-flight instance. Emits nothing.
pub fn start_tunnel_watchdog(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(TUNNEL_WATCHDOG_INTERVAL);
        let desired = desired_tunnel_ids();
        if desired.is_empty() {
            continue;
        }
        let busy = {
            let manager = match runtime_manager().lock() {
                Ok(manager) => manager,
                Err(_) => continue,
            };
            let claimed = match tunnel_start_claims().lock() {
                Ok(claims) => claims,
                Err(_) => continue,
            };
            manager
                .keys()
                .chain(claimed.iter())
                .cloned()
                .collect::<HashSet<String>>()
        };
        for id in watchdog_restart_candidates(&desired, &busy) {
            if let Err(error) = connect_internal_if_missing(app.clone(), id.clone(), false) {
                log::debug!("SSH tunnel watchdog could not restart {}: {}", id, error);
            }
        }
    });
}

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
/// disabled `auto_reconnect` end in `Error`. The desired flag is cleared on
/// every exit that does not continue retrying.
pub(in crate::ssh_tunnels) fn run_supervision<F, S, O>(
    id: &str,
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
            clear_tunnel_desired(id);
            apply_status(state, SshTunnelStatus::Disconnected, observer);
            return;
        }

        let (kind, message, category) = match next_outcome() {
            RuntimeOutcome::Stopped => {
                clear_tunnel_desired(id);
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
            clear_tunnel_desired(id);
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
            clear_tunnel_desired(id);
            apply_status(state, SshTunnelStatus::Disconnected, observer);
            return;
        }
    }
}
