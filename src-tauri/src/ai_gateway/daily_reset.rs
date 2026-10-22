//! Process-owned daily-reset recovery (REQ-003/AC-004, REQ-004/AC-007).
//!
//! One scan pass reads the latest encrypted gateway configuration through the
//! existing serialized writer, clears only the runtime mark fields of every
//! day-due daily-reset quota key, and emits the existing configuration-update
//! event through the supplied `AppHandle` only after a successful persisted
//! change. It performs no network request and rewrites nothing when no key is
//! due.
//!
//! A process-owned scanner runs the immediate startup pass once and then the
//! same pass every 60 seconds while the app runs, independently of WebViews,
//! the gateway listener and the template-refresh preference. It is installed
//! exactly once and stopped idempotently (the owned worker is signalled and
//! joined), so no overlapping writer is left running after shutdown.

use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Production period between periodic scans.
const DAILY_RESET_SCAN_PERIOD: Duration = Duration::from_secs(60);

/// Scan the latest persisted configuration once for day-due daily-reset quota
/// marks at the explicit `now`, clear their runtime mark fields, and return the
/// number of recovered keys.
///
/// Only keys satisfying [`super::selection::daily_quota_reset_due`] are touched,
/// so user `enabled`/`daily_reset` intent, authentication marks, missing or
/// future mark times, legacy (`daily_reset=false`) keys and unrelated
/// configuration all stay intact. A pass with nothing due does not rewrite the
/// file and emits no event; a failed write propagates the error, leaves the
/// previous file intact and emits nothing. After a successful persisted change
/// the existing [`super::AI_GATEWAY_CONFIG_UPDATED_EVENT`] is emitted with its
/// established `()` payload through the supplied `app` handle, independently of
/// the listener-captured handle, and the same successful notification is
/// recorded on the existing test-observable configuration-event seam.
pub(in crate::ai_gateway) fn scan_due_daily_reset_marks<R: tauri::Runtime>(
    app: &AppHandle<R>,
    now: u64,
) -> Result<usize, String> {
    let recovered = super::storage::modify_config::<usize>(|config| {
        let mut recovered = 0usize;
        let offset = config.settings.daily_reset_offset_hours;
        for provider in config.providers.iter_mut() {
            for key in provider.keys.iter_mut() {
                if super::selection::daily_quota_reset_due_with_offset(key, now, offset) {
                    super::selection::clear_key_runtime_state(key);
                    recovered += 1;
                }
            }
        }
        Ok((recovered > 0, recovered))
    })?;

    if recovered > 0 {
        record_config_update_event();
        let _ = app.emit(super::AI_GATEWAY_CONFIG_UPDATED_EVENT, ());
    }
    Ok(recovered)
}

/// Record the successful configuration-update notification on the existing
/// test-observable event seam. Recording the seam directly — rather than calling
/// [`super::runtime_http::emit_config_updated`] — keeps the emit on the supplied
/// `AppHandle` a single production emission instead of duplicating it through
/// the listener-captured handle.
#[cfg(test)]
fn record_config_update_event() {
    super::runtime_http::CONFIG_UPDATE_EVENTS.with(|events| {
        events
            .borrow_mut()
            .push(super::AI_GATEWAY_CONFIG_UPDATED_EVENT.to_string())
    });
}

/// Outside test builds no configuration-event seam exists.
#[cfg(not(test))]
fn record_config_update_event() {}

// ---------------------------------------------------------------------------
// Process-owned scanner lifecycle
// ---------------------------------------------------------------------------

/// Control handle of the installed process scanner. The stop sender keeps the
/// worker's receiver connected; dropping it ends the periodic loop, and the
/// stored join handle lets shutdown wait for the owned worker.
struct ScannerControl {
    stop: mpsc::Sender<()>,
    join: Option<std::thread::JoinHandle<()>>,
}

/// The installed scanner slot: `None` until the app setup (or a test seam)
/// installs it, and `None` again after stop/reset. The mutex-backed slot makes a
/// re-installation a non-replacing no-op.
fn scanner_state() -> &'static Mutex<Option<ScannerControl>> {
    static SCANNER: OnceLock<Mutex<Option<ScannerControl>>> = OnceLock::new();
    SCANNER.get_or_init(|| Mutex::new(None))
}

/// The periodic worker: read a fresh clock every pass and run one scan pass.
/// It returns when the stop sender is dropped (disconnected), so shutdown never
/// leaves a scanning writer behind.
fn scanner_loop<R, C>(app: AppHandle<R>, period: Duration, clock: C, stop: mpsc::Receiver<()>)
where
    R: tauri::Runtime,
    C: Fn() -> u64,
{
    loop {
        match stop.recv_timeout(period) {
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = scan_due_daily_reset_marks(&app, clock());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) | Ok(()) => return,
        }
    }
}

/// Install the scanner slot exactly once. `startup_scan` runs synchronously on
/// the calling thread before the worker is spawned (so a test seam's
/// thread-local event is observable and the immediate startup check never waits
/// for the periodic pass); `spawn` starts the owned worker and returns its join
/// handle. Returns `false` without replacing or re-spawning anything when a
/// scanner is already installed.
fn install_scanner<S>(startup_scan: impl FnOnce(), spawn: S) -> bool
where
    S: FnOnce(mpsc::Receiver<()>) -> std::thread::JoinHandle<()>,
{
    let mut slot = scanner_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if slot.is_some() {
        return false;
    }
    let (stop, stop_rx) = mpsc::channel();
    *slot = Some(ScannerControl { stop, join: None });
    startup_scan();
    let join = spawn(stop_rx);
    if let Some(control) = slot.as_mut() {
        control.join = Some(join);
    }
    true
}

/// Start the process-owned daily-reset scanner exactly once. A second call is a
/// no-op. The immediate startup scan runs synchronously before the worker is
/// spawned, and the worker then scans every [`DAILY_RESET_SCAN_PERIOD`] with a
/// fresh wall-clock time.
pub(crate) fn start_daily_reset_scanner(app: AppHandle) -> bool {
    let startup_app = app.clone();
    let loop_app = app;
    install_scanner(
        move || {
            let _ = scan_due_daily_reset_marks(&startup_app, super::types_config::now_ts());
        },
        move |stop| {
            std::thread::spawn(move || {
                scanner_loop(
                    loop_app,
                    DAILY_RESET_SCAN_PERIOD,
                    super::types_config::now_ts,
                    stop,
                )
            })
        },
    )
}

/// Stop the process-owned daily-reset scanner: drop the worker's stop sender so
/// its loop ends and join the owned worker, so shutdown never leaves an
/// overlapping writer. Idempotent and safe when no scanner runs.
pub(crate) fn stop_daily_reset_scanner() {
    let control = scanner_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    if let Some(control) = control {
        drop(control.stop);
        if let Some(join) = control.join {
            let _ = join.join();
        }
    }
}

/// Test seam: install the same production loop with an injected period and
/// clock instead of the real 60-second wait and wall clock. The immediate
/// startup scan runs synchronously on the calling thread; a second install
/// while started returns `false` and runs no second startup scan.
#[cfg(test)]
pub(in crate::ai_gateway) fn install_daily_reset_scanner_for_tests_with<R, C>(
    app: AppHandle<R>,
    period: Duration,
    clock: C,
) -> bool
where
    R: tauri::Runtime,
    C: Fn() -> u64 + Send + 'static,
{
    let startup_now = clock();
    let startup_app = app.clone();
    let loop_app = app;
    install_scanner(
        move || {
            let _ = scan_due_daily_reset_marks(&startup_app, startup_now);
        },
        move |stop| std::thread::spawn(move || scanner_loop(loop_app, period, clock, stop)),
    )
}

/// Test seam: whether a process scanner is currently installed.
#[cfg(test)]
pub(in crate::ai_gateway) fn daily_reset_scanner_started() -> bool {
    scanner_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_some()
}

/// Test seam: stop the scanner and release the slot so a later test can install
/// a fresh one. Idempotent.
#[cfg(test)]
pub(in crate::ai_gateway) fn reset_daily_reset_scanner_for_tests() {
    stop_daily_reset_scanner();
}
